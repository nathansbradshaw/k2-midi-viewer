# MIDI Playback Plan

## Decisions
1. **Audio** — on by default, toggleable with a speaker button; timing/highlighting runs regardless
2. **Octave** — auto-scale: scan the file's note range and find the octave offset that fits the most notes onto the keyboard; display how many notes fell outside and were skipped
3. **Tracks** — all tracks play simultaneously; each track has an individual mute toggle
4. **Viewer** — traditional staff notation (treble + bass clef); the keyboard itself is the piano roll
5. **Practice mode** — bonus; song pauses at each note/chord and waits for the user to play it on the MIDI keyboard before advancing

---

## New dependencies

```toml
[dependencies]
# existing
iced  = { version = "0.13", features = ["canvas"] }

# Phase 1 — load & parse
midly = "0.5"    # pure-Rust MIDI file parser; no unsafe, no_std compatible
rfd   = "0.17"   # native OS file-open dialog (macOS sheet, Windows Explorer, Linux portal)

# Phase 2 — playback
midir      = "0.11"  # cross-platform MIDI I/O → system synth / DAW / virtual port
spin_sleep = "1.2"   # accurate sub-millisecond sleep for the timing loop
```

**Why these specifically:**

| Crate | Alternatives | Reason chosen |
|-------|-------------|---------------|
| `midly` | `midi-file`, `ghakuf` | Fastest, cleanest API, actively maintained, handles all MIDI types |
| `rfd` | `native-dialog` | Only option with async support that fits iced's Task model |
| `midir` | `nodi`, `cpal`+soundfont | Most direct route to system audio; `nodi` wraps `midir` anyway |
| `spin_sleep` | `std::thread::sleep` | OS sleep jitter is 1–15ms; audible at fast tempos; spin_sleep gives ~10µs |

No extra crates for the staff — drawn entirely with iced canvas primitives.

---

## Phase 1 — Load & parse

### Goal
Open a `.mid` file; display metadata; determine the octave offset; identify tracks.

### Crates: `midly`, `rfd`

### New module: `src/midi.rs`

```rust
pub struct MidiFile {
    pub ticks_per_beat:  u16,
    pub tempo_map:       Vec<TempoChange>,     // (at_tick, micros_per_beat)
    pub time_sig:        (u8, u8),             // numerator, denominator
    pub key_sig:         i8,                   // -7..+7 sharps/flats (from meta 0x59)
    pub tracks:          Vec<TrackInfo>,
    pub events:          Vec<TimedEvent>,      // all tracks merged, absolute ticks
}

pub struct TrackInfo {
    pub index: usize,
    pub name:  Option<String>,   // from MIDI Name meta-event if present
}

pub struct TimedEvent {
    pub tick:    u64,
    pub track:   usize,
    pub channel: u8,
    pub kind:    EventKind,
}

pub enum EventKind {
    NoteOn  { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    Tempo   { micros_per_beat: u32 },
}
```

`midly` gives per-track delta-tick events. `midi.rs` merges all tracks, converts delta→absolute ticks, and extracts the tempo/key/time-signature meta-events.

Also exposes:
```rust
pub fn tick_to_micros(tick: u64, tempo_map: &[TempoChange], ticks_per_beat: u16) -> u64
```

### Auto-scaling octave offset

The keyboard has a fixed set of raw MIDI notes (54–104, with gaps — call this `KEYBOARD_NOTES: HashSet<u8>`).

On file load:
```
1. Collect all unique NoteOn notes from the file → file_notes: HashSet<u8>
2. For each candidate offset O in -4*12 .. +4*12 step 12:
       coverage(O) = |{ n + O : n ∈ file_notes } ∩ KEYBOARD_NOTES|
3. Choose O* = argmax coverage(O), breaking ties toward O=0
4. skipped = |file_notes| - coverage(O*)
5. Store O* in App state; apply it when building note_to_key lookups
```

Display in the toolbar: `"Shifted +1 oct  (4 notes out of range)"`  
If coverage is 0 for all offsets: show a warning banner instead of playing.

### Layout change

Export the keyboard's raw note set alongside the key→id map:

```rust
pub struct Layout {
    pub keys:         Vec<Key>,
    pub note_to_key:  HashMap<u8, KeyId>,   // raw note → KeyId (offset applied at lookup time)
    pub keyboard_notes: HashSet<u8>,        // for coverage calculation
}
pub fn build_layout() -> Layout
```

At lookup time: `note_to_key.get(&(midi_note.wrapping_add_signed(octave_offset)))`

