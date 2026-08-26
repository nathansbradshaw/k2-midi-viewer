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
/// Pixels of vertical drag needed to sweep a knob across its full 0.0..=1.0 range.
const KNOB_DRAG_RANGE_PX: f32 = 150.0;

fn board_inset_x(width: f32) -> f32 {
    ((width - BOARD_LAYOUT_WIDTH) / 2.0).max(MIN_BOARD_INSET_X)
}

pub struct BoardCanvas<'a> {
    pub keys:        &'a [Key],
    /// Maps KeyId → track index for color. usize::MAX = manually toggled (uses original colour).
    pub highlighted: &'a HashMap<KeyId, usize>,
    /// A persistent control selection, independent of playback note overlays.
    pub selected_control: Option<KeyId>,
    /// Keys currently held with the pointer.
    pub pressed: &'a HashSet<KeyId>,
    /// Computer-key labels shown over the note names in performance mode.
    pub projected_labels: Option<&'a HashMap<KeyId, String>>,
    /// Current 0.0..=1.0 dial position of each of the 12 encoder knobs.
    pub knob_values: &'a [f32],
}

#[derive(Default)]
pub struct CanvasState {
    cache: canvas::Cache,
    pressed: Option<KeyId>,
    dragging_knob: Option<u8>,
    drag_last_y: f32,
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
                                state.drag_last_y = pos.y;
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
                        let delta = (state.drag_last_y - pos.y) / KNOB_DRAG_RANGE_PX;
                        state.drag_last_y = pos.y;
                        let current = self.knob_values.get(idx as usize).copied().unwrap_or(0.0);
                        let value = (current + delta).clamp(0.0, 1.0);
                        return Some(
                            canvas::Action::publish(Message::KnobChanged(idx, value))
                                .and_capture(),
                        );
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
                self.selected_control,
                self.pressed,
                self.projected_labels,
                self.knob_values,
                active_knob,
            );
        });
        vec![geometry]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(pos) = cursor.position_in(bounds) {
            let inset_x = board_inset_x(bounds.width);
            for key in self.keys {
                if key_rect_with_inset(key, inset_x).contains(pos) {
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
    selected_control: Option<KeyId>,
    pressed: &HashSet<KeyId>,
    projected_labels: Option<&HashMap<KeyId, String>>,
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

    frame.fill_text(Text {
        content: "K2  /  MIDI PERFORMANCE KEYBOARD".to_string(),
        position: Point::new(inset_x + 8.45 * (UNIT + GAP), BOARD_INSET_Y + 12.0),
        color: rgb(0xF0, 0xC9, 0x9A),
        size: iced::Pixels(12.0),
        font: CANVAS_FONT,
        ..Text::default()
    });
    let display_bezel = rounded_rect(
        Rectangle {
            x: inset_x + 12.35 * (UNIT + GAP),
            y: BOARD_INSET_Y + 4.0,
            width: 2.55 * (UNIT + GAP),
            height: 36.0,
        },
        5.0,
    );
    frame.fill(&display_bezel, rgb(0x14, 0x11, 0x1B));
    let display = rounded_rect(
        Rectangle {
            x: inset_x + 12.55 * (UNIT + GAP),
            y: BOARD_INSET_Y + 11.0,
            width: 2.15 * (UNIT + GAP),
            height: 21.0,
        },
        3.0,
    );
    frame.fill(&display, rgb(0x28, 0x17, 0x18));
    let display_text = active_knob
        .and_then(|(idx, value)| {
            crate::synth::KNOB_PARAMS.get(idx as usize).map(|param| {
                let real = param.min + value.clamp(0.0, 1.0) * (param.max - param.min);
                format!("{} {:.2}", param.label, real)
            })
        })
        .unwrap_or_else(|| "MIDI READY".to_string());
    frame.fill_text(Text {
        content: display_text,
        position: Point::new(inset_x + 13.63 * (UNIT + GAP), BOARD_INSET_Y + 21.5),
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
        let lit_track = highlighted.get(&key.id).copied()
            .or_else(|| {
                (Some(key.id) == selected_control || pressed.contains(&key.id))
                    .then_some(usize::MAX)
            });

        if key.is_knob {
            let value = key.knob_index
                .and_then(|idx| knob_values.get(idx as usize))
                .copied()
                .unwrap_or(0.0);
            draw_knob(frame, rect, value);
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

        let projected_label = projected_labels.and_then(|labels| labels.get(&key.id));
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
    }
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

fn draw_knob(frame: &mut Frame, rect: Rectangle, value: f32) {
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
}
