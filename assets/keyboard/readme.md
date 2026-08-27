# K2 photographic keyboard assets

## Direction

Build the viewer from the real photographed keyboard, not from a stylized or
projected keyboard illustration. The finished instrument should look as close
as practical to the physical K2. Use the supplied photographic board, keycaps,
navigation cluster, arrows, numpad, LED panel, tape, surface wear, and shadows.

The only intentionally virtual hardware is the knob bank. Knobs must remain
code-rendered because clean photographic knob cutouts are not available. They
should still be positioned over the real board and styled to belong to it.

Do not return to the old approach where a faint keyboard photo sits underneath
a complete set of opaque, drawn keycaps. That creates doubled edges and legends.
There must be one visible key layer: the real key sprites.

## Next design pass: clean photographic instrument

The next implementation must treat `keyboard-clean-mask.png` as the primary
enclosure. It preserves the real blue-grey case, masking tape, wear, and
imperfect edges while removing the handwritten legends and manufacturer badge. The
application will supply those legends itself. The result should look like a
real custom instrument whose labels and active keys happen to be live, not a
photograph sitting inside a conventional dashboard.

This pass has five linked goals:

1. Use the clean photographed enclosure and render all tape writing in code.
2. Render an actual alternate-colour key image when a note is active; never
   paint a coloured rectangle, border, or halo over the resting row.
3. Preserve the enclosure and every control's photographic aspect ratio.
4. Redesign the surrounding chrome, typography, help, and staff background to
   belong to the same physical instrument.
5. Protect useful staff space on short displays through the existing Compact
   board option and height-aware responsive rules.

### Clean enclosure and tape legends

- Make the 1949 × 807 `keyboard-clean-mask.png` coordinate system canonical.
- Keep the photographed masking-tape shapes and texture exactly as supplied.
- Draw only the ink in code. Do not add opaque tape-coloured label boxes.
- Centralize label copy, anchor, maximum width, alignment, font size, line
  spacing, and small rotation in one tape-label manifest. Do not scatter magic
  coordinates throughout the renderer.
- Include all physical labels: Bitcrush, the other twelve knob functions,
  MIDI send/receive/status, waveform symbols, velocity-switch note, keyboard
  name, arrow functions, and drums.
- Bundle one hand-lettered font or purpose-built glyph atlas. Browser/system
  font fallback must not turn the tape into clean UI typography. Slightly
  imperfect baselines are welcome; random motion between frames is not.
- Static legends are rendered every time. A tape region may expose dynamic
  text only where it improves operation, but the physical display remains the
  primary place for short status values.

The older `keyboard-mask.png` and `keyboard.png` remain calibration references;
they are not the final visible enclosure after this migration.

### True alternate-colour key sprites

The complete row strips may be used only as extraction/calibration sources.
They cannot remain underneath active keys because a lowered replacement would
reveal the original key behind it. The runtime resting layer must therefore be
composed from independent transparent key sprites over the empty switch wells.

For every key:

1. Produce one clean RGBA sprite containing only that key, including its bevel,
   legend, texture, and local shadow. Remove neighbouring key fragments from
   the crop and record its rest anchor.
2. At build time, generate alternate sprites for every MIDI track colour,
   manual-press colour, and range-warning colour. Replace hue/chroma while
   retaining the source luminance, highlights, shadows, wear, and alpha
   silhouette. Runtime rendering must only select a prebuilt variant.
3. Draw either the resting sprite or its coloured variant. Do not draw a
   coloured canvas shape over the key image.
4. When active, translate the complete alternate sprite down by a small amount
   in canonical board pixels. The newly exposed dark switch well supplies the
   contact shadow. Labels and drum symbols move with the sprite; hit targets
   remain at their resting positions.
5. The alternate sprite must have identical pixel dimensions, alpha bounds,
   and anchor data to the resting sprite so swapping cannot jump, stretch, or
   expose rectangular crop edges.

`TRACK_COLORS` remains the single palette shared by keys and staff notes. A
manual computer-key press can use the appropriate instrument accent, but must
go through the same alternate-image path.

### Aspect-ratio and coordinate contract

Never size the board with independent width and height calculations. Given an
available rectangle and the 1949 × 807 canonical enclosure:

```text
scale = min(available_width / 1949, available_height / 807)
board_width = 1949 * scale
board_height = 807 * scale
board_x = available_x + (available_width - board_width) / 2
board_y = available_y + (available_height - board_height) / 2
```

The same uniform transform must place the enclosure, key sprites, control
images, virtual knobs, tape legends, status display, highlights, and pointer
targets. Do not fit a sprite independently into an approximate cell. Calibrate
the old 3618 × 1560 asset locations against landmarks in the clean enclosure
and store the resulting canonical rectangles/anchors in one geometry table.

Normal mode uses `contain` and shows the complete enclosure. Compact mode is
the explicit exception: it uses the fixed working-surface crop documented
below. Neither mode may distort the board or use an arbitrary `cover` crop.

### Surrounding interface system

The keyboard is the visual source of truth for the rest of the application.

