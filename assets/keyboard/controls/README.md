# K2 control-surface assets

Reusable, label-free physical controls for the K2 interface. All files have a
transparent canvas; color, text, pointer position, and light emission belong to
the consuming UI.

| Asset | Format / intrinsic size | Intended use |
| --- | --- | --- |
| `rotary-knob-face.png` | RGBA PNG, 1024 × 1024 | CH/OCT and other rotary controls. Add the pointer as a separate vector/canvas layer. |
| `rotary-knob-pointer.svg` | SVG, 64 × 64 viewBox | Warm ivory pointer with its pivot at the exact canvas center. |
| `rotary-tick-scale.svg` | SVG, 96 × 96 viewBox | Thirteen printed panel ticks spanning 270° with an open bottom. |
| `rocker-switch-off.svg` | SVG, 240 × 320 viewBox | Dark plastic rocker in its raised-top OFF state. |
| `rocker-switch-on.svg` | SVG, 240 × 320 viewBox | Matching raised-bottom ON state. |
| `fader-cap.png` | RGBA PNG, 1024 × 572 | Warm gray/beige slider thumb with a center groove. |
| `fader-track.svg` | SVG, 640 × 112 viewBox | Responsive recessed horizontal slot with thirteen positions and three major ticks. |
| `led-jewel.png` | RGBA PNG, 1024 × 1024 | Shared neutral/unlit lens and bezel for every track LED. |
| `lcd-glass-overlay.svg` | SVG, 960 × 300 viewBox | Low-opacity glass reflection and vignette above amber display content. |
| `display-bezel.svg` | SVG, 960 × 280 viewBox | Stretchable molded-plastic frame with a transparent display opening. |
| `transport-button-face.svg` | SVG, 180 × 112 viewBox | Empty raised transport keycap for separately layered glyphs. |
| `label-plate.svg` | SVG, 600 × 104 viewBox | Optional recessed plate behind track or section labels. |
| `transport-keycap.svg` | SVG, 180 × 112 viewBox | Next-generation blank molded transport key for Play, Stop, or Pause. |
| `transport-keycap-photo.png` | RGBA PNG, 1024 × 599 | Preferred photographed transport face with yellowing, edge wear, grime, and small surface defects. |
| `display-bezel-wide.svg` | SVG, 1200 × 300 viewBox | Shared wide frame for both the top LCD and lower visualizer. |
| `lcd-glass-overlay-wide.svg` | SVG, 1200 × 240 viewBox | Restrained glass reflection sized for wide display apertures. |
| `track-label-plate.svg` | SVG, 600 × 104 viewBox | Deeper inset plate dedicated to separately rendered track names. |
| `track-label-plate-photo.png` | RGBA PNG, 768 × 132 | Preferred photographed track-name recess with molded grain, rubbed edges, dust, and micro-scratches. |
| `led-jewel-unlit.png` | RGBA PNG, 1024 × 1024 | Low-gloss smoke-gray lens and black bezel in a true unlit state. |
| `panel-keycap-wide-photo.png` | RGBA PNG, 1024 × 256 | Blank wide auxiliary key with warm charcoal plastic, molded grain, dusty seams, and edge wear. |
| `panel-keycap-square-photo.png` | RGBA PNG, 512 × 448 | Matching compact key for pitch decrement, step, and increment controls. |
| `action-keycap-neutral-photo.png` | RGBA PNG, 960 × 320 | Full-size blank charcoal action key for Connect MIDI and similar commands. |
| `action-keycap-salmon-photo.png` | RGBA PNG, 960 × 320 | Geometry-matched aged salmon primary-action key for Open MIDI. |
| `button-keycap-neutral-v2.png` | RGBA PNG, 2144 × 733 | Cohesive photographed master now shared by auxiliary and full-size neutral actions: one bevel, grain, wear, grime, and lighting model. |
| `button-keycap-salmon-v2.png` | RGBA PNG, 2144 × 733 | Pixel-matched salmon colorway of the shared v2 master for primary actions. |
| `button-keycap-ivory-v2.png` | RGBA PNG, 2144 × 733 | Pixel-matched aged ivory colorway of the shared v2 master for transport actions. |
| `button-keycap-neutral-v3.png` | RGBA PNG, 1774 × 887 | Strict front-on rectangular replacement: parallel edges, nearly square corners, shallow bevel, photographic grime and defects. |
| `button-keycap-salmon-v3.png` | RGBA PNG, 1774 × 887 | Geometry-matched salmon colorway of the rectangular v3 master. |
| `button-keycap-ivory-v3.png` | RGBA PNG, 1774 × 887 | Geometry-matched aged ivory colorway of the rectangular v3 master. |
| `selector-housing-photo.png` | RGBA PNG, 1024 × 320 | Recessed charcoal selector chassis with a separate blank arrow well. |
| `module-frame.svg` | SVG, 1200 × 240 viewBox | Stretchable low-contrast frame for header, transport, mixer, or visualizer modules. |
| `horizontal-switch-off.png` | RGBA PNG, 2135 × 736 | Label-free horizontal Loop/Sound switch in its left/OFF position. |
| `horizontal-switch-on.png` | RGBA PNG, 2067 × 761 | Matching switch with the metal handle in its right/ON position. |
| `roller-selector-inline-photo.png` | RGB PNG, 1908 × 824 | Label-free inline CH/value well, substantial roller, and arrow well for the live-play MIDI channel. |
| `panel-screw-photo.png` | RGBA PNG, 1271 × 1237 | Isolated oxidized slotted screw used to anchor the shared console plate. |
| `icon-map-rows.svg` | SVG, 16 × 16 viewBox | Row-mapping utility symbol. |
| `icon-all-notes.svg` | SVG, 16 × 16 viewBox | All-notes visibility symbol. |
| `icon-computer-keys.svg` | SVG, 16 × 16 viewBox | Computer-keyboard input symbol. |
| `icon-drum.svg` | SVG, 16 × 16 viewBox | Drum-symbol overlay control. |
| `icon-board.svg` | SVG, 16 × 16 viewBox | Compact/full keyboard view symbol. |
| `icon-reset-pitch.svg` | SVG, 16 × 16 viewBox | Pitch-reset action symbol. |

