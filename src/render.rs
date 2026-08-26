use std::collections::{HashMap, HashSet};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::key::{Cluster, Key, KeyId};
use crate::layout::{KNOB_COL_STEP, NAV_COL, NUMPAD_COL};
use crate::Message;

/// Per-track highlight palette — cycled for files with more than 8 tracks.
/// RGB values tuned for visibility on the dark PCB and dark staff background.
pub const TRACK_COLORS: &[(u8, u8, u8)] = &[
    (0x4F, 0xC3, 0xF7), // sky blue
    (0xFF, 0xB7, 0x4D), // amber
    (0xA5, 0xD6, 0xA7), // green
    (0xF4, 0x8F, 0xB1), // pink
    (0xCE, 0x93, 0xD8), // lavender
    (0xFF, 0xF1, 0x76), // yellow
    (0xFF, 0x8A, 0x65), // coral
    (0x80, 0xCB, 0xC4), // teal
];

pub const UNIT: f32 = 54.0;
pub const GAP: f32 = 4.0;
pub(crate) const CANVAS_FONT: iced::Font = iced::Font::with_name("Fira Sans");
const BOARD_INSET_Y: f32 = 28.0;
const NUMPAD_Y_OFFSET: f32 = UNIT + GAP;
const BOARD_LAYOUT_WIDTH: f32 = 1330.0;
const MIN_BOARD_INSET_X: f32 = 8.0;

fn board_inset_x(width: f32) -> f32 {
    ((width - BOARD_LAYOUT_WIDTH) / 2.0).max(MIN_BOARD_INSET_X)
}

pub struct BoardCanvas<'a> {
    pub keys:        &'a [Key],
    /// Maps KeyId → track index for color. usize::MAX = manually toggled (uses original colour).
    pub highlighted: &'a HashMap<KeyId, usize>,
    /// Optional live overlay (for browser MIDI input) drawn above the base
    /// playback/selection highlight without requiring a temporary merged map.
    pub overlay_highlighted: Option<&'a HashMap<KeyId, usize>>,
    /// Chronological play steps to badge onto highlighted keys while a staff
    /// range is selected. Chords share a number; repeated keys have several.
    pub play_order:  Option<&'a HashMap<KeyId, Vec<usize>>>,
    /// Persistently-active control keys (e.g. layered waveform selects),
    /// independent of playback note overlays.
    pub selected_controls: &'a HashSet<KeyId>,
    /// Keys currently held with the pointer.
    pub pressed: &'a HashSet<KeyId>,
    /// Computer-key labels shown over the note names in performance mode.
    pub projected_labels: Option<&'a HashMap<KeyId, String>>,
    /// GM drum assignments for the numpad's 4x5 pad grid.
    pub drum_note_to_key: &'a HashMap<u8, KeyId>,
    /// Replace the numpad legends with instrument icons and names.
    pub show_drum_symbols: bool,
    /// Current 0.0..=1.0 dial position of each of the 12 encoder knobs.
    pub knob_values: &'a [f32],
}

#[derive(Default)]
pub struct CanvasState {
    cache: canvas::Cache,
    pressed: Option<KeyId>,
    dragging_knob: Option<u8>,
}

/// Vertical extent of the pop-up slider shown while a knob is held, in
/// canvas-local coordinates. The slider maps cursor Y to a value directly
/// (absolute position, not relative drag delta) over a much longer travel
/// than the knob itself — far easier to land on a precise setting than
/// eyeballing rotation on a small dial.
fn knob_slider_track(knob_rect: Rectangle) -> (f32, f32) {
    let top = knob_rect.y + knob_rect.height + 22.0;
    (top, top + 160.0)
}

