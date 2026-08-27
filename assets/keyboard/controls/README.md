# K2 control-surface assets

Reusable, label-free physical controls for the K2 interface. All files have a
transparent canvas; color, text, pointer position, and light emission belong to
the consuming UI.

| Asset | Format / intrinsic size | Intended use |
| --- | --- | --- |
| `rotary-knob-face.png` | RGBA PNG, 1024 × 1024 | CH/OCT and other rotary controls. Add the pointer as a separate vector/canvas layer. |
| `rocker-switch-off.svg` | SVG, 240 × 320 viewBox | Dark plastic rocker in its raised-top OFF state. |
| `rocker-switch-on.svg` | SVG, 240 × 320 viewBox | Matching raised-bottom ON state. |
| `fader-cap.png` | RGBA PNG, 1024 × 572 | Warm gray/beige slider thumb with a center groove. |
| `led-jewel.png` | RGBA PNG, 1024 × 1024 | Shared neutral/unlit lens and bezel for every track LED. |
| `lcd-glass-overlay.svg` | SVG, 960 × 300 viewBox | Low-opacity glass reflection and vignette above amber display content. |
| `label-plate.svg` | SVG, 600 × 104 viewBox | Optional recessed plate behind track or section labels. |

## State and color contract

- Never rotate `rotary-knob-face.png`; keep the physical lighting fixed and
  rotate a separate pointer from about -135° to +135°.
- Swap the complete rocker SVG when state changes. Their viewBoxes and outer
  geometry match, so layout does not shift.
- Keep `led-jewel.png` neutral. Put LED color under/over the lens with a blend
  layer and add the glow outside the image. This gives every track one shared
  piece of physical hardware.
- Stretch `lcd-glass-overlay.svg` exactly over the active display aperture. It
  uses `preserveAspectRatio="none"` intentionally and contains no display text.
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