## Runtime derivatives

The full-resolution PNGs above are editable masters. The app embeds the
Lanczos-filtered `*-runtime.png` derivatives instead, so WASM startup does not
decode and resize multi-megapixel images before its first frame.

| Runtime asset | Intrinsic size |
| --- | --- |
| `rotary-knob-face-runtime.png` | 128 × 128 |
| `fader-cap-runtime.png` | 96 × 54 |
| `led-jewel-unlit-runtime.png` | 64 × 64 |
| `transport-keycap-photo-runtime.png` | 256 × 150 |
| `track-label-plate-photo-runtime.png` | 384 × 66 |
| `panel-keycap-wide-photo-runtime.png` | 512 × 128 |
| `panel-keycap-square-photo-runtime.png` | 128 × 112 |
| `action-keycap-neutral-photo-runtime.png` | 384 × 128 |
| `action-keycap-salmon-photo-runtime.png` | 384 × 128 |
| `button-keycap-neutral-v2-runtime.png` | 384 × 128 |
| `button-keycap-salmon-v2-runtime.png` | 384 × 128 |
| `button-keycap-ivory-v2-runtime.png` | 384 × 128 |
| `button-keycap-neutral-v3-runtime.png` | 384 × 128 |
| `button-keycap-salmon-v3-runtime.png` | 384 × 128 |
| `button-keycap-ivory-v3-runtime.png` | 384 × 128 |
| `selector-housing-photo-runtime.png` | 416 × 130 |
| `horizontal-switch-off-runtime.png` | 192 × 64 |
| `horizontal-switch-on-runtime.png` | 192 × 64 |
| `roller-selector-inline-photo-runtime.png` | 426 × 126 |
| `panel-screw-photo-runtime.png` | 64 × 64 |

The panel-wide equivalent is `../panel-wear-overlay-runtime.png` at 512 × 512.
Keep the masters out of `include_bytes!`; otherwise their compressed bytes are
copied into the WASM bundle even when a smaller derivative is also present.


## State and color contract

- Never rotate `rotary-knob-face.png`; keep the physical lighting fixed and
  rotate `rotary-knob-pointer.svg` from about -135° to +135°. Its neutral source
  orientation is twelve o'clock and its pivot is `(32, 32)`.