impl<'a> canvas::Program<Message> for BoardCanvas<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    let inset_x = board_inset_x(bounds.width);
                    for key in self.keys {
                        if key_rect_with_inset(key, inset_x).contains(pos) {
                            if let Some(idx) = key.knob_index {
                                state.dragging_knob = Some(idx);
                                return Some(canvas::Action::capture());
                            }
                            state.pressed = Some(key.id);
                            return Some(
                                canvas::Action::publish(Message::KeyPressed(key.id))
                                    .and_capture(),
                            );
                        }
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(idx) = state.dragging_knob {
                    if let Some(pos) = cursor.position_in(bounds) {
                        let inset_x = board_inset_x(bounds.width);
                        if let Some(knob_rect) = self.keys.iter()
                            .find(|key| key.knob_index == Some(idx))
                            .map(|key| key_rect_with_inset(key, inset_x))
                        {
                            let (top, bottom) = knob_slider_track(knob_rect);
                            let value = 1.0 - ((pos.y - top) / (bottom - top)).clamp(0.0, 1.0);
                            return Some(
                                canvas::Action::publish(Message::KnobChanged(idx, value))
                                    .and_capture(),
                            );
                        }
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging_knob.take().is_some() {
                    return Some(canvas::Action::capture());
                }
                if let Some(id) = state.pressed.take() {
                    return Some(
                        canvas::Action::publish(Message::KeyReleased(id))
                            .and_capture(),
                    );
                }
            }
            // Hovering a knob and scrolling nudges its value — much easier
            // than dragging to "turn" it, which is what click-drag simulates.
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    let inset_x = board_inset_x(bounds.width);
                    for key in self.keys {
                        if key_rect_with_inset(key, inset_x).contains(pos) {
                            if let Some(idx) = key.knob_index {
                                let amount = match *delta {
                                    mouse::ScrollDelta::Lines { y, .. } => y * 0.05,
                                    mouse::ScrollDelta::Pixels { y, .. } => y / 200.0,
                                };
                                let current = self.knob_values.get(idx as usize).copied().unwrap_or(0.0);
                                let value = (current + amount).clamp(0.0, 1.0);
                                return Some(
                                    canvas::Action::publish(Message::KnobChanged(idx, value))
                                        .and_capture(),
                                );
                            }
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        state.cache.clear(); // highlighted set changes every 16 ms during playback
        let active_knob = state.dragging_knob.map(|idx| {
            (idx, self.knob_values.get(idx as usize).copied().unwrap_or(0.0))
        });
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            draw_board(
                frame,
                self.keys,
                self.highlighted,
                self.overlay_highlighted,
                self.play_order,
                self.selected_controls,
                self.pressed,
                self.projected_labels,
                self.drum_note_to_key,
                self.show_drum_symbols,
                self.knob_values,
                active_knob,
            );
        });
        vec![geometry]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_knob.is_some() {
            return mouse::Interaction::ResizingVertically;
        }
        if let Some(pos) = cursor.position_in(bounds) {
            let inset_x = board_inset_x(bounds.width);
            for key in self.keys {
                if key_rect_with_inset(key, inset_x).contains(pos) {
                    if key.is_knob {
                        return mouse::Interaction::ResizingVertically;
                    }
                    return mouse::Interaction::Pointer;
                }
            }
        }
        mouse::Interaction::default()
    }
}

#[cfg(test)]
pub fn key_rect(key: &Key) -> Rectangle {
    key_rect_with_inset(key, board_inset_x(1484.0))
}

fn key_rect_with_inset(key: &Key, inset_x: f32) -> Rectangle {
    let cluster_y = if key.cluster == Cluster::Numpad {
        NUMPAD_Y_OFFSET
    } else {
        0.0
    };
    Rectangle {
        x: inset_x + key.col * (UNIT + GAP),
        y: BOARD_INSET_Y + key.row * (UNIT + GAP) + cluster_y,
        width: key.w * UNIT + (key.w - 1.0).max(0.0) * GAP,
        height: key.h * UNIT + (key.h - 1.0).max(0.0) * GAP,
    }
}