- Replace the large tan manual/staff fallback with a deep charcoal or
  green-black workbench surface. Any texture must be subtle enough not to
  compete with the photographed enclosure.
- Make help a collapsible service-manual drawer or overlay instead of a large
  permanent beige region.
- Consolidate transport and MIDI utilities into restrained recessed equipment
  strips. Avoid generic cards, pills, large radii, and Bootstrap-style spacing.
- Use two deliberate type systems: hand lettering only on masking tape, and a
  narrow industrial sans/mono treatment for controls, time, MIDI, and staff.
- Derive accents from the instrument: enamel blue-grey, faded green, ivory,
  salmon, masking-tape yellow, black switch wells, and amber display light.
- Keep the real keyboard dominant. Application controls should support it, not
  frame it with equal visual weight.

### Short-screen and Compact board behavior

Responsive layout must consider both window width and height. The current
Compact board preference remains an explicit user override and URL-persisted
state, but the layout may also enter a constrained arrangement automatically
when the normal board would leave the staff unusable.

- Full mode: maximize the aspect-correct board while reserving transport,
  tracks, and a useful staff region.
- Compact mode: show canonical crop `(82, 178)–(1860, 757)`. This is the working
  surface from the power switch/control bank through all five key rows, the
  navigation/arrows, and numpad. It deliberately removes the manufacturer/header
  strip, lower footer, and narrow side margins instead of making the complete
  instrument into a smaller thumbnail.
- With a MIDI file loaded, reserve a practical minimum staff height before
  allocating remaining height to the board.
- Without a file, keep the service manual collapsed by default on short
  screens so it does not dictate page height.
- Let secondary controls wrap, collapse into a utility drawer, or reduce their
  padding before shrinking the photographed keyboard text below readability.
- The board and staff must remain usable at 1366 × 768 and 1024 × 600, in
  addition to the current large desktop viewport.

### Planned implementation order

1. Audit `keyboard-clean-mask.png`, measure every opening/tape region, and add
   the new canonical geometry table.
2. Replace the visible shell while keeping existing behavior registered to the
   old board until geometry parity tests pass.
3. Correct all 100 independent key sprites and add explicit rest/pressed anchor
   metadata.
4. Replace full-row resting imagery with independent sprites, then implement
   cached alternate-colour raster variants and physical displacement.
5. Add the tape-label manifest and bundled hand-lettered type treatment.
6. Replace width-only board sizing with the uniform contain transform and use
   it for rendering and hit testing.
7. Redesign transport, utilities, staff surface, and help drawer around the
   photographic material language.
8. Integrate Compact board with available-height budgeting and validate the
   large, 1366 × 768, and 1024 × 600 layouts.
9. Regression-test pointer input, computer keys, MIDI playback, drum symbols,
   multi-track colours, staff selection, native rendering, and WebAssembly.

### Acceptance tests for this pass

- The clean enclosure is visible and its tape contains code-rendered ink with
  no opaque text backgrounds.
- Comparing any active key to its resting state shows a different photographic
  key image, not a rectangular overlay or glow.
- No adjacent cap, switch well, or rectangular crop boundary changes colour.
- Active and resting key variants have the same scale and anchor; pressing only
  introduces the intentional downward travel.
- A circle in the source photograph remains circular at every supported window
  size, proving that the board is not stretched.
- Tape legends, knobs, images, and pointer targets remain registered after
  resizing and switching Compact board on or off.
- The staff remains visible and useful on 1366 × 768 and 1024 × 600 screens.
- No large tan surface, generic card grid, pill-heavy toolbar, or unrelated UI
  typography remains in the primary experience.

### Implementation status (August 2026)

The clean photographic pass is implemented. `keyboard-clean-mask.png` now
defines the 1949 × 807 board coordinate system. `build.rs` isolates the resting
keys from the supplied row/cluster photographs and generates all track-colour,
manual-press, and range-warning variants before the application is compiled.
They are stored as compact transparent WebP assets in Cargo's build output.
Active keys only select a cached prebuilt handle and apply a small downward
travel; no flood fill or per-pixel colour work runs during playback. The old
row strips are no longer visible beneath those sprites.

The alpha-row crops and hit bounds now use the measured red separators from the
annotated row copies in `src/key_geometry.rs`. Each crop contains its target
key's photographed side bevel, so build-time colour replacement recolours that
bevel completely; preserving an old-colour strip at the left edge is no longer
necessary on annotated rows.

Tape ink is rendered from the centralized manifest in `src/render.rs` using the
bundled Permanent Marker font. Board hardware, labels, key sprites, knobs, hit
targets, and status display all share the same uniform contain transform. The
outer UI uses the instrument-derived charcoal/olive/salmon palette, and the old
tan manual is now a collapsed dark service drawer. Compact mode uses the fixed
working-surface crop while height budgeting reserves the lower region for the
staff on short displays.

