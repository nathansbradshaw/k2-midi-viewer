# K2 control-surface redesign

Status: proposed design specification  
Scope: upper control console only  
Primary constraint: the redesign must not increase the interface's vertical footprint at any supported window size.

## Outcome

The upper console should read as one old, purpose-built instrument rather than a row of web controls placed on a dark background. This pass makes five coordinated changes:

1. Loop and Sound become compact horizontal hardware switches with permanent `OFF` and `ON` legends.
2. The live-play MIDI channel becomes a detented roller encoder instead of a conventional dropdown-looking control.
3. Pitch down, pitch step, pitch up, and reset become one coherent control group.
4. View and input options become compact, icon-led controls with short legends and explanatory tooltips.
5. The shared console plate gains restrained screws, edge wear, and stronger manufactured bevels without adding layout height.

This is a visual and interaction redesign, not a change to playback behavior, MIDI routing, URL persistence, keyboard geometry, or the staff visualizer.

## Non-negotiable height budget

The current dense-desktop layout reserves `220 px` for the upper chrome in `view()`. Keep that reserve unchanged. The redesign must fit inside the current band allocations rather than asking the keyboard or visualizer to surrender space.

| Area | Existing dense-desktop face height | Redesign limit | Rule |
| --- | ---: | ---: | --- |
| Main header controls | `42 px` | `42 px` | The channel encoder uses the existing selector footprint. |
| Secondary control faces | `30 px` | `30 px` | Icons, legends, pitch controls, and tooltips fit without a second text line. |
| Transport keys | `40 px` | `40 px` | Loop and Sound modules remain within the current transport row. |
| Mixer CH/OCT controls | approximately `47 px` including legends | unchanged | Per-track rotary knobs are out of scope. |
| Module dividers | `2 px` each | `2 px` each | Improve contrast inside the same two pixels. |
| Dense top-chrome reserve | `220 px` | `<= 220 px` | Hard acceptance criterion. |

At narrower breakpoints, the redesign may use fewer rows because the utility controls are narrower, but it must never use more rows than the current layout. Hover hints are overlays and never participate in layout.

## Console hierarchy

The control order stays familiar while the grouping becomes clearer:

```text
┌ screw ───────────────────────────────────────────────────────────── screw ┐
│ K2 identity │ amber status display │ MIDI I/O │ PLAY CH encoder │ OPEN  │
│             │                       │          │                  │ MIDI  │
├──────────────────────────────── shallow equipment groove ────────────────┤
│ PITCH [−][OCT][+][reset] │ RATE knob │ MAP │ ALL │ KEYS │ DRUM │ BOARD │
├──────────────────────────────── shallow equipment groove ────────────────┤
│ TRANSPORT [PLAY] [STOP] [LOOP off/slider/on] [SOUND off/slider/on] ────  │
├──────────────────────────────── shallow equipment groove ────────────────┤
│ TRACKS  [track channel strips; horizontally scrollable]                   │
└ screw ───────────────────────────────────────────────────────────── screw ┘
```

The drawing is semantic rather than proportional. Existing header, transport, and mixer row heights remain authoritative.

## 1. Loop and Sound switches

### Physical construction

Use a compact horizontal slide switch inspired by the supplied reference:

- Module footprint: keep the current `88 px` width and current row height.
- Module face: one subtle routed rectangle, not a raised card.
- Caption: `LOOP` or `SOUND`, engraved/printed at `8–9 px`, centered above the mechanism.
- State row: `OFF`, a `34 × 16 px` switch mechanism, then `ON`.
- Switch handle: visibly occupies the selected side. State must remain legible in grayscale and without the accent color.
- State change swaps complete OFF/ON artwork or moves a hardware handle; it must not merely recolor the same shape.
- The whole module, including its legends, is the hit target. Minimum pointer target remains `44 × 36 px` even though the mechanism is smaller.

Recommended new assets:

- `horizontal-switch-off.png`
- `horizontal-switch-on.png`
- Matching `*-runtime.png` derivatives, sampled for a display size near `34 × 16 px`

The assets should be label-free, front-on, and share identical transparent bounds so switching state cannot move the layout. Warm black plastic, a small aged metal handle, recessed socket shadow, and restrained dust should match the existing photographed control family.

### Behavior