/// Returns (fill_color, text_color, glow_color) for a key.
/// `lit_track`: Some(track) when playing, None when unlit.
fn key_colors(cluster: Cluster, lit_track: Option<usize>) -> (Color, Color, Color) {
    let lit = lit_track.is_some();

    // For MIDI keys, use the track colour; fall back to gold for non-MIDI clusters.
    // usize::MAX - 1 = out-of-range warning (nearest key to a note off the board)
    if lit_track == Some(usize::MAX - 1) {
        let fill = rgb(0xFF, 0x3D, 0x72);
        return (fill, Color::WHITE, Color { a: 0.88, ..fill });
    }

    let track_fill = |default_r, default_g, default_b| -> (Color, Color) {
        match lit_track {
            Some(t) if t != usize::MAX => {
                let (r, g, b) = TRACK_COLORS[t % TRACK_COLORS.len()];
                (Color::from_rgb8(r, g, b), Color::BLACK)
            }
            _ => (rgb(default_r, default_g, default_b), Color::BLACK),
        }
    };

    let (fill, text) = match (cluster, lit) {
        (Cluster::Alpha,      false) => (rgb(0x70, 0x62, 0x86), Color::WHITE),
        (Cluster::Alpha,      true)  => track_fill(0xFF, 0xA3, 0x55),
        (Cluster::AlphaLight, false) => (rgb(0xE6, 0xD9, 0xBD), Color::BLACK),
        (Cluster::AlphaLight, true)  => track_fill(0xFF, 0xA3, 0x55),
        (Cluster::Nav,        false) => (rgb(0xC8, 0xB8, 0xC9), Color::BLACK),
        (Cluster::Nav,        true)  => (rgb(0xFF, 0xB4, 0x58), Color::BLACK),
        (Cluster::Arrow,      false) => (rgb(0xEF, 0x70, 0x68), Color::BLACK),
        (Cluster::Arrow,      true)  => (rgb(0xFF, 0x4C, 0x82), Color::WHITE),
        (Cluster::Numpad,     false) => (rgb(0xD8, 0xCC, 0xBC), Color::BLACK),
        (Cluster::Numpad,     true)  => (rgb(0x50, 0xD4, 0xC8), Color::BLACK),
        (Cluster::Encoder,    _)     => (rgb(0x1D, 0x1B, 0x28), Color::WHITE),
    };

    // Glow ring matches the fill colour at high opacity.
    let glow = Color { a: 0.88, ..fill };
    (fill, text, glow)
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn draw_board(
    frame: &mut Frame,
    keys: &[Key],
    highlighted: &HashMap<KeyId, usize>,
    overlay_highlighted: Option<&HashMap<KeyId, usize>>,
    play_order: Option<&HashMap<KeyId, Vec<usize>>>,
    selected_controls: &HashSet<KeyId>,
    pressed: &HashSet<KeyId>,
    projected_labels: Option<&HashMap<KeyId, String>>,
    drum_note_to_key: &HashMap<u8, KeyId>,
    show_drum_symbols: bool,
    knob_values: &[f32],
    active_knob: Option<(u8, f32)>,
) {
    let size = frame.size();
    let inset_x = board_inset_x(size.width);

    // The real K2 is one substantial, warm-grey enclosure. Keeping the board
    // visually continuous makes the unusual clusters read as one instrument.
    frame.fill(&Path::rectangle(Point::ORIGIN, size), rgb(0x08, 0x08, 0x13));
    if size.width > 8.0 && size.height > 8.0 {
        let shadow = rounded_rect(
            Rectangle { x: 3.0, y: 5.0, width: size.width - 6.0, height: size.height - 7.0 },
            18.0,
        );
        frame.fill(&shadow, Color::from_rgba8(0, 0, 0, 0.48));

        let case = rounded_rect(
            Rectangle { x: 2.0, y: 1.0, width: size.width - 4.0, height: size.height - 8.0 },
            18.0,
        );
        frame.fill(&case, rgb(0x35, 0x28, 0x40));
        frame.stroke(
            &case,
            canvas::Stroke::default().with_color(rgb(0xA3, 0x56, 0x78)).with_width(1.5),
        );
    }

    // Recessed switch plates. They are deliberately subtle; the key colours,
    // not giant background blocks, carry the functional grouping.
    let alpha_well = rounded_rect(
        Rectangle {
            x: inset_x - 8.0,
            y: BOARD_INSET_Y + (UNIT + GAP) - 8.0,
            width: 15.0 * (UNIT + GAP) + 4.0,
            height: 5.0 * (UNIT + GAP) + 4.0,
        },
        9.0,
    );
    frame.fill(&alpha_well, rgb(0x16, 0x13, 0x20));

    for rect in [
        Rectangle {
            x: inset_x + NAV_COL * (UNIT + GAP) - 8.0,
            y: BOARD_INSET_Y + (UNIT + GAP) - 8.0,
            width: 3.0 * (UNIT + GAP) + 12.0,
            height: 2.0 * (UNIT + GAP) + 12.0,
        },
        Rectangle {
            x: inset_x + NAV_COL * (UNIT + GAP) - 8.0,
            y: BOARD_INSET_Y + 4.0 * (UNIT + GAP) - 8.0,
            width: 3.0 * (UNIT + GAP) + 12.0,
            height: 2.0 * (UNIT + GAP) + 12.0,
        },
        Rectangle {
            x: inset_x + NUMPAD_COL * (UNIT + GAP) - 8.0,
            y: BOARD_INSET_Y + NUMPAD_Y_OFFSET - 8.0,
            width: 4.0 * (UNIT + GAP) + 12.0,
            height: 5.0 * (UNIT + GAP) + 12.0,
        },
    ] {
        let well = rounded_rect(rect, 8.0);
        frame.fill(&well, rgb(0x16, 0x13, 0x20));
        frame.stroke(
            &well,
            canvas::Stroke::default().with_color(rgb(0x63, 0x3D, 0x60)).with_width(1.0),
        );
    }

    // Encoder banks, status display and identity plate mirror the top deck of
    // the physical board. The 12 knobs are grouped into 3 trays of 4, packed
    // at KNOB_COL_STEP pitch so the whole row still fits the footprint the
    // original 8 full-width knobs occupied.
    let tray_width = 4.0 * KNOB_COL_STEP;
    for group in 0..3 {
        let start = group as f32 * tray_width;
        let tray = rounded_rect(
            Rectangle {
                x: inset_x + start * (UNIT + GAP) - 6.0,
                y: BOARD_INSET_Y - 6.0,
                width: tray_width * (UNIT + GAP) + 8.0,
                height: UNIT + 12.0,
            },
            7.0,
        );
        frame.fill(&tray, rgb(0x10, 0x0F, 0x19));
        frame.stroke(
            &tray,
            canvas::Stroke::default().with_color(rgb(0x7B, 0x43, 0x68)).with_width(1.0),
        );
    }

    // Status display sits directly left of the NUM/CAPS/SCROLL panel, both
    // above the numpad, instead of floating alone in the middle of the deck.
    let display_width_cols = 2.55;
    let display_col = NUMPAD_COL - display_width_cols - 0.3;
    let display_bezel = rounded_rect(
        Rectangle {
            x: inset_x + display_col * (UNIT + GAP),
            y: BOARD_INSET_Y + 4.0,
            width: display_width_cols * (UNIT + GAP),
            height: 36.0,
        },
        5.0,
    );
    frame.fill(&display_bezel, rgb(0x14, 0x11, 0x1B));
    let display = rounded_rect(
        Rectangle {
            x: inset_x + (display_col + 0.2) * (UNIT + GAP),
            y: BOARD_INSET_Y + 11.0,
            width: 2.15 * (UNIT + GAP),
            height: 21.0,
        },
        3.0,
    );
    frame.fill(&display, rgb(0x28, 0x17, 0x18));
    let display_text = active_knob
        .and_then(|(idx, value)| knob_readout(idx, value))
        .unwrap_or_else(|| "MIDI READY".to_string());
    frame.fill_text(Text {
        content: display_text,
        position: Point::new(inset_x + (display_col + display_width_cols / 2.0) * (UNIT + GAP), BOARD_INSET_Y + 21.5),
        color: rgb(0xFF, 0xB5, 0x58),
        size: iced::Pixels(8.0),
        font: CANVAS_FONT,
        align_x: Horizontal::Center.into(),
        align_y: Vertical::Center,
        ..Text::default()
    });

    let status_panel = Rectangle {
        x: inset_x + NUMPAD_COL * (UNIT + GAP) - 8.0,
        y: BOARD_INSET_Y - 8.0,
        width: 4.0 * (UNIT + GAP) + 12.0,
        height: UNIT,
    };
    let status_path = rounded_rect(status_panel, 8.0);
    frame.fill(&status_path, rgb(0x16, 0x13, 0x20));
    frame.stroke(
        &status_path,
        canvas::Stroke::default().with_color(rgb(0x63, 0x3D, 0x60)).with_width(1.0),
    );

    let segment_width = (status_panel.width - 12.0) / 3.0;
    for (i, label) in ["NUM", "CAPS", "SCROLL"].iter().enumerate() {
        let segment = Rectangle {
            x: status_panel.x + 6.0 + i as f32 * segment_width,
            y: status_panel.y + 6.0,
            width: segment_width,
            height: status_panel.height - 12.0,
        };
        let segment_path = rounded_rect(segment, 4.0);
        frame.fill(&segment_path, rgb(0x2C, 0x22, 0x35));
        frame.stroke(
            &segment_path,
            canvas::Stroke::default().with_color(rgb(0x72, 0x45, 0x6D)).with_width(0.75),
        );

        let x = segment.x + segment.width / 2.0;
        frame.fill(
            &Path::circle(Point::new(x, segment.y + 10.0), 3.0),
            if i == 0 { rgb(0x50, 0xD4, 0xC8) } else { rgb(0xF0, 0x68, 0x68) },
        );
        frame.fill_text(Text {
            content: (*label).to_string(),
            position: Point::new(x, segment.y + 25.0),
            color: rgb(0xC3, 0x9F, 0xB6),
            size: iced::Pixels(8.0),
            font: CANVAS_FONT,
            align_x: Horizontal::Center.into(),
            ..Text::default()
        });
    }

    for key in keys {
        let rect = key_rect_with_inset(key, inset_x);
        let lit_track = overlay_highlighted.and_then(|overlay| overlay.get(&key.id)).copied()
            .or_else(|| highlighted.get(&key.id).copied())
            .or_else(|| {
                (selected_controls.contains(&key.id) || pressed.contains(&key.id))
                    .then_some(usize::MAX)
            });

        if key.is_knob {
            let value = key.knob_index
                .and_then(|idx| knob_values.get(idx as usize))
                .copied()
                .unwrap_or(0.0);
            let label = key.knob_index
                .and_then(|idx| crate::synth::KNOB_PARAMS.get(idx as usize))
                .map(|param| param.label);
            draw_knob(frame, rect, value, label);
            continue;
        }

        let (fill, text_color, glow_color) = key_colors(key.cluster, lit_track);
        let radius = 7.0;

        let key_shadow = rounded_rect(
            Rectangle { x: rect.x, y: rect.y + 4.0, ..rect },
            radius,
        );
        frame.fill(&key_shadow, Color::from_rgba8(0x05, 0x04, 0x0A, 0.80));

        let path = rounded_rect(rect, radius);
        frame.fill(&path, fill);
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.32))
                .with_width(1.0),
        );

        if lit_track.is_some() {
            let glow = rounded_rect(
                Rectangle {
                    x: rect.x - 2.0,
                    y: rect.y - 2.0,
                    width: rect.width + 4.0,
                    height: rect.height + 4.0,
                },
                radius + 2.0,
            );
            frame.stroke(
                &glow,
                canvas::Stroke::default().with_color(glow_color).with_width(2.5),
            );
        }

        let drum_note = show_drum_symbols
            .then(|| {
                drum_note_to_key.iter()
                    .find_map(|(&note, &id)| (id == key.id).then_some(note))
            })
            .flatten();
        let projected_label = projected_labels.and_then(|labels| labels.get(&key.id));

        if let Some(note) = drum_note {
            draw_drum_symbol(frame, rect, note, text_color, projected_label.map(String::as_str));
            if let Some(steps) = play_order.and_then(|order| order.get(&key.id)) {
                draw_play_order_badge(frame, rect, steps);
            }
            continue;
        }

        let primary_label = projected_label.map(String::as_str).unwrap_or(key.label);
        let secondary_label = if projected_label.is_some() && key.midi_note.is_some() {
            Some(key.label)
        } else {
            key.sublabel
        };

        if !primary_label.is_empty() {
            frame.fill_text(Text {
                content: primary_label.to_string(),
                position: Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0 - if secondary_label.is_some() { 6.0 } else { 0.0 },
                ),
                color: text_color,
                size: iced::Pixels(if primary_label.len() > 4 { 11.0 } else { 14.0 }),
                font: CANVAS_FONT,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Text::default()
            });
        }

        if let Some(sub) = secondary_label {
            frame.fill_text(Text {
                content: sub.to_string(),
                position: Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0 + 12.0,
                ),
                color: Color::from_rgba8(0, 0, 0, 0.6),
                size: iced::Pixels(10.0),
                font: CANVAS_FONT,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Text::default()
            });
        }

        if let Some(steps) = play_order.and_then(|order| order.get(&key.id)) {
            draw_play_order_badge(frame, rect, steps);
        }
    }

    // Pop-up slider for whichever knob is currently held, drawn last so it
    // sits on top of the keys it temporarily overlaps.
    if let Some((idx, value)) = active_knob {
        if let Some(knob_rect) = keys.iter()
            .find(|key| key.knob_index == Some(idx))
            .map(|key| key_rect_with_inset(key, inset_x))
        {
            draw_knob_slider(frame, knob_rect, value, knob_readout(idx, value));
        }
    }
}

