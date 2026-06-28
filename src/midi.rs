use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MidiFile {
    pub ticks_per_beat: u16,
    pub tempo_map:      Vec<TempoChange>,
    pub time_sig:       (u8, u8),   // numerator, denominator
    pub key_sig:        i8,         // -7..+7, negative = flats, positive = sharps
    pub tracks:         Vec<TrackInfo>,
    pub events:         Vec<TimedEvent>,
    pub notes:          Vec<Note>,
    pub total_ticks:    u64,
}

#[derive(Debug, Clone)]
pub struct TempoChange {
    pub at_tick:         u64,
    pub micros_per_beat: u32,
}

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub index: usize,
    pub name:  Option<String>,
}

#[derive(Debug, Clone)]
pub struct TimedEvent {
    pub tick:    u64,
    pub track:   usize,
    pub channel: u8,
    pub kind:    EventKind,
}

#[derive(Debug, Clone)]
pub enum EventKind {
    NoteOn  { note: u8, velocity: u8 },
    NoteOff { note: u8 },
}

/// A fully resolved note with start and end time.
#[derive(Debug, Clone)]
pub struct Note {
    pub start_tick: u64,
    pub end_tick:   u64,
    pub midi_note:  u8,
    pub track:      usize,
    pub channel:    u8,
    pub velocity:   u8,
}

/// A chord-sized group of notes the user must play in practice mode.
#[derive(Debug, Clone)]
pub struct PlayEvent {
    pub tick:     u64,
    pub notes:    Vec<u8>,  // raw MIDI notes (before octave offset)
    pub duration: u64,      // ticks until next play event
}

// ---------------------------------------------------------------------------
// Load & parse
// ---------------------------------------------------------------------------

pub fn load(path: PathBuf) -> Result<MidiFile, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read error: {e}"))?;

    // midly borrows from the byte slice; we extract everything into owned types
    let smf = midly::Smf::parse(&bytes).map_err(|e| format!("parse error: {e}"))?;

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int(),
        midly::Timing::Timecode(fps, sub) => {
            // Convert SMPTE to approximate ticks/beat (rare in practice)
            (fps.as_f32() * sub as f32) as u16
        }
    };

    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut tempo_map: Vec<TempoChange> = Vec::new();
    let mut events: Vec<TimedEvent> = Vec::new();
    let mut time_sig = (4u8, 4u8);
    let mut key_sig = 0i8;

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut abs_tick = 0u64;
        let mut track_name: Option<String> = None;

        for event in track {
            abs_tick += event.delta.as_int() as u64;

            match event.kind {
                midly::TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        midly::MidiMessage::NoteOn { key, vel } => {
                            let note = key.as_int();
                            let velocity = vel.as_int();
                            // NoteOn with vel=0 is a NoteOff in MIDI spec
                            let kind = if velocity > 0 {
                                EventKind::NoteOn { note, velocity }
                            } else {
                                EventKind::NoteOff { note }
                            };
                            events.push(TimedEvent { tick: abs_tick, track: track_idx, channel: ch, kind });
                        }
                        midly::MidiMessage::NoteOff { key, .. } => {
                            events.push(TimedEvent {
                                tick:    abs_tick,
                                track:   track_idx,
                                channel: ch,
                                kind:    EventKind::NoteOff { note: key.as_int() },
                            });
                        }
                        _ => {}
                    }
                }
                midly::TrackEventKind::Meta(meta) => match meta {
                    midly::MetaMessage::Tempo(t) => {
                        tempo_map.push(TempoChange {
                            at_tick:         abs_tick,
                            micros_per_beat: t.as_int(),
                        });
                    }
                    midly::MetaMessage::TimeSignature(num, den_pow, _, _) => {
                        time_sig = (num, 1u8 << den_pow);
                    }
                    midly::MetaMessage::KeySignature(key, _scale) => {
                        key_sig = key;
                    }
                    midly::MetaMessage::TrackName(name) => {
                        track_name = std::str::from_utf8(name).ok().map(|s| s.to_string());
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        tracks.push(TrackInfo { index: track_idx, name: track_name });
    }

    // Sort merged event list by tick; stable so track order is preserved within a tick
    events.sort_by_key(|e| e.tick);

    if tempo_map.is_empty() {
        tempo_map.push(TempoChange { at_tick: 0, micros_per_beat: 500_000 }); // 120 BPM default
    } else if tempo_map[0].at_tick > 0 {
        tempo_map.insert(0, TempoChange { at_tick: 0, micros_per_beat: 500_000 });
    }
    tempo_map.sort_by_key(|t| t.at_tick);

    let total_ticks = events.iter().map(|e| e.tick).max().unwrap_or(0);

    let notes = pair_notes(&events, total_ticks);

    Ok(MidiFile { ticks_per_beat, tempo_map, time_sig, key_sig, tracks, events, notes, total_ticks })
}

// ---------------------------------------------------------------------------
// Note pairing (NoteOn → NoteOff)
// ---------------------------------------------------------------------------

