use std::collections::HashSet;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::key::{Cluster, Key, KeyId};
use crate::Message;

pub const UNIT: f32 = 54.0;
pub const GAP: f32 = 4.0;

pub struct BoardCanvas<'a> {
    pub keys: &'a [Key],
    pub highlighted: &'a HashSet<KeyId>,
}

#[derive(Default)]
pub struct CanvasState {
    cache: canvas::Cache,
}

impl<'a> canvas::Program<Message> for BoardCanvas<'a> {
    type State = CanvasState;

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            if let Some(pos) = cursor.position_in(bounds) {
                for key in self.keys {
                    if key_rect(key).contains(pos) {
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::Toggle(key.id)),
                        );
                    }
                }
            }
        }
        (canvas::event::Status::Ignored, None)
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
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            draw_board(frame, self.keys, self.highlighted);
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
            for key in self.keys {
                if key_rect(key).contains(pos) {
                    return mouse::Interaction::Pointer;
                }
            }
        }
        mouse::Interaction::default()
    }
}

pub fn key_rect(key: &Key) -> Rectangle {
    Rectangle {
        x: key.col * (UNIT + GAP),
        y: key.row * (UNIT + GAP),
        width: key.w * UNIT + (key.w - 1.0).max(0.0) * GAP,
        height: key.h * UNIT + (key.h - 1.0).max(0.0) * GAP,
    }
}

fn cluster_colors(cluster: Cluster, lit: bool) -> (Color, Color) {
    match (cluster, lit) {
        (Cluster::Alpha, false) => (rgb(0x80, 0x8E, 0x62), Color::BLACK),
        (Cluster::Alpha, true) => (rgb(0x9B, 0xE6, 0x6B), Color::BLACK),
        (Cluster::AlphaLight, false) => (rgb(0xCC, 0xD8, 0xBC), Color::BLACK),
        (Cluster::AlphaLight, true) => (rgb(0x9B, 0xE6, 0x6B), Color::BLACK),
        (Cluster::Nav, false) => (rgb(0xCE, 0xCB, 0xC2), Color::BLACK),
        (Cluster::Nav, true) => (rgb(0xF5, 0xD9, 0x5E), Color::BLACK),
        (Cluster::Arrow, false) => (rgb(0xD8, 0x8B, 0x66), Color::BLACK),
        (Cluster::Arrow, true) => (rgb(0xFF, 0xA9, 0x4D), Color::BLACK),
        (Cluster::Numpad, false) => (rgb(0xD8, 0xD5, 0xCB), Color::BLACK),
        (Cluster::Numpad, true) => (rgb(0xF5, 0xD9, 0x5E), Color::BLACK),
        (Cluster::Encoder, _) => (rgb(0xB8, 0xBE, 0xC2), Color::BLACK),
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn draw_board(frame: &mut Frame, keys: &[Key], highlighted: &HashSet<KeyId>) {
    let bg = Path::rectangle(Point::ORIGIN, frame.size());
    frame.fill(&bg, rgb(0x0A, 0x0A, 0x0A));

    let pcb_green = Path::rectangle(
        Point::new(0.0, 0.0),
        Size::new(16.0 * (UNIT + GAP), 6.0 * (UNIT + GAP)),
    );
    frame.fill(&pcb_green, rgb(0x16, 0x6B, 0x3A));

    let pcb_red = Path::rectangle(
        Point::new(16.0 * (UNIT + GAP), 0.0),
        Size::new(9.5 * (UNIT + GAP), 6.0 * (UNIT + GAP)),
    );
    frame.fill(&pcb_red, rgb(0x80, 0x1E, 0x1C));

    for key in keys {
        let rect = key_rect(key);
        let lit = highlighted.contains(&key.id);

        if key.is_knob {
            draw_knob(frame, rect);
            continue;
        }

        let (fill, text_color) = cluster_colors(key.cluster, lit);
        let radius = 8.0;

        let path = rounded_rect(rect, radius);
        frame.fill(&path, fill);

        if lit {
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
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xFF, 0xF3, 0x9C, 0.9))
                    .with_width(2.5),
            );
        }

        if !key.label.is_empty() {
            frame.fill_text(Text {
                content: key.label.to_string(),
                position: Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0 - if key.sublabel.is_some() { 6.0 } else { 0.0 },
                ),
                color: text_color,
                size: iced::Pixels(14.0),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }

        if let Some(sub) = key.sublabel {
            frame.fill_text(Text {
                content: sub.to_string(),
                position: Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0 + 12.0,
                ),
                color: Color::from_rgba8(0, 0, 0, 0.6),
                size: iced::Pixels(10.0),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
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

fn draw_knob(frame: &mut Frame, rect: Rectangle) {
    let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let radius = rect.width.min(rect.height) / 2.0 - 4.0;

    let base = Path::circle(center, radius);
    frame.fill(&base, rgb(0xC4, 0xC8, 0xCB));

    let knurl = Path::circle(center, radius * 0.55);
    frame.fill(&knurl, rgb(0x8A, 0x8E, 0x91));

    let notch = Path::new(|b| {
        b.move_to(Point::new(center.x, center.y - radius * 0.8));
        b.line_to(Point::new(center.x - 3.0, center.y - radius * 0.4));
        b.line_to(Point::new(center.x + 3.0, center.y - radius * 0.4));
        b.close();
    });
    frame.fill(&notch, rgb(0x3A, 0x3D, 0x3F));
}