fn drum_pad_name(note: u8) -> Option<&'static str> {
    Some(match note {
        36 => "KICK",
        37 => "STICK",
        38 => "SNARE",
        39 => "CLAP",
        40 => "E.SNARE",
        41 => "LO FLOOR",
        42 => "CLOSED HH",
        43 => "HI FLOOR",
        44 => "PEDAL HH",
        45 => "LOW TOM",
        46 => "OPEN HH",
        47 => "LO-MID",
        48 => "HI-MID",
        49 => "CRASH",
        50 => "HIGH TOM",
        51 => "RIDE",
        52 => "CHINA",
        53 => "RIDE BELL",
        54 => "TAMBO",
        55 => "SPLASH",
        _ => return None,
    })
}

fn icon_line(frame: &mut Frame, from: Point, to: Point, color: Color, width: f32) {
    frame.stroke(
        &Path::line(from, to),
        canvas::Stroke::default().with_color(color).with_width(width),
    );
}

fn draw_drum_body(frame: &mut Frame, center: Point, color: Color, floor_legs: bool) {
    let body = rounded_rect(
        Rectangle { x: center.x - 9.0, y: center.y - 6.0, width: 18.0, height: 12.0 },
        3.0,
    );
    frame.stroke(
        &body,
        canvas::Stroke::default().with_color(color).with_width(1.7),
    );
    icon_line(
        frame,
        Point::new(center.x - 8.0, center.y - 2.0),
        Point::new(center.x + 8.0, center.y - 2.0),
        color,
        1.2,
    );
    if floor_legs {
        icon_line(frame, Point::new(center.x - 6.0, center.y + 6.0), Point::new(center.x - 8.0, center.y + 10.0), color, 1.5);
        icon_line(frame, Point::new(center.x + 6.0, center.y + 6.0), Point::new(center.x + 8.0, center.y + 10.0), color, 1.5);
    }
}