A drag rail is attached directly below the keyboard viewport. Dragging it with
a mouse or touch input chooses a custom viewport height and gives the released
space to the staff/manual below. Compact stays in its working-surface crop while
it is resized; normal mode keeps its complete-board behavior. Its responsive
bounds retain a small usable lower pane. Using the Compact board control again
clears the custom height and returns sizing to the compact/automatic preset.

Validated layouts: 1568 × 994, 1366 × 768, and 1024 × 600. A held-key visual
check confirms the alternate sprite has no coloured rectangle or neighbouring
key contamination. Native unit tests and the release WebAssembly build pass.

## Source assets

- `keyboard.png`: complete straight-on photographic reference.
- `keyboard-clean-mask.png`: clean 1949 × 807 target enclosure with retained
  masking-tape texture and transparent hardware openings. This becomes the
  primary visible shell in the next design pass.
- `keyboard-mask.png`: real enclosure with transparent openings for interactive
  controls and key clusters. This is the current shell and future calibration
  reference.
- `Pixel.png`: additional full-resolution source/reference.
- `top row.png`: alpha-key row 1.
- `2nd row.png`: alpha-key row 2.
- `3rd row.png`: alpha-key row 3.
- `4 row.png`: alpha-key row 4.
- `bottom row.png`: alpha-key row 5.
- `home.png`: real six-key navigation cluster.
- `arrow keys.png`: real four-key arrow cluster.
- `numpad.png`: real 4 × 5 numpad/drum cluster.
- `led panel.png`: supplied Num/Caps/Scroll cutout. This file is retained as a
  source but is nearly fully transparent, so it is not used at runtime.
- `led-panel-board-crop.png`: lossless replacement cropped from `keyboard.png`.
- `power-switch.png`: retained only as a historical calibration reference; the
  standalone left bay now contains the virtual Bitcrush knob.
- `display.png`: lossless display crop from `keyboard.png`.

The alpha row order is always:

1. top row
2. 2nd row
3. 3rd row
4. 4th row
5. bottom row

Do not reorder the row images by filename sorting alone because `4 row.png` is
the fourth row and `bottom row.png` is the fifth.

## Geometry and extraction contract

The five alpha-row sources are approximately 2,120 × 180 pixels and share a
15-unit logical coordinate system. Extract sprites using the measured pixel
separators in `src/key_geometry.rs`; the logical grid remains the fallback and
continues to define key order and bottom-row gaps. The original photographic
pixels remain unchanged.

- Top and third rows: 1.5-unit first key, twelve 1-unit keys, 1.5-unit last key.
- Second and fourth rows: fifteen 1-unit keys.
- Bottom row: 1.5-unit first key, 1-unit empty gap, ten 1-unit keys, 1-unit
  empty gap, 1.5-unit last key.

Keep a small transparent safety margin around every extracted sprite so its
original edge shadow is not clipped. Never repaint, regenerate, normalize, or
invent a key. Generated imagery is unsuitable here because exact key shape,
color, wear, and identity matter.

Derived alpha sprites belong in `assets/keyboard/keys/` with stable row/column
names, for example `r1-c01.png`. Navigation, arrow, and numpad sprites belong
in `nav/`, `arrows/`, and `numpad-keys/`. Preserve the source images unchanged.

The current build temporarily uses each complete photographed row as its
resting layer. When a key activates, its cell is isolated from that source,
colour-swapped, and drawn slightly lower. This transitional implementation is
the source of the visible rectangular crop problem and must be replaced by the
independent-sprite architecture specified above.

## Rendering architecture

1. Draw `keyboard-clean-mask.png` at its canonical 1949 × 807 aspect ratio.
2. Place the real independent alpha, nav, arrow, numpad, LED, switch, and
   display imagery in the matching openings.
3. Use per-key transparent sprites for resting and alternate-colour images,
   pressed displacement, note labels, and pointer hit targets.
4. Preserve the existing logical MIDI/key mappings from `src/layout.rs`; visual
   assets must not change note behavior.
5. Draw the twelve functional knobs virtually in the photographed knob wells.
   Keep Bitcrush represented by the standalone left control.
6. Scale the complete board from one canonical coordinate system. Image bounds,
   highlight bounds, and pointer bounds must use the same transform.

Active notes use their MIDI track colour as a colour swap on the photographed
key face. The filter must retain the real key's grain, wear, legend, bevel, and
lighting. A darker upper surface, lowered filtered key, and matching label
displacement make the key appear physically depressed. Filter masks are built
from the connected keycap material so neighbouring keys and switch wells never
inherit the colour. Never put a halo behind an active key. Computer-key labels,
note names, play-order badges, and drum symbols remain code-rendered overlays
and move down with the pressed key.

## Baseline acceptance criteria

- No doubled photographed/drawn keys or legends.
- No visible seams, clipped shadows, or stretched individual keycaps.
- The row order, bottom-row gaps, navigation layout, arrows, and 4 × 5 numpad
  match the real board.
- Every visible key retains its existing MIDI behavior and can highlight
  independently.
- The board stays registered with hit targets at supported window sizes.
- The source photographs remain untouched; all processing is reproducible from
  derived files.
