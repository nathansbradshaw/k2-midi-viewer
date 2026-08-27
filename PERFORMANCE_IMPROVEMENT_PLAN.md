# Performance Improvement Plan

## Objective

Keep playback responsive and glitch-free at the application's maximum supported
polyphony on both native and browser builds, without changing the sound, MIDI
timing, keyboard mapping, or staff presentation.

This plan treats performance as two independent budgets:

1. **Audio deadline** — the synth must finish each output buffer before the audio
   device needs it.
2. **Visual deadline** — playback updates and canvas rendering must leave enough
   main-thread/GPU time for smooth interaction.

A dense chord can stress both at once, so each phase has separate audio and UI
measurements.

---

## Current bottlenecks

### 1. Per-sample synth work scales poorly with voice count

`SoftSynth::render` currently loops over every active voice and every output
sample. Each voice/sample pair calculates:

- an LFO phase;
- a vibrato sine, even when vibrato depth is zero;
- a tremolo cosine, even when tremolo depth is zero;
- glide interpolation, even when glide is disabled;
- one or more waveform samples.

The default Organ waveform adds three more sine calls per voice/sample. At 16
melodic voices and 44.1 kHz, this produces millions of transcendental operations
per second. Layering waveforms increases that work further. The Noise waveform
also uses a sine-based hash rather than a cheap stateful PRNG.

Relevant code: `src/synth.rs`, especially `Waveform::sample` and
`SoftSynth::render`.

### 2. The audio callback can intentionally output silence during lock contention

The CPAL callback calls `try_lock` on the shared synth. If playback or the UI is
changing notes/knobs at that moment, the callback fills the entire buffer with
silence. Dense NoteOn/NoteOff bursts create more lock acquisitions and therefore
more chances to lose a buffer.

Relevant code: `src/synth.rs::start_soft_synth`, plus direct synth locking in
`src/playback.rs`, `src/playback_web.rs`, and `src/main.rs`.

### 3. The complete keyboard overlay is rebuilt on every redraw

`BoardCanvas::draw` clears `overlay_cache` unconditionally. It then revisits
every key and redraws resting or active key sprites, labels, tape markings,
knobs, and display text. Only a small set of highlighted keys normally changes.

Relevant code: `src/render.rs::BoardCanvas::draw` and `draw_board_overlay`.

### 4. The complete staff is rebuilt and every MIDI note is scanned

`StaffCanvas::draw` clears its cache every time. `draw_staff` then scans the
entire `MidiFile::notes` collection before rejecting notes outside the visible
window. This makes frame cost depend on total song size instead of the number of
notes on screen.

Relevant code: `src/staff.rs::StaffCanvas::draw` and `draw_staff`.

### 5. Playback polling continues when no animation is needed

Once a file creates a playback handle, the application subscribes to a 16 ms
timer even while stopped. On native playback the worker publishes position only
every 50 ms, so several UI polls may also redraw the same position.

Relevant code: `src/main.rs::subscription` and `Message::PollPlayback`.

### 6. Dense MIDI timestamps are dispatched one event at a time

Events sharing a tick repeatedly calculate timing, acquire the synth lock, and
acquire the UI event-queue lock. The browser performs this work on the UI thread.

Relevant code: `src/playback.rs` and `src/playback_web.rs`.

---

## Performance targets

Establish a named reference machine and browser before recording results. Keep
the raw measurements in benchmark output or a short table in the pull request.

### Audio

- No deliberately silent buffers caused by synth command contention.
- No underruns during 60 seconds of maximum melodic polyphony.
- No underruns during 60 seconds of maximum melodic polyphony plus the maximum
  supported drum voices.
- Synth render p99 at or below 50% of the output-buffer duration on the reference
  machine, leaving headroom for the host and browser.
- NoteOn-to-audio behavior and envelope shape remain audibly and numerically
  consistent with the current synth.

### UI

- Playback remains interactive during dense passages.
- p95 frame time stays below 16.7 ms for the native release build on the
  reference machine.
- Browser playback maintains at least 30 visual updates per second under the
  maximum-polyphony test, with no long tasks over 50 ms caused by K2.
- Staff-render cost depends on visible notes, not total notes in the file.
- A loaded but stopped song does not run a continuous animation subscription
  unless live MIDI input requires polling.

### Build discipline

- Performance comparisons use `cargo run --release` and
  `trunk serve --release`.