fn draw_hi_hat(frame: &mut Frame, center: Point, color: Color, open: bool, pedal: bool) {
    let spread = if open { 3.5 } else { 1.5 };
    icon_line(frame, Point::new(center.x - 9.0, center.y - spread), Point::new(center.x + 9.0, center.y - spread), color, 1.7);
    icon_line(frame, Point::new(center.x - 9.0, center.y + spread), Point::new(center.x + 9.0, center.y + spread), color, 1.7);
    icon_line(frame, Point::new(center.x, center.y + spread), Point::new(center.x, center.y + 10.0), color, 1.4);
    icon_line(frame, Point::new(center.x - 6.0, center.y + 10.0), Point::new(center.x + 6.0, center.y + 10.0), color, 1.4);
    if pedal {
        icon_line(frame, Point::new(center.x, center.y + 7.0), Point::new(center.x + 8.0, center.y + 10.0), color, 1.5);
    }
}

fn draw_cymbal(frame: &mut Frame, center: Point, color: Color, small: bool) {
    let half_width = if small { 7.0 } else { 11.0 };
    let cymbal = Path::new(|b| {
        b.move_to(Point::new(center.x - half_width, center.y));
        b.quadratic_curve_to(
            Point::new(center.x, center.y - 4.0),
            Point::new(center.x + half_width, center.y),
        );
    });
    frame.stroke(
        &cymbal,
        canvas::Stroke::default().with_color(color).with_width(1.8),
    );
    frame.fill(&Path::circle(Point::new(center.x, center.y - 2.0), 2.0), color);
    icon_line(frame, Point::new(center.x, center.y), Point::new(center.x, center.y + 10.0), color, 1.4);
    icon_line(frame, Point::new(center.x - 5.0, center.y + 10.0), Point::new(center.x + 5.0, center.y + 10.0), color, 1.4);
}

