# K2 MIDI Viewer

K2 renders MIDI files across a full computer-keyboard layout with transport,
track muting, pitch controls, staff notation, and a built-in soft synth.

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

## GitHub Pages

Pushing `main` runs [the Pages workflow](.github/workflows/pages.yml), builds the
WebAssembly release with the repository subpath as its public URL, and deploys
the `dist` artifact through GitHub Pages. In the repository's GitHub settings,
set **Pages → Source** to **GitHub Actions** once before the first deployment.
