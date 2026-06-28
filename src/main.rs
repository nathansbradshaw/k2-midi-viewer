mod key;
mod layout;
mod render;

use std::collections::HashSet;

use iced::widget::canvas::Canvas;
use iced::widget::container;
use iced::{Element, Length, Size, Theme};

use key::KeyId;
use layout::build_layout;
use render::BoardCanvas;

const BOARD_PAD: f32 = 24.0;

fn main() -> iced::Result {
    iced::application("Macropad Visualizer", App::update, App::view)
        .theme(|_| Theme::Dark)
        .window_size(Size::new(1520.0, 480.0))
        .run()
}

struct App {
    keys: Vec<key::Key>,
    highlighted: HashSet<KeyId>,
}

impl Default for App {
    fn default() -> Self {
        App {
            keys: build_layout(),
            highlighted: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Toggle(KeyId),
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::Toggle(id) => {
                if !self.highlighted.remove(&id) {
                    self.highlighted.insert(id);
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let canvas = Canvas::new(BoardCanvas {
            keys: &self.keys,
            highlighted: &self.highlighted,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        container(canvas)
            .padding(BOARD_PAD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