/// Draw a small pictogram plus a compact GM instrument name. The text keeps
/// closely-related toms and cymbals unambiguous, while the shapes remain easy
/// to scan during performance.
fn draw_drum_symbol(
    frame: &mut Frame,
    rect: Rectangle,
    note: u8,
    color: Color,
    computer_key: Option<&str>,
) {
    let Some(name) = drum_pad_name(note) else { return };
    let center = Point::new(rect.x + rect.width / 2.0, rect.y + 20.0);

    match note {
        36 => {
            let drum = Path::circle(center, 9.0);
            frame.stroke(&drum, canvas::Stroke::default().with_color(color).with_width(1.8));
            frame.fill(&Path::circle(center, 2.2), color);
            icon_line(frame, Point::new(center.x - 6.0, center.y + 7.0), Point::new(center.x - 9.0, center.y + 11.0), color, 1.4);
            icon_line(frame, Point::new(center.x + 6.0, center.y + 7.0), Point::new(center.x + 9.0, center.y + 11.0), color, 1.4);
        }
        37 => {
            icon_line(frame, Point::new(center.x - 8.0, center.y + 8.0), Point::new(center.x + 8.0, center.y - 8.0), color, 2.2);
            icon_line(frame, Point::new(center.x - 8.0, center.y - 8.0), Point::new(center.x + 8.0, center.y + 8.0), color, 2.2);
            frame.fill(&Path::circle(Point::new(center.x - 8.0, center.y + 8.0), 1.8), color);
            frame.fill(&Path::circle(Point::new(center.x + 8.0, center.y + 8.0), 1.8), color);
        }
        38 | 40 => {
            draw_drum_body(frame, center, color, false);
            if note == 40 {
                let bolt = Path::new(|b| {
                    b.move_to(Point::new(center.x + 1.0, center.y - 8.0));
                    b.line_to(Point::new(center.x - 2.0, center.y - 1.0));
                    b.line_to(Point::new(center.x + 2.0, center.y - 1.0));
                    b.line_to(Point::new(center.x - 1.0, center.y + 7.0));
                });
                frame.stroke(&bolt, canvas::Stroke::default().with_color(color).with_width(1.4));
            }
        }
        39 => {
            let hand = rounded_rect(
                Rectangle { x: center.x - 6.0, y: center.y - 1.0, width: 12.0, height: 10.0 },
                3.0,
            );
            frame.stroke(&hand, canvas::Stroke::default().with_color(color).with_width(1.5));
            for offset in [-6.0, -2.0, 2.0, 6.0] {
                icon_line(frame, Point::new(center.x + offset, center.y - 1.0), Point::new(center.x + offset, center.y - 9.0 + offset.abs() * 0.25), color, 1.5);
            }
        }
        41 | 43 => {
            draw_drum_body(frame, center, color, true);
            let dot_y = if note == 41 { center.y + 2.0 } else { center.y - 4.0 };
            frame.fill(&Path::circle(Point::new(center.x, dot_y), 1.8), color);
        }
        42 => draw_hi_hat(frame, center, color, false, false),
        44 => draw_hi_hat(frame, center, color, false, true),
        46 => draw_hi_hat(frame, center, color, true, false),
        45 | 47 | 48 | 50 => {
            draw_drum_body(frame, center, color, false);
            let dot_y = match note {
                45 => center.y + 3.0,
                47 => center.y + 1.0,
                48 => center.y - 2.0,
                _ => center.y - 4.0,
            };
            frame.fill(&Path::circle(Point::new(center.x, dot_y), 1.8), color);
        }
        49 => {
            draw_cymbal(frame, center, color, false);
            for (dx, dy) in [(-12.0, -5.0), (12.0, -5.0), (0.0, -11.0)] {
                icon_line(frame, Point::new(center.x + dx * 0.72, center.y + dy * 0.72), Point::new(center.x + dx, center.y + dy), color, 1.2);
            }
        }
        51 => draw_cymbal(frame, center, color, false),
        52 => {
            let china = Path::new(|b| {
                b.move_to(Point::new(center.x - 11.0, center.y - 3.0));
                b.quadratic_curve_to(Point::new(center.x, center.y + 5.0), Point::new(center.x + 11.0, center.y - 3.0));
            });
            frame.stroke(&china, canvas::Stroke::default().with_color(color).with_width(1.8));
            frame.fill(&Path::circle(Point::new(center.x, center.y), 2.0), color);
            icon_line(frame, Point::new(center.x, center.y + 1.0), Point::new(center.x, center.y + 10.0), color, 1.4);
        }
        53 => {
            let bell = Path::new(|b| {
                b.move_to(Point::new(center.x - 8.0, center.y + 6.0));
                b.quadratic_curve_to(Point::new(center.x - 5.0, center.y - 7.0), Point::new(center.x, center.y - 8.0));
                b.quadratic_curve_to(Point::new(center.x + 5.0, center.y - 7.0), Point::new(center.x + 8.0, center.y + 6.0));
                b.line_to(Point::new(center.x - 8.0, center.y + 6.0));
            });
            frame.stroke(&bell, canvas::Stroke::default().with_color(color).with_width(1.7));
            frame.fill(&Path::circle(Point::new(center.x, center.y + 8.0), 2.0), color);
        }
        54 => {
            let ring = Path::circle(center, 8.0);
            frame.stroke(&ring, canvas::Stroke::default().with_color(color).with_width(1.8));
            for (dx, dy) in [(0.0, -10.0), (9.0, -4.0), (9.0, 4.0), (0.0, 10.0), (-9.0, 4.0), (-9.0, -4.0)] {
                frame.fill(&Path::circle(Point::new(center.x + dx, center.y + dy), 1.7), color);
            }
        }
        55 => {
            draw_cymbal(frame, center, color, true);
            for (dx, dy) in [(-9.0, -7.0), (0.0, -11.0), (9.0, -7.0)] {
                icon_line(frame, Point::new(center.x + dx * 0.7, center.y + dy * 0.7), Point::new(center.x + dx, center.y + dy), color, 1.2);
            }
        }
        _ => {}
    }

    if let Some(key) = computer_key {
        frame.fill_text(Text {
            content: key.to_string(),
            position: Point::new(rect.x + 5.0, rect.y + 4.0),
            color: Color { a: 0.58, ..color },
            size: iced::Pixels(7.0),
            font: CANVAS_FONT,
            align_x: Horizontal::Left.into(),
            ..Text::default()
        });
    }

    frame.fill_text(Text {
        content: name.to_string(),
        position: Point::new(rect.x + rect.width / 2.0, rect.y + rect.height - 7.0),
        color,
        size: iced::Pixels(if name.len() > 8 { 7.0 } else { 8.0 }),
        font: CANVAS_FONT,
        align_x: Horizontal::Center.into(),
        align_y: Vertical::Center,
        ..Text::default()
    });
}

