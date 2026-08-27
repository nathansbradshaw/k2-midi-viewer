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
  `transport-button-face.svg`; the keycap intentionally contains no symbol.
- The label plate is optional. A CSS inset background is equivalent where a
  raster/vector dependency would be unnecessary.

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

The three PNG surfaces were created with OpenAI's built-in image generator,
then their low-alpha edge noise was removed and their transparent bounds were
normalized. The switch pair, LCD overlay, and optional plate are project-native
SVG so state alignment and opacity remain deterministic.