Loop keeps its current behavior. Its visible caption stays `LOOP`; do not change the hardware legend to `LOOP SEL`, because changing engraved text makes the panel feel software-generated and causes visual jitter.

Its tooltip provides the changing scope:

- No staff selection: `Loop song — Off/On`
- Staff range selected: `Loop selected range — Off/On`

Sound also keeps its current behavior:

- Available: `Sound output — Off/On`
- External MIDI output active: describe the routed output in the tooltip.
- Unavailable: show the switch in OFF, disable interaction, mute the legends, and put the specific error in the tooltip. Keep the face caption `SOUND` instead of changing it to `ERROR`.

Tooltips appear below after roughly `350 ms` and also appear when the control receives keyboard focus.

## 2. Live channel roller encoder

The header's `PLAY CH` control should become a discrete, detented encoder similar to the supplied recessed roller reference. This applies to the live-play channel selector only; the per-track CH and OCT rotary knobs remain as they are.

### Physical construction

- Footprint: exactly the current `142 × 42 px` header-selector box.
- Housing: a deep rectangular recess with the same outer bevel and wear language as the action keys.
- Top legend: `PLAY CH 2` with a small downward triangle for direct-list access.
- Lower mechanism: a ribbed horizontal roller, approximately `40 × 18 px`, centered in a black slot.
- The wheel's lighting stays stationary. Selection changes shift rib highlights/pointer position rather than rotating the entire photographic texture.
- Channel values are displayed as the user-facing range `1–16`; internal zero-based values remain an implementation detail.

The current `selector-housing-photo` may inform the material treatment, but its side-by-side wells do not match this layout. Prefer a dedicated label-free roller housing and separate wheel asset rather than stretching the existing selector photograph into a visibly distorted shape.

### Interaction contract

- Click/tap the legend or housing: open the complete channel list for direct selection.
- Scroll while hovered: one channel per wheel detent.
- Vertical drag on the roller: one channel per `8–10 px` of travel.
- Focus + Arrow Up/Right: increment one.
- Focus + Arrow Down/Left: decrement one.
- Home/End: channel 1/channel 16.
- Clamp at the endpoints; do not wrap from 16 to 1.
- Update MIDI routing immediately after each detent.

Preserve a native accessible selection control as the semantic input, even if custom canvas/photo layers supply the visible hardware. Accessible name example: `Live-play MIDI channel, 2 of 16`.

Tooltip: `Live-play MIDI channel — 2. Scroll or drag to adjust; click to choose.`

## 3. Cohesive Pitch group

Pitch currently mixes four pitch actions with the unrelated Rows mapping mode, and Reset uses a larger text key than the other pitch actions. Correct both issues.

### Proposed group

```text
PITCH  [ − ] [ OCT ] [ + ] │ [ reset-arrow ]
```

- Keep the existing left-side `PITCH` legend so the group does not grow vertically.
- Mount the four controls on one shallow recessed base/groove.
- Use the same neutral photographed cap family, height, bevel, baseline, and gap for all four controls.
- Down, up, and reset use equal square footprints near `32–36 px`.
- Step may be slightly wider only if `OCT`/`ST` needs it.
- A one-pixel engraved separator before Reset distinguishes recovery from adjustment without ejecting it into another module.
- Replace the word `Reset` with a purpose-drawn circular reset-arrow SVG. Do not depend on a Unicode glyph whose coverage and weight can vary.
- The amber LCD's OCT field remains the persistent readout for the resulting pitch offset.

Tooltips:

- Minus: `Lower pitch by one octave` or `Lower pitch by one semitone`
- Step: `Pitch step — Octave` or `Pitch step — Semitone`
- Plus: `Raise pitch by one octave` or `Raise pitch by one semitone`
- Reset: `Reset pitch to ±0`

Reset stays visibly disabled at `±0`. Down and Up stay disabled when no MIDI file is loaded. Step remains available because it changes the next adjustment mode.

Move `Rows: Closest/L/R/U/D` into the utility control group described below.

## 4. Purposeful utility controls

Use an icon-led hybrid rather than fully icon-only buttons. Symbols reduce scanning and width; tiny stable legends prevent the interface from becoming indecipherable on touch devices where hover does not exist. Tooltips provide the full explanation.