/// Draws a compact, high-contrast sequence badge in the key's upper-right
/// corner. A repeated key reads e.g. "2·5"; simultaneous chord keys all show
/// the same number.
fn draw_play_order_badge(frame: &mut Frame, key_rect: Rectangle, steps: &[usize]) {
    if steps.is_empty() { return; }

    let label = steps
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("·");
    let char_count = label.chars().count().max(1) as f32;
    let width = (char_count * 6.0 + 10.0).clamp(18.0, key_rect.width - 8.0);
    let height = 18.0;
    let badge = Rectangle {
        x: key_rect.x + key_rect.width - width - 4.0,
        y: key_rect.y + 4.0,
        width,
        height,
    };
    let path = rounded_rect(badge, height / 2.0);
    frame.fill(&path, Color::from_rgba8(0x12, 0x0E, 0x18, 0.92));
    frame.stroke(
        &path,
        canvas::Stroke::default()
            .with_color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.78))
            .with_width(1.0),
    );

    // Long repeated-note sequences shrink to stay inside their key rather
    // than spilling across adjacent labels.
    let font_size = (width / (char_count * 0.62)).clamp(6.0, 10.0);
    frame.fill_text(Text {
        content: label,
        position: Point::new(badge.x + badge.width / 2.0, badge.y + badge.height / 2.0),
        color: Color::WHITE,
        size: iced::Pixels(font_size),
        font: CANVAS_FONT,
        align_x: Horizontal::Center.into(),
        align_y: Vertical::Center,
        ..Text::default()
    });
}

/// Formats a knob's current position as "<label> <real value>" (e.g.
/// "Volume 1.00"), scaling the normalized 0.0..=1.0 dial position into the
/// parameter's real engine units via `KNOB_PARAMS`.
fn knob_readout(idx: u8, value: f32) -> Option<String> {
    crate::synth::KNOB_PARAMS.get(idx as usize).map(|param| {
        let real = param.min + value.clamp(0.0, 1.0) * (param.max - param.min);
        format!("{} {:.2}", param.label, real)
    })
}