- Debug builds are used for correctness and development, not performance
  acceptance.

---

## Phase 0 — Establish reproducible baselines

### Work

1. Add deterministic test MIDI fixtures or fixture generators for:
   - 1, 4, 8, and 16 sustained melodic voices;
   - 16 melodic voices plus 12 drum voices;
   - repeated 16-note chord onsets and releases;
   - a long song with many total notes but only a small visible window;
   - rapid tempo changes and dense same-tick events.
2. Add a synth benchmark that renders a fixed number of stereo frames without
   starting an audio device. Record time per buffer for each voice count and
   waveform configuration.
3. Add lightweight, feature-gated counters for:
   - audio callback duration;
   - missed command-queue reads or audio deadline overruns;
   - active melodic and drum voice counts;
   - playback queue high-water mark;
   - total notes scanned and notes drawn by the staff;
   - keyboard overlay keys redrawn per frame.
4. Capture native release measurements with a sampling profiler.
5. Capture browser release measurements with the browser Performance panel.
6. Repeat the browser test with audio disabled. This separates synth cost from
   canvas and scheduling cost.

### Acceptance

- Every later phase can be compared against the same fixtures and metrics.
- Instrumentation is disabled or effectively free in normal release builds.
- Baseline results identify whether audio or rendering is the first failing
  budget on the reference machine.

---

## Phase 1 — Low-risk scheduling and redraw reductions

### Work

1. Make the playback timer conditional:
   - active while playing;
   - active while browser live-MIDI input needs polling;
   - inactive while stopped or paused with no live input work.
2. Do not mutate `position_tick` or highlight collections when a poll produces
   no change.
3. Coalesce multiple queued `Position` events and apply only the newest one.
4. Use a visual update rate appropriate to the displayed playhead:
   - start with 30 Hz;
   - retain exact audio/MIDI scheduling independently of that rate;
   - increase only if profiling shows sufficient headroom and a visible benefit.
5. Update the local-development instructions to call out
   `trunk serve --release` for performance testing.

### Acceptance

- Loading a file and leaving playback stopped does not cause continuous canvas
  rendering.
- Reducing the visual tick rate does not alter MIDI event timing.
- Play, pause, stop, seek, looping, and live MIDI input retain their current
  behavior.

---

## Phase 2 — Remove avoidable synth math

Implement this phase in small commits so benchmarks show the value of each
change.

### 2A. Disabled-effect fast paths

- When vibrato depth is zero, skip vibrato sine calculation and use the base
  frequency directly.
- When tremolo depth is zero, skip tremolo cosine calculation and use gain 1.0.
- When glide is disabled, keep `hz` at `target_hz` and avoid per-sample
  interpolation.
- When bitcrush is disabled, keep its existing whole-stage bypass.
- Precompute buffer-invariant values once before entering the sample loop.

### 2B. Calculate shared modulation once

- Restructure rendering so the shared LFO value is calculated once per output
  frame rather than once per voice.
- Preserve per-voice phase, filter, envelope, glide, and velocity state.
- Avoid allocating temporary buffers in the real-time callback. If a scratch
  buffer is needed, allocate it when the stream is created and reuse it.

### 2C. Replace expensive oscillator operations

- Introduce a fixed sine wavetable with linear interpolation for Sine and the
  Organ harmonics.
- Keep Triangle, Square, Saw, and Pulse as arithmetic waveforms.
- Give each voice a small deterministic PRNG state and replace Noise's sine
  hash with a cheap generator such as xorshift.
- Replace `fract` in the phase accumulator with a branch/subtraction where
  safe for the supported frequency range.

### 2D. Preserve sound and voice behavior

- Add reference tests for waveform samples, envelope stages, release cleanup,
  voice stealing, layered waveforms, drums, pan, and effects.
- Compare a short deterministic render against the old implementation before
  removing it. Allow a documented numerical tolerance for the wavetable.
- Keep `MAX_VOICES` and `MAX_DRUM_VOICES` unchanged during optimization so a
  polyphony change does not hide or distort benchmark results.

### Acceptance

- The 16-voice synth benchmark improves materially over baseline.
- Default settings execute no vibrato, tremolo, or glide transcendental math.
- The optimized synth meets the audio p99 budget from the Performance Targets.
- Existing synth tests and new audio reference tests pass on native and WASM.

---

## Phase 3 — Give the audio callback sole ownership of the synth

### Design

