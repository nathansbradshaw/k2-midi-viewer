<p align="center">
  <img src="assets/k2-logo-v2.png" alt="K2 MIDI Viewer logo" width="160" />
</p>

# K2 MIDI Viewer

K2—short for “keyboard squared”—renders MIDI files across a full
computer-keyboard layout with transport, track muting, pitch controls, staff
notation, and a built-in soft synth.

## Run locally

Native app:

```sh
cargo run --release
```

Browser app:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
trunk serve
```

`trunk serve` uses the LTO-optimized release profile by default so the browser
does not have to compile the much larger debug WASM. During rapid code
iteration, opt back into faster rebuilds explicitly:

```sh
trunk serve --release=false
```

Then open <http://localhost:8080>. Browser audio starts after the first Play or
keyboard interaction because Web Audio requires a user gesture.

## Brand assets

The full-size logo is [`assets/k2-logo-v2.png`](assets/k2-logo-v2.png). Small
browser and desktop icons use the simplified
[`assets/k2-micro-mark.png`](assets/k2-micro-mark.png), which keeps `K²`
legible down to 16 pixels. Platform-ready `.ico` and `.icns` files are in
[`assets/desktop`](assets/desktop).

## Web MIDI

Desktop Chrome and Firefox support connected MIDI hardware. Click **Connect
MIDI**, grant the browser permission, then use the independent **MIDI IN** and
**MIDI OUT** buttons to cycle through available ports or turn either side off.
Web MIDI requires HTTPS in production; localhost is accepted for development.

Set **IN OCT** to the same value as **Settings → Octave** on the physical
Keyboard Keyboard. The viewer defaults to `2`, matching the hardware firmware,
and uses this only to locate the originating virtual key; MIDI audio keeps the
pitch sent by the hardware. The selection is saved in the page URL, so another
octave setting can be kept for future sessions or shared setups.

For a live hardware demonstration, select both a **MIDI IN** and **MIDI OUT**,
then enable **THRU**. Every MIDI message received from the input is forwarded
unchanged to the output while the virtual board continues to show incoming
notes. Turning THRU off or changing the input sends All Notes Off to prevent
held notes from becoming stuck. THRU is off by default and its setting is also
saved in the page URL.

Firefox uses a stricter permission flow: connect the MIDI device before starting
Firefox, then approve the site-specific MIDI permission add-on when prompted.
Firefox for Android does not support Web MIDI.

## Key-image annotations

Annotated copies let the renderer extract precise key and label geometry. All
five alpha-row copies now supply text guides and calibrated boundaries; the
fourth row's final divider is temporarily inferred from its photographic valley
until that missing red mark is added. The numpad copy now supplies measured
boundaries and yellow safe-text boxes for all 20 keys. The navigation and arrow
source images remain future annotation work. Keep the originals unchanged and
preserve the exact image dimensions. On each annotated copy:

- Draw every key boundary in solid red, including unambiguous borders where keys
  touch.
- Prefer a yellow text-boundary box on every key. Its center places the label,
  and rendered text is fitted so it never extends outside the box.
- A small purple dot or cross is also supported when only a text center is
  needed.
- Use opaque, non-antialiased marks when possible so the colors are easy to detect
  programmatically.
- Name the copy after the original with an `_annotated` suffix or ` copy`.

The red guides drive sprite extraction and hit geometry; the purple or yellow
guides drive label placement. When **Drum symbols** is enabled, the numpad swaps
to prebuilt blank photographic caps from `numpad-clean.png` before drawing the
symbols; normal mode retains the photographed number legends.

## GitHub Pages

Pushing `main` runs [the Pages workflow](.github/workflows/pages.yml), builds the
LTO-optimized WebAssembly release with the repository subpath as its public URL,
and deploys the `dist` artifact through GitHub Pages. In the repository's GitHub
settings, set **Pages → Source** to **GitHub Actions** once before the first
deployment.