- Keep `rotary-tick-scale.svg` stationary beneath the knob. Its thirteen ticks
  use the same 270° travel and remain legible around a 24–40 px control.
- Swap the complete rocker SVG when state changes. Their viewBoxes and outer
  geometry match, so layout does not shift.
- Keep `led-jewel.png` neutral. Put LED color under/over the lens with a blend
  layer and add the glow outside the image. This gives every track one shared
  piece of physical hardware.
- Stretch `lcd-glass-overlay.svg` exactly over the active display aperture. It
  uses `preserveAspectRatio="none"` intentionally and contains no display text.
- `fader-track.svg` and `display-bezel.svg` also permit non-uniform horizontal
  scaling. Their strokes use non-scaling treatment where line weight matters.
- Layer Play, Pause, Stop, and other glyphs separately above
  `transport-keycap-photo.png`; the keycap intentionally contains no symbol.
- Put track text above `track-label-plate-photo.png`. Keep both photographic
  parts stationary so their workshop lighting and defects do not move with UI
  state.
- Render auxiliary and header-action legends above the shared blank
  `button-keycap-*-v3` family. Compact pitch keys reuse the same neutral master
  at a narrower mount size, so bevel depth, plastic grain, grime, and lighting
  stay consistent across the console. Keep the deeper aged-ivory
  `transport-keycap-photo.png` for Play, Pause, and Stop as an intentional
  transport-family exception and retain its recessed two-pixel chassis socket.
  Mount the shallow header and utility caps flush without that black socket.
  Indicate active state with a separately rendered jewel lamp; do not bake
  labels, state colors, or glowing borders into the keycap photographs.
- Keep action-key legends live above the shared v3 family, and use identical
  layout dimensions for its neutral and salmon variants so changing action
  importance never shifts the header. Render the selector text and arrow above
  the same neutral v3 cap so Connect MIDI, channel selection, and Open MIDI use
  one 142 × 42 construction; `selector-housing-photo.png` remains an optional
  legacy chassis and contains no baked symbols.
- `display-bezel-wide.svg`, `lcd-glass-overlay-wide.svg`, and
  `module-frame.svg` use `preserveAspectRatio="none"` so one product-family
  asset can fit several responsive housings. Keep the bezel and frame corner
  radii visually near their source proportions when resizing aggressively.
- Use `led-jewel-unlit.png` as the preferred neutral base. Put colored light
  inside the lens region with CSS/canvas and keep the hardware image above it;
  do not hue-rotate the bezel.
- Keep `horizontal-switch-off-runtime.png` and
  `horizontal-switch-on-runtime.png` in identical UI bounds. The permanent
  OFF/ON legends are rendered live beside them; neither bitmap contains text.
- Keep live `CH`/value text in the left well and the dropdown triangle in the
  right well of `roller-selector-inline-photo-runtime.png`. The substantial
  roller stays between them on the same baseline, so channel changes remain
  sharp, compact, and accessible.
- Place exactly four `panel-screw-photo-runtime.png` instances over the shared
  console corners. They are decorative overlays and must never reserve layout
  space or intercept input.

Example browser layering for the jewel:

```css
.track-led {
  position: relative;
  isolation: isolate;
}

.track-led::before {
  position: absolute;
  inset: 22%;
  border-radius: 50%;
  background: var(--track-color);
  box-shadow: 0 0 0.45rem color-mix(in srgb, var(--track-color) 75%, transparent);
  content: "";
  opacity: var(--led-on, 0);
}

.track-led > img {
  position: relative;
  width: 100%;
  mix-blend-mode: luminosity;
}
```

The photographic PNG surfaces were created with OpenAI's built-in image
generator, then their low-alpha edge noise was removed and their transparent
bounds were normalized. The switch pair is pre-rasterized for clean icon-scale
sampling. The SVG files remain as deterministic geometry sources and overlays;
the interface prefers the photographed transport and track-label surfaces.

`../panel-wear-overlay.png` is the shared transparent grime, scuff, and scratch
pass for the surrounding panel material. Apply it at low opacity below live
controls and text so the equipment looks handled without making the interface
dirty or hard to read.