All icons should be simple custom SVGs with the same `16 × 16` view box, `1.5 px` warm-ivory strokes, rounded joins, and no baked state color. This avoids platform-dependent font glyphs.

| Control | Visible face | Type and state treatment | Tooltip content |
| --- | --- | --- | --- |
| Row mapping | stacked-row icon + `NEAR`, `L/R`, or `U/D` | Multi-state selector; current value is visible, no on/off lamp | `Key mapping — Closest. Click to cycle how repeated notes choose a keyboard row.` |
| All notes | note-group icon + `ALL` | Latching toggle with small jewel when on | `Show all notes — On/Off. Include notes outside the current playback moment.` |
| Computer keys | keyboard-grid icon + `KEYS` | Latching toggle with small jewel when on | `Computer keyboard input — On/Off.` |
| Drum symbols | drum/pad icon + `DRUM` | Latching toggle with small jewel when on | `Drum symbols — On/Off. Show GM percussion labels on the numpad.` |
| Compact board | crop/frame icon + `BOARD` | Latching toggle with small jewel when compact | `Keyboard view — Compact/Full.` |

Recommended face sizes:

- Mapping selector: `68–76 × 30 px`, because it carries a changing value.
- Binary utility keys: `48–58 × 30 px` each.
- Icon-to-legend gap: `4 px`.
- Group gap: `4–5 px`.

This replaces roughly five wide sentence buttons with compact instrument controls and should save well over `200 px` on desktop. The freed width is breathing room, not permission to increase control height.

### State and feedback

- Toggle state uses both the physical/latching treatment and a jewel lamp; never rely only on salmon text.
- Hover adds a very faint warm reflection, not a browser-style colored outline.
- Press shifts/darkens the face by `1–2 px` visually while preserving its layout bounds.
- Keyboard focus gets a high-contrast inner keyline that is distinct from hover and passes contrast requirements.
- Disabled controls retain their icon silhouette and tooltip but reduce legend/lamp contrast.
- Tooltip panels use warm black, a one-pixel aged-metal border, `11 px` text, and a maximum width around `260 px`.

## 5. Aged console plate, screws, and bevels

The supplied large-console reference works because wear follows the object's construction: exposed corners brighten, seams collect grime, screws anchor the plate, and neighboring modules sit at different depths. Reproduce that logic rather than adding uniformly random noise.

### Material stack

From back to front:

1. Charcoal-brown plate base (`PANEL_BG`, currently `#1c1d17`).
2. Existing dark grain at about `8–10%` opacity.
3. Existing panel-wear overlay at about `6–9%` opacity.
4. Edge-biased warm metal exposure on the outer bevel and high-touch control edges.
5. Live labels and controls.

Keep wear out of display apertures and above live text. A few larger chips belong near the shell perimeter and divider ends; dense scratches beneath labels will look like texture pasted on top of the UI.

### Screws

- Add four decorative screw heads to the shared upper console, one near each outer corner.
- Diameter: `8–10 px` at 1×.
- Inset: roughly `8 px` from the plate edges, inside the existing horizontal padding.
- Material: oxidized dark steel with one soft rim highlight and a recessed contact shadow.
- Slot angles may vary between the four instances but remain fixed between frames.
- Screws are decorative, excluded from hit testing and accessibility, and layered as overlays so they consume zero layout space.
- Do not add screws to every small cluster. Four anchors are enough to sell the construction without turning the console into a pattern.

### Bevel and separators

- Preserve the existing one-pixel top highlight and bottom shadow on the continuous panel.
- Preserve each module divider's current `2 px` allocation: first pixel deep warm black, second pixel a low-alpha warm highlight.
- Increase depth through contrast, not thickness.
- Recess the LCD and roller encoder more deeply than buttons; transport sockets are the next-deepest layer; utility keys remain shallow and flush.
- Keep the wood cheeks narrow and secondary. They frame the instrument but should not become a wood-themed page background.

## Responsive behavior

### `>= 1180 px`

- Keep one main header row.
- Keep one secondary control row.
- Keep one transport row.
- Use the reduced utility widths to create calmer spacing around clusters.

### `720–1179 px`

- Preserve the current stacked main-header behavior.
- The compact secondary controls should fit in one horizontally scrollable row where practical; if wrapping is required, cap it at the current two rows.
- Keep Loop and Sound on the first transport line next to Play and Stop.