### New App state
```rust
midi:           Option<MidiFile>,
octave_offset:  i8,    // semitones; auto-set on load, user-adjustable ±12
skipped_notes:  usize,
track_muted:    Vec<bool>,
```

### New Messages
```rust
OpenFile,
FileChosen(Option<PathBuf>),
MidiLoaded(MidiFile),
OctaveOffsetChanged(i8),   // manual ±12 nudge buttons
TrackMuted(usize, bool),
Noop,
```

### File dialog wiring
```rust
// iced 0.13 Task API
Message::OpenFile => Task::perform(
    async {
        rfd::AsyncFileDialog::new()
            .add_filter("MIDI", &["mid", "midi"])
            .pick_file().await
            .map(|h| h.path().to_path_buf())
    },
    Message::FileChosen,
),
Message::FileChosen(Some(path)) => Task::perform(
    async move { midi::load(path) },
    |r| r.map(Message::MidiLoaded).unwrap_or(Message::Noop),
),
```

### UI additions (Phase 1)
- **[Open MIDI]** button in toolbar
- On load: `filename.mid  |  120 BPM  |  4/4  |  1:47  |  Shifted +1 oct  (4 notes out of range)`
- Track list panel: one row per track with name + **[M]** mute toggle
- **[− oct]  [+ oct]** nudge buttons to manually adjust if auto-scale isn't quite right

---

## Phase 2 — Playback with key highlighting

### Goal
Play/pause/stop with accurate timing. Keys light up. Audio through system synth, toggleable.

### Crates: `midir`, `spin_sleep`

### `midir` setup

```rust
// enumerate available output ports at startup
let midi_out  = MidiOutput::new("k2-midi-viewer")?;
let ports     = midi_out.ports();   // Vec<MidiOutputPort>
let port_names: Vec<String> = ports.iter()
    .map(|p| midi_out.port_name(p).unwrap_or_default())
    .collect();

// connect to selected port
let connection = midi_out.connect(&ports[selected_idx], "k2-playback")?;
```

On macOS the first port is usually the built-in DLS synthesizer — no setup needed.  
On Linux a running `timidity --alsa-patch` or `fluidsynth` process creates a connectable port.

### New module: `src/playback.rs`

Runs a dedicated OS thread — not async — so the iced runtime's scheduling can't introduce jitter:

```rust
pub enum PlayCmd   { Play, Pause, Stop, SeekTo(u64) }
pub enum PlayEvent { NoteOn(u8), NoteOff(u8), Position(u64), Done }

pub struct PlaybackHandle {
    pub cmd_tx: SyncSender<PlayCmd>,
    pub evt_rx: Arc<Mutex<Receiver<PlayEvent>>>,
}
```

**Timing loop (inside the spawned thread):**
```
midi_out    = MidiOutputConnection passed in
audio_on    = Arc<AtomicBool> shared with App (for the mute toggle)
cursor      = 0
wall_start  = Instant::now()
tick_start  = 0u64

loop {
    // drain command channel (non-blocking)
    match cmd_rx.try_recv() {
        Pause       → stash cursor + current_tick, send all-notes-off, break inner
        Stop        → send all-notes-off, break outer
        SeekTo(t)   → recalculate cursor to tick t, reset wall_start
    }

    next = events[cursor]
    target_us = tick_to_micros(next.tick - tick_start, tempo_map, ticks_per_beat)
    elapsed   = wall_start.elapsed().as_micros()

    if target_us > elapsed {
        spin_sleep::sleep(Duration::from_micros(target_us - elapsed))
    }

    // skip events from muted tracks
    if !track_muted[next.track] {
        if audio_on.load(Relaxed) {
            match next.kind {
                NoteOn  { note, vel } → midi_out.send(&[0x90 | ch, note, vel])
                NoteOff { note }      → midi_out.send(&[0x80 | ch, note, 0])
            }
        }
        evt_tx.send(PlayEvent::NoteOn/Off(note))
    }

    // send position update every ~80ms for scrubber
    if last_pos_update.elapsed() > 80ms {
        evt_tx.send(PlayEvent::Position(next.tick))
        last_pos_update = Instant::now()
    }

    cursor += 1
    if cursor == events.len() { evt_tx.send(Done); break }
}
```

**All-notes-off on pause/stop:** send `0xB0 0x7B 0x00` on every channel (MIDI CC 123) to avoid stuck notes.

### Iced subscription bridge