fn pair_notes(events: &[TimedEvent], total_ticks: u64) -> Vec<Note> {
    // key: (channel, note) → (start_tick, track, velocity)
    let mut active: HashMap<(u8, u8), (u64, usize, u8)> = HashMap::new();
    let mut notes: Vec<Note> = Vec::new();

    for ev in events {
        match ev.kind {
            EventKind::NoteOn { note, velocity } => {
                active.insert((ev.channel, note), (ev.tick, ev.track, velocity));
            }
            EventKind::NoteOff { note } => {
                if let Some((start_tick, track, velocity)) = active.remove(&(ev.channel, note)) {
                    notes.push(Note {
                        start_tick,
                        end_tick: ev.tick,
                        midi_note: note,
                        track,
                        channel: ev.channel,
                        velocity,
                    });
                }
            }
        }
    }

    // Close any notes that never received a NoteOff
    for ((channel, note), (start_tick, track, velocity)) in active {
        notes.push(Note { start_tick, end_tick: total_ticks, midi_note: note, track, channel, velocity });
    }

    notes.sort_by_key(|n| n.start_tick);
    notes
}

// ---------------------------------------------------------------------------
// Octave auto-scaling
// ---------------------------------------------------------------------------

/// Returns (offset_in_semitones, notes_covered, total_unique_notes).
/// The offset should be added to a file MIDI note to get the keyboard MIDI note.
/// i.e., keyboard_note = file_note + offset  →  key = note_to_key[keyboard_note]
pub fn best_octave_offset(
    file: &MidiFile,
    keyboard_notes: &HashSet<u8>,
) -> (i8, usize, usize) {
    let file_notes: HashSet<u8> = file
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::NoteOn { note, .. } => Some(note),
            _ => None,
        })
        .collect();

    let total = file_notes.len();
    if total == 0 {
        return (0, 0, 0);
    }

    let mut best = (0i8, 0usize);

    for steps in -4i16..=4 {
        let offset_semitones = (steps * 12) as i8;
        let coverage = file_notes
            .iter()
            .filter(|&&n| {
                let shifted = n as i16 + offset_semitones as i16;
                (0..=127).contains(&shifted) && keyboard_notes.contains(&(shifted as u8))
            })
            .count();

        // Prefer more coverage; break ties toward offset=0
        if coverage > best.1 || (coverage == best.1 && offset_semitones.abs() < best.0.abs()) {
            best = (offset_semitones, coverage);
        }
    }

    (best.0, best.1, total)
}

// ---------------------------------------------------------------------------
// Practice mode chord grouping
// ---------------------------------------------------------------------------

/// Group notes into PlayEvents — chords of notes starting within `threshold`
/// ticks of each other that map onto the keyboard at the given octave offset.
pub fn group_play_events(
    notes: &[Note],
    keyboard_notes: &HashSet<u8>,
    octave_offset: i8,
    threshold_ticks: u64,
) -> Vec<PlayEvent> {
    let mut events: Vec<PlayEvent> = Vec::new();
    let mut i = 0;

    while i < notes.len() {
        let base_tick = notes[i].start_tick;
        let mut chord: Vec<u8> = Vec::new();
        let mut j = i;

        while j < notes.len() && notes[j].start_tick.saturating_sub(base_tick) <= threshold_ticks {
            let note = notes[j].midi_note;
            let shifted = note as i16 + octave_offset as i16;
            if (0..=127).contains(&shifted) && keyboard_notes.contains(&(shifted as u8)) {
                if !chord.contains(&note) {
                    chord.push(note);
                }
            }
            j += 1;
        }

        if !chord.is_empty() {
            let next_tick = notes.get(j).map(|n| n.start_tick).unwrap_or(base_tick + 480);
            events.push(PlayEvent {
                tick:     base_tick,
                notes:    chord,
                duration: next_tick - base_tick,
            });
        }

        i = j.max(i + 1);
    }

    events
}

// ---------------------------------------------------------------------------
// Timing conversion
// ---------------------------------------------------------------------------

/// Convert an absolute tick to absolute microseconds, accounting for all
/// tempo changes up to that point.
pub fn tick_to_micros_abs(tick: u64, tempo_map: &[TempoChange], ticks_per_beat: u16) -> u64 {
    let mut total_us = 0u64;
    let mut prev_tick = 0u64;
    let mut current_mpb = 500_000u64;

    for tc in tempo_map {
        if tc.at_tick >= tick {
            break;
        }
        total_us += (tc.at_tick - prev_tick) * current_mpb / ticks_per_beat as u64;
        prev_tick = tc.at_tick;
        current_mpb = tc.micros_per_beat as u64;
    }

    total_us += (tick - prev_tick) * current_mpb / ticks_per_beat as u64;
    total_us
}

/// Duration of the whole file in seconds.
pub fn total_duration_secs(file: &MidiFile) -> f64 {
    tick_to_micros_abs(file.total_ticks, &file.tempo_map, file.ticks_per_beat) as f64 / 1_000_000.0
}

/// Current tempo in BPM at a given tick.
pub fn bpm_at(tick: u64, tempo_map: &[TempoChange]) -> f64 {
    let mpb = tempo_map
        .iter()
        .rev()
        .find(|tc| tc.at_tick <= tick)
        .map(|tc| tc.micros_per_beat)
        .unwrap_or(500_000);
    60_000_000.0 / mpb as f64
}