fn draw_knob_slider(frame: &mut Frame, knob_rect: Rectangle, value: f32, readout: Option<String>) {
    let (top, bottom) = knob_slider_track(knob_rect);
    let center_x = knob_rect.x + knob_rect.width / 2.0;
    let value = value.clamp(0.0, 1.0);

    let panel = rounded_rect(
        Rectangle {
            x: center_x - 50.0,
            y: top - 24.0,
            width: 100.0,
            height: (bottom - top) + 34.0,
        },
        8.0,
    );
    frame.fill(&panel, Color::from_rgba8(0x10, 0x0F, 0x19, 0.97));
    frame.stroke(
        &panel,
        canvas::Stroke::default().with_color(rgb(0x9A, 0x54, 0x82)).with_width(1.0),
    );

    if let Some(readout) = readout {
        frame.fill_text(Text {
            content: readout,
            position: Point::new(center_x, top - 12.0),
            color: rgb(0xFF, 0xB5, 0x58),
            size: iced::Pixels(10.0),
            font: CANVAS_FONT,
            align_x: Horizontal::Center.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
    }

    let track = rounded_rect(
        Rectangle { x: center_x - 3.0, y: top, width: 6.0, height: bottom - top },
        3.0,
    );
    frame.fill(&track, rgb(0x24, 0x1E, 0x2D));

    let handle_y = top + (1.0 - value) * (bottom - top);
    let filled = rounded_rect(
        Rectangle { x: center_x - 3.0, y: handle_y, width: 6.0, height: bottom - handle_y },
        3.0,
    );
    frame.fill(&filled, rgb(0xFF, 0x76, 0x7B));

    let handle = Path::circle(Point::new(center_x, handle_y), 8.0);
    frame.fill(&handle, rgb(0xFF, 0x76, 0x7B));
    frame.stroke(&handle, canvas::Stroke::default().with_color(Color::WHITE).with_width(1.5));
}

fn rounded_rect(rect: Rectangle, radius: f32) -> Path {
    Path::new(|builder| {
        builder.rounded_rectangle(
            Point::new(rect.x, rect.y),
            Size::new(rect.width, rect.height),
            radius.into(),
        );
    })
}

fn draw_knob(frame: &mut Frame, rect: Rectangle, value: f32, label: Option<&str>) {
    let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let radius = rect.width.min(rect.height) / 2.0 - 4.0;

    frame.fill(
        &Path::circle(Point::new(center.x, center.y + 3.0), radius + 1.0),
        Color::from_rgba8(0, 0, 0, 0.45),
    );

    let base = Path::circle(center, radius);
    frame.fill(&base, rgb(0x13, 0x12, 0x1C));
    frame.stroke(
        &base,
        canvas::Stroke::default().with_color(rgb(0x6F, 0x3D, 0x61)).with_width(1.0),
    );

    let knurl = Path::circle(center, radius * 0.55);
    frame.fill(&knurl, rgb(0x24, 0x1E, 0x2D));

    // Sweeps -135°..+135° (bottom-left to bottom-right through the top) as
    // `value` goes 0.0..=1.0, mirroring a real knob's travel.
    let angle = -135f32.to_radians() + value.clamp(0.0, 1.0) * 270f32.to_radians();
    let (s, c) = (angle.sin(), angle.cos());
    let inner = radius * 0.35;
    let outer = radius * 0.85;
    let tip = Point::new(center.x + s * outer, center.y - c * outer);
    let pointer = Path::new(|b| {
        b.move_to(Point::new(center.x + s * inner, center.y - c * inner));
        b.line_to(tip);
    });
    frame.stroke(
        &pointer,
        canvas::Stroke::default().with_color(rgb(0xFF, 0x76, 0x7B)).with_width(2.5),
    );
    frame.fill(&Path::circle(tip, 2.5), rgb(0xFF, 0x76, 0x7B));

    if let Some(label) = label {
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(center.x, rect.y + rect.height + 9.0),
            color: rgb(0xC3, 0x9F, 0xB6),
            size: iced::Pixels(8.0),
            font: CANVAS_FONT,
            align_x: Horizontal::Center.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_side_padding_collapses_on_narrow_canvases() {
        assert_eq!(board_inset_x(1484.0), 77.0);
        assert_eq!(board_inset_x(1400.0), 35.0);
        assert_eq!(board_inset_x(1200.0), MIN_BOARD_INSET_X);
    }

    #[test]
    fn every_drum_pad_has_a_distinct_display_name() {
        let names: Vec<&str> = (36..=55).filter_map(drum_pad_name).collect();
        let distinct: HashSet<&str> = names.iter().copied().collect();

        assert_eq!(names.len(), 20);
        assert_eq!(distinct.len(), 20);
        assert_eq!(drum_pad_name(35), None);
        assert_eq!(drum_pad_name(56), None);
    }
}
