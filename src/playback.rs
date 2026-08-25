use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::midi::{EventKind, MidiFile, tick_to_micros_abs};
use crate::synth::SoftSynth;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PlayCmd {
    Play,
    Pause,
    Stop,
    SeekTo(u64),
    SetAudio(bool),
    SetTrackMuted(usize, bool),
    SetOctaveOffset(i8),
    SetWaveform(crate::synth::Waveform),
    LiveNoteOn(u8, u8, u8), // note, velocity, channel
    LiveNoteOff(u8, u8),    // note, channel
}

#[derive(Debug)]
pub enum PlayEvent {
    NoteOn(u8, usize, u8), // note, track index, channel
    NoteOff(u8, u8),       // note, channel
    Position(u64),
    Done,
}

pub struct PlaybackHandle {
    pub cmd_tx: Sender<PlayCmd>,
}

pub fn spawn(
    file: Arc<MidiFile>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    track_muted: Vec<bool>,
    midi_conn: Option<midir::MidiOutputConnection>,
    keyboard_notes: Arc<HashSet<u8>>,
    octave_offset: i8,
    waveform: crate::synth::Waveform,
    shared_synth: Option<Arc<Mutex<SoftSynth>>>,
) -> PlaybackHandle {
    // Transport controls must never block the UI thread. Seeking can generate
    // many updates while the slider is dragged, so use an unbounded channel;
    // the playback loop coalesces those updates naturally as it resets.
    let (cmd_tx, cmd_rx) = channel();

    // The main application owns the audio stream so live keys also work before
    // a file is loaded. Playback shares its synth when no hardware port is active.
    let synth = midi_conn.is_none().then_some(shared_synth).flatten();
    if let Some(ref synth) = synth {
        if let Ok(mut synth) = synth.lock() { synth.set_waveform(waveform); }
    }

    std::thread::spawn(move || {
        run(
            file, cmd_rx, events_out, audio_enabled, track_muted, midi_conn, synth,
            keyboard_notes, octave_offset,
        );
    });

    PlaybackHandle { cmd_tx }
}

// ---------------------------------------------------------------------------
// MIDI output helpers
// ---------------------------------------------------------------------------

pub fn list_output_ports() -> Vec<String> {
    midir::MidiOutput::new("k2-viewer-probe")
        .map(|out| {
            let ports = out.ports();
            ports.iter().filter_map(|p| out.port_name(p).ok()).collect()
        })
        .unwrap_or_default()
}

pub fn open_output(port_idx: usize) -> Option<midir::MidiOutputConnection> {
    let out = midir::MidiOutput::new("k2-viewer").ok()?;
    let ports = out.ports();
    let port = ports.get(port_idx)?;
    out.connect(port, "k2-viewer-out").ok()
}

// ---------------------------------------------------------------------------
// Thread implementation
// ---------------------------------------------------------------------------

fn all_notes_off(
    conn:  &mut Option<midir::MidiOutputConnection>,
    synth: &Option<Arc<Mutex<SoftSynth>>>,
) {
    if let Some(c) = conn {
        for ch in 0..16u8 {
            c.send(&[0xB0 | ch, 0x7B, 0x00]).ok();
        }
    }
    if let Some(s) = synth {
        if let Ok(mut s) = s.lock() { s.all_notes_off(); }
    }
}

fn live_note_on(
    conn: &mut Option<midir::MidiOutputConnection>,
    synth: &Option<Arc<Mutex<SoftSynth>>>,
    note: u8,
    velocity: u8,
    channel: u8,
) {
    if let Some(conn) = conn {
        conn.send(&[0x90 | (channel & 0x0F), note, velocity]).ok();
    } else if let Some(synth) = synth {
        if let Ok(mut synth) = synth.lock() { synth.note_on(note, velocity, channel); }
    }
}

fn live_note_off(
    conn: &mut Option<midir::MidiOutputConnection>,
    synth: &Option<Arc<Mutex<SoftSynth>>>,
    note: u8,
    channel: u8,
) {
    if let Some(conn) = conn {
        conn.send(&[0x80 | (channel & 0x0F), note, 0]).ok();
    } else if let Some(synth) = synth {
        if let Ok(mut synth) = synth.lock() { synth.note_off(note, channel); }
    }
}

fn find_cursor(events: &[crate::midi::TimedEvent], tick: u64) -> usize {
    events.partition_point(|e| e.tick < tick)
}

