# GitHub Pages Port Plan

## Scope

The primary use case: open a `.mid` file in the browser and watch it play on the
on-screen keyboard. Live MIDI hardware input (practice mode, external controller)
is **not** implemented natively yet either, so it's out of scope here too — this
plan only covers what the app does today.

GitHub Pages serves static files only. A Rust app compiled to `wasm32-unknown-unknown`
plus the resulting `.wasm` + JS glue + `index.html` satisfies that; no server-side
piece is needed.

---

## What ports with little or no change

Pure-Rust, no OS dependency:

- `midly` — MIDI file parsing (`src/midi.rs`)
- Layout/keyboard logic (`src/layout.rs`, `src/key.rs`)
- Playback timing math (tick → micros, tempo map)
- `render.rs` / `staff.rs` — canvas drawing, once iced itself renders on web (see below)

These compile to wasm as-is.

---

## What needs replacing

### 1. `iced` GUI (biggest unknown)

Iced's wasm target (via its `wgpu` backend → WebGL/WebGPU) is experimental and not
as polished as the native targets. This is the piece most likely to cause layout,
font, or canvas-rendering surprises. Worth a small throwaway spike — get a blank
iced window rendering in a browser tab — before committing to the rest of the port.

### 2. `rfd` (file picker)

Already has a web backend (falls back to an HTML `<input type="file">` under the
hood), so this should work with little to no change — just confirm the async
`AsyncFileDialog` path used in `main.rs:411` behaves the same on wasm.

### 3. `cpal` (built-in soft synth, `src/synth.rs`)

No meaningful wasm backend. The soft synth's audio callback (`start_soft_synth`,
`synth.rs:182`) needs a parallel implementation using Web Audio — either:
- an `AudioWorklet` running the same oscillator/envelope logic compiled to wasm, or
- simpler: drive `OscillatorNode`/`GainNode` per active note directly via `web-sys`
  (less faithful to the current synth's waveform, but far less work).

Gate behind `#[cfg(target_arch = "wasm32")]`, with the existing `cpal` path kept
for native builds.

### 4. `midir` (external MIDI output port, e.g. routing to a DAW/system synth)

This is optional routing on top of the built-in synth, used for output only —
there's no live *input* handling in the app today. On the web this has no direct
equivalent (Web MIDI's output side exists but requires a connected device/synth
the browser can see, which is a niche case for this app). Recommend dropping this
feature entirely for the web build rather than porting it — cut the "Port out"
selector and always use the soft synth on wasm.

---

## Build tooling

- Add `wasm32-unknown-unknown` as a build target.
- Use `trunk` (or `wasm-pack` + a small `index.html`) to build the wasm bundle and
  static assets into a `dist/` folder.
- Add a GitHub Actions workflow that builds on push to `main` and deploys `dist/`
  to the `gh-pages` branch (or via the native Pages Actions deploy step).

---

## Feature parity — native vs. web v1

| Feature | Native | Web v1 |
|---|---|---|
| Open & parse `.mid` file | ✅ | ✅ (`rfd` web fallback) |
| Keyboard visual playback | ✅ | ✅ (pending iced-wasm spike) |
| Staff notation view | ✅ | ✅ |
| Built-in soft-synth audio | ✅ (`cpal`) | ✅ (Web Audio, reimplemented) |
| External MIDI output port | ✅ (`midir`) | ❌ (dropped) |
| Live MIDI input / practice mode | ❌ (not built yet) | ❌ (out of scope) |

---

## Suggested build order

| Step | What |
|---|---|
| 1 | Spike: minimal iced app rendering in a browser via wasm — de-risk the biggest unknown first |
| 2 | Add `wasm32-unknown-unknown` target + `trunk` config; get the real app compiling (native-only code paths stubbed with `cfg`) |
| 3 | Verify `rfd` file-open works on web; wire up file parsing → layout, same as native |
| 4 | Confirm keyboard + staff canvases render correctly in-browser |
| 5 | Implement Web Audio playback path behind `#[cfg(target_arch = "wasm32")]`, matching `synth.rs` behavior as closely as practical |
| 6 | Strip/hide the external MIDI output port selector on web builds |
| 7 | GitHub Actions workflow: build on push, deploy `dist/` to GitHub Pages |
| 8 | Cross-browser check (Chrome/Edge/Firefox/Safari) — canvas + Web Audio support varies |

---

## Risks

- **iced-wasm maturity** — could force rendering compromises or extra workarounds; this is the step to validate first.
- **Web Audio fidelity** — reimplementing `synth.rs`'s oscillator/envelope in an `AudioWorklet` is real work if exact sound parity matters; the simpler `OscillatorNode`-per-note approach will sound different.
- **Autoplay/permission restrictions** — browsers block audio until a user gesture; the existing "play" button click should satisfy this naturally, but worth confirming.