```rust
fn playback_subscription(state: &App) -> Subscription<Message> {
    let Some(handle) = &state.playback else { return Subscription::none() };
    let evt_rx = handle.evt_rx.clone();

    subscription::channel(
        TypeId::of::<PlaybackHandle>(),
        64,
        |mut output| async move {
            loop {
                let evt = { evt_rx.lock().unwrap().recv().unwrap() };
                let msg = match evt {
                    PlayEvent::NoteOn(n)   => Message::NoteOn(n),
                    PlayEvent::NoteOff(n)  => Message::NoteOff(n),
                    PlayEvent::Position(t) => Message::PlayPosition(t),
                    PlayEvent::Done        => Message::PlaybackStopped,
                };
                output.send(msg).await.ok();
            }
        },
    )
}
```

### Key highlighting

`Message::NoteOn(note)`:
1. `layout.note_to_key.get(&note.wrapping_add_signed(octave_offset))` → `KeyId`
2. Insert into `highlighted`; invalidate canvas cache

`Message::NoteOff(note)`:
1. Same lookup → remove from `highlighted`; invalidate cache

Notes outside the keyboard range are silently skipped (already accounted for in the "N notes out of range" count).

### Audio toggle

```rust
audio_enabled: Arc<AtomicBool>,   // shared with the playback thread
```

The speaker button flips it. The playback thread reads it before every `midi_out.send(...)`. No thread synchronization overhead beyond a single atomic load per event.

### New App state
```rust
playback:       Option<PlaybackHandle>,
play_state:     PlayState,   // Stopped | Playing | Paused { at_tick: u64 }
position_tick:  u64,
audio_enabled:  Arc<AtomicBool>,
midi_port_idx:  usize,
```

### New Messages
```rust
Play, Pause, Stop,
NoteOn(u8), NoteOff(u8),
PlayPosition(u64),
PlaybackStopped,
ToggleAudio,
MidiPortSelected(usize),
```

### UI additions (Phase 2)
```
[Open MIDI]  filename.mid  120 BPM  4/4  1:47  Shifted +1 oct  (4 skipped)  [🔊]
[▶] [⏸] [⏹]  ████████░░░░░░░  0:32 / 1:47   Port: [Built-in DLS Synth ▾]
Track 1: Piano    [M]
Track 2: Bass     [M]
```

---

## Phase 3 — Traditional staff notation

### Goal
A scrolling treble + bass clef view. Notes enter from the right, the playhead is fixed at ~30% from the left. Current notes are highlighted. No extra crates — pure iced canvas.

### New module: `src/staff.rs`

Implements `canvas::Program<Message>`. Driven by `PlayPosition` events which update the scroll offset.

### Staff layout

```
┌─ treble clef ──────────────────────────────────────────────┐
│  𝄞 ──────────────────────────────────────── bar ── bar ──  │
│    ──────────────────────────────────────────────────────  │  line spacing: 10px
│  ──────────────────────────────────────────────────────    │
│    ──────────────────────────────────────────────────────  │
│  ──────────────────────────────────────────────────────    │
└────────────────────────────────────────────────────────────┘
┌─ bass clef ────────────────────────────────────────────────┐
│  𝄢 ──────────────────────────────────────────────────────  │
│    ...                                                     │
└────────────────────────────────────────────────────────────┘
```

- **Treble clef:** notes C4 and above (MIDI ≥ 60)
- **Bass clef:** notes B3 and below (MIDI ≤ 59)
- **Line spacing:** 10px — standard proportions
- **Note head radius:** 4px (filled circle); stems are 30px vertical lines
- **Clef symbols:** rendered as canvas text using a music font character (`𝄞` U+1D11E, `𝄢` U+1D122) — may need a fallback path-drawn version if the system font doesn't include them

### Note positioning

Each MIDI note maps to a staff position (line or space) and an accidental:

```rust
// Returns (staff_slot, accidental)
// staff_slot: 0 = first ledger below bass, positive = up
// accidental: None | Sharp | Flat (depends on key signature)
fn note_to_staff_position(midi: u8, key_sig: i8) -> (i32, Option<Accidental>)
```

The key signature (from the parsed MIDI `key_sig` field) determines which notes are implicitly sharp/flat, so accidental symbols only appear when they deviate from the key.

### Scrolling

The X position of a note at tick T with playhead at `position_tick`:

```rust
let pixels_per_tick = (UNIT + GAP) * 2.0 / ticks_per_beat as f32;  // tunable
let x = PLAYHEAD_X + (T as f32 - position_tick as f32) * pixels_per_tick;
```