/// Inverse of `tick_to_micros_abs`, used to report a smooth playhead even
/// across long rests where there are no MIDI events to supply a tick value.
fn tick_at_micros(file: &MidiFile, micros: u64) -> u64 {
    let mut low = 0u64;
    let mut high = file.total_ticks;

    while low < high {
        let mid = low + (high - low + 1) / 2;
        if tick_to_micros_abs(mid, &file.tempo_map, file.ticks_per_beat) <= micros {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    low
}

fn publish_position(events_out: &Arc<Mutex<VecDeque<PlayEvent>>>, tick: u64, clear: bool) {
    let mut events = events_out.lock().unwrap();
    if clear { events.clear(); }
    events.push_back(PlayEvent::Position(tick));
}

fn run(
    file: Arc<MidiFile>,
    cmd_rx: Receiver<PlayCmd>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    mut track_muted: Vec<bool>,
    mut midi_conn: Option<midir::MidiOutputConnection>,
    synth: Option<Arc<Mutex<SoftSynth>>>,
    keyboard_notes: Arc<HashSet<u8>>,
    mut octave_offset: i8,
) {
    let mut cursor = 0usize;
    let mut playing = false;
    let mut position_tick = 0u64;

    // A note only actually sounds if it lands on a physical key once shifted.
    // GM percussion (channel 10) is exempt: those note numbers select a drum
    // sound rather than a pitch, so "does it fit on the keyboard" doesn't apply.
    let fits_keyboard = |note: u8, channel: u8, octave_offset: i8| -> bool {
        if channel == crate::synth::DRUM_CHANNEL { return true; }
        let shifted = (note as i16 + octave_offset as i16).clamp(0, 127) as u8;
        keyboard_notes.contains(&shifted)
    };

    loop {
        // ── Idle: wait for Play ────────────────────────────────────────────
        if !playing {
            match cmd_rx.recv() {
                Ok(PlayCmd::Play) => {
                    if position_tick >= file.total_ticks {
                        position_tick = 0;
                        cursor = 0;
                    }
                    playing = true;
                }
                Ok(PlayCmd::Pause) => {}
                Ok(PlayCmd::Stop) => {
                    all_notes_off(&mut midi_conn, &synth);
                    position_tick = 0;
                    cursor = 0;
                    publish_position(&events_out, 0, true);
                }
                Ok(PlayCmd::SeekTo(t)) => {
                    all_notes_off(&mut midi_conn, &synth);
                    position_tick = t.min(file.total_ticks);
                    cursor = find_cursor(&file.events, position_tick);
                    publish_position(&events_out, position_tick, true);
                }
                Ok(PlayCmd::SetTrackMuted(i, m)) => {
                    if let Some(s) = track_muted.get_mut(i) { *s = m; }
                }
                Ok(PlayCmd::SetAudio(v)) => {
                    audio_enabled.store(v, Ordering::Relaxed);
                }
                Ok(PlayCmd::SetOctaveOffset(v)) => { octave_offset = v; }
                Ok(PlayCmd::SetWaveform(waveform)) => {
                    if let Some(ref synth) = synth {
                        if let Ok(mut synth) = synth.lock() { synth.set_waveform(waveform); }
                    }
                }
                Ok(PlayCmd::LiveNoteOn(note, velocity, channel)) => {
                    if audio_enabled.load(Ordering::Relaxed) {
                        live_note_on(&mut midi_conn, &synth, note, velocity, channel);
                    }
                }
                Ok(PlayCmd::LiveNoteOff(note, channel)) => {
                    live_note_off(&mut midi_conn, &synth, note, channel);
                }
                Err(_) => {
                    all_notes_off(&mut midi_conn, &synth);
                    return;
                }
            }
            continue;
        }

        // ── Playing: anchor the clock to the exact playhead tick ────────────
        let start_us = tick_to_micros_abs(position_tick, &file.tempo_map, file.ticks_per_beat);
        let wall_start = Instant::now();
        let mut last_position_report = Instant::now();

        // A seek can land in the middle of a held note. Restore those notes so
        // playback resumes from the requested musical state instead of waiting
        // for the next NoteOn event.
        if position_tick > 0 {
            let audio = audio_enabled.load(Ordering::Relaxed);
            for note in &file.notes {
                if note.start_tick >= position_tick || note.end_tick <= position_tick { continue; }
                if track_muted.get(note.track).copied().unwrap_or(false) { continue; }

                if audio && fits_keyboard(note.midi_note, note.channel, octave_offset) {
                    if let Some(ref mut c) = midi_conn {
                        c.send(&[0x90 | (note.channel & 0x0F), note.midi_note, note.velocity]).ok();
                    } else if let Some(ref s) = synth {
                        if let Ok(mut s) = s.lock() {
                            s.note_on(note.midi_note, note.velocity, note.channel);
                        }
                    }
                }
                events_out.lock().unwrap().push_back(PlayEvent::NoteOn(
                    note.midi_note, note.track, note.channel,
                ));
            }
        }

        'playing: loop {
            let now_us = start_us.saturating_add(wall_start.elapsed().as_micros() as u64);
            let timeline_finished = cursor >= file.events.len();
            let event_us = file.events.get(cursor)
                .map(|ev| tick_to_micros_abs(ev.tick, &file.tempo_map, file.ticks_per_beat))
                .unwrap_or_else(|| {
                    tick_to_micros_abs(file.total_ticks, &file.tempo_map, file.ticks_per_beat)
                });
            let until_event_us = event_us.saturating_sub(now_us);

            // Wake at least every 20 ms to keep the playhead smooth, but wake
            // immediately for any transport command—even during a long rest.
            let wait_us = until_event_us.min(20_000);
            match cmd_rx.recv_timeout(Duration::from_micros(wait_us)) {
                Ok(PlayCmd::Pause) => {
                    position_tick = tick_at_micros(
                        &file,
                        start_us.saturating_add(wall_start.elapsed().as_micros() as u64),
                    );
                    all_notes_off(&mut midi_conn, &synth);
                    publish_position(&events_out, position_tick, true);
                    playing = false;
                    break 'playing;
                }
                Ok(PlayCmd::Stop) => {
                    all_notes_off(&mut midi_conn, &synth);
                    position_tick = 0;
                    cursor = 0;
                    publish_position(&events_out, 0, true);
                    playing = false;
                    break 'playing;
                }
                Ok(PlayCmd::SeekTo(t)) => {
                    all_notes_off(&mut midi_conn, &synth);
                    position_tick = t.min(file.total_ticks);
                    cursor = find_cursor(&file.events, position_tick);
                    publish_position(&events_out, position_tick, true);
                    // Keep `playing` true and reset the timing anchor.
                    break 'playing;
                }
                Ok(PlayCmd::SetTrackMuted(i, m)) => {
                    if let Some(s) = track_muted.get_mut(i) { *s = m; }
                    continue;
                }
                Ok(PlayCmd::SetAudio(v)) => {
                    audio_enabled.store(v, Ordering::Relaxed);
                    if !v { all_notes_off(&mut midi_conn, &synth); }
                    continue;
                }
                Ok(PlayCmd::SetOctaveOffset(v)) => {
                    octave_offset = v;
                    continue;
                }
                Ok(PlayCmd::SetWaveform(waveform)) => {
                    if let Some(ref synth) = synth {
                        if let Ok(mut synth) = synth.lock() { synth.set_waveform(waveform); }
                    }
                    continue;
                }
                Ok(PlayCmd::LiveNoteOn(note, velocity, channel)) => {
                    if audio_enabled.load(Ordering::Relaxed) {
                        live_note_on(&mut midi_conn, &synth, note, velocity, channel);
                    }
                    continue;
                }
                Ok(PlayCmd::LiveNoteOff(note, channel)) => {
                    live_note_off(&mut midi_conn, &synth, note, channel);
                    continue;
                }
                Ok(PlayCmd::Play) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    all_notes_off(&mut midi_conn, &synth);
                    return;
                }
                Err(RecvTimeoutError::Timeout) if until_event_us > wait_us => {
                    if last_position_report.elapsed() >= Duration::from_millis(50) {
                        position_tick = tick_at_micros(
                            &file,
                            start_us.saturating_add(wall_start.elapsed().as_micros() as u64),
                        );
                        publish_position(&events_out, position_tick, false);
                        last_position_report = Instant::now();
                    }
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }

            // The final note event can precede EndOfTrack. Keep the transport
            // alive through that trailing timeline instead of snapping to zero
            // as soon as the event list is exhausted.
            if timeline_finished {
                events_out.lock().unwrap().push_back(PlayEvent::Done);
                all_notes_off(&mut midi_conn, &synth);
                cursor = 0;
                position_tick = 0;
                playing = false;
                break 'playing;
            }

            // Fire event
            let ev = &file.events[cursor];
            let audio = audio_enabled.load(Ordering::Relaxed);
            let muted = track_muted.get(ev.track).copied().unwrap_or(false);

            if !muted {
                match ev.kind {
                    EventKind::NoteOn { note, velocity } => {
                        if audio && fits_keyboard(note, ev.channel, octave_offset) {
                            if let Some(ref mut c) = midi_conn {
                                c.send(&[0x90 | (ev.channel & 0x0F), note, velocity]).ok();
                            } else if let Some(ref s) = synth {
                                if let Ok(mut s) = s.lock() { s.note_on(note, velocity, ev.channel); }
                            }
                        }
                        events_out.lock().unwrap().push_back(PlayEvent::NoteOn(note, ev.track, ev.channel));
                    }
                    EventKind::NoteOff { note } => {
                        if audio && fits_keyboard(note, ev.channel, octave_offset) {
                            if let Some(ref mut c) = midi_conn {
                                c.send(&[0x80 | (ev.channel & 0x0F), note, 0]).ok();
                            } else if let Some(ref s) = synth {
                                if let Ok(mut s) = s.lock() { s.note_off(note, ev.channel); }
                            }
                        }
                        events_out.lock().unwrap().push_back(PlayEvent::NoteOff(note, ev.channel));
                    }
                }
            }

            if last_position_report.elapsed() >= Duration::from_millis(50) {
                position_tick = ev.tick;
                publish_position(&events_out, position_tick, false);
                last_position_report = Instant::now();
            }

            cursor += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{EventKind, MidiFile, TempoChange, TimedEvent};

    fn timeline(events: Vec<TimedEvent>, total_ticks: u64) -> MidiFile {
        MidiFile {
            ticks_per_beat: 480,
            tempo_map: vec![
                TempoChange { at_tick: 0, micros_per_beat: 500_000 },
                TempoChange { at_tick: 960, micros_per_beat: 1_000_000 },
            ],
            time_sig: (4, 4),
            key_sig: 0,
            tracks: Vec::new(),
            events,
            notes: Vec::new(),
            total_ticks,
        }
    }

    #[test]
    fn cursor_includes_events_exactly_at_seek_tick() {
        let events = vec![
            TimedEvent {
                tick: 100,
                track: 0,
                channel: 0,
                kind: EventKind::NoteOn { note: 60, velocity: 100 },
            },
            TimedEvent {
                tick: 200,
                track: 0,
                channel: 0,
                kind: EventKind::NoteOff { note: 60 },
            },
        ];

        assert_eq!(find_cursor(&events, 100), 0);
        assert_eq!(find_cursor(&events, 101), 1);
        assert_eq!(find_cursor(&events, 200), 1);
        assert_eq!(find_cursor(&events, 201), 2);
    }

    #[test]
    fn clock_inverse_handles_tempo_changes() {
        let file = timeline(Vec::new(), 1_920);

        assert_eq!(tick_at_micros(&file, 500_000), 480);
        assert_eq!(tick_at_micros(&file, 1_500_000), 1_200);
        assert_eq!(tick_at_micros(&file, u64::MAX), file.total_ticks);
    }

    #[test]
    fn seek_interrupts_a_long_rest() {
        let file = Arc::new(timeline(
            vec![
                TimedEvent {
                    tick: 0,
                    track: 0,
                    channel: 0,
                    kind: EventKind::NoteOn { note: 60, velocity: 100 },
                },
                TimedEvent {
                    tick: 4_800,
                    track: 0,
                    channel: 0,
                    kind: EventKind::NoteOff { note: 60 },
                },
            ],
            4_800,
        ));
        let events_out = Arc::new(Mutex::new(VecDeque::new()));
        let thread_events = Arc::clone(&events_out);
        let (tx, rx) = channel();
        let worker = std::thread::spawn(move || {
            run(
                file,
                rx,
                thread_events,
                Arc::new(AtomicBool::new(false)),
                vec![false],
                None,
                None,
                Arc::new(HashSet::from([60])),
                0,
            );
        });

        tx.send(PlayCmd::Play).unwrap();
        tx.send(PlayCmd::SeekTo(4_700)).unwrap();

        let deadline = Instant::now() + Duration::from_millis(250);
        let mut reached_seek = false;
        while Instant::now() < deadline {
            reached_seek = events_out.lock().unwrap().iter()
                .any(|event| matches!(event, PlayEvent::Position(4_700)));
            if reached_seek { break; }
            std::thread::sleep(Duration::from_millis(5));
        }

        tx.send(PlayCmd::Stop).ok();
        drop(tx);
        worker.join().unwrap();
        assert!(reached_seek, "seek command was not handled promptly");
    }
}