Replace `Arc<Mutex<SoftSynth>>` with a control handle that sends bounded,
non-blocking commands to the audio callback:

```rust
enum SynthCommand {
    NoteOn { note: u8, velocity: u8, channel: u8 },
    NoteOff { note: u8, channel: u8 },
    AllNotesOff,
    SetWaveforms(Vec<Waveform>),
    SetKnob { index: u8, value: f32 },
}
```

The CPAL callback owns `SoftSynth` directly. At the start of each output buffer,
it drains the commands currently available and then renders. The callback never
waits for the UI/playback thread and never replaces a whole buffer with silence
because another thread touched synth state.

Choose the bounded queue after a small native/WASM compatibility spike. Required
properties:

- non-blocking consumer operation in the audio callback;
- no allocation in the callback;
- explicit full-queue behavior;
- support for every producer used by native and browser builds.

For queue saturation:

- NoteOn, NoteOff, and AllNotesOff must not be silently discarded;
- knob updates may be coalesced to the newest value per knob;
- waveform updates may be coalesced to the newest complete selection;
- expose a debug counter whenever saturation occurs.

### Work

1. Introduce `SynthController` and `SynthCommand`.
2. Make `start_soft_synth` return the controller and stream instead of shared
   mutable synth state.
3. Route live keys, playback events, knobs, waveform changes, and all-notes-off
   through the controller.
4. Remove duplicate direct-plus-playback synth updates from `main.rs`.
5. Keep external MIDI output on its existing path.
6. Verify command ordering around stop, seek, mute, loop wrap, and stream startup.

### Acceptance

- The audio callback contains no mutex acquisition and no blocking operation.
- UI and playback code no longer lock `SoftSynth`.
- Dense event bursts produce no contention-induced silent buffers.
- Queue saturation is covered by tests and visible through diagnostics.

---

## Phase 4 — Split static and dynamic keyboard rendering

### Proposed layers

1. **Photographic base, cached**
   - board shell, display, LEDs, wells, and shadows.
2. **Resting controls, cached**
   - resting key sprites and legends;
   - tape markings;
   - controls that rarely change.
3. **State-dependent controls, invalidated on state change**
   - knob positions;
   - selected waveform keys;
   - projected computer-key labels;
   - drum-symbol mode;
   - compact crop and size-dependent geometry.
4. **Playback highlights, dynamic**
   - only currently highlighted, pressed, or live-MIDI keys;
   - play-order badges when selection mode is active.
5. **Knob drag popup, dynamic and isolated**
   - retain the existing isolated layer behavior.

### Work

1. Split `draw_board_overlay` into static/control/highlight functions.
2. Draw resting sprites once; active-key geometry should only overlay keys whose
   visual state differs from rest.
3. Add explicit cache invalidation signatures for size, crop mode, labels, drum
   mode, knob values, and selected controls.
4. Precompute reverse drum-pad lookup instead of searching
   `drum_note_to_key` for every key.
5. Avoid cloning play-order vectors while collecting badges.
6. Track the number of keys drawn into the dynamic layer for regression tests.

### Acceptance

- During ordinary playback, dynamic keyboard work scales with the number of
  changed/highlighted keys rather than all board keys.
- Resting labels, tape, knobs, and key sprites are not rebuilt every visual tick.
- Compact mode, resizing, drum symbols, computer-key labels, waveform selection,
  track colors, and knob dragging remain visually correct.
- Existing high-DPI visual tests continue to pass.

---

## Phase 5 — Index visible staff notes and cache static geometry

### Work

1. Split the staff into:
   - a cached background layer for the CRT background, grid, scanlines, staff
     lines, clef, and playable-range band;
   - a selection layer invalidated only when the selected range changes;
   - a dynamic note/playhead layer.
2. Use the fact that `MidiFile::notes` is sorted by `start_tick`:
   - convert the visible x bounds into a start-tick range;
   - use `partition_point` to find the first and last possible visible note;
   - iterate only that slice;
   - preserve the existing end-tick overlap and screen-bound checks.
3. Replace `plotted.sort_by_key(|p| p.is_active)` with two linear foreground
   passes: inactive notes first, active notes second.
4. Reuse or right-size temporary storage where practical, without putting
   mutable shared state into the renderer.
5. Record scanned-note and drawn-note counts under performance diagnostics.

### Acceptance

- A long MIDI file and a short MIDI file with the same visible passage have
  comparable staff-render time.