### `< 720 px`

- Use at most two secondary rows: Pitch + Rate, then the utility controls.
- If width is extremely constrained, horizontally scroll the utility row rather than stacking each control into its own row.
- Do not shrink hit targets below `44 px` in either pointer dimension.
- Tooltips are supplementary. The icon + micro-legend must remain sufficient to identify every action without hover.

## Accessibility and semantics

- Every icon button has a complete accessible name and current state.
- Tooltips trigger on hover and keyboard focus and dismiss with Escape.
- Latching controls expose pressed/selected state; the mapping and channel controls expose their current values.
- Do not communicate state with color alone.
- Preserve native keyboard behavior behind custom photographic/canvas artwork.
- Respect reduced-motion settings. No animated grime, light flicker, or inertial encoder spinning is needed.
- Verify focus treatment against both the darkest panel regions and the brightest wear marks.

## Implementation map

The design aligns with the current Rust/Iced structure:

| Current implementation | Proposed direction |
| --- | --- |
| `rocker_switch()` | Replace or supplement with `horizontal_switch()` using paired OFF/ON assets and permanent side legends. |
| `selector_housing()` + live-channel `pick_list` | Replace visible surface with `roller_channel_selector()` while retaining an accessible discrete selector/list path. |
| `panel_key()` | Add compact icon content and a square reset variant; keep photographic surface layering. |
| `pitch_controls` | Remove `layout_label`; wrap pitch actions in one shared shallow mount. |
| `KeyPickMode::label()` | Add a short face value (`NEAR`, `L/R`, `U/D`) while retaining the full descriptive value for accessibility/tooltips. |
| Utility labels in `view()` | Replace sentences and embedded `: on/: off` state text with icon + micro-legend + lamp state. |
| `textured_panel()` | Add a decorative four-screw overlay and refine wear opacity/placement without changing the sizing base. |
| `module_divider()` | Tune the two existing pixels; do not increase divider height. |
| Existing `tooltip()` pattern on Sound | Extract a shared hardware-tooltip helper and use it for every redesigned compact control. |

New UI icons should be deterministic SVG assets rather than generated bitmaps. The horizontal switches, roller wheel, roller housing, and screw heads benefit from photographed/raster assets because their material realism depends on texture and lighting.

## Implementation sequence

1. Capture baseline screenshots and measured upper-console heights at representative widths: `1600`, `1180`, `900`, `719`, and a phone width.
2. Add shared tooltip styling and the five utility SVG icons.
3. Rebuild the Pitch group and move row mapping into the utility group.
4. Replace utility button labels and verify all focus/disabled/toggled states.
5. Add the horizontal switch assets/component and migrate Loop and Sound.
6. Add the roller encoder assets/component and migrate the live channel selector.
7. Add screw overlays and tune grain/wear/bevel contrast last, after control geometry is stable.
8. Repeat screenshot and keyboard-accessibility checks at every baseline width.

## Acceptance criteria

- Dense-desktop top chrome remains within the existing `220 px` reserve.
- At every tested width, the distance from the window top to the keyboard top is no greater than the baseline.
- Loop and Sound always show `OFF` and `ON`, and their physical position communicates state without color.
- The live channel reads as a detented hardware control, displays channels `1–16`, and remains directly selectable and keyboard-accessible.
- Pitch down, step, up, and reset share one mount, one cap family, and one baseline; row mapping is no longer inside the Pitch group.
- Utility controls use custom symbols plus stable short legends; no face contains a sentence or appends `: on/: off`.
- Every compact control has a hover/focus tooltip and a complete accessible label.
- Four screws anchor the shared console without intercepting input or adding layout size.
- Wear is visible on exposed plate areas and edges but never reduces the readability of displays, labels, icons, or focus rings.
- Existing playback, looping, sound, channel routing, pitch, mapping, view-toggle, and URL-state behavior remains unchanged.

## Explicitly out of scope

- Changing keyboard or staff height to make room for the console
- Redesigning the photographed keyboard itself
- Replacing per-track CH/OCT rotary knobs with the header roller encoder
- Adding animation purely for atmosphere
- Changing playback, MIDI, loop-range, or pitch semantics
- Making the entire application background wooden or heavily distressed

