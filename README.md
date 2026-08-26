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

Then open <http://localhost:8080>. Browser audio starts after the first Play or
keyboard interaction because Web Audio requires a user gesture.

## Brand assets

The full-size logo is [`assets/k2-logo-v2.png`](assets/k2-logo-v2.png). Small
browser and desktop icons use the simplified
[`assets/k2-micro-mark.png`](assets/k2-micro-mark.png), which keeps `K²`
legible down to 16 pixels. Platform-ready `.ico` and `.icns` files are in
[`assets/desktop`](assets/desktop).

## Web MIDI

Desktop Chrome also supports connected MIDI hardware. Click **Connect MIDI**,
grant the browser permission, then use the independent **MIDI IN** and
**MIDI OUT** buttons to cycle through available ports or turn either side off.
Web MIDI requires HTTPS in production; localhost is accepted for development.

## GitHub Pages

Pushing `main` runs [the Pages workflow](.github/workflows/pages.yml), builds the
WebAssembly release with the repository subpath as its public URL, and deploys
the `dist` artifact through GitHub Pages. In the repository's GitHub settings,
set **Pages → Source** to **GitHub Actions** once before the first deployment.
