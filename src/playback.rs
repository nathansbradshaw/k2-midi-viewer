use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
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
}

#[derive(Debug)]
pub enum PlayEvent {
    NoteOn(u8),
    NoteOff(u8),
    Position(u64),
    Done,
}

pub struct PlaybackHandle {
    pub cmd_tx: SyncSender<PlayCmd>,
    // cpal::Stream is !Send on macOS CoreAudio, so it lives here in the main thread.
    // Dropping the handle drops the stream, stopping audio cleanly.
    _stream: Option<cpal::Stream>,
}

pub fn spawn(
    file: Arc<MidiFile>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    track_muted: Vec<bool>,
    midi_conn: Option<midir::MidiOutputConnection>,
) -> PlaybackHandle {
    let (cmd_tx, cmd_rx) = sync_channel(32);

    // When no hardware MIDI port is available, fall back to the built-in soft synth.
    // The Arc<Mutex<SoftSynth>> (Send) goes to the playback thread.
    // The cpal::Stream (!Send on CoreAudio) stays in the handle on the main thread.
    let (synth, stream) = match (midi_conn.is_none(), crate::synth::start_soft_synth()) {
        (true, Some((s, st))) => (Some(s), Some(st)),
        _                     => (None, None),
    };

    std::thread::spawn(move || {
        run(file, cmd_rx, events_out, audio_enabled, track_muted, midi_conn, synth);
    });

    PlaybackHandle { cmd_tx, _stream: stream }
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

fn find_cursor(events: &[crate::midi::TimedEvent], tick: u64) -> usize {
    events.partition_point(|e| e.tick < tick)
}

fn run(
    file: Arc<MidiFile>,
    cmd_rx: Receiver<PlayCmd>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    mut track_muted: Vec<bool>,
    mut midi_conn: Option<midir::MidiOutputConnection>,
    synth: Option<Arc<Mutex<SoftSynth>>>,
) {
    let mut cursor = 0usize;
    let mut playing = false;

    loop {
        // ── Idle: wait for Play ────────────────────────────────────────────
        if !playing {
            loop {
                match cmd_rx.recv() {
                    Ok(PlayCmd::Play) => { playing = true; break; }
                    Ok(PlayCmd::Stop) => { cursor = 0; }
                    Ok(PlayCmd::SeekTo(t)) => { cursor = find_cursor(&file.events, t); }
                    Ok(PlayCmd::SetTrackMuted(i, m)) => {
                        if let Some(s) = track_muted.get_mut(i) { *s = m; }
                    }
                    Ok(PlayCmd::SetAudio(v)) => {
                        audio_enabled.store(v, Ordering::Relaxed);
                    }
                    Ok(_) => {}
                    Err(_) => return, // handle dropped — exit thread
                }
            }
        }

        if cursor >= file.events.len() {
            cursor = 0;
        }

        // ── Playing: set up timing reference at current cursor ─────────────
        let start_tick = file.events[cursor].tick;
        let start_us = tick_to_micros_abs(start_tick, &file.tempo_map, file.ticks_per_beat);
        let wall_start = Instant::now();
        let mut last_pos_us = u64::MAX; // sentinel: no update sent yet

        'playing: loop {
            // Drain commands non-blocking
            loop {
                match cmd_rx.try_recv() {
                    Ok(PlayCmd::Pause) => {
                        all_notes_off(&mut midi_conn, &synth);
                        playing = false;
                        break 'playing;
                    }
                    Ok(PlayCmd::Stop) => {
                        all_notes_off(&mut midi_conn, &synth);
                        cursor = 0;
                        playing = false;
                        break 'playing;
                    }
                    Ok(PlayCmd::SeekTo(t)) => {
                        all_notes_off(&mut midi_conn, &synth);
                        cursor = find_cursor(&file.events, t);
                        // Break inner loop; playing=true so outer loop re-enters
                        // timing setup immediately with the new cursor.
                        break 'playing;
                    }
                    Ok(PlayCmd::SetTrackMuted(i, m)) => {
                        if let Some(s) = track_muted.get_mut(i) { *s = m; }
                    }
                    Ok(PlayCmd::SetAudio(v)) => {
                        audio_enabled.store(v, Ordering::Relaxed);
                        if !v { all_notes_off(&mut midi_conn, &synth); }
                    }
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }

            if cursor >= file.events.len() {
                events_out.lock().unwrap().push_back(PlayEvent::Done);
                cursor = 0;
                playing = false;
                break 'playing;
            }

            let ev = &file.events[cursor];
            let event_us = tick_to_micros_abs(ev.tick, &file.tempo_map, file.ticks_per_beat)
                .saturating_sub(start_us);
            let elapsed_us = wall_start.elapsed().as_micros() as u64;

            if event_us > elapsed_us {
                spin_sleep::sleep(Duration::from_micros(event_us - elapsed_us));
            }

            // Fire event
            let audio = audio_enabled.load(Ordering::Relaxed);
            let muted = track_muted.get(ev.track).copied().unwrap_or(false);

            if !muted {
                match ev.kind {
                    EventKind::NoteOn { note, velocity } => {
                        if audio {
                            if let Some(ref mut c) = midi_conn {
                                c.send(&[0x90 | (ev.channel & 0x0F), note, velocity]).ok();
                            } else if let Some(ref s) = synth {
                                if let Ok(mut s) = s.lock() { s.note_on(note, velocity); }
                            }
                        }
                        events_out.lock().unwrap().push_back(PlayEvent::NoteOn(note));
                    }
                    EventKind::NoteOff { note } => {
                        if audio {
                            if let Some(ref mut c) = midi_conn {
                                c.send(&[0x80 | (ev.channel & 0x0F), note, 0]).ok();
                            } else if let Some(ref s) = synth {
                                if let Ok(mut s) = s.lock() { s.note_off(note); }
                            }
                        }
                        events_out.lock().unwrap().push_back(PlayEvent::NoteOff(note));
                    }
                }
            }

            // Position update every ~80 ms
            let now_us = wall_start.elapsed().as_micros() as u64;
            if last_pos_us == u64::MAX || now_us.saturating_sub(last_pos_us) >= 80_000 {
                events_out.lock().unwrap().push_back(PlayEvent::Position(ev.tick));
                last_pos_us = now_us;
            }

            cursor += 1;
        }
    }
}