- Static staff geometry is not regenerated as the playhead moves.
- Duration bars, active-note ordering, ledger lines, sharps, selection,
  out-of-range rings, muting, and octave shifts remain correct.
- Staff interaction and selection-to-loop behavior remain unchanged.

---

## Phase 6 — Batch playback work

### Work

1. Precompute absolute event times when loading the MIDI file, or store tempo
   segments with cumulative microseconds and use binary search. Avoid rescanning
   the tempo map for every event and every clock conversion.
2. Dispatch all events sharing a tick/time as one batch.
3. Send the batch's synth commands together while preserving stable event order.
4. Acquire the UI playback-event queue once per batch instead of once per note.
5. Store the latest position separately or coalesce it before publishing so
   note bursts cannot build a backlog of obsolete playhead positions.
6. On the browser, cap work per visual callback only if profiling demonstrates
   a long-task problem. Never defer an audio/MIDI event merely to improve the
   animation; audio timing remains the priority.

### Acceptance

- Same-tick chords require constant queue-lock overhead per chord, not per note.
- Dense files do not grow an unbounded UI event backlog.
- Note ordering, track muting, channel routing, seeks, and loops remain correct.
- Timing tests cover tempo boundaries and same-tick NoteOff/NoteOn ordering.

---

## Validation matrix

Run the following after every phase that touches the relevant subsystem:

| Scenario | Native debug | Native release | Browser debug | Browser release |
|---|---:|---:|---:|---:|
| Unit and integration tests | Required | Required | Compile/test | Compile/test |
| 1/4/8/16 melodic voices | Smoke | Measure | Smoke | Measure |
| 16 melodic + 12 drums | Smoke | Measure | Smoke | Measure |
| Repeated dense chords | Smoke | Measure | Smoke | Measure |
| Long-song staff rendering | Smoke | Measure | Smoke | Measure |
| Play/pause/stop/seek/loop | Required | Required | Required | Required |
| Live computer keyboard | Required | Required | Required | Required |
| Web MIDI input/output | N/A | N/A | Required | Required |

Also run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
cargo test --release
cargo build --release
trunk build --release
```

Use the existing high-DPI browser test after the canvas phases and compare
screenshots at normal and compact board sizes.

---

## Rollout order

Recommended pull-request sequence:

1. Baseline fixtures, benchmark harness, and diagnostics.
2. Polling/coalescing quick wins.
3. Synth disabled-effect fast paths and shared LFO work.
4. Oscillator wavetable and noise PRNG.
5. Audio-owned synth command queue.
6. Keyboard canvas layer split.
7. Staff indexing and layer split.
8. Playback time precomputation and event batching.
9. Remove temporary diagnostics or retain them behind a performance feature.

Each pull request should include before/after numbers from the same reference
machine. Do not combine polyphony increases, visual redesign, or sound changes
with these optimizations.

---

## Risks and mitigations

### Sound changes from oscillator optimization

Wavetables and PRNG noise will not be bit-identical to the existing functions.
Keep deterministic reference renders, use interpolation, document the tolerance,
and perform an A/B listening check before removing the old path.

### Lost or reordered synth commands

A bounded queue needs explicit saturation and ordering rules. Test chord bursts,
rapid retriggering, stop/seek races, and AllNotesOff. Never treat a dropped note
command as an acceptable performance tradeoff.

### Stale canvas caches

Splitting layers introduces invalidation risk. Define explicit state signatures
for each cache rather than relying on incidental redraws. Add visual tests for
every setting that changes a cached layer.

### Browser and native audio paths differ

Keep the synth command API platform-neutral, but benchmark both targets. Avoid
assuming a native queue or timing primitive behaves identically in WASM.

### Lower visual update rate looks less fluid

Keep audio timing independent. Measure 30 Hz first, then choose the highest
visual rate that consistently meets the frame budget on the reference browser.

---

## Definition of done

- Performance targets are met on the documented reference native and browser
  environments.
- Maximum supported polyphony plays for 60 seconds without underruns or
  contention-induced silent buffers.
- Dense playback remains responsive while resizing, seeking, muting tracks, and
  manipulating controls.
- Render cost scales with visible or changed content rather than total song or
  keyboard content.
- All correctness, timing, browser, and visual regression tests pass.
- Release-mode development and profiling instructions are documented.
- Before/after benchmark results are retained with the implementation history.