Notes to the left of the canvas are clipped. Notes to the right are visible as upcoming.

### Bar lines

From `time_sig` and `ticks_per_beat`:
```rust
let ticks_per_bar = ticks_per_beat as u64 * time_sig.0 as u64 / (time_sig.1 as u64 / 4);
```
Bar lines are drawn at every multiple of `ticks_per_bar`, positioned by the same scroll formula.

### Note duration

Notes have a start tick and an end tick (from matching NoteOn/NoteOff pairs — do this pre-processing in `midi.rs`). Duration is shown as a horizontal line extending from the note head to the right.

```rust
pub struct Note {
    pub start_tick: u64,
    pub end_tick:   u64,
    pub midi_note:  u8,
    pub track:      usize,
    pub channel:    u8,
    pub velocity:   u8,
}
```

Pre-process all notes into this structure at load time (not during playback).

### Highlighting

Notes at the playhead (i.e., `start_tick <= position_tick < end_tick`) are drawn in the lit color (matching the key highlight color). Past notes fade slightly; future notes are normal color.

---

## Phase 4 (Bonus) — Practice mode

### Goal
The song pauses before each note or chord and waits for the user to play the correct note(s) on the physical MIDI keyboard. Wrong notes flash red; correct notes advance the song. The staff and keyboard both show what's expected.

### No new crates
`midir` already handles MIDI input — `MidiInput` lives in the same crate as `MidiOutput`. No additional dependencies.

### MIDI input wiring

```rust
// At startup / port selection, open an input connection alongside the output
let midi_in = MidiInput::new("k2-practice-in")?;
let in_ports = midi_in.ports();

// midir input uses a callback on its own thread → pipe through a channel
let (note_tx, note_rx) = std::sync::mpsc::sync_channel(64);

let _conn = midi_in.connect(&in_ports[selected_idx], "k2-input", move |_ts, msg, _| {
    // Raw MIDI bytes: [status, note, velocity]
    if msg.len() >= 3 {
        let status = msg[0] & 0xF0;
        let note   = msg[1];
        let vel    = msg[2];
        match status {
            0x90 if vel > 0 => note_tx.send(InputEvent::NoteOn(note)).ok(),
            0x90 | 0x80     => note_tx.send(InputEvent::NoteOff(note)).ok(),
            _ => None,
        };
    }
}, ())?;
```

The `note_rx` end plugs into a second iced subscription (same pattern as the playback subscription) that fires `Message::InputNoteOn(u8)` / `Message::InputNoteOff(u8)`.

### Pre-processing: chord grouping

At load time, group the `Note` list into **play events** — sets of notes that must be played together before the song advances. Notes within a small tick window (≤ 10 ticks, tunable) are treated as one chord:

```rust
pub struct PlayEvent {
    pub tick:     u64,
    pub notes:    Vec<u8>,   // MIDI notes the user must play (offset-adjusted)
    pub duration: u64,       // ticks until next event (used to resume playback)
}
```

This pre-processing runs once in `midi.rs` after parsing. Only notes on non-muted tracks and within the keyboard's range (after offset) are included in `PlayEvent::notes` — everything else auto-advances.

### Practice state machine

```rust
pub enum PracticeState {
    Idle,
    WaitingForNotes {
        expected:  HashSet<u8>,    // MIDI notes still needed
        held:      HashSet<u8>,    // notes the user is currently holding
        event_idx: usize,
    },
    WrongNote {
        flash_until: Instant,      // show red flash for ~300ms then return to Waiting
    },
}
```

**State transitions:**

```
Idle
  → user starts practice mode → WaitingForNotes { expected = first PlayEvent.notes }

WaitingForNotes
  → InputNoteOn(n) where n ∈ expected → remove n from expected, add to held
      if expected is now empty → play audio for the chord → advance to next PlayEvent
                               → if more events: WaitingForNotes { expected = next }
                               → if done: Idle (song complete)
  → InputNoteOn(n) where n ∉ expected → WrongNote { flash_until = now + 300ms }
  → InputNoteOff(n) → remove from held (no state change)

WrongNote
  → timer expires → back to WaitingForNotes (same expected set)
  → InputNoteOn(n) where n ∈ expected → accepted (forgiving mode, see below)
```

**Forgiving vs strict mode** (toggle in UI):
- **Forgiving** — wrong notes flash but don't block; correct notes still count
- **Strict** — must release all keys and try again after a wrong note

### Visual feedback

Two new key states needed in `render.rs`:

| State | Key color | Glow color |
|-------|-----------|------------|
| Expected (waiting) | normal fill | **blue** pulse — `rgba(80, 160, 255, 0.9)` |
| Wrong note played | normal fill | **red** flash — `rgba(255, 60, 60, 0.9)` |
| Correct (held) | lit fill (existing) | existing yellow glow |

In `App`:
```rust
expected_keys: HashSet<KeyId>,   // blue glow
wrong_keys:    HashSet<KeyId>,   // red flash, cleared after ~300ms
```

`BoardCanvas` gets both sets alongside `highlighted`. The `draw_board` function renders three glow passes in order: wrong (red) → expected (blue) → highlighted (yellow).

### Resuming playback after a chord

When the user completes a `PlayEvent`, the playback thread needs to play the audio for that chord and then advance. Two options:

- **Simple**: fire `NoteOn` bytes immediately when the chord is completed, wait `duration` ticks (converted to µs), fire `NoteOff`, then pause again at the next event.
- **Full audio**: resume the playback thread from the current tick, let it play normally until the next `PlayEvent` tick, then pause again. This correctly plays any filler events (passing notes on other tracks) between practice checkpoints.

The full-audio approach is better and reuses the existing playback thread. Add a new command:

```rust
PlayCmd::AdvanceTo(tick: u64)   // play until this tick then pause and request next chord
```

### Practice mode UI

```
[▶ Practice]  ──  replaces [▶ Play] when practice mode is active
[Forgiving ▾]  ──  mode selector dropdown
Streak: 12  Mistakes: 3  ──  live stats
```

When waiting: the staff shows the current chord highlighted in blue at the playhead, future notes greyed out. The keyboard shows expected keys glowing blue.

When correct: brief yellow flash on keyboard + staff, then advance.

When wrong: red flash on keyboard key that was pressed (300ms), stat counter increments.

### Stats tracking

```rust
pub struct PracticeStats {
    pub correct:  u32,
    pub mistakes: u32,
    pub streak:   u32,       // consecutive correct chords
    pub best_streak: u32,
}
```

Displayed live in the toolbar. Could be extended to per-note accuracy tracking later.

---

## Final layout (all phases)

```
┌──────────────────────────────────────────────────────────────────┐
│ [Open MIDI]  filename.mid  120 BPM  4/4  1:47  +1 oct  4 skip  [🔊]│  toolbar
│ [▶] [⏸] [⏹] [▶ Practice]  [Forgiving▾]  Streak: 12  Mistakes: 3  │
│ ████████░░░░░░  0:32 / 1:47   Port out: [DLS▾]  Port in: [K2▾]   │  transport
│ Track 1: Piano [M]   Track 2: Bass [M]   Track 3: Drums [M]       │  tracks
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│                  keyboard canvas (existing)                      │  ~280px
│                  blue glow = waiting  red = wrong  yellow = held │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│              treble + bass clef staff canvas                     │  ~220px
│              blue note head at playhead = waiting for input      │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

---

## Suggested build order

| Step | What | Crates |
|------|------|--------|
| 1 | Add `midly`; parse a file; `println!` all events to console | `midly` |
| 2 | Pre-process NoteOn/Off pairs into `Note` structs with durations | — |
| 3 | Auto-scale octave: coverage algorithm, log result | — |
| 4 | Build `Layout` with `note_to_key` + `keyboard_notes` | — |
| 5 | Add `rfd`; wire file-open dialog; show metadata in toolbar | `rfd` |
| 6 | Track list UI with mute toggles | — |
| 7 | Timing math: `tick_to_micros` with tempo map | — |
| 8 | Playback thread + `spin_sleep`; send NoteOn/Off through channel | `spin_sleep` |
| 9 | Iced subscription bridge; keys highlight silently | — |
| 10 | Add `midir`; enumerate output ports; send bytes for audio; speaker toggle | `midir` |
| 11 | Play/pause/stop/seek; all-notes-off on stop/pause; scrubber | — |
| 12 | Staff canvas: staves, clef symbols, bar lines, note heads | — |
| 13 | Staff scrolling, accidentals, key signature, note duration lines | — |
| 14 | Highlight active notes in staff; fade past notes | — |
| 15 | Open MIDI input port; pipe events into iced via second subscription | `midir` |
| 16 | Pre-process `PlayEvent` chord groups | — |
| 17 | Practice state machine + `expected_keys` / `wrong_keys` rendering | — |
| 18 | `AdvanceTo` playback command; resume-between-chords audio | — |
| 19 | Forgiving/strict toggle; stats display; blue staff note heads | — |
