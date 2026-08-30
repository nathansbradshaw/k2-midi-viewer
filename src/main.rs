mod drums;
mod key;
mod key_geometry;
mod layout;
mod midi;
#[cfg(not(target_arch = "wasm32"))]
mod playback;
#[cfg(target_arch = "wasm32")]
#[path = "playback_web.rs"]
mod playback;
mod render;
mod staff;
mod synth;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use iced::advanced::svg;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Image as CanvasImage, Path};
use iced::widget::image::{self, FilterMethod};
use iced::widget::{
    Space, Stack, button, column, container, image as image_widget, pick_list, row, scrollable,
    slider, text, tooltip,
};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Length, Padding, Point, Radians,
    Rectangle, Renderer, Shadow, Size, Subscription, Task, Theme, Vector, mouse,
};

use key::{Cluster, Key, KeyId};
use layout::build_layout;
use playback::{PlayCmd, PlayEvent, PlaybackHandle};
use render::{BoardCanvas, BoardResizeHandle, PhotoBoardAssets};
use staff::StaffCanvas;

const SEEK_STEP: f32 = 0.0001;

const SPEED_MIN: f32 = 0.25;
const SPEED_MAX: f32 = 2.0;

/// Signals the HTML loading screen (see `index.html`) to hide once the app
/// has actually rendered its first frame. Wasm instantiation finishing is
/// not enough of a signal on its own: the wgpu/WebGL adapter and device are
/// set up asynchronously afterward, and that gap is where the "boot takes a
/// long time" complaint came from — the loading screen was gone but the
/// canvas stayed blank for several more seconds.
#[cfg(target_arch = "wasm32")]
mod boot {
    use std::sync::atomic::{AtomicBool, Ordering};

    static NOTIFIED: AtomicBool = AtomicBool::new(false);

    pub fn notify_first_frame() {
        if NOTIFIED.swap(true, Ordering::Relaxed) {
            return;
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        if let Some(el) = document.get_element_by_id("loading-screen") {
            let _ = el.class_list().add_1("hidden");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod boot {
    pub fn notify_first_frame() {}
}

/// Wood-grain and dark-grain equipment-panel textures for the surrounding UI
/// chrome, decoded once and cached — mirrors [`PhotoBoardAssets`]'s pattern.
struct ChromeAssets {
    wood_grain: image::Handle,
    dark_grain: image::Handle,
    panel_wear: image::Handle,
}

impl ChromeAssets {
    fn new() -> Self {
        Self {
            wood_grain: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/wood-grain.png")[..],
            ),
            dark_grain: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/dark-panel-grain.png")[..],
            ),
            panel_wear: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/panel-wear-overlay-runtime.png")[..],
            ),
        }
    }
}

/// The shared physical-control asset family (see
/// `assets/keyboard/controls/README.md`): one rotary knob face, one fader
/// cap, one neutral LED lens, a matched rocker-switch pair, and the label
/// plate / LCD glass overlays. Decoded once and cloned per view, mirroring
/// `ChromeAssets`.
struct ControlAssets {
    rotary_knob_face: image::Handle,
    fader_cap: image::Handle,
    fader_track: svg::Handle,
    led_jewel: image::Handle,
    transport_keycap: image::Handle,
    panel_keycap_wide: image::Handle,
    panel_keycap_square: image::Handle,
    action_keycap_neutral: image::Handle,
    action_keycap_salmon: image::Handle,
    display_bezel: svg::Handle,
    // Pre-rasterized from the shared rocker-switch-{off,on}.svg pair (kept
    // alongside, unmodified) rather than drawn live at icon scale: their
    // fine plastic-grain noise filter aliases into rainbow static at
    // ~15-30px in every renderer tested, resvg included — it needs the
    // supersampled-then-downsampled raster a real asset pipeline would bake,
    // the same treatment already given the other photographic PNG assets.
    rocker_off: image::Handle,
    rocker_on: image::Handle,
    horizontal_switch_off: image::Handle,
    horizontal_switch_on: image::Handle,
    roller_selector: image::Handle,
    panel_screw: image::Handle,
    icon_map_rows: svg::Handle,
    icon_all_notes: svg::Handle,
    icon_computer_keys: svg::Handle,
    icon_drum: svg::Handle,
    icon_board: svg::Handle,
    icon_reset_pitch: svg::Handle,
    label_plate: image::Handle,
    lcd_glass: svg::Handle,
}

impl ControlAssets {
    fn new() -> Self {
        Self {
            rotary_knob_face: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/rotary-knob-face-runtime.png")[..],
            ),
            fader_cap: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/fader-cap-runtime.png")[..],
            ),
            fader_track: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/fader-track.svg")[..],
            ),
            led_jewel: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/led-jewel-unlit-runtime.png")[..],
            ),
            transport_keycap: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/transport-keycap-photo-runtime.png"
                )[..],
            ),
            panel_keycap_wide: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/button-keycap-neutral-v3-runtime.png"
                )[..],
            ),
            panel_keycap_square: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/button-keycap-neutral-v3-runtime.png"
                )[..],
            ),
            action_keycap_neutral: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/button-keycap-neutral-v3-runtime.png"
                )[..],
            ),
            action_keycap_salmon: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/button-keycap-salmon-v3-runtime.png"
                )[..],
            ),
            display_bezel: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/display-bezel-wide.svg")[..],
            ),
            rocker_off: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/rocker-switch-off.png")[..],
            ),
            rocker_on: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/rocker-switch-on.png")[..],
            ),
            horizontal_switch_off: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/horizontal-switch-off-runtime.png"
                )[..],
            ),
            horizontal_switch_on: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/horizontal-switch-on-runtime.png"
                )[..],
            ),
            roller_selector: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/roller-selector-photo-runtime.png"
                )[..],
            ),
            panel_screw: image::Handle::from_bytes(
                &include_bytes!("../assets/keyboard/controls/panel-screw-photo-runtime.png")[..],
            ),
            icon_map_rows: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-map-rows.svg")[..],
            ),
            icon_all_notes: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-all-notes.svg")[..],
            ),
            icon_computer_keys: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-computer-keys.svg")[..],
            ),
            icon_drum: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-drum.svg")[..],
            ),
            icon_board: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-board.svg")[..],
            ),
            icon_reset_pitch: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/icon-reset-pitch.svg")[..],
            ),
            label_plate: image::Handle::from_bytes(
                &include_bytes!(
                    "../assets/keyboard/controls/track-label-plate-photo-runtime.png"
                )[..],
            ),
            lcd_glass: svg::Handle::from_memory(
                &include_bytes!("../assets/keyboard/controls/lcd-glass-overlay-wide.svg")[..],
            ),
        }
    }
}

// Styled after the workshop-built instrument itself: wood-cheeked vintage
// electronic gear — molded dark control panels, an amber LCD readout, and
// the salmon accent carried over from the board's own arrow cluster.
const APP_BG: Color = Color::from_rgb8(0x09, 0x0a, 0x08);
// Three panel shades from darkest to lightest — charcoal rather than
// near-black, so the dark-grain texture layered over them (see
// `textured_panel`) actually reads at close range instead of looking flat.
const PANEL_BG_DARK: Color = Color::from_rgb8(0x16, 0x17, 0x13);
const PANEL_BG: Color = Color::from_rgb8(0x1c, 0x1d, 0x17);
const PANEL_BORDER: Color = Color::from_rgb8(0x3a, 0x39, 0x2e);
const TEXT_MAIN: Color = Color::from_rgb8(0xde, 0xd7, 0xc4);
const TEXT_MUTED: Color = Color::from_rgb8(0x91, 0x8c, 0x7d);
const ACCENT: Color = Color::from_rgb(0.830, 0.405, 0.330);
// Amber LCD display — status readout in the header.
const LCD_BG: Color = Color::from_rgb8(0x07, 0x08, 0x04);
const LCD_TEXT: Color = Color::from_rgb8(0xe9, 0x9b, 0x24);
const LCD_BORDER: Color = Color::from_rgb8(0x2a, 0x22, 0x10);
// A narrow dark bezel around the keyboard and staff stages — intentional
// framing so each reads as mounted hardware, not an image viewer. Shared and
// kept tight so it doesn't reopen the empty-space problem the sizing math
// (see `width_limited` in `view()`) already accounts for.
const CHROME_BEZEL: f32 = 5.0;
const BEZEL_BG: Color = Color::from_rgb8(0x08, 0x09, 0x07);
// Warm near-blacks — used in place of neutral/pure black on borders and
// recessed fills so every dark surface in the web chrome stays in the same
// charcoal-brown family as the board's own molded materials, rather than
// switching to a colder, razor-sharp black the instant a border darkens.
const WARM_BLACK_DEEP: Color = Color::from_rgb8(0x12, 0x0d, 0x08);

fn app_theme() -> Theme {
    Theme::custom(
        "K2".to_string(),
        iced::theme::Palette {
            background: APP_BG,
            text: TEXT_MAIN,
            primary: ACCENT,
            success: Color::from_rgb(0.22, 0.76, 0.70),
            warning: Color::from_rgb(1.0, 0.72, 0.25),
            danger: Color::from_rgb(0.98, 0.27, 0.44),
        },
    )
}

fn app_theme_for(_: &App) -> Theme {
    app_theme()
}

#[cfg(not(target_arch = "wasm32"))]
fn app_icon() -> iced::window::Icon {
    iced::window::icon::from_rgba(
        include_bytes!("../assets/desktop/k2-app-icon-v3.rgba").to_vec(),
        256,
        256,
    )
    .expect("the embedded K2 app icon must be 256x256 RGBA")
}

/// Wraps `content` as a distinct hardware module: a solid charcoal base +
/// low-opacity dark-grain texture sit *under* it, and thin top-highlight /
/// bottom-shadow hairlines sit *inside* it at the very top and bottom edge.
///
/// `content`'s own (padded, `Shrink`) size drives the whole stack's size —
/// via [`iced::widget::Stack::push`] first, so it becomes the base layer —
/// so the texture/color layers underneath exactly fill that box via
/// [`iced::widget::Stack::push_under`] instead of risking a `Length::Fill`
/// child inflating the surrounding layout.
fn textured_panel<'a>(
    content: impl Into<Element<'a, Message>>,
    bg: Color,
    dark_grain: image::Handle,
    panel_wear: image::Handle,
) -> Element<'a, Message> {
    let hairline = |color: Color| {
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(move |_: &Theme| container::Style {
                background: Some(Background::Color(color)),
                ..Default::default()
            })
    };
    let framed = column![
        hairline(Color::from_rgba8(0xf3, 0xe8, 0xcf, 0.07)),
        content.into(),
        hairline(WARM_BLACK_DEEP.scale_alpha(0.55)),
    ]
    .spacing(0);

    let color_base = container(text(""))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                color: PANEL_BORDER,
                width: 1.0,
                radius: 3.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 0.0,
            },
            ..Default::default()
        });
    let grain = image_widget(dark_grain)
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Cover)
        .opacity(0.10f32);
    let wear = image_widget(panel_wear)
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Cover)
        .opacity(0.075f32);

    Stack::new()
        .push(framed)
        .push_under(wear)
        .push_under(grain)
        .push_under(color_base)
        .into()
}

/// Adds only breathing room around a logical cluster. The photographed keys
/// provide their own physical edges, so another rectangular CSS frame here
/// would make the strip read as nested interface boxes instead of hardware.
fn control_cluster<'a>(
    content: impl Into<Element<'a, Message>>,
    compact: bool,
) -> Element<'a, Message> {
    let padding = if compact {
        Padding::from([1.0, 3.0])
    } else {
        Padding::from([4.0, 6.0])
    };
    container(content).padding(padding).into()
}

/// A shallow groove pressed into the shared upper chassis. Unlike a framed
/// card edge, this divides functional zones without making each row look like
/// a separate browser panel.
fn module_divider<'a>() -> Element<'a, Message> {
    column![
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(WARM_BLACK_DEEP.scale_alpha(0.72))),
                ..Default::default()
            }),
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba8(
                    0xf3, 0xe8, 0xcf, 0.045,
                ))),
                ..Default::default()
            }),
    ]
    .spacing(0)
    .into()
}

/// Shared narrow-bezel frame for the keyboard and staff stages — dark,
/// bordered, no texture (kept distinct from `textured_panel`'s equipment
/// surfaces since neither the board nor the CRT should compete with grain).
fn bezel_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BEZEL_BG)),
        border: Border {
            color: PANEL_BORDER,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// Deeper inset treatment for the notation display. The outer visualizer
/// housing carries the panel grain; this inner surface stays clean and dark
/// so the amber grid and notation remain legible.
fn crt_screen_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x08, 0x09, 0x04))),
        border: Border {
            color: Color::from_rgb8(0x2b, 0x24, 0x18),
            width: 3.0,
            radius: 4.0.into(),
        },
        shadow: Shadow {
            color: WARM_BLACK_DEEP.scale_alpha(0.8),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 0.0,
        },
        ..Default::default()
    }
}

fn mixer_strip_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_BG)),
        border: Border {
            color: PANEL_BORDER,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 0.0,
        },
        ..Default::default()
    }
}

/// Amber-on-black status readout — a recessed display let into the panel,
/// not another equipment-panel surface, so it gets its own darker treatment.
/// A thicker, darker border than a flush panel control reads as a deeper
/// inset — the display is let *into* the panel, not sitting on it.
fn lcd_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(LCD_BG)),
        border: Border {
            color: LCD_BORDER,
            width: 2.0,
            radius: 3.0.into(),
        },
        shadow: Shadow {
            color: WARM_BLACK_DEEP.scale_alpha(0.65),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 0.0,
        },
        ..Default::default()
    }
}

fn control_style(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, border_color) = match status {
        button::Status::Active => (
            Color::from_rgb(0.155, 0.160, 0.135),
            TEXT_MAIN,
            Color::from_rgb(0.355, 0.350, 0.285),
        ),
        button::Status::Hovered => (
            Color::from_rgb(0.225, 0.225, 0.180),
            Color::from_rgb8(0xf7, 0xf0, 0xe0),
            Color::from_rgb(0.520, 0.495, 0.385),
        ),
        button::Status::Pressed => (Color::from_rgb(0.080, 0.085, 0.070), TEXT_MAIN, ACCENT),
        button::Status::Disabled => (
            Color::from_rgb(0.095, 0.100, 0.085),
            TEXT_MUTED,
            Color::from_rgb(0.180, 0.180, 0.150),
        ),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow {
            color: WARM_BLACK_DEEP.scale_alpha(0.4),
            offset: Vector::new(
                0.0,
                if status == button::Status::Pressed {
                    0.0
                } else {
                    3.0
                },
            ),
            blur_radius: 0.0,
        },
        snap: false,
    }
}

/// A visually subordinate control — same interaction as `control_style`, but
/// near-transparent at rest so a strip of these (the header's secondary
/// pitch/toggle row) doesn't compete with the LCD or transport for attention.
#[cfg(target_arch = "wasm32")]
fn secondary_control_style(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, border_color) = match status {
        button::Status::Active => (
            Color::TRANSPARENT,
            TEXT_MUTED,
            PANEL_BORDER.scale_alpha(0.5),
        ),
        button::Status::Hovered => (PANEL_BG_DARK, TEXT_MAIN, PANEL_BORDER),
        button::Status::Pressed => (WARM_BLACK_DEEP, TEXT_MAIN, ACCENT),
        button::Status::Disabled => (
            Color::TRANSPARENT,
            TEXT_MUTED.scale_alpha(0.5),
            Color::TRANSPARENT,
        ),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// A 1-indexed MIDI channel number for display in a [`pick_list`] dropdown,
/// labeled with `prefix` (e.g. "CH" or "PLAY CH") to match the control it
/// replaces.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ChannelOption {
    prefix: &'static str,
    channel: u8,
}

impl std::fmt::Display for ChannelOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.prefix, self.channel)
    }
}

fn channel_options(prefix: &'static str) -> Vec<ChannelOption> {
    (1..=16u8)
        .map(|channel| ChannelOption { prefix, channel })
        .collect()
}

/// The octave selected in the physical Keyboard Keyboard's Settings menu.
/// The firmware defines its untransposed Wicki-Hayden map at octave 4, then
/// shifts outgoing melodic MIDI by `(octave - 4) * 12`. Mirroring that setting
/// here lets incoming notes be associated with the physical switch that sent
/// them, even when the shifted pitch also exists on another keyboard row.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq, Eq)]
struct MidiInputOctaveOption(i8);

#[cfg(target_arch = "wasm32")]
impl std::fmt::Display for MidiInputOctaveOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IN OCT {}", self.0)
    }
}

#[cfg(target_arch = "wasm32")]
fn midi_input_octave_options() -> Vec<MidiInputOctaveOption> {
    (MIN_MIDI_INPUT_OCTAVE..=MAX_MIDI_INPUT_OCTAVE)
        .map(MidiInputOctaveOption)
        .collect()
}

/// A per-track octave shift for display in a [`pick_list`] dropdown, mirroring
/// [`ChannelOption`]. Octave-only (not semitones) — layered on top of the
/// whole-song pitch/octave controls for a track that needs its own register.
#[derive(Clone, Copy, PartialEq, Eq)]
struct TrackOctaveOption(i8);

impl std::fmt::Display for TrackOctaveOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            write!(f, "OCT ±0")
        } else {
            write!(f, "OCT {:+}", self.0)
        }
    }
}

fn track_octave_options() -> Vec<TrackOctaveOption> {
    (-3..=3i8).map(TrackOctaveOption).collect()
}

#[cfg(target_arch = "wasm32")]
fn channel_pick_list_style(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let (background, border_color) = match status {
        pick_list::Status::Active => (PANEL_BG_DARK, PANEL_BORDER),
        pick_list::Status::Hovered => (
            Color::from_rgb(0.225, 0.225, 0.180),
            Color::from_rgb(0.520, 0.495, 0.385),
        ),
        pick_list::Status::Opened { .. } => (Color::from_rgb(0.080, 0.085, 0.070), ACCENT),
    };
    pick_list::Style {
        text_color: TEXT_MAIN,
        placeholder_color: TEXT_MUTED,
        handle_color: TEXT_MUTED,
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 3.0.into(),
        },
    }
}

/// A real transport key: the photographed PNG supplies the worn molded face
/// while the button remains the accessible input layer. The small jewel is
/// rendered separately so Play, Pause, and Stop can share one keycap master.
fn transport_key<'a>(
    assets: &ControlAssets,
    label: &'static str,
    lamp: Option<Color>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let lamp = led_jewel(
        assets,
        lamp.unwrap_or(Color::from_rgb8(0x2a, 0x29, 0x22)),
        lamp.is_some(),
    );
    let control = button(
        row![
            lamp,
            text(label)
                .size(13)
                .color(Color::from_rgb8(0x20, 0x1f, 0x19)),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([8, 11])
    .style(transparent_control_style)
    .on_press_maybe(on_press);

    let key = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(control)
        .push_under(
            Canvas::new(PhotoSurface {
                handle: assets.transport_keycap.clone(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        );

    container(key)
        .width(Length::Fixed(76.0))
        .height(Length::Fixed(40.0))
        .padding(2)
        .style(key_socket_style)
        .into()
}

fn panel_key_style(active: bool, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgba8(0xe9, 0xdf, 0xc8, 0.08),
        button::Status::Pressed => Color::from_rgba8(0x08, 0x07, 0x05, 0.32),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    let text_color = match status {
        button::Status::Disabled => TEXT_MUTED.scale_alpha(0.48),
        _ if active => ACCENT,
        _ => TEXT_MAIN.scale_alpha(0.86),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// A deep mounting socket reserved for the large ivory transport keys. Their
/// tall rounded shells visibly sit inside it; the shallow header/utility caps
/// mount flush to the panel instead of gaining a heavy black UI border.
fn key_socket_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x0b, 0x0c, 0x09))),
        border: Border {
            color: Color::from_rgba8(0xd8, 0xc9, 0xa7, 0.10),
            width: 1.0,
            radius: 2.0.into(),
        },
        shadow: Shadow {
            color: WARM_BLACK_DEEP.scale_alpha(0.75),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 0.0,
        },
        ..Default::default()
    }
}

/// A small photographed molded key with live text and interaction layered
/// above it. Active toggles gain a real jewel lamp instead of a glowing CSS
/// outline, while every label continues to be accessible and state-driven.
fn panel_key(
    assets: &ControlAssets,
    label: impl Into<String>,
    active: bool,
    on_press: Option<Message>,
    padding: Padding,
) -> Element<'static, Message> {
    let label = text(label.into()).size(12);
    let face: Element<Message> = if active {
        row![
            Canvas::new(LedJewel {
                handle: assets.led_jewel.clone(),
                color: ACCENT,
                lit: true,
            })
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0)),
            label,
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        label.into()
    };
    let control = button(face)
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(30.0))
        .padding(padding)
        .style(move |_: &Theme, status| panel_key_style(active, status))
        .on_press_maybe(on_press);

    Stack::new()
        .push(control)
        .push_under(
            Canvas::new(PhotoSurface {
                handle: assets.panel_keycap_square.clone(),
            })
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .into()
}

/// Shared tooltip treatment for the compact physical controls. Tooltips are
/// overlays, so adding help text never changes the console's height budget.
fn hardware_tooltip<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(label.into()).size(11).color(TEXT_MAIN))
            .max_width(260.0)
            .padding([5, 8]),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .delay(std::time::Duration::from_millis(350))
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(WARM_BLACK_DEEP)),
        border: Border {
            color: PANEL_BORDER,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        ..Default::default()
    })
    .into()
}

/// Icon-only companion to [`panel_key`], used for the reset control where a
/// short hardware symbol is clearer and more cohesive than a wide text key.
fn icon_panel_key(
    assets: &ControlAssets,
    icon: svg::Handle,
    on_press: Option<Message>,
    hint: impl Into<String>,
) -> Element<'static, Message> {
    let control = button(
        container(
            iced::widget::svg(icon)
                .width(Length::Fixed(16.0))
                .height(Length::Fixed(16.0)),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    )
    .width(Length::Fixed(36.0))
    .height(Length::Fixed(30.0))
    .padding(0)
    .style(move |_: &Theme, status| panel_key_style(false, status))
    .on_press_maybe(on_press);
    let key: Element<Message> = Stack::new()
        .push(control)
        .push_under(
            Canvas::new(PhotoSurface {
                handle: assets.panel_keycap_square.clone(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .into();
    hardware_tooltip(key, hint)
}

/// Compact icon + micro-legend utility key. Binary controls keep a jewel in
/// a fixed slot so toggling never changes width; the row-mapping selector can
/// omit the lamp and use its visible value as the state indicator instead.
fn utility_key(
    assets: &ControlAssets,
    icon: svg::Handle,
    legend: impl Into<String>,
    lamp: Option<bool>,
    on_press: Option<Message>,
    width: f32,
    hint: impl Into<String>,
) -> Element<'static, Message> {
    let active = lamp.unwrap_or(false);
    let mut contents: Vec<Element<Message>> = vec![
        iced::widget::svg(icon)
            .width(Length::Fixed(15.0))
            .height(Length::Fixed(15.0))
            .into(),
        text(legend.into()).size(9).into(),
    ];
    if let Some(lit) = lamp {
        contents.push(
            Canvas::new(LedJewel {
                handle: assets.led_jewel.clone(),
                color: ACCENT,
                lit,
            })
            .width(Length::Fixed(7.0))
            .height(Length::Fixed(7.0))
            .into(),
        );
    }
    let control = button(row(contents).spacing(4).align_y(Alignment::Center))
        .width(Length::Fixed(width))
        .height(Length::Fixed(30.0))
        .padding([4, 6])
        .style(move |_: &Theme, status| panel_key_style(active, status))
        .on_press_maybe(on_press);
    let key: Element<Message> = Stack::new()
        .push(control)
        .push_under(
            Canvas::new(PhotoSurface {
                handle: assets.panel_keycap_wide.clone(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .into();
    hardware_tooltip(key, hint)
}

fn pitch_mount_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(WARM_BLACK_DEEP.scale_alpha(0.26))),
        border: Border {
            color: Color::from_rgba8(0xd8, 0xc9, 0xa7, 0.09),
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow {
            color: WARM_BLACK_DEEP.scale_alpha(0.55),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 0.0,
        },
        ..Default::default()
    }
}

const HEADER_CONTROL_HEIGHT: f32 = 42.0;

fn action_key_style(salmon: bool, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgba8(0xff, 0xf1, 0xda, 0.10),
        button::Status::Pressed => Color::from_rgba8(0x09, 0x06, 0x04, 0.28),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    let text_color = match status {
        button::Status::Disabled => TEXT_MUTED.scale_alpha(0.48),
        _ if salmon => Color::from_rgb8(0xff, 0xf1, 0xe2),
        _ => TEXT_MAIN,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// A full-size photographed action key. Text and pointer behavior stay live;
/// the bitmap supplies only the physical plastic, wear, bevel, and shadow.
fn action_key(
    assets: &ControlAssets,
    label: impl Into<String>,
    salmon: bool,
    on_press: Option<Message>,
    width: f32,
) -> Element<'static, Message> {
    let legend = container(text(label.into()).size(14))
        .center_x(Length::Fill)
        .center_y(Length::Fill);
    let control = button(legend)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .style(move |_: &Theme, status| action_key_style(salmon, status))
        .on_press_maybe(on_press);
    let handle = if salmon {
        assets.action_keycap_salmon.clone()
    } else {
        assets.action_keycap_neutral.clone()
    };

    Stack::new()
        .width(Length::Fixed(width))
        .height(Length::Fixed(HEADER_CONTROL_HEIGHT))
        .push(control)
        .push_under(
            Canvas::new(PhotoSurface { handle })
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .into()
}

/// A button style with no resting chrome of its own — used to wrap a
/// decorative canvas (a rocker face, a knob dial) so the canvas art is the
/// only visible surface, while the button underneath stays the real
/// clickable/keyboard-focusable control. A faint hover/press wash keeps it
/// from feeling dead to the pointer.
fn transparent_control_style(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgba8(0xf3, 0xe8, 0xcf, 0.06),
        button::Status::Pressed => WARM_BLACK_DEEP.scale_alpha(0.4),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: TEXT_MAIN,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// Stretches one photographed control surface to fill its bounds. The image
/// remains decorative; interaction lives on the button wrapping it.
struct PhotoSurface {
    handle: image::Handle,
}

impl canvas::Program<Message> for PhotoSurface {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.draw_image(
            Rectangle::new(Point::ORIGIN, frame.size()),
            CanvasImage::new(self.handle.clone()),
        );
        vec![frame.into_geometry()]
    }
}

/// A compact horizontal OFF/ON hardware switch. The complete photographed
/// mechanism swaps by state, while the permanent side legends make the state
/// readable without color and the whole 88×40 module remains the hit target.
fn horizontal_switch(
    assets: &ControlAssets,
    caption: &'static str,
    on: bool,
    on_press: Option<Message>,
    hint: impl Into<String>,
) -> Element<'static, Message> {
    let handle = if on {
        assets.horizontal_switch_on.clone()
    } else {
        assets.horizontal_switch_off.clone()
    };
    let mechanism = Canvas::new(PhotoSurface { handle })
        .width(Length::Fixed(42.0))
        .height(Length::Fixed(14.0));
    let off_color = if on { TEXT_MUTED } else { TEXT_MAIN };
    let on_color = if on { TEXT_MAIN } else { TEXT_MUTED };
    let face = column![
        text(caption).size(8).color(TEXT_MAIN.scale_alpha(0.82)),
        row![
            text("OFF").size(8).color(off_color),
            mechanism,
            text("ON").size(8).color(on_color),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ]
    .spacing(1)
    .align_x(Alignment::Center);
    let control = button(face)
        .width(Length::Fixed(88.0))
        .height(Length::Fixed(40.0))
        .padding([2, 4])
        .style(transparent_control_style)
        .on_press_maybe(on_press);
    hardware_tooltip(
        container(control)
            .width(Length::Fixed(88.0))
            .height(Length::Fixed(40.0))
            .style(mixer_strip_style),
        hint,
    )
}

/// Photographed detented roller housing for the live-play MIDI channel. A
/// transparent native pick-list remains the real control underneath the live
/// legend, preserving direct selection and keyboard semantics.
fn roller_channel_selector<'a>(
    assets: &ControlAssets,
    selected: ChannelOption,
    on_select: impl Fn(ChannelOption) -> Message + 'a,
) -> Element<'a, Message> {
    const WIDTH: f32 = 142.0;
    const HEIGHT: f32 = HEADER_CONTROL_HEIGHT;
    let picker = container(
        pick_list(channel_options("PLAY CH"), Some(selected), on_select)
            .text_size(1)
            .padding([11, 2])
            .width(Length::Fill)
            .style(invisible_pick_list_style),
    )
    .width(Length::Fixed(WIDTH))
    .height(Length::Fixed(HEIGHT))
    .align_y(Alignment::Center);
    let legend = container(
        row![
            text("PLAY CH").size(9).color(TEXT_MAIN.scale_alpha(0.82)),
            text(format!("{}", selected.channel)).size(11).color(TEXT_MAIN),
            text("▼").size(8).color(TEXT_MUTED),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(WIDTH))
    .height(Length::Fixed(HEIGHT))
    .padding([4, 14])
    .align_y(Alignment::Start);
    let face = Canvas::new(PhotoSurface {
        handle: assets.roller_selector.clone(),
    })
    .width(Length::Fixed(WIDTH))
    .height(Length::Fixed(HEIGHT));
    let selector: Element<Message> = Stack::new()
        .width(Length::Fixed(WIDTH))
        .height(Length::Fixed(HEIGHT))
        .push(legend)
        .push_under(picker)
        .push_under(face)
        .into();
    hardware_tooltip(
        selector,
        format!(
            "Live-play MIDI channel — {}. Click to choose.",
            selected.channel
        ),
    )
}

/// Four fixed decorative fasteners anchor the shared console plate. The
/// overlay contains no interactive widgets and consumes no layout space.
fn console_screws<'a>(
    content: impl Into<Element<'a, Message>>,
    screw: image::Handle,
) -> Element<'a, Message> {
    let screw_image = || {
        image_widget(screw.clone())
            .width(Length::Fixed(9.0))
            .height(Length::Fixed(9.0))
            .filter_method(FilterMethod::Linear)
    };
    let top = row![
        screw_image(),
        Space::new().width(Length::Fill),
        screw_image(),
    ];
    let bottom = row![
        screw_image(),
        Space::new().width(Length::Fill),
        screw_image(),
    ];
    let overlay = container(
        column![top, Space::new().height(Length::Fill), bottom]
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(7);

    Stack::new().push(content).push(overlay).into()
}

/// A compact hardware rocker switch built from the real on/off SVG asset
/// pair — its raised-top/raised-bottom geometry *is* the state indicator,
/// the way a physical rocker works, rather than a color-coded dot.
fn rocker_switch(
    assets: &ControlAssets,
    caption: &'static str,
    on: bool,
    on_press: Option<Message>,
) -> Element<'static, Message> {
    let handle = if on {
        assets.rocker_on.clone()
    } else {
        assets.rocker_off.clone()
    };
    let face = Canvas::new(PhotoSurface { handle })
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(22.0));
    let label_color = if on { ACCENT } else { TEXT_MUTED };
    button(
        column![text(caption).size(8).color(label_color), face]
            .spacing(1)
            .align_x(Alignment::Center),
    )
    .padding([1, 4])
    .style(transparent_control_style)
    .on_press_maybe(on_press)
    .into()
}

/// A rotary knob dial: a faint tick ring, the shared photographic knob-face
/// asset — held at a fixed orientation, per the asset README, since the face
/// itself is never meant to rotate — and a hand-drawn pointer swept across
/// the same -135°..+135° arc as the ticks.
struct KnobDial {
    face: image::Handle,
    angle: Radians,
}

impl canvas::Program<Message> for KnobDial {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(frame.width() / 2.0, frame.height() / 2.0);
        let radius = frame.width().min(frame.height()) / 2.0;

        // A soft cast shadow, offset down, sells the knob as a raised molded
        // cap sitting above the panel rather than a flat printed circle.
        frame.fill(
            &Path::circle(Point::new(center.x, center.y + radius * 0.12), radius * 0.86),
            WARM_BLACK_DEEP.scale_alpha(0.4),
        );

        const TICKS: usize = 11;
        const SWEEP_DEG: f32 = 270.0;
        for i in 0..TICKS {
            let t = i as f32 / (TICKS - 1) as f32;
            let angle = (-SWEEP_DEG / 2.0 + t * SWEEP_DEG).to_radians();
            let (sin, cos) = angle.sin_cos();
            let tall = i == 0 || i == TICKS - 1 || i == TICKS / 2;
            let inner = radius - if tall { 4.5 } else { 2.5 };
            let outer = radius + 0.5;
            frame.stroke(
                &Path::line(
                    Point::new(center.x + sin * inner, center.y - cos * inner),
                    Point::new(center.x + sin * outer, center.y - cos * outer),
                ),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0x91, 0x8c, 0x7d, 0.5))
                    .with_width(1.0),
            );
        }

        let face_size = radius * 1.6;
        frame.draw_image(
            Rectangle::new(
                Point::new(center.x - face_size / 2.0, center.y - face_size / 2.0),
                Size::new(face_size, face_size),
            ),
            CanvasImage::new(self.face.clone()),
        );

        frame.with_save(|frame| {
            frame.translate(Vector::new(center.x, center.y));
            frame.rotate(self.angle);
            frame.stroke(
                &Path::line(
                    Point::new(0.0, -face_size * 0.10),
                    Point::new(0.0, -face_size * 0.46),
                ),
                canvas::Stroke::default()
                    .with_color(ACCENT)
                    .with_width(2.0)
                    .with_line_cap(canvas::LineCap::Round),
            );
        });
        frame.fill(&Path::circle(center, face_size * 0.07), WARM_BLACK_DEEP);

        vec![frame.into_geometry()]
    }
}

/// Fully invisible `pick_list` chrome — used to keep the real, accessible
/// dropdown as the interactive layer of a [`rotary_knob`] while its native
/// box/arrow never paint over the knob-dial artwork drawn beneath it.
fn invisible_pick_list_style(_: &Theme, _status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: Color::TRANSPARENT,
        placeholder_color: Color::TRANSPARENT,
        handle_color: Color::TRANSPARENT,
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
    }
}

/// A compact rotary hardware control for a per-track CH/OCT selector: the
/// [`KnobDial`] visual (tick ring, face, pointer) with a real `pick_list`
/// overlaid on top — invisible, but still the actual keyboard-accessible
/// control, sized to roughly cover the dial (see the `Stack::push_under`
/// idiom used for the fader below). The caption/value print beside the dial
/// rather than stacked above and below it, so the whole control stays no
/// taller than the mixer strip's other controls — see "do not increase mixer
/// height" in the design brief.
fn rotary_knob<'a, T>(
    assets: &ControlAssets,
    caption: &'static str,
    value_label: String,
    angle_fraction: f32,
    options: Vec<T>,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
{
    const SIZE: f32 = 30.0;
    let angle = Radians(
        (-135.0 + angle_fraction.clamp(0.0, 1.0) * 270.0).to_radians(),
    );
    let dial = Canvas::new(KnobDial {
        face: assets.rotary_knob_face.clone(),
        angle,
    })
    .width(Length::Fixed(SIZE))
    .height(Length::Fixed(SIZE));
    let picker = pick_list(options, selected, on_select)
        .text_size(11)
        .padding([5, 2])
        .width(Length::Fixed(SIZE))
        .style(invisible_pick_list_style);
    let control: Element<Message> = Stack::new()
        .width(Length::Fixed(SIZE))
        .height(Length::Fixed(SIZE))
        .push(dial)
        .push_under(picker)
        .into();
    column![
        text(caption).size(8).color(TEXT_MUTED),
        control,
        text(value_label).size(9).color(TEXT_MAIN),
    ]
    .spacing(0)
    .align_x(Alignment::Center)
    .into()
}

/// Non-interactive companion used by the idle mixer. Keeping the real knob
/// face mounted when no file is loaded prevents the channel strip from
/// collapsing into placeholder web controls.
fn static_rotary_knob(
    assets: &ControlAssets,
    caption: &'static str,
    value_label: &'static str,
    angle_fraction: f32,
) -> Element<'static, Message> {
    const SIZE: f32 = 30.0;
    let dial = Canvas::new(KnobDial {
        face: assets.rotary_knob_face.clone(),
        angle: Radians(
            (-135.0 + angle_fraction.clamp(0.0, 1.0) * 270.0).to_radians(),
        ),
    })
    .width(Length::Fixed(SIZE))
    .height(Length::Fixed(SIZE));

    column![
        text(caption).size(8).color(TEXT_MUTED),
        dial,
        text(value_label).size(9).color(TEXT_MUTED),
    ]
    .spacing(0)
    .align_x(Alignment::Center)
    .into()
}

/// Drag state for [`drag_knob`]: whether a drag is active, and the (cursor Y,
/// knob value) captured on mouse-down — mirrors `BoardCanvas`'s `dragging_knob`
/// / `drag_start` fields (see `render.rs`), just scoped to a single knob
/// instead of the whole board's 13 encoder knobs.
#[derive(Default)]
struct DragKnobState {
    dragging: bool,
    drag_start: Option<(f32, f32)>,
}

/// Vertical drag distance, in pixels, mapped onto the knob's full 0.0..=1.0
/// travel — matches `render::knob_slider_track`'s feel: a much longer throw
/// than the knob's own on-screen size, so a precise setting is easy to land.
const DRAG_KNOB_TRACK_PX: f32 = 120.0;

/// A rotary knob driven by vertical mouse drag (or scroll-wheel nudge), the
/// same interaction as the board's own synth-parameter knobs (see
/// `render::BoardCanvas`'s knob handling) — unlike [`rotary_knob`], which is
/// a dial paired with a `pick_list` for a small set of discrete options, this
/// is for a continuous range.
struct DragKnob<F: Fn(f32) -> Message> {
    face: image::Handle,
    /// Current position, 0.0..=1.0.
    value: f32,
    on_change: F,
}

impl<F: Fn(f32) -> Message> canvas::Program<Message> for DragKnob<F> {
    type State = DragKnobState;

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
                    let start_y = cursor.position().map(|p| p.y).unwrap_or(pos.y);
                    state.dragging = true;
                    state.drag_start = Some((start_y, self.value));
                    return Some(canvas::Action::capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let (Some(pos), Some((start_y, start_value))) =
                        (cursor.position(), state.drag_start)
                    {
                        let delta = (pos.y - start_y) / DRAG_KNOB_TRACK_PX;
                        let value = (start_value - delta).clamp(0.0, 1.0);
                        return Some(
                            canvas::Action::publish((self.on_change)(value)).and_capture(),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    state.drag_start = None;
                    return Some(canvas::Action::capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_some() {
                    let amount = match *delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * 0.05,
                        mouse::ScrollDelta::Pixels { y, .. } => y / 200.0,
                    };
                    let value = (self.value + amount).clamp(0.0, 1.0);
                    return Some(
                        canvas::Action::publish((self.on_change)(value)).and_capture(),
                    );
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        KnobDial {
            face: self.face.clone(),
            angle: Radians((-135.0 + self.value.clamp(0.0, 1.0) * 270.0).to_radians()),
        }
        .draw(&(), renderer, theme, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging || cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

/// A knob for a continuous range, dragged vertically like the board's own
/// synth-parameter knobs (see [`DragKnob`]) rather than picked from a fixed
/// list like [`rotary_knob`].
fn drag_knob<'a>(
    assets: &ControlAssets,
    caption: &'static str,
    value_label: String,
    value: f32,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    const SIZE: f32 = 30.0;
    let dial = Canvas::new(DragKnob {
        face: assets.rotary_knob_face.clone(),
        value,
        on_change,
    })
    .width(Length::Fixed(SIZE))
    .height(Length::Fixed(SIZE));

    column![
        text(caption).size(8).color(TEXT_MUTED),
        dial,
        text(value_label).size(9).color(TEXT_MAIN),
    ]
    .spacing(0)
    .align_x(Alignment::Center)
    .into()
}

/// A tiny mounted indicator lamp: the shared neutral lens/bezel asset with
/// the track color composited as the lamp's own light source underneath it
/// — glowing only when lit — so it reads as physical hardware rather than a
/// flat CSS dot.
struct LedJewel {
    handle: image::Handle,
    color: Color,
    lit: bool,
}

impl canvas::Program<Message> for LedJewel {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(frame.width() / 2.0, frame.height() / 2.0);
        let radius = frame.width().min(frame.height()) / 2.0;
        if self.lit {
            // Radii stay within the canvas's own half-width (not its corner
            // reach) so the glow reads as a soft circular halo — earlier
            // scales here exceeded the canvas's half-diagonal and flooded
            // every pixel, corners included, into one flat tinted square.
            for (scale, alpha) in [(1.0, 0.20), (0.72, 0.34), (0.46, 0.85)] {
                frame.fill(
                    &Path::circle(center, radius * scale),
                    self.color.scale_alpha(alpha),
                );
            }
        } else {
            frame.fill(
                &Path::circle(center, radius * 0.65),
                Color::from_rgba8(0x2a, 0x29, 0x22, 0.6),
            );
        }
        if self.lit {
            frame.fill(
                &Path::circle(center, radius * 0.9),
                Color::from_rgb8(0x0b, 0x0c, 0x0a),
            );
            frame.stroke(
                &Path::circle(center, radius * 0.76),
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(0x55, 0x58, 0x50))
                    .with_width(1.0),
            );
            frame.fill(&Path::circle(center, radius * 0.58), self.color);
            frame.fill(
                &Path::circle(
                    Point::new(center.x - radius * 0.14, center.y - radius * 0.16),
                    radius * 0.11,
                ),
                Color::WHITE.scale_alpha(0.58),
            );
        } else {
            frame.draw_image(
                Rectangle::new(Point::ORIGIN, frame.size()),
                CanvasImage::new(self.handle.clone()),
            );
        }
        vec![frame.into_geometry()]
    }
}

/// A small mounted LED jewel — see [`LedJewel`] — sized like a real
/// panel-mount indicator lamp rather than a large status dot.
fn led_jewel(assets: &ControlAssets, color: Color, lit: bool) -> Element<'static, Message> {
    Canvas::new(LedJewel {
        handle: assets.led_jewel.clone(),
        color,
        lit,
    })
    .width(Length::Fixed(13.0))
    .height(Length::Fixed(13.0))
    .into()
}

/// Stretches an SVG surface to fill its bounds.
struct SvgSurface {
    handle: svg::Handle,
}

impl canvas::Program<Message> for SvgSurface {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.draw_svg(
            Rectangle::new(Point::ORIGIN, frame.size()),
            svg::Svg::new(self.handle.clone()),
        );
        vec![frame.into_geometry()]
    }
}

/// A track name set into a physical recessed label plate — the real asset
/// behind warm off-white text — instead of sitting in an HTML-input-looking
/// box on the bare panel surface.
fn label_plate<'a>(assets: &ControlAssets, label: String) -> Element<'a, Message> {
    let content = container(text(label).size(12).color(Color::from_rgb8(0xe6, 0xdf, 0xcb)))
        .padding([3, 10])
        .align_y(Alignment::Center);
    Stack::new()
        .push(content)
        .push_under(
            Canvas::new(PhotoSurface {
                handle: assets.label_plate.clone(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .into()
}

/// Adds a transparent-opening molded bezel around live display content. The
/// content remains the sizing and interactive base; the SVG only supplies the
/// physical frame around its padded aperture.
fn display_bezel_wrap<'a>(
    assets: &ControlAssets,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let inset = container(content).padding([5, 8]);
    Stack::new()
        .push(inset)
        .push_under(
            Canvas::new(SvgSurface {
                handle: assets.display_bezel.clone(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .into()
}

/// Stretches the shared low-opacity LCD glass overlay to fill its bounds —
/// the topmost layer over the amber readout, giving it recessed, under-glass
/// depth instead of a flat rectangle.
struct LcdGlassOverlay {
    /// `None` skips the sheen entirely — the shared glass SVG's reflection
    /// streaks are calibrated for the header LCD's wide, short aspect and
    /// warp into a visible curved artifact when stretched over a very
    /// differently-shaped display (the visualizer), so that one gets the
    /// vignette below with no glass drawn over it.
    handle: Option<svg::Handle>,
}

impl canvas::Program<Message> for LcdGlassOverlay {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(handle) = &self.handle {
            frame.draw_svg(
                Rectangle::new(Point::ORIGIN, frame.size()),
                svg::Svg::new(handle.clone()),
            );
        }
        // A restrained vignette — a few inset strokes fading toward the
        // edge — reads as a deeper recess without brightening or otherwise
        // drawing attention away from the amber content itself.
        let size = frame.size();
        for (inset, alpha) in [(0.5, 0.35), (2.5, 0.18), (5.0, 0.08)] {
            let path = Path::new(|b| {
                b.rounded_rectangle(
                    Point::new(inset, inset),
                    Size::new(
                        (size.width - inset * 2.0).max(0.0),
                        (size.height - inset * 2.0).max(0.0),
                    ),
                    4.0.into(),
                );
            });
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(WARM_BLACK_DEEP.scale_alpha(alpha))
                    .with_width(1.0),
            );
        }
        vec![frame.into_geometry()]
    }
}

/// Layers the shared LCD glass overlay on top of an existing display's
/// content — bezel → LCD surface → amber content already drawn by `content`
/// → glass, with the glass painted last so it always reads above the data.
fn lcd_glass_wrap<'a>(
    assets: &ControlAssets,
    content: Element<'a, Message>,
    with_sheen: bool,
) -> Element<'a, Message> {
    Stack::new()
        .push(content)
        .push(
            Canvas::new(LcdGlassOverlay {
                handle: with_sheen.then(|| assets.lcd_glass.clone()),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .into()
}

/// A horizontal hardware fader: a narrow dark slot with the shared
/// photographic fader-cap thumb, in place of a rounded web slider. The
/// handle/rail chrome is fully transparent — [`FaderTrack`] draws the actual
/// slot, ticks, and cap underneath as a backdrop layer (see its
/// `Stack::push_under` usage), so this real `slider` stays only the
/// invisible, fully functional interactive surface.
fn fader_style(_: &Theme, _status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(Color::TRANSPARENT),
                Background::Color(Color::TRANSPARENT),
            ),
            width: 1.0,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 1,
                border_radius: 0.0.into(),
            },
            background: Background::Color(Color::TRANSPARENT),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// The physical fader's recessed slot: a dark inset track, evenly spaced
/// ticks, and the shared fader-cap asset positioned at the current value —
/// drawn as a backdrop layer under the real, fully functional slider (see
/// its `Stack::push_under` usage), which stays the interactive/accessible
/// base and never has its hit-testing intercepted by this decoration.
struct FaderTrack {
    progress: f32,
    cap: image::Handle,
    track: svg::Handle,
}

impl canvas::Program<Message> for FaderTrack {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let width = frame.width();
        let mid_y = frame.height() / 2.0;
        frame.draw_svg(
            Rectangle::new(Point::ORIGIN, frame.size()),
            svg::Svg::new(self.track.clone()),
        );
        frame.stroke(
            &Path::line(
                Point::new(2.0, mid_y),
                Point::new((width - 2.0).max(2.0), mid_y),
            ),
            canvas::Stroke::default()
                .with_color(WARM_BLACK_DEEP)
                .with_width(4.0)
                .with_line_cap(canvas::LineCap::Round),
        );
        frame.stroke(
            &Path::line(
                Point::new(3.0, mid_y - 1.0),
                Point::new((width - 3.0).max(3.0), mid_y - 1.0),
            ),
            canvas::Stroke::default()
                .with_color(PANEL_BORDER.scale_alpha(0.55))
                .with_width(1.0)
                .with_line_cap(canvas::LineCap::Round),
        );
        const TICK_COUNT: usize = 13;
        for i in 0..TICK_COUNT {
            let x = 3.0 + (width - 6.0).max(0.0) * (i as f32) / (TICK_COUNT - 1) as f32;
            let major = i == 0 || i == TICK_COUNT / 2 || i == TICK_COUNT - 1;
            let tick_h = if major { 13.0 } else { 8.0 };
            frame.stroke(
                &Path::line(
                    Point::new(x, mid_y - tick_h / 2.0),
                    Point::new(x, mid_y + tick_h / 2.0),
                ),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(
                        0xc9,
                        0xc0,
                        0xa2,
                        if major { 0.62 } else { 0.36 },
                    ))
                    .with_width(if major { 1.5 } else { 1.0 }),
            );
        }

        let cap_w = width.min(22.0);
        let cap_h = cap_w * (572.0 / 1024.0);
        let half = cap_w / 2.0;
        let x = (width * self.progress.clamp(0.0, 1.0)).clamp(half, (width - half).max(half));
        frame.draw_image(
            Rectangle::new(Point::new(x - half, mid_y - cap_h / 2.0), Size::new(cap_w, cap_h)),
            CanvasImage::new(self.cap.clone()),
        );

        vec![frame.into_geometry()]
    }
}

/// Styles an "on/active" indicator (a connected port, an enabled toggle) —
/// visually distinct from the salmon photographic action key so those states
/// don't compete with actual primary actions for attention.
#[cfg(target_arch = "wasm32")]
fn toggled_style(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb(0.30, 0.20, 0.20),
        button::Status::Pressed => Color::from_rgb(0.14, 0.11, 0.10),
        button::Status::Disabled => Color::from_rgb(0.16, 0.12, 0.14),
        button::Status::Active => Color::from_rgb(0.20, 0.15, 0.15),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: ACCENT,
        border: Border {
            color: ACCENT,
            width: 1.0,
            radius: 2.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let app = iced::application(App::default, App::update, App::view)
        .title("K2 MIDI Viewer")
        .theme(app_theme_for)
        .font(include_bytes!("../assets/fonts/PermanentMarker-Regular.ttf").as_slice())
        .subscription(App::subscription);
    #[cfg(not(target_arch = "wasm32"))]
    let app = app.window(iced::window::Settings {
        size: Size::new(1520.0, 900.0),
        icon: Some(app_icon()),
        ..Default::default()
    });
    app.run()
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComputerKeyLocation {
    Standard,
    Left,
    Right,
    Numpad,
}

impl From<iced::keyboard::Location> for ComputerKeyLocation {
    fn from(location: iced::keyboard::Location) -> Self {
        match location {
            iced::keyboard::Location::Standard => Self::Standard,
            iced::keyboard::Location::Left => Self::Left,
            iced::keyboard::Location::Right => Self::Right,
            iced::keyboard::Location::Numpad => Self::Numpad,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComputerKey {
    Character(String, ComputerKeyLocation),
    Named(iced::keyboard::key::Named, ComputerKeyLocation),
}

fn normalize_computer_key(
    key: iced::keyboard::Key,
    location: iced::keyboard::Location,
) -> Option<ComputerKey> {
    let location = ComputerKeyLocation::from(location);
    match key {
        iced::keyboard::Key::Named(named) => Some(ComputerKey::Named(named, location)),
        iced::keyboard::Key::Character(character) => {
            let character = character.to_lowercase();
            let character = if location == ComputerKeyLocation::Numpad {
                character.as_str()
            } else {
                match character.as_str() {
                    "~" => "`",
                    "!" => "1",
                    "@" => "2",
                    "#" => "3",
                    "$" => "4",
                    "%" => "5",
                    "^" => "6",
                    "&" => "7",
                    "*" => "8",
                    "(" => "9",
                    ")" => "0",
                    "_" => "-",
                    "+" => "=",
                    "{" => "[",
                    "}" => "]",
                    "|" => "\\",
                    ":" => ";",
                    "\"" => "'",
                    "<" => ",",
                    ">" => ".",
                    "?" => "/",
                    other => other,
                }
            };
            Some(ComputerKey::Character(character.to_string(), location))
        }
        iced::keyboard::Key::Unidentified => None,
    }
}

fn computer_keyboard_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, location, .. }) => {
            normalize_computer_key(key, location).map(Message::ComputerKeyPressed)
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, location, .. }) => {
            normalize_computer_key(key, location).map(Message::ComputerKeyReleased)
        }
        iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::ReleaseComputerKeys),
        _ => None,
    }
}

/// Arrow Up/Down transpose hand-played notes an octave, independent of
/// "Computer keys" mode (the arrow cluster has no note assigned, so there's
/// nothing for it to conflict with). `repeat: false` keeps a held key to one
/// step instead of free-running.
fn octave_shortcut_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key, repeat: false, ..
        }) => match key {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                Some(Message::LiveOctaveUp)
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                Some(Message::LiveOctaveDown)
            }
            _ => None,
        },
        _ => None,
    }
}

/// How to pick a key when a note repeats across this keyboard's overlapping
/// rows. LeftRight/UpDown are fixed, predictable preferences (always the
/// last/first occurrence). Closest instead solves for the key assignment
/// that minimizes total on-screen travel across a whole sequence of notes —
/// see `shortest_path_keys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyPickMode {
    LeftRight,
    UpDown,
    Closest,
}

impl KeyPickMode {
    fn next(self) -> Self {
        match self {
            KeyPickMode::LeftRight => KeyPickMode::UpDown,
            KeyPickMode::UpDown => KeyPickMode::Closest,
            KeyPickMode::Closest => KeyPickMode::LeftRight,
        }
    }

    fn label(self) -> &'static str {
        match self {
            KeyPickMode::LeftRight => "Rows: L/R",
            KeyPickMode::UpDown => "Rows: U/D",
            KeyPickMode::Closest => "Rows: Closest",
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            KeyPickMode::LeftRight => "L/R",
            KeyPickMode::UpDown => "U/D",
            KeyPickMode::Closest => "NEAR",
        }
    }
}

/// Keep these aligned with `keyboard-keyboard/code/src/settings.rs`. Octave 4
/// is the firmware's untransposed SWITCH_TO_NOTE table; shipped hardware
/// defaults to octave 2.
#[cfg(target_arch = "wasm32")]
const MIN_MIDI_INPUT_OCTAVE: i8 = 2;
#[cfg(target_arch = "wasm32")]
const MAX_MIDI_INPUT_OCTAVE: i8 = 5;
#[cfg(any(test, target_arch = "wasm32"))]
const DEFAULT_MIDI_INPUT_OCTAVE: i8 = 2;
#[cfg(any(test, target_arch = "wasm32"))]
const MIDI_INPUT_REFERENCE_OCTAVE: i8 = 4;

/// Reverses the physical keyboard's octave transpose for visualization only.
/// The original incoming note is still used for audio and note-off messages.
#[cfg(any(test, target_arch = "wasm32"))]
fn midi_input_board_note(note: u8, input_octave: i8) -> (u8, bool) {
    let shift = (MIDI_INPUT_REFERENCE_OCTAVE - input_octave) as i16 * 12;
    let mapped = note as i16 + shift;
    (mapped.clamp(0, 127) as u8, !(0..=127).contains(&mapped))
}

struct App {
    window_size: Size,
    photo_assets: PhotoBoardAssets,
    chrome_assets: ChromeAssets,
    control_assets: ControlAssets,

    // keyboard
    keys: Vec<Key>,
    note_to_all_keys: HashMap<u8, Vec<KeyId>>,
    drum_note_to_key: HashMap<u8, KeyId>, // GM percussion note → drum pad key
    key_pos: HashMap<KeyId, (f32, f32)>,  // KeyId → (col, row), for nearest-key picking
    keyboard_notes: std::collections::HashSet<u8>,
    keyboard_notes_sorted: Vec<u8>, // ascending, for nearest-key search
    highlighted: HashMap<KeyId, usize>, // KeyId → track index
    /// The exact key each currently-sounding (note, channel) pair lit up, so
    /// its matching note-off can clear precisely that key even though a
    /// per-track octave shift means the same raw note can map to different
    /// keys depending on which track played it.
    active_highlight_keys: HashMap<(u8, u8), KeyId>,
    /// Nav-cluster waveform-select keys currently toggled on; each one is a
    /// layer summed together in the synth (empty ⇒ Organ). See
    /// `active_waveforms`.
    waveform_keys: HashSet<KeyId>,
    pressed_keys: HashSet<KeyId>,
    keyboard_hits_enabled: bool,
    drum_symbols_enabled: bool,
    compact_keyboard: bool,
    /// User-selected board viewport height. `None` keeps the responsive
    /// automatic/compact sizing presets in control.
    keyboard_height_override: Option<f32>,
    computer_keys_down: HashMap<ComputerKey, Vec<KeyId>>,
    computer_key_labels: HashMap<KeyId, String>,
    knob_values: [f32; synth::KNOB_COUNT], // 0.0..=1.0 dial position per knob
    /// Semitone transpose applied only to hand-played notes (mouse/computer
    /// keys), independent of `octave_offset` — which instead remaps which
    /// board key lights up for a file's notes and shifts file playback.
    live_octave: i8,
    /// The exact (shifted note, channel) actually sent for each currently
    /// held hand-played key, so release always turns off what was actually
    /// turned on even if `live_octave` changes while the key is held.
    live_note_overrides: HashMap<KeyId, (u8, u8)>,
    /// Output channel (0-indexed) for hand-played notes — mouse/computer
    /// keys and the physical board. Drum pads always send on
    /// `synth::DRUM_CHANNEL` regardless of this setting.
    live_channel: u8,

    // MIDI file
    midi_file: Option<midi::MidiFile>,
    octave_offset: i8,
    pitch_step: i8,             // 1 = semitone, 12 = octave
    key_pick_mode: KeyPickMode, // which duplicate key to light when a note repeats across rows
    /// Closest mode's precomputed answer, shared by every highlight path (live
    /// playback, the selection view, and the all-notes overlay) so a given
    /// note always lands on the same key everywhere instead of live playback
    /// re-deciding greedily — and losing context — every time a note re-fires.
    closest_key_for_note: HashMap<u8, KeyId>,
    show_all_notes: bool, // overlay every note in the file on the keyboard
    all_notes_cache: HashMap<KeyId, usize>, // precomputed for show_all_notes
    skipped_notes: usize,
    track_muted: Vec<bool>,
    /// Output channel (0-indexed) each track sends on. Defaults to the
    /// track's original channel from the file, but can be remapped per
    /// track — e.g. to route a specific voice to a specific channel on the
    /// connected hardware.
    track_channel: Vec<u8>,
    /// Per-track octave shift (in octaves, not semitones), layered on top of
    /// `octave_offset` — for a track that needs a different register than the
    /// rest of the song to land on the keyboard.
    track_octave: Vec<i8>,
    load_error: Option<String>,

    // playback
    playback_handle: Option<PlaybackHandle>,
    play_state: PlayState,
    looper_enabled: bool,
    position_tick: u64,
    /// Playback rate multiplier applied to file playback (1.0 = normal speed).
    playback_speed: f32,
    audio_enabled: Arc<AtomicBool>,
    playback_events: Arc<Mutex<VecDeque<PlayEvent>>>,
    soft_synth: Option<Arc<Mutex<synth::SoftSynth>>>,
    _audio_stream: Option<cpal::Stream>,
    audio_error: Option<String>,

    // MIDI output
    midi_port_names: Vec<String>,
    midi_port_idx: usize,

    // Web MIDI for supporting desktop browsers (browser build only)
    #[cfg(target_arch = "wasm32")]
    web_midi_access: Option<playback::MidiAccessHandle>,
    #[cfg(target_arch = "wasm32")]
    web_midi_inputs: Vec<playback::MidiPortInfo>,
    #[cfg(target_arch = "wasm32")]
    web_midi_outputs: Vec<playback::MidiPortInfo>,
    #[cfg(target_arch = "wasm32")]
    web_midi_input_id: Option<String>,
    #[cfg(target_arch = "wasm32")]
    web_midi_output_id: Option<String>,
    #[cfg(target_arch = "wasm32")]
    web_midi_input: Option<playback::MidiInputConnection>,
    #[cfg(target_arch = "wasm32")]
    web_midi_output: Option<playback::MidiOutputConnection>,
    #[cfg(target_arch = "wasm32")]
    web_midi_events: Arc<Mutex<VecDeque<Vec<u8>>>>,
    #[cfg(target_arch = "wasm32")]
    web_midi_active_notes: HashMap<(u8, u8), KeyId>,
    #[cfg(target_arch = "wasm32")]
    web_midi_highlighted: HashMap<KeyId, usize>,
    #[cfg(target_arch = "wasm32")]
    web_midi_pending: bool,
    #[cfg(target_arch = "wasm32")]
    web_midi_status: Option<String>,
    /// Bumped on every manual rescan so the status text visibly changes even
    /// when the result is unchanged (still no devices) — otherwise clicking
    /// retry looks like it did nothing.
    #[cfg(target_arch = "wasm32")]
    web_midi_retry_count: u32,
    /// Mirrors the octave value in the physical Keyboard Keyboard's Settings
    /// menu so octave-transposed MIDI can still light the originating switch.
    #[cfg(target_arch = "wasm32")]
    web_midi_input_octave: i8,
    /// When enabled, every raw message from the selected Web MIDI input is
    /// forwarded byte-for-byte to the selected output before visualization.
    #[cfg(target_arch = "wasm32")]
    web_midi_thru_enabled: bool,

    // staff selection
    staff_selection: Option<(u64, u64)>,
    selection_highlight_cache: HashMap<KeyId, usize>,
    /// One or more chronological play-step numbers displayed on each key while
    /// a staff range is selected. Notes that begin together share a step.
    selection_play_order: HashMap<KeyId, Vec<usize>>,
}

impl Default for App {
    fn default() -> Self {
        let layout = build_layout();
        let midi_port_names = playback::list_output_ports();
        #[cfg(not(target_arch = "wasm32"))]
        let (soft_synth, audio_stream, audio_error) = match synth::start_soft_synth() {
            Ok((synth, stream)) => (Some(synth), Some(stream), None),
            Err(error) => (None, None, Some(error)),
        };
        // Creating and starting Web Audio outside a user gesture leaves the
        // context suspended. Initialize it from the first click/key instead.
        #[cfg(target_arch = "wasm32")]
        let (soft_synth, audio_stream, audio_error) = (None, None, None);
        let mut keyboard_notes_sorted: Vec<u8> = layout.keyboard_notes.iter().copied().collect();
        keyboard_notes_sorted.sort_unstable();

        let key_pos: HashMap<KeyId, (f32, f32)> =
            layout.keys.iter().map(|k| (k.id, (k.col, k.row))).collect();
        let computer_key_labels = computer_projection_labels(&layout.keys);

        let mut app = App {
            window_size: Size::new(1520.0, 900.0),
            photo_assets: PhotoBoardAssets::new(),
            chrome_assets: ChromeAssets::new(),
            control_assets: ControlAssets::new(),

            keyboard_notes: layout.keyboard_notes,
            keyboard_notes_sorted,
            keys: layout.keys,
            note_to_all_keys: layout.note_to_all_keys,
            drum_note_to_key: layout.drum_note_to_key,
            key_pos,
            highlighted: HashMap::new(),
            active_highlight_keys: HashMap::new(),
            waveform_keys: HashSet::new(),
            pressed_keys: HashSet::new(),
            keyboard_hits_enabled: false,
            drum_symbols_enabled: false,
            compact_keyboard: false,
            keyboard_height_override: None,
            computer_keys_down: HashMap::new(),
            computer_key_labels,
            live_octave: 0,
            live_note_overrides: HashMap::new(),
            live_channel: 1,
            knob_values: {
                let mut values = [0.0f32; synth::KNOB_COUNT];
                for (slot, param) in values.iter_mut().zip(synth::KNOB_PARAMS.iter()) {
                    *slot = (param.default - param.min) / (param.max - param.min);
                }
                values
            },

            midi_file: None,
            octave_offset: 0,
            pitch_step: 12,
            key_pick_mode: KeyPickMode::Closest,
            show_all_notes: false,
            closest_key_for_note: HashMap::new(),
            all_notes_cache: HashMap::new(),
            skipped_notes: 0,
            track_muted: Vec::new(),
            track_channel: Vec::new(),
            track_octave: Vec::new(),
            load_error: None,

            playback_handle: None,
            play_state: PlayState::Stopped,
            looper_enabled: false,
            position_tick: 0,
            playback_speed: 1.0,
            audio_enabled: Arc::new(AtomicBool::new(true)),
            playback_events: Arc::new(Mutex::new(VecDeque::new())),
            soft_synth,
            _audio_stream: audio_stream,
            audio_error,

            midi_port_idx: 0,
            midi_port_names,

            #[cfg(target_arch = "wasm32")]
            web_midi_access: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_inputs: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            web_midi_outputs: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            web_midi_input_id: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_output_id: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_input: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_output: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_events: Arc::new(Mutex::new(VecDeque::new())),
            #[cfg(target_arch = "wasm32")]
            web_midi_active_notes: HashMap::new(),
            #[cfg(target_arch = "wasm32")]
            web_midi_highlighted: HashMap::new(),
            #[cfg(target_arch = "wasm32")]
            web_midi_pending: false,
            #[cfg(target_arch = "wasm32")]
            web_midi_status: None,
            #[cfg(target_arch = "wasm32")]
            web_midi_retry_count: 0,
            #[cfg(target_arch = "wasm32")]
            web_midi_input_octave: DEFAULT_MIDI_INPUT_OCTAVE,
            #[cfg(target_arch = "wasm32")]
            web_midi_thru_enabled: false,

            staff_selection: None,
            selection_highlight_cache: HashMap::new(),
            selection_play_order: HashMap::new(),
        };
        apply_url_settings(&mut app);
        app
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    WindowResized(Size),
    // keyboard
    KeyPressed(KeyId),
    KeyReleased(KeyId),
    KnobChanged(u8, f32), // knob index, 0.0..=1.0 dial position
    ToggleKeyboardHits,
    ToggleDrumSymbols,
    ToggleCompactKeyboard,
    KeyboardHeightChanged(f32),
    ComputerKeyPressed(ComputerKey),
    ComputerKeyReleased(ComputerKey),
    ReleaseComputerKeys,
    // file
    OpenFile,
    FileChosen(Option<Vec<u8>>),
    MidiLoaded(Result<midi::MidiFile, String>),
    // pitch nudge
    PitchUp,
    PitchDown,
    PitchStepToggle,
    PitchReset,
    OctaveLayoutToggle,
    LiveOctaveUp,
    LiveOctaveDown,
    ToggleAllNotes,
    // tracks
    TrackMuted(usize, bool),
    TrackChannel(usize, u8),
    TrackOctave(usize, i8),
    // channel
    LiveChannel(u8),
    // transport
    Play,
    Pause,
    Stop,
    ToggleLooper,
    SeekTo(f32), // 0.0..=1.0 progress
    PollPlayback,
    SpeedChanged(f32),
    // audio
    ToggleAudio,
    // port
    NextPort,
    #[cfg(target_arch = "wasm32")]
    RequestWebMidi,
    #[cfg(target_arch = "wasm32")]
    WebMidiReady(Result<web_sys::MidiAccess, String>),
    #[cfg(target_arch = "wasm32")]
    NextWebMidiInput,
    #[cfg(target_arch = "wasm32")]
    NextWebMidiOutput,
    #[cfg(target_arch = "wasm32")]
    RefreshWebMidiPorts,
    #[cfg(target_arch = "wasm32")]
    WebMidiInputOctave(i8),
    #[cfg(target_arch = "wasm32")]
    ToggleWebMidiThru,
    // staff selection
    StaffSelectionChanged(Option<(u64, u64)>),
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// LeftRight/UpDown mode: always the same fixed occurrence, no context needed.
fn pick_key_fixed(kids: &[KeyId], mode: KeyPickMode) -> Option<KeyId> {
    match kids {
        [] => None,
        _ => Some(if mode == KeyPickMode::UpDown {
            kids[0]
        } else {
            *kids.last().unwrap()
        }),
    }
}

/// The Nav-cluster label ↔ waveform pairing for the six waveform-select
/// keys (Insert/Home/PgUp/Delete/End/PgDn). Each is an independent on/off
/// layer — see `App::active_waveforms` — rather than a mutually exclusive
/// choice, so multiple can be toggled on at once and their outputs blend.
const WAVEFORM_KEYS: [(&str, synth::Waveform); 6] = [
    ("Insert", synth::Waveform::Triangle),
    ("Home", synth::Waveform::Square),
    ("PgUp", synth::Waveform::Saw),
    ("Delete", synth::Waveform::Sine),
    ("End", synth::Waveform::Pulse),
    ("PgDn", synth::Waveform::Noise),
];

fn waveform_for_label(label: &str) -> Option<synth::Waveform> {
    WAVEFORM_KEYS
        .iter()
        .find(|(l, _)| *l == label)
        .map(|(_, w)| *w)
}

// Only consumed by `url_state`, which is wasm-only.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn label_for_waveform(w: synth::Waveform) -> Option<&'static str> {
    WAVEFORM_KEYS
        .iter()
        .find(|(_, ww)| *ww == w)
        .map(|(l, _)| *l)
}

/// Toggles `pressed_key` in the set of active waveform-select keys — press
/// once to add that waveform as a layer, press again to remove it.
fn toggle_waveform_key(active: &mut HashSet<KeyId>, pressed_key: KeyId) {
    if !active.remove(&pressed_key) {
        active.insert(pressed_key);
    }
}

fn key_range(keys: &[Key], row: f32, start: usize, end: usize) -> Vec<KeyId> {
    let mut row_keys: Vec<&Key> = keys
        .iter()
        .filter(|key| matches!(key.cluster, Cluster::Alpha | Cluster::AlphaLight) && key.row == row)
        .collect();
    row_keys.sort_by(|a, b| a.col.total_cmp(&b.col));
    row_keys
        .get(start..=end)
        .unwrap_or(&[])
        .iter()
        .map(|key| key.id)
        .collect()
}

fn cluster_key(keys: &[Key], cluster: Cluster, label: &str) -> Vec<KeyId> {
    keys.iter()
        .find(|key| key.cluster == cluster && key.label == label)
        .map(|key| vec![key.id])
        .unwrap_or_default()
}

fn numpad_range(keys: &[Key], indices: &[usize]) -> Vec<KeyId> {
    let mut numpad: Vec<&Key> = keys
        .iter()
        .filter(|key| key.cluster == Cluster::Numpad)
        .collect();
    numpad.sort_by(|a, b| a.row.total_cmp(&b.row).then(a.col.total_cmp(&b.col)));
    indices
        .iter()
        .filter_map(|&index| numpad.get(index).map(|key| key.id))
        .collect()
}

fn mapped_computer_keys(keys: &[Key], computer_key: &ComputerKey) -> Vec<KeyId> {
    use iced::keyboard::key::Named;

    let alpha_span = match computer_key {
        ComputerKey::Character(character, ComputerKeyLocation::Standard) => {
            const ROW_1: [&str; 13] = [
                "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=",
            ];
            const ROW_2: [&str; 13] = [
                "q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]", "\\",
            ];
            const ROW_3: [&str; 11] = ["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"];
            const ROW_4: [&str; 10] = ["z", "x", "c", "v", "b", "n", "m", ",", ".", "/"];

            ROW_1
                .iter()
                .position(|value| *value == character)
                .map(|index| (1.0, index, index))
                .or_else(|| {
                    ROW_2
                        .iter()
                        .position(|value| *value == character)
                        .map(|index| (2.0, index + 2, index + 2))
                })
                .or_else(|| {
                    ROW_3
                        .iter()
                        .position(|value| *value == character)
                        .map(|index| (3.0, index + 1, index + 1))
                })
                .or_else(|| {
                    ROW_4
                        .iter()
                        .position(|value| *value == character)
                        .map(|index| (4.0, index + 2, index + 2))
                })
        }
        ComputerKey::Named(Named::Backspace, ComputerKeyLocation::Standard) => Some((1.0, 13, 13)),
        ComputerKey::Named(Named::Tab, ComputerKeyLocation::Standard) => Some((2.0, 0, 1)),
        ComputerKey::Named(Named::CapsLock, ComputerKeyLocation::Standard) => Some((3.0, 0, 0)),
        ComputerKey::Named(Named::Enter, ComputerKeyLocation::Standard) => Some((3.0, 12, 13)),
        ComputerKey::Named(Named::Shift, ComputerKeyLocation::Left) => Some((4.0, 0, 1)),
        ComputerKey::Named(Named::Shift, ComputerKeyLocation::Right) => Some((4.0, 12, 14)),
        ComputerKey::Named(Named::Control, ComputerKeyLocation::Left) => Some((5.0, 0, 0)),
        ComputerKey::Named(Named::Fn | Named::Meta | Named::Super, ComputerKeyLocation::Left) => {
            Some((5.0, 1, 1))
        }
        ComputerKey::Named(Named::Alt, ComputerKeyLocation::Left) => Some((5.0, 2, 2)),
        ComputerKey::Named(Named::Space, _) => Some((5.0, 3, 8)),
        ComputerKey::Named(Named::Alt | Named::AltGraph, ComputerKeyLocation::Right) => {
            Some((5.0, 9, 9))
        }
        ComputerKey::Named(Named::Meta | Named::Super, ComputerKeyLocation::Right) => {
            Some((5.0, 10, 10))
        }
        ComputerKey::Named(Named::Control, ComputerKeyLocation::Right) => Some((5.0, 11, 11)),
        _ => None,
    };

    if let Some((row, start, end)) = alpha_span {
        return key_range(keys, row, start, end);
    }

    match computer_key {
        ComputerKey::Named(Named::Insert, _) => cluster_key(keys, Cluster::Nav, "Insert"),
        ComputerKey::Named(Named::Home, _) => cluster_key(keys, Cluster::Nav, "Home"),
        ComputerKey::Named(Named::PageUp, _) => cluster_key(keys, Cluster::Nav, "PgUp"),
        ComputerKey::Named(Named::Delete, _) => cluster_key(keys, Cluster::Nav, "Delete"),
        ComputerKey::Named(Named::End, _) => cluster_key(keys, Cluster::Nav, "End"),
        ComputerKey::Named(Named::PageDown, _) => cluster_key(keys, Cluster::Nav, "PgDn"),
        ComputerKey::Named(Named::ArrowUp, _) => cluster_key(keys, Cluster::Arrow, "↑"),
        ComputerKey::Named(Named::ArrowLeft, _) => cluster_key(keys, Cluster::Arrow, "←"),
        ComputerKey::Named(Named::ArrowDown, _) => cluster_key(keys, Cluster::Arrow, "↓"),
        ComputerKey::Named(Named::ArrowRight, _) => cluster_key(keys, Cluster::Arrow, "→"),
        ComputerKey::Named(Named::NumLock, _) => numpad_range(keys, &[0]),
        ComputerKey::Named(Named::Enter, ComputerKeyLocation::Numpad) => {
            numpad_range(keys, &[15, 19])
        }
        ComputerKey::Character(character, ComputerKeyLocation::Numpad) => {
            match character.as_str() {
                "/" => numpad_range(keys, &[1]),
                "*" => numpad_range(keys, &[2]),
                "-" => numpad_range(keys, &[3]),
                "7" => numpad_range(keys, &[4]),
                "8" => numpad_range(keys, &[5]),
                "9" => numpad_range(keys, &[6]),
                "+" => numpad_range(keys, &[7, 11]),
                "4" => numpad_range(keys, &[8]),
                "5" => numpad_range(keys, &[9]),
                "6" => numpad_range(keys, &[10]),
                "1" => numpad_range(keys, &[12]),
                "2" => numpad_range(keys, &[13]),
                "3" => numpad_range(keys, &[14]),
                "0" => numpad_range(keys, &[16, 17]),
                "." | "," => numpad_range(keys, &[18]),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

fn computer_projection_labels(keys: &[Key]) -> HashMap<KeyId, String> {
    let mut labels = HashMap::new();
    let mut label_range = |row: f32, start: usize, end: usize, label: &str| {
        for id in key_range(keys, row, start, end) {
            labels.insert(id, label.to_string());
        }
    };

    for (index, label) in [
        "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "BKSP",
    ]
    .iter()
    .enumerate()
    {
        label_range(1.0, index, index, label);
    }

    label_range(2.0, 0, 1, "TAB");
    for (index, label) in [
        "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\",
    ]
    .iter()
    .enumerate()
    {
        label_range(2.0, index + 2, index + 2, label);
    }

    label_range(3.0, 0, 0, "CAPS");
    for (index, label) in ["A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'"]
        .iter()
        .enumerate()
    {
        label_range(3.0, index + 1, index + 1, label);
    }
    label_range(3.0, 12, 13, "ENTER");

    label_range(4.0, 0, 1, "SHIFT");
    for (index, label) in ["Z", "X", "C", "V", "B", "N", "M", ",", ".", "/"]
        .iter()
        .enumerate()
    {
        label_range(4.0, index + 2, index + 2, label);
    }
    label_range(4.0, 12, 14, "SHIFT");

    for (start, end, label) in [
        (0, 0, "CTRL"),
        (1, 1, "META"),
        (2, 2, "ALT"),
        (3, 8, "SPACE"),
        (9, 9, "ALT"),
        (10, 10, "META"),
        (11, 11, "CTRL"),
    ] {
        label_range(5.0, start, end, label);
    }

    // The physical numpad maps one computer key to each drum pad (with 0 and
    // Enter each spanning two pads). These become unobtrusive corner hints
    // when drum symbols and computer-key performance mode are both enabled.
    let mut numpad: Vec<&Key> = keys
        .iter()
        .filter(|key| key.cluster == Cluster::Numpad)
        .collect();
    numpad.sort_by(|a, b| a.row.total_cmp(&b.row).then(a.col.total_cmp(&b.col)));
    for (key, label) in numpad.into_iter().zip([
        "NUM", "/", "*", "-", "7", "8", "9", "+", "4", "5", "6", "+", "1", "2", "3", "ENTER", "0",
        "0", ".", "ENTER",
    ]) {
        labels.insert(key.id, label.to_string());
    }

    labels
}

/// Closest mode without lookahead (live playback, or the out-of-range nearest-
/// keyboard-key fallback): picks whichever candidate is nearest to *any* key
/// already in `placed`, rather than a centroid blend of all of them. Used where
/// there's no well-defined note sequence to solve `shortest_path_keys` over.
fn pick_key_nearest(
    kids: &[KeyId],
    key_pos: &HashMap<KeyId, (f32, f32)>,
    placed: &HashMap<KeyId, usize>,
) -> Option<KeyId> {
    match kids {
        [] => None,
        [only] => Some(*only),
        _ if placed.is_empty() => kids.last().copied(),
        _ => kids.iter().copied().min_by(|&a, &b| {
            let dist_to_nearest_placed = |k: KeyId| -> f32 {
                let Some(&(c, r)) = key_pos.get(&k) else {
                    return f32::MAX;
                };
                placed
                    .keys()
                    .filter_map(|p| key_pos.get(p))
                    .map(|&(pc, pr)| (c - pc).powi(2) + (r - pr).powi(2))
                    .fold(f32::MAX, f32::min)
            };
            dist_to_nearest_placed(a).total_cmp(&dist_to_nearest_placed(b))
        }),
    }
}

/// True nearest-key pathfinding for Closest mode: given a time-ordered sequence
/// of notes, each with a list of candidate keys (this keyboard repeats several
/// notes across overlapping rows), finds the one-key-per-note assignment that
/// minimizes *total* travel distance across the whole sequence — a Viterbi-style
/// dynamic program, not a note-by-note greedy guess. `stages` must be non-empty
/// and every inner Vec must be non-empty.
fn shortest_path_keys(stages: &[Vec<KeyId>], key_pos: &HashMap<KeyId, (f32, f32)>) -> Vec<KeyId> {
    let dist = |a: KeyId, b: KeyId| -> f32 {
        match (key_pos.get(&a), key_pos.get(&b)) {
            (Some(&(c1, r1)), Some(&(c2, r2))) => ((c1 - c2).powi(2) + (r1 - r2).powi(2)).sqrt(),
            _ => 0.0,
        }
    };

    // dp[i][k] = (cheapest total cost to reach stages[i][k], index into stages[i-1] that got us there)
    let mut dp: Vec<Vec<(f32, usize)>> = vec![vec![(0.0, 0); stages[0].len()]];
    for i in 1..stages.len() {
        let row = stages[i]
            .iter()
            .map(|&cand| {
                stages[i - 1]
                    .iter()
                    .enumerate()
                    .map(|(j, &prev)| (dp[i - 1][j].0 + dist(prev, cand), j))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap()
            })
            .collect();
        dp.push(row);
    }

    let last = stages.len() - 1;
    let mut idx = (0..dp[last].len())
        .min_by(|&a, &b| dp[last][a].0.total_cmp(&dp[last][b].0))
        .unwrap();

    let mut chosen = vec![stages[0][0]; stages.len()];
    for i in (0..stages.len()).rev() {
        chosen[i] = stages[i][idx];
        idx = dp[i][idx].1;
    }
    chosen
}

/// Converts the actual note-on ticks assigned to each highlighted key into
/// human-friendly, one-based play steps. Equal ticks deliberately receive the
/// same number because those keys form a chord; a key used again later keeps
/// every step number so repeated notes are not lost in the highlight map.
fn play_order_from_ticks(mut ticks_by_key: HashMap<KeyId, Vec<u64>>) -> HashMap<KeyId, Vec<usize>> {
    let mut ticks: Vec<u64> = ticks_by_key.values().flatten().copied().collect();
    ticks.sort_unstable();
    ticks.dedup();

    ticks_by_key
        .drain()
        .map(|(key, mut key_ticks)| {
            key_ticks.sort_unstable();
            key_ticks.dedup();
            let steps = key_ticks
                .into_iter()
                .filter_map(|tick| ticks.binary_search(&tick).ok().map(|index| index + 1))
                .collect();
            (key, steps)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MidiInputAction {
    NoteOn { note: u8, velocity: u8, channel: u8 },
    NoteOff { note: u8, channel: u8 },
    AllNotesOff { channel: u8 },
}

/// Reduces the MIDI messages relevant to this viewer to explicit actions.
/// Note On with velocity zero is the MIDI-standard spelling of Note Off.
fn parse_midi_input(data: &[u8]) -> Option<MidiInputAction> {
    let status = *data.first()?;
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x90 if data.len() >= 3 && data[2] > 0 => Some(MidiInputAction::NoteOn {
            note: data[1] & 0x7F,
            velocity: data[2] & 0x7F,
            channel,
        }),
        0x80 | 0x90 if data.len() >= 3 => Some(MidiInputAction::NoteOff {
            note: data[1] & 0x7F,
            channel,
        }),
        0xB0 if data.len() >= 3 && matches!(data[1], 120 | 123) => {
            Some(MidiInputAction::AllNotesOff { channel })
        }
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn next_web_midi_port(current: Option<&str>, ports: &[playback::MidiPortInfo]) -> Option<String> {
    match current.and_then(|id| ports.iter().position(|port| port.id == id)) {
        Some(index) if index + 1 < ports.len() => Some(ports[index + 1].id.clone()),
        Some(_) => None,
        None => ports.first().map(|port| port.id.clone()),
    }
}

impl App {
    #[cfg(target_arch = "wasm32")]
    fn ensure_web_audio(&mut self) {
        if self._audio_stream.is_some() {
            return;
        }

        // `start_soft_synth` starts CPAL's scheduler. It must happen exactly
        // once: calling Stream::play repeatedly creates additional permanent
        // Web Audio buffer chains and eventually starves the UI thread.
        match synth::start_soft_synth() {
            Ok((synth, stream)) => {
                self.soft_synth = Some(synth);
                self._audio_stream = Some(stream);
                self.audio_error = None;
                // The synth is created fresh here, well after any
                // URL-restored waveform/knob settings were applied to
                // `self` — push them in now so it doesn't silently start
                // at engine defaults.
                self.sync_engine_state();
            }
            Err(error) => self.audio_error = Some(error),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn refresh_web_midi_ports(&mut self, auto_select: bool) {
        let Some(access) = self.web_midi_access.as_ref() else {
            return;
        };
        let inputs = access.input_ports();
        let outputs = access.output_ports();

        let input_id = self
            .web_midi_input_id
            .as_deref()
            .filter(|id| inputs.iter().any(|port| port.id == *id))
            .map(str::to_string)
            .or_else(|| {
                auto_select
                    .then(|| inputs.first().map(|port| port.id.clone()))
                    .flatten()
            });
        let output_id = self
            .web_midi_output_id
            .as_deref()
            .filter(|id| outputs.iter().any(|port| port.id == *id))
            .map(str::to_string)
            .or_else(|| {
                auto_select
                    .then(|| outputs.first().map(|port| port.id.clone()))
                    .flatten()
            });

        let reconnect_input = input_id != self.web_midi_input_id
            || (input_id.is_some() && self.web_midi_input.is_none());
        let reconnect_output = output_id != self.web_midi_output_id
            || (output_id.is_some() && self.web_midi_output.is_none());
        self.web_midi_inputs = inputs;
        self.web_midi_outputs = outputs;

        if reconnect_input {
            self.select_web_midi_input(input_id);
        }
        if reconnect_output {
            self.select_web_midi_output(output_id);
        }

        if self.web_midi_inputs.is_empty() && self.web_midi_outputs.is_empty() {
            self.web_midi_status = Some(if self.web_midi_retry_count > 0 {
                format!(
                    "MIDI access granted — still no devices found (checked {}×) · click MIDI IN/OUT to retry",
                    self.web_midi_retry_count + 1
                )
            } else {
                "MIDI access granted — no devices found · click MIDI IN/OUT to retry".to_string()
            });
        } else if self
            .web_midi_status
            .as_deref()
            .is_none_or(|s| !s.starts_with("MIDI error:"))
        {
            self.web_midi_status = Some(format!(
                "MIDI connected · {} input{} · {} output{}",
                self.web_midi_inputs.len(),
                if self.web_midi_inputs.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.web_midi_outputs.len(),
                if self.web_midi_outputs.len() == 1 {
                    ""
                } else {
                    "s"
                },
            ));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn select_web_midi_input(&mut self, id: Option<String>) {
        // A disconnected input cannot deliver releases for notes it already
        // sent through, so explicitly silence the destination before swapping.
        if self.web_midi_thru_enabled {
            if let Some(ref output) = self.web_midi_output {
                output.all_notes_off();
            }
        }
        self.release_web_midi_input_notes(None);
        self.web_midi_input = None;
        self.web_midi_input_id = id.clone();
        let Some(id) = id else { return };
        let Some(access) = self.web_midi_access.as_ref() else {
            return;
        };
        match access.connect_input(&id, Arc::clone(&self.web_midi_events)) {
            Ok(connection) => self.web_midi_input = Some(connection),
            Err(error) => {
                self.web_midi_input_id = None;
                self.web_midi_status = Some(format!("MIDI error: {error}"));
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn select_web_midi_output(&mut self, id: Option<String>) {
        if let Some(output) = self.web_midi_output.take() {
            output.all_notes_off();
        }
        self.web_midi_output_id = id.clone();
        self.web_midi_output = id.as_deref().and_then(|id| {
            self.web_midi_access
                .as_ref()
                .and_then(|access| match access.connect_output(id) {
                    Ok(output) => Some(output),
                    Err(error) => {
                        self.web_midi_status = Some(format!("MIDI error: {error}"));
                        None
                    }
                })
        });
        if self.web_midi_output.is_none() && id.is_some() {
            self.web_midi_output_id = None;
        }

        if let Some(ref synth) = self.soft_synth {
            if let Ok(mut synth) = synth.lock() {
                synth.all_notes_off();
            }
        }
        if let Some(ref handle) = self.playback_handle {
            handle
                .cmd_tx
                .send(PlayCmd::SetMidiOutput(self.web_midi_output.clone()))
                .ok();
        }
    }

    /// Sends the untouched browser MIDI payload to the selected output. This
    /// happens independently of `parse_midi_input`, which intentionally only
    /// understands the subset needed to animate keys and drive the soft synth.
    #[cfg(target_arch = "wasm32")]
    fn forward_web_midi_thru(&mut self, data: &[u8]) {
        if !self.web_midi_thru_enabled {
            return;
        }
        let error = self
            .web_midi_output
            .as_ref()
            .and_then(|output| output.send(data).err());
        if let Some(error) = error {
            self.web_midi_thru_enabled = false;
            self.web_midi_status = Some(format!("MIDI error: thru failed — {error}"));
            self.sync_url();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_midi_key_for_note(&self, note: u8, channel: u8) -> Option<(KeyId, bool)> {
        if channel == synth::DRUM_CHANNEL {
            return self
                .drum_note_to_key
                .get(&note)
                .copied()
                .map(|key| (key, false));
        }
        let (board_note, clipped) = midi_input_board_note(note, self.web_midi_input_octave);
        if let Some(keys) = self.note_to_all_keys.get(&board_note) {
            let key = if self.key_pick_mode == KeyPickMode::Closest {
                pick_key_nearest(keys, &self.key_pos, &self.web_midi_highlighted)
            } else {
                pick_key_fixed(keys, self.key_pick_mode)
            }?;
            return Some((key, clipped));
        }
        let nearest = self.nearest_keyboard_note(board_note)?;
        let keys = self.note_to_all_keys.get(&nearest)?;
        let key = if self.key_pick_mode == KeyPickMode::Closest {
            pick_key_nearest(keys, &self.key_pos, &self.web_midi_highlighted)
        } else {
            pick_key_fixed(keys, self.key_pick_mode)
        }?;
        Some((key, true))
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_web_midi_input(&mut self, action: MidiInputAction) {
        match action {
            MidiInputAction::NoteOn {
                note,
                velocity,
                channel,
            } => {
                if self
                    .audio_enabled
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    if let Some(ref synth) = self.soft_synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.note_on(note, velocity, channel);
                        }
                    }
                }
                if let Some((key, warning)) = self.web_midi_key_for_note(note, channel) {
                    if let Some(old_key) = self.web_midi_active_notes.insert((channel, note), key) {
                        if old_key != key
                            && !self.web_midi_active_notes.values().any(|&id| id == old_key)
                        {
                            self.web_midi_highlighted.remove(&old_key);
                        }
                    }
                    self.web_midi_highlighted
                        .insert(key, if warning { usize::MAX - 1 } else { usize::MAX });
                }
            }
            MidiInputAction::NoteOff { note, channel } => {
                if let Some(ref synth) = self.soft_synth {
                    if let Ok(mut synth) = synth.lock() {
                        synth.note_off(note, channel);
                    }
                }
                if let Some(key) = self.web_midi_active_notes.remove(&(channel, note)) {
                    if !self.web_midi_active_notes.values().any(|&id| id == key) {
                        self.web_midi_highlighted.remove(&key);
                    }
                }
            }
            MidiInputAction::AllNotesOff { channel } => {
                self.release_web_midi_input_notes(Some(channel))
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn release_web_midi_input_notes(&mut self, channel: Option<u8>) {
        let notes: Vec<(u8, u8)> = self
            .web_midi_active_notes
            .keys()
            .copied()
            .filter(|(active_channel, _)| channel.is_none_or(|channel| *active_channel == channel))
            .collect();
        for (active_channel, note) in notes {
            if let Some(ref synth) = self.soft_synth {
                if let Ok(mut synth) = synth.lock() {
                    synth.note_off(note, active_channel);
                }
            }
            if let Some(key) = self.web_midi_active_notes.remove(&(active_channel, note)) {
                if !self.web_midi_active_notes.values().any(|&id| id == key) {
                    self.web_midi_highlighted.remove(&key);
                }
            }
        }
        if channel.is_none() {
            self.web_midi_events
                .lock()
                .map(|mut queue| queue.clear())
                .ok();
        }
    }

    /// Precomputes Closest mode's answer for every melodic note value in the
    /// file: one `shortest_path_keys` solve over the *entire* time-ordered
    /// sequence, producing a single preferred key per (octave-shifted) note.
    /// Every highlight path — live playback, the selection view, the all-notes
    /// overlay — reads from this shared map instead of each deciding on its
    /// own. That matters most for live playback: without a precomputed answer
    /// it can only greedily pick "nearest to whatever's currently lit," which
    /// loses all context the moment a note's highlight clears — exactly what
    /// happens between two non-overlapping notes — and that's what was
    /// producing the wrong jumps.
    /// The pitch a note lands on once both the whole-song octave offset and
    /// its track's own octave shift are applied — this is what decides which
    /// physical key lights up, or whether it fits the keyboard at all.
    fn shifted_note(&self, midi_note: u8, track: usize) -> u8 {
        let shift = midi::combined_octave_shift(self.octave_offset, &self.track_octave, track);
        (midi_note as i16 + shift).clamp(0, 127) as u8
    }

    fn rebuild_closest_key_map(&mut self) {
        self.closest_key_for_note.clear();
        if self.key_pick_mode != KeyPickMode::Closest {
            return;
        }
        let Some(ref f) = self.midi_file else { return };

        let mut shifted_notes: Vec<u8> = Vec::new();
        let mut stages: Vec<Vec<KeyId>> = Vec::new();
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) {
                continue;
            }
            if note.channel == synth::DRUM_CHANNEL {
                continue;
            }
            let shifted = self.shifted_note(note.midi_note, note.track);
            if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                shifted_notes.push(shifted);
                stages.push(kids.clone());
            }
        }
        if stages.is_empty() {
            return;
        }

        for (shifted, kid) in shifted_notes
            .into_iter()
            .zip(shortest_path_keys(&stages, &self.key_pos))
        {
            self.closest_key_for_note.insert(shifted, kid);
        }
    }

    fn rebuild_all_notes_cache(&mut self) {
        self.all_notes_cache.clear();
        let Some(ref f) = self.midi_file else { return };

        if self.key_pick_mode == KeyPickMode::Closest {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) {
                    continue;
                }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.all_notes_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = self.shifted_note(note.midi_note, note.track);
                if let Some(&kid) = self.closest_key_for_note.get(&shifted) {
                    self.all_notes_cache.insert(kid, note.track);
                }
            }
        } else {
            // In-range notes (track color). Drum-channel notes go straight to
            // their dedicated pad — no octave shift, no nearest-key fallback.
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) {
                    continue;
                }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.all_notes_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = self.shifted_note(note.midi_note, note.track);
                if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                    if let Some(kid) = pick_key_fixed(kids, self.key_pick_mode) {
                        self.all_notes_cache.insert(kid, note.track);
                    }
                }
            }
        }

        // Out-of-range melodic notes — highlight the nearest keyboard key with the
        // warning sentinel (usize::MAX - 1) only if that key isn't already lit.
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) {
                continue;
            }
            if note.channel == synth::DRUM_CHANNEL {
                continue;
            }
            let shifted = self.shifted_note(note.midi_note, note.track);
            if self.note_to_all_keys.contains_key(&shifted) {
                continue;
            }
            if let Some(nearest) = self.nearest_keyboard_note(shifted) {
                if let Some(kids) = self.note_to_all_keys.get(&nearest) {
                    let kid = if self.key_pick_mode == KeyPickMode::Closest {
                        pick_key_nearest(kids, &self.key_pos, &self.all_notes_cache)
                    } else {
                        pick_key_fixed(kids, self.key_pick_mode)
                    };
                    if let Some(kid) = kid {
                        self.all_notes_cache.entry(kid).or_insert(usize::MAX - 1);
                    }
                }
            }
        }
    }

    /// Recomputes `selection_highlight_cache`: the keys lit up for the notes under
    /// the current staff selection, so a drag on the staff shows exactly what's
    /// selected on the keyboard.
    fn rebuild_selection_highlight(&mut self) {
        self.selection_highlight_cache.clear();
        self.selection_play_order.clear();
        let Some(ref f) = self.midi_file else { return };
        let Some((s, e)) = self.staff_selection else {
            return;
        };
        let e = e.max(s + 1);
        let in_range = |note: &midi::Note| note.start_tick < e && note.end_tick > s;
        let mut play_ticks: HashMap<KeyId, Vec<u64>> = HashMap::new();

        if self.key_pick_mode == KeyPickMode::Closest {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) {
                    continue;
                }
                if !in_range(note) {
                    continue;
                }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.selection_highlight_cache.insert(kid, note.track);
                        play_ticks.entry(kid).or_default().push(note.start_tick);
                    }
                    continue;
                }

                let shifted = self.shifted_note(note.midi_note, note.track);
                if let Some(&kid) = self.closest_key_for_note.get(&shifted) {
                    self.selection_highlight_cache.insert(kid, note.track);
                    play_ticks.entry(kid).or_default().push(note.start_tick);
                }
            }
        } else {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) {
                    continue;
                }
                if !in_range(note) {
                    continue;
                }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.selection_highlight_cache.insert(kid, note.track);
                        play_ticks.entry(kid).or_default().push(note.start_tick);
                    }
                    continue;
                }

                let shifted = self.shifted_note(note.midi_note, note.track);
                if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                    if let Some(kid) = pick_key_fixed(kids, self.key_pick_mode) {
                        self.selection_highlight_cache.insert(kid, note.track);
                        play_ticks.entry(kid).or_default().push(note.start_tick);
                    }
                }
            }
        }

        // Out-of-range melodic notes within the selection — same nearest-keyboard
        // fallback as rebuild_all_notes_cache.
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) {
                continue;
            }
            if !in_range(note) {
                continue;
            }
            if note.channel == synth::DRUM_CHANNEL {
                continue;
            }
            let shifted = self.shifted_note(note.midi_note, note.track);
            if self.note_to_all_keys.contains_key(&shifted) {
                continue;
            }
            if let Some(nearest) = self.nearest_keyboard_note(shifted) {
                if let Some(kids) = self.note_to_all_keys.get(&nearest) {
                    let kid = if self.key_pick_mode == KeyPickMode::Closest {
                        pick_key_nearest(kids, &self.key_pos, &self.selection_highlight_cache)
                    } else {
                        pick_key_fixed(kids, self.key_pick_mode)
                    };
                    if let Some(kid) = kid {
                        self.selection_highlight_cache
                            .entry(kid)
                            .or_insert(usize::MAX - 1);
                        play_ticks.entry(kid).or_default().push(note.start_tick);
                    }
                }
            }
        }

        self.selection_play_order = play_order_from_ticks(play_ticks);
    }

    /// Tells the playback thread about a new octave offset, so it can tell which
    /// notes actually land on the physical keyboard and skip audio for the rest.
    fn sync_octave_offset(&self) {
        if let Some(ref h) = self.playback_handle {
            h.cmd_tx
                .send(PlayCmd::SetOctaveOffset(self.octave_offset))
                .ok();
        }
    }

    /// Tells the playback thread about a new playback speed multiplier.
    fn sync_playback_speed(&self) {
        if let Some(ref h) = self.playback_handle {
            h.cmd_tx.send(PlayCmd::SetSpeed(self.playback_speed)).ok();
        }
    }

    /// The waveforms currently layered together, derived from whichever
    /// Nav waveform-select keys are toggled on.
    fn active_waveforms(&self) -> Vec<synth::Waveform> {
        self.keys
            .iter()
            .filter(|key| self.waveform_keys.contains(&key.id))
            .filter_map(|key| waveform_for_label(key.label))
            .collect()
    }

    fn sync_active_waveforms(&self) {
        let waveforms = self.active_waveforms();
        if let Some(ref synth) = self.soft_synth {
            if let Ok(mut synth) = synth.lock() {
                synth.set_active_waveforms(waveforms.clone());
            }
        }
        if let Some(ref h) = self.playback_handle {
            h.cmd_tx.send(PlayCmd::SetWaveforms(waveforms)).ok();
        }
    }

    /// Pushes the current loop range to the playback thread: the staff
    /// selection when one is active, the whole file otherwise, or `None`
    /// when looping is off. Called whenever looping is toggled or the
    /// selection changes, so the active loop always tracks what's selected.
    fn sync_loop_range(&self) {
        let Some(ref h) = self.playback_handle else {
            return;
        };
        let range = self
            .looper_enabled
            .then(|| {
                self.staff_selection
                    .map(|(s, e)| (s, e.max(s + 1)))
                    .or_else(|| self.midi_file.as_ref().map(|f| (0, f.total_ticks)))
            })
            .flatten();
        h.cmd_tx.send(PlayCmd::SetLoopRange(range)).ok();
    }

    /// Pushes every App-level audio setting (waveforms, knobs) directly into
    /// `self.soft_synth`. Needed on the web build: the synth is created
    /// lazily on first user gesture (see `ensure_web_audio`), well after any
    /// URL-restored settings were already applied to `self`, so a freshly
    /// created engine wouldn't otherwise see them until something changed.
    #[cfg(target_arch = "wasm32")]
    fn sync_engine_state(&self) {
        self.sync_active_waveforms();
        if let Some(ref synth) = self.soft_synth {
            if let Ok(mut synth) = synth.lock() {
                for (i, param) in synth::KNOB_PARAMS.iter().enumerate() {
                    let pos = self
                        .knob_values
                        .get(i)
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let real = param.min + pos * (param.max - param.min);
                    synth.set_knob(i as u8, real);
                }
            }
        }
    }

    fn key_sound(&self, id: KeyId) -> Option<(u8, u8)> {
        if let Some(note) = self
            .keys
            .iter()
            .find(|key| key.id == id)
            .and_then(|key| key.midi_note)
        {
            return Some((note, self.live_channel));
        }

        self.drum_note_to_key
            .iter()
            .find_map(|(&note, &key_id)| (key_id == id).then_some((note, synth::DRUM_CHANNEL)))
    }

    fn press_board_key(&mut self, id: KeyId) {
        self.pressed_keys.insert(id);
        let is_waveform_key = self
            .keys
            .iter()
            .find(|key| key.id == id && key.cluster == Cluster::Nav)
            .is_some_and(|key| waveform_for_label(key.label).is_some());

        if is_waveform_key {
            toggle_waveform_key(&mut self.waveform_keys, id);
            self.sync_active_waveforms();
            self.sync_url();
            return;
        }

        if let Some((note, channel)) = self.key_sound(id) {
            // Drum hits aren't pitched — GM percussion notes select a sound,
            // not a frequency — so the live octave shift doesn't apply.
            let shifted = if channel == synth::DRUM_CHANNEL {
                note
            } else {
                (note as i16 + self.live_octave as i16).clamp(0, 127) as u8
            };
            self.live_note_overrides.insert(id, (shifted, channel));
            if self
                .audio_enabled
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx
                        .send(PlayCmd::LiveNoteOn(shifted, 108, channel))
                        .ok();
                } else {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(ref output) = self.web_midi_output {
                        let _ = output.send(&[0x90 | (channel & 0x0F), shifted, 108]);
                        return;
                    }
                    if let Some(ref synth) = self.soft_synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.note_on(shifted, 108, channel);
                        }
                    }
                }
            }
        }
    }

    fn release_board_key(&mut self, id: KeyId) {
        self.pressed_keys.remove(&id);
        // Use whatever note was actually turned on for this key, even if
        // live_octave has changed since — otherwise the wrong voice (or none)
        // gets the note-off and the original one hangs.
        if let Some((note, channel)) = self.live_note_overrides.remove(&id) {
            if let Some(ref h) = self.playback_handle {
                h.cmd_tx.send(PlayCmd::LiveNoteOff(note, channel)).ok();
            } else {
                #[cfg(target_arch = "wasm32")]
                if let Some(ref output) = self.web_midi_output {
                    let _ = output.send(&[0x80 | (channel & 0x0F), note, 0]);
                    return;
                }
                if let Some(ref synth) = self.soft_synth {
                    if let Ok(mut synth) = synth.lock() {
                        synth.note_off(note, channel);
                    }
                }
            }
        }
    }

    fn release_computer_keys(&mut self) {
        let keys: Vec<KeyId> = self
            .computer_keys_down
            .drain()
            .flat_map(|(_, keys)| keys)
            .collect();
        for id in keys {
            self.release_board_key(id);
        }
    }

    fn nearest_keyboard_note(&self, note: u8) -> Option<u8> {
        let s = &self.keyboard_notes_sorted;
        if s.is_empty() {
            return None;
        }
        let pos = s.partition_point(|&n| n < note);
        Some(if pos == 0 {
            s[0]
        } else if pos == s.len() {
            *s.last().unwrap()
        } else {
            let below = s[pos - 1];
            let above = s[pos];
            if note - below <= above - note {
                below
            } else {
                above
            }
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowResized(size) => {
                self.window_size = size;
                Task::none()
            }

            // ── Keyboard ──────────────────────────────────────────────────
            Message::KeyPressed(id) => {
                #[cfg(target_arch = "wasm32")]
                self.ensure_web_audio();
                self.press_board_key(id);
                Task::none()
            }

            Message::KeyReleased(id) => {
                self.release_board_key(id);
                Task::none()
            }

            Message::KnobChanged(index, pos) => {
                let pos = pos.clamp(0.0, 1.0);
                if let Some(slot) = self.knob_values.get_mut(index as usize) {
                    *slot = pos;
                }
                if let Some(param) = synth::KNOB_PARAMS.get(index as usize) {
                    let value = param.min + pos * (param.max - param.min);
                    if let Some(ref synth) = self.soft_synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.set_knob(index, value);
                        }
                    }
                    if let Some(ref h) = self.playback_handle {
                        h.cmd_tx.send(PlayCmd::SetKnob(index, value)).ok();
                    }
                }
                self.sync_url();
                Task::none()
            }

            Message::ToggleKeyboardHits => {
                if self.keyboard_hits_enabled {
                    self.release_computer_keys();
                }
                self.keyboard_hits_enabled = !self.keyboard_hits_enabled;
                self.sync_url();
                Task::none()
            }

            Message::ToggleDrumSymbols => {
                self.drum_symbols_enabled = !self.drum_symbols_enabled;
                self.sync_url();
                Task::none()
            }

            Message::ToggleCompactKeyboard => {
                self.compact_keyboard = !self.compact_keyboard;
                self.keyboard_height_override = None;
                self.sync_url();
                Task::none()
            }

            Message::KeyboardHeightChanged(height) => {
                self.keyboard_height_override = Some(height);
                Task::none()
            }

            Message::ComputerKeyPressed(key) => {
                if self.keyboard_hits_enabled && !self.computer_keys_down.contains_key(&key) {
                    #[cfg(target_arch = "wasm32")]
                    self.ensure_web_audio();
                    let targets = mapped_computer_keys(&self.keys, &key);
                    for &id in &targets {
                        self.press_board_key(id);
                    }
                    self.computer_keys_down.insert(key, targets);
                }
                Task::none()
            }

            Message::ComputerKeyReleased(key) => {
                if let Some(targets) = self.computer_keys_down.remove(&key) {
                    for id in targets {
                        self.release_board_key(id);
                    }
                }
                Task::none()
            }

            Message::ReleaseComputerKeys => {
                self.release_computer_keys();
                Task::none()
            }

            // ── File loading ──────────────────────────────────────────────
            Message::OpenFile => {
                #[cfg(target_arch = "wasm32")]
                self.ensure_web_audio();
                Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("MIDI", &["mid", "midi"])
                            .pick_file()
                            .await;
                        match handle {
                            Some(handle) => Some(handle.read().await),
                            None => None,
                        }
                    },
                    Message::FileChosen,
                )
            }

            Message::FileChosen(None) => Task::none(),
            Message::FileChosen(Some(bytes)) => {
                Task::perform(async move { midi::load_bytes(&bytes) }, Message::MidiLoaded)
            }

            Message::MidiLoaded(Err(e)) => {
                self.load_error = Some(e);
                Task::none()
            }
            Message::MidiLoaded(Ok(file)) => {
                let (offset, covered, total) =
                    midi::best_octave_offset(&file, &self.keyboard_notes);
                self.skipped_notes = total.saturating_sub(covered);
                self.octave_offset = offset;
                self.track_muted = vec![false; file.tracks.len()];
                self.track_channel = file.tracks.iter().map(|t| t.channel.unwrap_or(0)).collect();
                self.track_octave = vec![0i8; file.tracks.len()];
                self.load_error = None;
                self.play_state = PlayState::Stopped;
                self.position_tick = 0;
                self.highlighted.clear();
                self.active_highlight_keys.clear();
                self.staff_selection = None;
                self.selection_highlight_cache.clear();
                self.selection_play_order.clear();

                // Drop any existing playback thread
                self.playback_handle = None;

                // Spawn a new idle playback thread ready for this file
                #[cfg(not(target_arch = "wasm32"))]
                let conn = playback::open_output(self.midi_port_idx);
                #[cfg(target_arch = "wasm32")]
                let conn = self.web_midi_output.clone();
                let handle = playback::spawn(
                    Arc::new(file.clone()),
                    Arc::clone(&self.playback_events),
                    Arc::clone(&self.audio_enabled),
                    self.track_muted.clone(),
                    self.track_channel.clone(),
                    self.track_octave.clone(),
                    conn,
                    Arc::new(self.keyboard_notes.clone()),
                    self.octave_offset,
                    self.active_waveforms(),
                    self.soft_synth.as_ref().map(Arc::clone),
                    self.playback_speed,
                );
                self.playback_handle = Some(handle);
                self.midi_file = Some(file);
                self.sync_loop_range();
                self.rebuild_closest_key_map();
                self.rebuild_all_notes_cache();
                Task::none()
            }

            // ── Pitch nudge ────────────────────────────────────────────────
            Message::PitchUp => {
                self.octave_offset = self.octave_offset.saturating_add(self.pitch_step);
                self.sync_octave_offset();
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                Task::none()
            }
            Message::PitchDown => {
                self.octave_offset = self.octave_offset.saturating_sub(self.pitch_step);
                self.sync_octave_offset();
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                Task::none()
            }
            Message::LiveOctaveUp => {
                self.live_octave = self.live_octave.saturating_add(12);
                self.sync_url();
                Task::none()
            }
            Message::LiveOctaveDown => {
                self.live_octave = self.live_octave.saturating_sub(12);
                self.sync_url();
                Task::none()
            }
            Message::PitchStepToggle => {
                self.pitch_step = if self.pitch_step == 12 { 1 } else { 12 };
                Task::none()
            }
            Message::PitchReset => {
                self.octave_offset = 0;
                self.sync_octave_offset();
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                Task::none()
            }
            Message::OctaveLayoutToggle => {
                self.key_pick_mode = self.key_pick_mode.next();
                self.highlighted.clear();
                self.active_highlight_keys.clear();
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                self.sync_url();
                Task::none()
            }
            Message::ToggleAllNotes => {
                self.show_all_notes = !self.show_all_notes;
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                } else {
                    self.highlighted.clear();
                    self.active_highlight_keys.clear();
                }
                Task::none()
            }

            // ── Tracks ─────────────────────────────────────────────────────
            Message::TrackMuted(idx, muted) => {
                if let Some(s) = self.track_muted.get_mut(idx) {
                    *s = muted;
                }
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetTrackMuted(idx, muted)).ok();
                }
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                Task::none()
            }
            Message::TrackChannel(idx, channel) => {
                if let Some(s) = self.track_channel.get_mut(idx) {
                    *s = channel;
                }
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetTrackChannel(idx, channel)).ok();
                }
                Task::none()
            }
            Message::TrackOctave(idx, octaves) => {
                if let Some(s) = self.track_octave.get_mut(idx) {
                    *s = octaves;
                }
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetTrackOctave(idx, octaves)).ok();
                }
                self.rebuild_closest_key_map();
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                }
                if self.staff_selection.is_some() {
                    self.rebuild_selection_highlight();
                }
                Task::none()
            }

            // ── Transport ──────────────────────────────────────────────────
            Message::Play => {
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::Play).ok();
                    self.play_state = PlayState::Playing;
                }
                Task::none()
            }
            Message::Pause => {
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::Pause).ok();
                    self.play_state = PlayState::Paused;
                }
                Task::none()
            }
            Message::Stop => {
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::Stop).ok();
                }
                self.play_state = PlayState::Stopped;
                self.position_tick = 0;
                self.highlighted.clear();
                self.active_highlight_keys.clear();
                Task::none()
            }
            Message::ToggleLooper => {
                self.looper_enabled = !self.looper_enabled;
                self.sync_loop_range();
                self.sync_url();
                Task::none()
            }
            Message::SeekTo(progress) => {
                if let Some(ref f) = self.midi_file {
                    let tick = (progress.clamp(0.0, 1.0) * f.total_ticks as f32) as u64;
                    // Discard notes and position reports from the old timeline
                    // before asking the playback clock to re-anchor.
                    self.highlighted.clear();
                    self.active_highlight_keys.clear();
                    self.playback_events.lock().unwrap().clear();
                    if let Some(ref h) = self.playback_handle {
                        h.cmd_tx.send(PlayCmd::SeekTo(tick)).ok();
                    }
                    self.position_tick = tick;
                }
                Task::none()
            }
            Message::SpeedChanged(value) => {
                self.playback_speed = value.clamp(SPEED_MIN, SPEED_MAX);
                self.sync_playback_speed();
                Task::none()
            }

            // ── Poll playback events (fired by subscription every 16 ms) ──
            Message::PollPlayback => {
                #[cfg(target_arch = "wasm32")]
                if let Some(ref h) = self.playback_handle {
                    h.poll();
                }

                let events: Vec<PlayEvent> = {
                    let mut q = self.playback_events.lock().unwrap();
                    q.drain(..).collect()
                };
                for evt in events {
                    match evt {
                        PlayEvent::NoteOn(note, track, channel) => {
                            if !self.show_all_notes {
                                if channel == synth::DRUM_CHANNEL {
                                    if let Some(&kid) = self.drum_note_to_key.get(&note) {
                                        self.highlighted.insert(kid, track);
                                    }
                                    continue;
                                }
                                let shifted = self.shifted_note(note, track);
                                // Closest mode reads the whole-file precomputed answer
                                // (rebuild_closest_key_map) so live playback always agrees
                                // with the selection/all-notes views instead of re-deciding
                                // greedily with no lookahead.
                                let kid = if self.key_pick_mode == KeyPickMode::Closest {
                                    self.closest_key_for_note.get(&shifted).copied()
                                } else if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                                    pick_key_fixed(kids, self.key_pick_mode)
                                } else {
                                    None
                                };
                                if let Some(kid) = kid {
                                    self.highlighted.insert(kid, track);
                                    // A per-track octave shift means the same raw
                                    // (note, channel) can map to a different key
                                    // depending on which track it came from, so the
                                    // matching note-off can't safely recompute the
                                    // shift itself — remember exactly what lit here.
                                    self.active_highlight_keys.insert((note, channel), kid);
                                }
                            }
                        }
                        PlayEvent::NoteOff(note, channel) => {
                            if !self.show_all_notes {
                                if channel == synth::DRUM_CHANNEL {
                                    if let Some(&kid) = self.drum_note_to_key.get(&note) {
                                        self.highlighted.remove(&kid);
                                    }
                                    continue;
                                }
                                if let Some(kid) =
                                    self.active_highlight_keys.remove(&(note, channel))
                                {
                                    self.highlighted.remove(&kid);
                                }
                            }
                        }
                        PlayEvent::Position(t) => {
                            self.position_tick = t;
                        }
                        PlayEvent::Done => {
                            self.position_tick = 0;
                            self.highlighted.clear();
                            self.active_highlight_keys.clear();
                            let can_loop = self.looper_enabled
                                && self
                                    .midi_file
                                    .as_ref()
                                    .is_some_and(|file| file.total_ticks > 0);
                            if can_loop {
                                if let Some(ref handle) = self.playback_handle {
                                    handle.cmd_tx.send(PlayCmd::Play).ok();
                                }
                                self.play_state = PlayState::Playing;
                            } else {
                                self.play_state = PlayState::Stopped;
                            }
                        }
                    }
                }

                #[cfg(target_arch = "wasm32")]
                {
                    let ports_changed = self
                        .web_midi_access
                        .as_ref()
                        .is_some_and(playback::MidiAccessHandle::take_ports_changed);
                    if ports_changed {
                        self.refresh_web_midi_ports(false);
                    }
                    let midi_messages: Vec<Vec<u8>> = self
                        .web_midi_events
                        .lock()
                        .map(|mut queue| queue.drain(..).collect())
                        .unwrap_or_default();
                    for data in midi_messages {
                        self.forward_web_midi_thru(&data);
                        if let Some(action) = parse_midi_input(&data) {
                            self.handle_web_midi_input(action);
                        }
                    }
                }
                Task::none()
            }

            // ── Audio toggle ───────────────────────────────────────────────
            Message::ToggleAudio => {
                let was = self
                    .audio_enabled
                    .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetAudio(!was)).ok();
                } else if was {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(ref output) = self.web_midi_output {
                        output.all_notes_off();
                    }
                    if let Some(ref synth) = self.soft_synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.all_notes_off();
                        }
                    }
                }
                self.sync_url();
                Task::none()
            }

            // ── Port cycling ───────────────────────────────────────────────
            Message::NextPort => {
                if !self.midi_port_names.is_empty() {
                    self.midi_port_idx = (self.midi_port_idx + 1) % self.midi_port_names.len();
                }
                Task::none()
            }

            Message::LiveChannel(channel) => {
                self.live_channel = channel;
                Task::none()
            }

            #[cfg(target_arch = "wasm32")]
            Message::RequestWebMidi => {
                self.ensure_web_audio();
                self.web_midi_pending = true;
                self.web_midi_status = Some("Waiting for browser MIDI permission…".to_string());
                match playback::request_midi_access() {
                    Ok(promise) => Task::perform(
                        async move { playback::resolve_midi_access(promise).await },
                        Message::WebMidiReady,
                    ),
                    Err(error) => {
                        self.web_midi_pending = false;
                        self.web_midi_status = Some(playback::midi_access_error_status(&error));
                        Task::none()
                    }
                }
            }

            #[cfg(target_arch = "wasm32")]
            Message::WebMidiReady(result) => {
                self.web_midi_pending = false;
                match result {
                    Ok(access) => {
                        self.web_midi_access = Some(playback::MidiAccessHandle::new(access));
                        self.web_midi_status = None;
                        self.refresh_web_midi_ports(true);
                    }
                    Err(error) => {
                        self.web_midi_status = Some(playback::midi_access_error_status(&error));
                    }
                }
                Task::none()
            }

            #[cfg(target_arch = "wasm32")]
            Message::NextWebMidiInput => {
                let id =
                    next_web_midi_port(self.web_midi_input_id.as_deref(), &self.web_midi_inputs);
                self.select_web_midi_input(id);
                Task::none()
            }

            #[cfg(target_arch = "wasm32")]
            Message::NextWebMidiOutput => {
                let id =
                    next_web_midi_port(self.web_midi_output_id.as_deref(), &self.web_midi_outputs);
                self.select_web_midi_output(id);
                Task::none()
            }

            // Devices plugged in after granting access should show up via
            // `onstatechange`, but that event is easy to miss (e.g. a device
            // that was already connected but slow to enumerate). Re-scanning
            // on demand gives the user a way to retry without reloading.
            #[cfg(target_arch = "wasm32")]
            Message::RefreshWebMidiPorts => {
                self.web_midi_retry_count += 1;
                self.refresh_web_midi_ports(true);
                Task::none()
            }

            #[cfg(target_arch = "wasm32")]
            Message::WebMidiInputOctave(octave) => {
                // Clear held highlights before changing their mapping; their
                // raw MIDI notes remain the synth's source of truth.
                self.release_web_midi_input_notes(None);
                self.web_midi_input_octave =
                    octave.clamp(MIN_MIDI_INPUT_OCTAVE, MAX_MIDI_INPUT_OCTAVE);
                self.sync_url();
                Task::none()
            }

            #[cfg(target_arch = "wasm32")]
            Message::ToggleWebMidiThru => {
                if self.web_midi_thru_enabled {
                    // A held note's eventual Note Off will no longer be
                    // forwarded after disabling THRU, so clear it now.
                    if let Some(ref output) = self.web_midi_output {
                        output.all_notes_off();
                    }
                    self.web_midi_thru_enabled = false;
                } else if self.web_midi_input.is_some() && self.web_midi_output.is_some() {
                    self.web_midi_thru_enabled = true;
                }
                self.sync_url();
                Task::none()
            }

            // ── Staff selection ───────────────────────────────────────────
            Message::StaffSelectionChanged(sel) => {
                self.staff_selection = sel;
                self.rebuild_selection_highlight();
                self.sync_loop_range();
                Task::none()
            }
        }
    }

    /// Human-readable summary of the notes under the current staff selection.
    fn selection_summary(&self) -> Option<String> {
        let f = self.midi_file.as_ref()?;
        let (s, e) = self.staff_selection?;
        let e = e.max(s + 1); // a zero-width selection still catches notes at that instant

        let mut notes: Vec<&midi::Note> = f
            .notes
            .iter()
            .filter(|n| n.start_tick < e && n.end_tick > s)
            .filter(|n| !self.track_muted.get(n.track).copied().unwrap_or(false))
            .collect();

        if notes.is_empty() {
            return Some("No notes in selection".to_string());
        }

        notes.sort_by_key(|n| (n.track, n.start_tick));

        let mut by_track: Vec<(usize, Vec<String>)> = Vec::new();
        for n in &notes {
            let name = staff::note_name(n.midi_note);
            match by_track.last_mut() {
                Some((t, names)) if *t == n.track => names.push(name),
                _ => by_track.push((n.track, vec![name])),
            }
        }

        let track_strs: Vec<String> = by_track
            .iter()
            .map(|(t, names)| {
                let tname = f
                    .tracks
                    .get(*t)
                    .and_then(|ti| ti.name.as_deref())
                    .unwrap_or("Track");
                format!("T{} {}: {}", t + 1, tname, names.join(", "))
            })
            .collect();

        Some(format!(
            "{} note{} · {}",
            notes.len(),
            if notes.len() == 1 { "" } else { "s" },
            track_strs.join("   |   "),
        ))
    }

    fn subscription(&self) -> Subscription<Message> {
        #[cfg(not(target_arch = "wasm32"))]
        let needs_poll = self.playback_handle.is_some();
        #[cfg(target_arch = "wasm32")]
        let needs_poll = self.playback_handle.is_some() || self.web_midi_access.is_some();
        let playback = if needs_poll {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::PollPlayback)
        } else {
            Subscription::none()
        };
        let resize = iced::window::resize_events().map(|(_, size)| Message::WindowResized(size));
        let computer_keyboard = if self.keyboard_hits_enabled {
            iced::event::listen_with(computer_keyboard_event)
        } else {
            Subscription::none()
        };
        let octave_shortcut = iced::event::listen_with(octave_shortcut_event);

        Subscription::batch([playback, resize, computer_keyboard, octave_shortcut])
    }

    // ---------------------------------------------------------------------------
    // View
    // ---------------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        boot::notify_first_frame();
        let has_file = self.midi_file.is_some();
        let dense_desktop = self.window_size.width >= 1180.0;
        let (outer_pad, section_gap, panel_v, panel_h, row_gap, track_gap) =
            if self.window_size.width < 1200.0 {
                (4.0, 4.0, 5.0, 6.0, 5.0, 8.0)
            } else if self.window_size.width < 1600.0 {
                (4.0, 3.0, 3.0, 6.0, 4.0, 6.0)
            } else {
                (6.0, 4.0, 3.0, 8.0, 4.0, 8.0)
            };
        let lcd_padding = if dense_desktop {
            Padding::from([2.0, 14.0])
        } else {
            Padding::from([12.0, 18.0])
        };
        let mixer_strip_padding = if dense_desktop {
            Padding::from([1.0, 6.0])
        } else {
            Padding::from([10.0, 12.0])
        };
        let band_pad_y = if dense_desktop { 4.0 } else { panel_v + 7.0 };
        let mixer_band_pad_y = if dense_desktop {
            2.0
        } else {
            panel_v + 11.0
        };
        let resize_handle_height = if dense_desktop { 8.0 } else { 16.0 };
        // Wood side rails shrink on narrower windows and disappear entirely
        // on phone-width screens rather than competing with the app for space.
        // Narrower than the first pass — an accent, not a dominant frame.
        let rail_width: f32 = if self.window_size.width < 700.0 {
            0.0
        } else if self.window_size.width < 1200.0 {
            9.0
        } else {
            14.0
        };

        // ── Header: identity, file, metadata and pitch mapping ──────────────
        let open_btn = action_key(
            &self.control_assets,
            "Open MIDI",
            true,
            Some(Message::OpenFile),
            142.0,
        );

        // Amber-on-black LCD readout — discrete labeled segments rather than
        // one interpolated string, matching a real status display's fields.
        // This is the header's main focal element, not a small status aside,
        // so its type is sized to actually dominate the row.
        let lcd_segment = |label: &'static str, value: String| -> Element<Message> {
            column![
                text(label)
                    .size(9)
                    .color(Color::from_rgba8(0xe9, 0x9b, 0x24, 0.55)),
                text(value).size(19).color(LCD_TEXT),
            ]
            .spacing(1)
            .align_x(Alignment::Start)
            .into()
        };
        let midi_status = if self.audio_error.is_some() {
            "ERR"
        } else {
            "READY"
        };
        let meta: Element<Message> = if let Some(ref e) = self.load_error {
            container(
                text(format!("ERROR · {e}"))
                    .size(15)
                    .color(Color::from_rgb(0.98, 0.48, 0.38)),
            )
            .padding(lcd_padding)
            .width(Length::Fill)
            .style(lcd_style)
            .into()
        } else if let Some(ref f) = self.midi_file {
            let bpm = midi::bpm_at(0, &f.tempo_map);
            let dur = midi::total_duration_secs(f);
            let mins = (dur / 60.0) as u32;
            let secs = (dur % 60.0) as u32;
            let offset_label = if self.octave_offset == 0 {
                "±0".to_string()
            } else if self.octave_offset % 12 == 0 {
                format!("{:+} oct", self.octave_offset / 12)
            } else {
                format!("{:+} st", self.octave_offset)
            };
            let segments = vec![
                lcd_segment("FILE", "LOADED".to_string()),
                lcd_segment("BPM", format!("{bpm:.0}")),
                lcd_segment("TIME SIG", format!("{}/{}", f.time_sig.0, f.time_sig.1)),
                lcd_segment("DUR", format!("{mins}:{secs:02}")),
                lcd_segment("OCT", offset_label),
                lcd_segment("MIDI", midi_status.to_string()),
                lcd_segment("SKIP", self.skipped_notes.to_string()),
            ];
            container(
                scrollable(row(segments).spacing(26).align_y(Alignment::Center)).direction(
                    scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(3).scroller_width(3),
                    ),
                ),
            )
            .padding(lcd_padding)
            .width(Length::Fill)
            .style(lcd_style)
            .into()
        } else {
            container(
                scrollable(
                    row![
                        lcd_segment("FILE", "NONE".to_string()),
                        lcd_segment("BPM", "--".to_string()),
                        lcd_segment("TIME SIG", "--/--".to_string()),
                        lcd_segment("DUR", "0:00".to_string()),
                        lcd_segment("OCT", "±0".to_string()),
                        lcd_segment("MIDI", midi_status.to_string()),
                        lcd_segment("SKIP", "0".to_string()),
                    ]
                    .spacing(26)
                    .align_y(Alignment::Center),
                )
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(3).scroller_width(3),
                )),
            )
            .padding(lcd_padding)
            .width(Length::Fill)
            .style(lcd_style)
            .into()
        };
        let meta = lcd_glass_wrap(&self.control_assets, meta, true);
        let meta = display_bezel_wrap(&self.control_assets, meta);

        let step_label = if self.pitch_step == 12 { "OCT" } else { "ST" };
        let step_name = if self.pitch_step == 12 {
            "one octave"
        } else {
            "one semitone"
        };
        let pitch_down = hardware_tooltip(
            panel_key(
                &self.control_assets,
                "−",
                false,
                has_file.then_some(Message::PitchDown),
                Padding::from([5.0, 10.0]),
            ),
            format!("Lower pitch by {step_name}"),
        );
        let pitch_step = hardware_tooltip(
            panel_key(
                &self.control_assets,
                step_label,
                false,
                Some(Message::PitchStepToggle),
                Padding::from([5.0, 10.0]),
            ),
            format!("Pitch step — {step_name}"),
        );
        let pitch_up = hardware_tooltip(
            panel_key(
                &self.control_assets,
                "+",
                false,
                has_file.then_some(Message::PitchUp),
                Padding::from([5.0, 10.0]),
            ),
            format!("Raise pitch by {step_name}"),
        );
        let pitch_reset = icon_panel_key(
            &self.control_assets,
            self.control_assets.icon_reset_pitch.clone(),
            (self.octave_offset != 0).then_some(Message::PitchReset),
            "Reset pitch to ±0",
        );
        let pitch_separator = container(text(""))
            .width(1)
            .height(Length::Fixed(20.0))
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(PANEL_BORDER.scale_alpha(0.7))),
                ..Default::default()
            });
        let pitch_keys = container(
            row![
                pitch_down,
                pitch_step,
                pitch_up,
                pitch_separator,
                pitch_reset,
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([0, 2])
        .style(pitch_mount_style);
        let pitch_controls = row![
            text("PITCH").size(10).color(TEXT_MUTED),
            pitch_keys,
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let speed_percent = (self.playback_speed * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
        let speed_fraction = (self.playback_speed - SPEED_MIN) / (SPEED_MAX - SPEED_MIN);
        let speed_controls = row![
            text("SPEED").size(10).color(TEXT_MUTED),
            drag_knob(
                &self.control_assets,
                "RATE",
                format!("{speed_percent}%"),
                speed_fraction,
                |frac: f32| Message::SpeedChanged(SPEED_MIN + frac * (SPEED_MAX - SPEED_MIN)),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let all_notes_btn = utility_key(
            &self.control_assets,
            self.control_assets.icon_all_notes.clone(),
            "ALL",
            Some(self.show_all_notes),
            has_file.then_some(Message::ToggleAllNotes),
            54.0,
            format!(
                "Show all notes — {}. Include notes outside the current playback moment.",
                if self.show_all_notes { "On" } else { "Off" }
            ),
        );

        #[cfg(not(target_arch = "wasm32"))]
        let port_label = self
            .midi_port_names
            .get(self.midi_port_idx)
            .map(|s| s.as_str())
            .unwrap_or("No MIDI output");
        #[cfg(not(target_arch = "wasm32"))]
        let port_btn = button(text(format!("MIDI OUT  ·  {port_label}")).size(12))
            .padding([8, 12])
            .style(control_style)
            .on_press(Message::NextPort);

        let live_channel_btn = roller_channel_selector(
            &self.control_assets,
            ChannelOption {
                prefix: "PLAY CH",
                channel: self.live_channel + 1,
            },
            |opt: ChannelOption| Message::LiveChannel(opt.channel - 1),
        );

        #[cfg(target_arch = "wasm32")]
        let web_midi_controls: Element<Message> = if self.web_midi_access.is_none() {
            let label = if self.web_midi_pending {
                "Connecting MIDI…"
            } else {
                "Connect MIDI"
            };
            let connect = action_key(
                &self.control_assets,
                label,
                false,
                (!self.web_midi_pending).then_some(Message::RequestWebMidi),
                142.0,
            );
            let status = self.web_midi_status.as_deref().unwrap_or("");
            // An empty Text widget still contributes a line box. Omitting the
            // entire status row keeps Connect MIDI on the same 42px chassis
            // baseline as the selector and Open MIDI in the initial web view.
            if status.is_empty() {
                connect
            } else {
                column![
                    connect,
                    text(status)
                        .size(9)
                        .color(if status.starts_with("MIDI error:") {
                            Color::from_rgb(0.98, 0.48, 0.38)
                        } else {
                            TEXT_MUTED
                        }),
                ]
                .spacing(2)
                .into()
            }
        } else {
            let port_name = |ports: &[playback::MidiPortInfo], selected: Option<&str>| {
                selected
                    .and_then(|id| ports.iter().find(|port| port.id == id))
                    .map(|port| {
                        let mut name: String = port.name.chars().take(22).collect();
                        if port.name.chars().count() > 22 {
                            name.push('…');
                        }
                        name
                    })
                    .unwrap_or_else(|| "Off".to_string())
            };
            let input_name = port_name(&self.web_midi_inputs, self.web_midi_input_id.as_deref());
            let output_name = port_name(&self.web_midi_outputs, self.web_midi_output_id.as_deref());
            let input = button(text(format!("MIDI IN · {input_name}")).size(11))
                .padding([7, 10])
                .style(if self.web_midi_input_id.is_some() {
                    toggled_style
                } else {
                    control_style
                })
                .on_press(
                    if !self.web_midi_inputs.is_empty() || self.web_midi_input_id.is_some() {
                        Message::NextWebMidiInput
                    } else {
                        // No inputs to cycle through — clicking retries the
                        // scan instead of being a dead button.
                        Message::RefreshWebMidiPorts
                    },
                );
            let output = button(text(format!("MIDI OUT · {output_name}")).size(11))
                .padding([7, 10])
                .style(if self.web_midi_output_id.is_some() {
                    toggled_style
                } else {
                    control_style
                })
                .on_press(
                    if !self.web_midi_outputs.is_empty() || self.web_midi_output_id.is_some() {
                        Message::NextWebMidiOutput
                    } else {
                        Message::RefreshWebMidiPorts
                    },
                );
            let input_octave = pick_list(
                midi_input_octave_options(),
                Some(MidiInputOctaveOption(self.web_midi_input_octave)),
                |option| Message::WebMidiInputOctave(option.0),
            )
            .text_size(9)
            .padding([3, 6])
            .style(channel_pick_list_style);
            let thru_available = self.web_midi_input.is_some() && self.web_midi_output.is_some();
            let thru = button(
                text(if self.web_midi_thru_enabled {
                    "THRU: ON"
                } else {
                    "THRU"
                })
                .size(9),
            )
            .padding([3, 6])
            .style(if self.web_midi_thru_enabled {
                toggled_style
            } else {
                secondary_control_style
            })
            .on_press_maybe(
                (self.web_midi_thru_enabled || thru_available)
                    .then_some(Message::ToggleWebMidiThru),
            );
            let status = self.web_midi_status.as_deref().unwrap_or("");
            column![
                row![input, output].spacing(5),
                row![
                    text(status)
                        .size(9)
                        .color(if status.starts_with("MIDI error:") {
                            Color::from_rgb(0.98, 0.48, 0.38)
                        } else {
                            TEXT_MUTED
                        }),
                    Space::new().width(Length::Fill),
                    thru,
                    input_octave,
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            ]
            .spacing(2)
            .into()
        };

        // Brand stays intrinsic width — the LCD claims the header's leftover
        // space instead, so it reads as the row's dominant element.
        let identity = column![
            text("K2").size(24).color(TEXT_MAIN),
            text("KEYBOARD KEYBOARD / MIDI VIEWER")
                .size(10)
                .color(TEXT_MUTED),
        ]
        .spacing(0);

        #[cfg(not(target_arch = "wasm32"))]
        let midi_controls = row![port_btn, live_channel_btn, open_btn]
            .spacing(row_gap)
            .align_y(Alignment::Center);
        #[cfg(target_arch = "wasm32")]
        let midi_controls = row![web_midi_controls, live_channel_btn, open_btn]
            .spacing(row_gap)
            // Connected MIDI controls may grow a real status row. Align the
            // three chassis at their top edge instead of recentering the two
            // single-row controls against that taller column.
            .align_y(Alignment::Start);

        let mapping_btn = utility_key(
            &self.control_assets,
            self.control_assets.icon_map_rows.clone(),
            self.key_pick_mode.short_label(),
            None,
            Some(Message::OctaveLayoutToggle),
            76.0,
            format!(
                "Key mapping — {}. Click to cycle how repeated notes choose a keyboard row.",
                self.key_pick_mode.label().trim_start_matches("Rows: ")
            ),
        );
        let keyboard_hits_btn = utility_key(
            &self.control_assets,
            self.control_assets.icon_computer_keys.clone(),
            "KEYS",
            Some(self.keyboard_hits_enabled),
            Some(Message::ToggleKeyboardHits),
            60.0,
            format!(
                "Computer keyboard input — {}.",
                if self.keyboard_hits_enabled { "On" } else { "Off" }
            ),
        );

        let drum_symbols_btn = utility_key(
            &self.control_assets,
            self.control_assets.icon_drum.clone(),
            "DRUM",
            Some(self.drum_symbols_enabled),
            Some(Message::ToggleDrumSymbols),
            64.0,
            format!(
                "Drum symbols — {}. Show GM percussion labels on the numpad.",
                if self.drum_symbols_enabled { "On" } else { "Off" }
            ),
        );

        let compact_keyboard_btn = utility_key(
            &self.control_assets,
            self.control_assets.icon_board.clone(),
            "BOARD",
            Some(self.compact_keyboard),
            Some(Message::ToggleCompactKeyboard),
            68.0,
            format!(
                "Keyboard view — {}.",
                if self.compact_keyboard { "Compact" } else { "Full" }
            ),
        );

        // Main header row: brand, the big amber LCD (dominant, Fill width),
        // then MIDI controls — visually secondary to the LCD by virtue of
        // being ordinary-sized buttons next to a much larger display.
        let header_main: Element<Message> = if self.window_size.width < 1180.0 {
            column![
                identity,
                meta,
                scrollable(midi_controls).direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(4).scroller_width(4),
                )),
            ]
            .spacing(row_gap)
            .into()
        } else {
            row![identity, meta, midi_controls]
                .spacing(row_gap * 2.0)
                .align_y(Alignment::Center)
                .into()
        };

        // Secondary strip: pitch mapping and view toggles, muted styling so
        // they read as auxiliary controls under the main header row.
        // Three logical clusters — pitch, playback speed, and compact
        // icon-led utility controls. Tooltips carry the longer explanations
        // without contributing another line to the layout.
        let pitch_cluster = control_cluster(pitch_controls, dense_desktop);
        let speed_cluster = control_cluster(speed_controls, dense_desktop);
        let utility_cluster = control_cluster(
            row![
                mapping_btn,
                all_notes_btn,
                keyboard_hits_btn,
                drum_symbols_btn,
                compact_keyboard_btn,
            ]
                .spacing(5)
                .align_y(Alignment::Center),
            dense_desktop,
        );

        let header_secondary: Element<Message> = if self.window_size.width < 1180.0 {
            column![
                row![pitch_cluster, speed_cluster]
                    .spacing(row_gap)
                    .align_y(Alignment::Center),
                scrollable(utility_cluster).direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(3).scroller_width(3),
                )),
            ]
            .spacing(row_gap)
            .into()
        } else {
            row![pitch_cluster, speed_cluster, utility_cluster]
                .spacing(row_gap * 1.5)
                .align_y(Alignment::Center)
                .into()
        };

        let file_row: Element<Message> =
            container(column![header_main, header_secondary].spacing(row_gap))
                .padding([band_pad_y, panel_h + 11.0])
                .into();

        // ── Row 2: transport ───────────────────────────────────────────────
        let play_pause_btn: Element<Message> = match self.play_state {
            PlayState::Playing => transport_key(
                &self.control_assets,
                "PAUSE",
                Some(Color::from_rgb8(0xd9, 0xa2, 0x2d)),
                Some(Message::Pause),
            ),
            PlayState::Paused | PlayState::Stopped => transport_key(
                &self.control_assets,
                "PLAY",
                Some(Color::from_rgb8(0x78, 0xa9, 0x2c)),
                has_file.then_some(Message::Play),
            ),
        };
        let stop_btn = transport_key(
            &self.control_assets,
            "STOP",
            None,
            (has_file && self.play_state != PlayState::Stopped).then_some(Message::Stop),
        );

        let loop_scope = if self.staff_selection.is_some() {
            "Loop selected range"
        } else {
            "Loop song"
        };
        let looper_btn = horizontal_switch(
            &self.control_assets,
            "LOOP",
            self.looper_enabled,
            Some(Message::ToggleLooper),
            format!(
                "{loop_scope} — {}.",
                if self.looper_enabled { "On" } else { "Off" }
            ),
        );

        let audio_label = if let Some(error) = &self.audio_error {
            #[cfg(target_arch = "wasm32")]
            if self.web_midi_output.is_some() {
                if self
                    .audio_enabled
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    "MIDI output on".to_string()
                } else {
                    "MIDI output muted".to_string()
                }
            } else {
                format!("Sound unavailable · {error}")
            }
            #[cfg(not(target_arch = "wasm32"))]
            format!("Sound unavailable · {error}")
        } else if self
            .audio_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            "Sound on".to_string()
        } else {
            "Muted".to_string()
        };
        #[cfg(not(target_arch = "wasm32"))]
        let audio_available = self.soft_synth.is_some();
        #[cfg(target_arch = "wasm32")]
        let audio_available = self.soft_synth.is_some() || self.web_midi_output.is_some();
        let sound_on = self.audio_error.is_none()
            && self
                .audio_enabled
                .load(std::sync::atomic::Ordering::Relaxed);
        let audio_btn = horizontal_switch(
            &self.control_assets,
            "SOUND",
            sound_on,
            audio_available.then_some(Message::ToggleAudio),
            audio_label,
        );

        // Arrow Up/Down transpose hand-played notes; only worth showing once
        // it's actually been nudged off center.
        let live_octave_label: Element<Message> = if self.live_octave != 0 {
            text(format!("Live {:+} oct", self.live_octave / 12))
                .size(13)
                .color(ACCENT)
                .into()
        } else {
            text("").into()
        };

        let (progress, time_str) = if let Some(ref f) = self.midi_file {
            let p = if f.total_ticks > 0 {
                self.position_tick as f32 / f.total_ticks as f32
            } else {
                0.0
            };
            let cur_us =
                midi::tick_to_micros_abs(self.position_tick, &f.tempo_map, f.ticks_per_beat);
            let tot_us = midi::tick_to_micros_abs(f.total_ticks, &f.tempo_map, f.ticks_per_beat);
            let fmt = |us: u64| format!("{}:{:02}", us / 60_000_000, (us % 60_000_000) / 1_000_000);
            (p, format!("{} / {}", fmt(cur_us), fmt(tot_us)))
        } else {
            (0.0f32, "0:00 / 0:00".to_string())
        };

        let slider_widget: Element<Message> = if has_file {
            slider(0.0f32..=1.0, progress, Message::SeekTo)
                .step(SEEK_STEP)
                .shift_step(SEEK_STEP / 10.0)
                .style(fader_style)
                .width(Length::Fill)
                .into()
        } else {
            slider(0.0f32..=1.0, 0.0f32, |_| Message::SeekTo(0.0))
                .step(SEEK_STEP)
                .style(fader_style)
                .width(Length::Fill)
                .into()
        };
        // The recessed slot, ticks, and fader-cap sit behind the real slider
        // — pushed under it, so the slider (the sizing base) stays the fully
        // functional control and the artwork is purely decorative backdrop,
        // never intercepting input.
        let scrubber: Element<Message> = Stack::new()
            .width(Length::Fill)
            .push(slider_widget)
            .push_under(
                Canvas::new(FaderTrack {
                    progress,
                    cap: self.control_assets.fader_cap.clone(),
                    track: self.control_assets.fader_track.clone(),
                })
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .into();

        let transport_content: Element<Message> = if self.window_size.width < 1100.0 {
            column![
                row![
                    text("TRANSPORT").size(9).color(TEXT_MUTED),
                    play_pause_btn,
                    stop_btn,
                    looper_btn,
                    audio_btn,
                    live_octave_label,
                ]
                .spacing(row_gap)
                .align_y(Alignment::Center),
                row![scrubber, text(time_str).size(13).color(TEXT_MUTED),]
                    .spacing(row_gap)
                    .align_y(Alignment::Center),
            ]
            .spacing(row_gap)
            .into()
        } else {
            row![
                text("TRANSPORT").size(9).color(TEXT_MUTED),
                play_pause_btn,
                stop_btn,
                looper_btn,
                audio_btn,
                live_octave_label,
                scrubber,
                text(time_str).size(13).color(TEXT_MUTED),
            ]
            .spacing(row_gap)
            .align_y(Alignment::Center)
            .into()
        };

        let transport_row: Element<Message> = container(transport_content)
            .padding([band_pad_y, panel_h + 11.0])
            .into();

        // ── Row 3: track mixer ───────────────────────────────────────────────
        // Always present — a structural module between transport and keyboard,
        // not something that appears/disappears with file state. Shows an
        // idle placeholder strip until a file with tracks is loaded.
        let track_row: Element<Message> = match self.midi_file.as_ref() {
            Some(f) if !f.tracks.is_empty() => {
                let items: Vec<Element<Message>> = f
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let name = t.name.as_deref().unwrap_or("Track");
                        let label = format!("{}: {}", i + 1, name);
                        let muted = self.track_muted.get(i).copied().unwrap_or(false);
                        let (r, g, b) = render::TRACK_COLORS[i % render::TRACK_COLORS.len()];
                        // A tiny mounted LED jewel, lit in the track's own color.
                        let led = led_jewel(&self.control_assets, Color::from_rgb8(r, g, b), true);
                        // The track name sits set into its own recessed label
                        // plate rather than straight on the panel surface.
                        let name_plate = label_plate(&self.control_assets, label);
                        let mute_btn = rocker_switch(
                            &self.control_assets,
                            "MUTE",
                            muted,
                            Some(Message::TrackMuted(i, !muted)),
                        );
                        let channel = self.track_channel.get(i).copied().unwrap_or(0);
                        let channel_knob = rotary_knob(
                            &self.control_assets,
                            "CH",
                            format!("{}", channel + 1),
                            channel as f32 / 15.0,
                            channel_options("CH"),
                            Some(ChannelOption {
                                prefix: "CH",
                                channel: channel + 1,
                            }),
                            move |opt: ChannelOption| Message::TrackChannel(i, opt.channel - 1),
                        );
                        let octave = self.track_octave.get(i).copied().unwrap_or(0);
                        let octave_label = if octave == 0 {
                            "±0".to_string()
                        } else {
                            format!("{octave:+}")
                        };
                        let octave_knob = rotary_knob(
                            &self.control_assets,
                            "OCT",
                            octave_label,
                            (octave + 3) as f32 / 6.0,
                            track_octave_options(),
                            Some(TrackOctaveOption(octave)),
                            move |opt: TrackOctaveOption| Message::TrackOctave(i, opt.0),
                        );
                        // Each track is its own bordered mini channel strip —
                        // a distinct module nested in the mixer bed, not a
                        // toolbar row of loose controls.
                        container(
                            row![led, name_plate, mute_btn, channel_knob, octave_knob]
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .width(Length::Fixed(410.0))
                        .padding(mixer_strip_padding)
                        .style(mixer_strip_style)
                        .into()
                    })
                    .collect();
                let tracks = scrollable(row(items).spacing(track_gap).align_y(Alignment::Center))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(6).scroller_width(6),
                    ));
                container(
                    row![text("TRACKS").size(10).color(TEXT_MUTED), tracks]
                        .spacing(row_gap)
                        .align_y(Alignment::Center),
                )
                // A bit taller than the other panels — with both a channel
                // and an octave picker per track now, the row reads as
                // cramped at the same vertical padding as single-line panels.
                .padding([mixer_band_pad_y, panel_h + 11.0])
                .into()
            }
            _ => {
                // Idle state mirrors real channel-strip geometry. Controls are
                // visibly inactive, but the mixer never collapses into a status
                // sentence merely because its data source is empty.
                let placeholder_items: Vec<Element<Message>> = [
                    ("1: 8-BIT SAWTOOTH", 0u8, 0.0f32),
                    ("2: ELECTRIC PIANO", 1u8, 0.5f32),
                    ("3: POP SYNTH", 2u8, 1.0f32),
                ]
                    .into_iter()
                    .map(|(name, channel, angle)| {
                        let (r, g, b) = render::TRACK_COLORS
                            [channel as usize % render::TRACK_COLORS.len()];
                        let led = led_jewel(
                            &self.control_assets,
                            Color::from_rgb8(r, g, b),
                            true,
                        );
                        let name_plate = label_plate(&self.control_assets, name.to_string());
                        let mute = rocker_switch(&self.control_assets, "MUTE", false, None);
                        let channel_knob = static_rotary_knob(
                            &self.control_assets,
                            "CH",
                            match channel {
                                0 => "1",
                                1 => "2",
                                _ => "3",
                            },
                            angle,
                        );
                        let octave_knob =
                            static_rotary_knob(&self.control_assets, "OCT", "±0", 0.5);

                        container(
                            row![led, name_plate, mute, channel_knob, octave_knob]
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .width(Length::Fixed(410.0))
                        .padding(mixer_strip_padding)
                        .style(mixer_strip_style)
                        .into()
                    })
                    .collect();
                let placeholders = scrollable(
                    row(placeholder_items)
                        .spacing(track_gap)
                        .align_y(Alignment::Center),
                )
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(6).scroller_width(6),
                ));
                container(
                    row![
                        column![
                            text("TRACKS").size(10).color(TEXT_MUTED),
                            text("IDLE").size(9).color(TEXT_MUTED.scale_alpha(0.5)),
                        ]
                        .spacing(2),
                        placeholders,
                    ]
                    .spacing(row_gap)
                    .align_y(Alignment::Center),
                )
                .padding([mixer_band_pad_y, panel_h + 11.0])
                .into()
            }
        };

        // ── Keyboard canvas ────────────────────────────────────────────────
        // A staff selection takes priority: it shows exactly what's selected,
        // overriding the live playback highlight or the "show all notes" overlay.
        let highlighted_ref = if self.staff_selection.is_some() {
            &self.selection_highlight_cache
        } else if self.show_all_notes {
            &self.all_notes_cache
        } else {
            &self.highlighted
        };
        #[cfg(target_arch = "wasm32")]
        let overlay_highlighted = Some(&self.web_midi_highlighted);
        #[cfg(not(target_arch = "wasm32"))]
        let overlay_highlighted = None;

        // Keep one uniform photographic scale for the shell, keys and controls.
        // Compact heights shorten the viewport around that board instead
        // of scaling the instrument down to fit the shorter rectangle.
        const BOARD_ASPECT: f32 = 1949.0 / 807.0;
        // `CHROME_BEZEL` (the keyboard/staff bezel width) is baked into this
        // sizing math so it can't reintroduce the earlier letterboxing bug.
        // The true available width inside the centered, rail-flanked shell —
        // capped at the shell's own max-width and reduced by the wood rails —
        // not the raw window width, or the board's contain-fit scale would be
        // computed against a wider box than it's actually laid out in and
        // leave letterboxed empty space around the keyboard.
        let shell_width = self.window_size.width.min(1600.0 + rail_width * 2.0) - rail_width * 2.0;
        let width_limited =
            ((shell_width - outer_pad * 2.0 - CHROME_BEZEL * 2.0) / BOARD_ASPECT).max(1.0);
        // The track mixer module is always present now, so its height is
        // always reserved (previously conditional on a multi-track file).
        let chrome_reserve = if dense_desktop { 220.0 } else { 410.0 };
        let staff_reserve = if has_file {
            if self.compact_keyboard || !dense_desktop {
                155.0
            } else {
                150.0
            }
        } else if self.compact_keyboard {
            90.0
        } else if dense_desktop {
            150.0
        } else {
            175.0
        };
        let height_limited = (self.window_size.height
            - chrome_reserve
            - staff_reserve
            - outer_pad * 2.0
            - CHROME_BEZEL * 2.0)
            .max(120.0);
        // Compact mode is the photographed working-surface crop: the power
        // switch/control bank through every key row, without the outer header
        // and footer. Its own aspect ratio determines the viewport height.
        let mode_height = if self.compact_keyboard {
            (shell_width - outer_pad * 2.0 - CHROME_BEZEL * 2.0) / render::COMPACT_BOARD_ASPECT
        } else {
            width_limited.min(620.0)
        };
        let automatic_keyboard_height = height_limited.min(mode_height);

        // Manual resizing may reclaim more room than the automatic layout, but
        // always leaves a usable strip for the visualizer and never grows
        // beyond the photograph's width-limited natural height.
        let resize_min = width_limited
            .min(if self.window_size.height < 650.0 {
                105.0
            } else {
                135.0
            })
            .max(80.0);
        let staff_floor = if has_file { 82.0 } else { 46.0 };
        let resize_max = width_limited
            .min(620.0)
            .min(
                (self.window_size.height
                    - chrome_reserve
                    - staff_floor
                    - outer_pad * 2.0
                    - CHROME_BEZEL * 2.0
                    - resize_handle_height)
                    .max(resize_min),
            )
            .max(resize_min);
        let keyboard_height = self
            .keyboard_height_override
            .unwrap_or(automatic_keyboard_height)
            .clamp(resize_min, resize_max);
        let keyboard = Canvas::new(BoardCanvas {
            photo_assets: &self.photo_assets,
            compact_crop: self.compact_keyboard,
            keys: &self.keys,
            highlighted: highlighted_ref,
            overlay_highlighted,
            play_order: self
                .staff_selection
                .is_some()
                .then_some(&self.selection_play_order),
            selected_controls: &self.waveform_keys,
            pressed: &self.pressed_keys,
            projected_labels: self
                .keyboard_hits_enabled
                .then_some(&self.computer_key_labels),
            drum_note_to_key: &self.drum_note_to_key,
            show_drum_symbols: self.drum_symbols_enabled,
            knob_values: &self.knob_values,
        })
        .width(Length::Fill)
        .height(keyboard_height);

        let keyboard_resize = Canvas::new(BoardResizeHandle {
            current_height: keyboard_height,
            min_height: resize_min,
            max_height: resize_max,
        })
        .width(Length::Fill)
        .height(resize_handle_height);
        let keyboard_region = container(
            container(column![keyboard, keyboard_resize].spacing(0))
                .padding(CHROME_BEZEL)
                .style(bezel_style),
        )
        .padding(Padding {
            top: 0.0,
            right: outer_pad,
            bottom: 0.0,
            left: outer_pad,
        });

        // ── Visualizer panel ──────────────────────────────────────────────
        // The complete housing remains mounted in the rack in every state;
        // only the content drawn on its inner screen changes when a file loads.
        let staff_screen: Element<Message> = container(
            Canvas::new(StaffCanvas {
                midi_file: self.midi_file.as_ref(),
                position_tick: self.position_tick,
                track_muted: &self.track_muted,
                octave_offset: self.octave_offset,
                track_octave: &self.track_octave,
                selection: self.staff_selection,
                keyboard_notes: &self.keyboard_notes,
                drum_note_to_key: &self.drum_note_to_key,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .padding(CHROME_BEZEL)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(crt_screen_style)
        .into();
        // The shared LCD sheen is proportioned for the short header display;
        // on this tall screen it warps into a curved streak. Keep the inset
        // edge vignette, but omit that aspect-specific SVG layer here.
        let staff_screen = lcd_glass_wrap(&self.control_assets, staff_screen, false);
        let staff_screen = display_bezel_wrap(&self.control_assets, staff_screen);
        let visualizer_state = if has_file {
            "SIGNAL / STAFF"
        } else {
            "STANDBY / NO FILE"
        };
        let visualizer_header = row![
            text("VISUALIZER").size(10).color(TEXT_MUTED),
            text(visualizer_state)
                .size(9)
                .color(LCD_TEXT.scale_alpha(if has_file { 0.78 } else { 0.45 })),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let staff_panel = textured_panel(
            container(
                column![visualizer_header, staff_screen]
                    .spacing(6)
                    .height(Length::Fill),
            )
            .padding([7, 8])
            .height(Length::Fill),
            PANEL_BG_DARK,
            self.chrome_assets.dark_grain.clone(),
            self.chrome_assets.panel_wear.clone(),
        );
        let visualizer_height = if dense_desktop && !has_file {
            Length::Fixed(150.0)
        } else {
            Length::Fill
        };
        let staff: Element<Message> = container(staff_panel)
            .width(Length::Fill)
            .height(visualizer_height)
            .into();

        // ── Selection info ────────────────────────────────────────────────
        let selection_row: Element<Message> = if has_file {
            let msg = self
                .selection_summary()
                .unwrap_or_else(|| "Drag on the staff to inspect notes in a range".to_string());
            text(msg).size(12).color(TEXT_MUTED).into()
        } else {
            row![].into()
        };

        // The keyboard stage shares `outer_pad` as its left/right margin (see
        // `keyboard_region` above) so its bezel lines up with the panels
        // above and below it instead of sitting flush against the rails.
        // The upper controls are one manufactured console, not three stacked
        // cards. One shared grain/wear layer now runs behind every module;
        // shallow grooves provide hierarchy without restarting the material.
        let console = textured_panel(
            column![
                file_row,
                module_divider(),
                transport_row,
                module_divider(),
                track_row,
            ]
            .spacing(0),
            PANEL_BG,
            self.chrome_assets.dark_grain.clone(),
            self.chrome_assets.panel_wear.clone(),
        );
        let console = console_screws(console, self.control_assets.panel_screw.clone());
        let above_keyboard = container(console).padding(Padding {
                top: outer_pad,
                right: outer_pad,
                bottom: 0.0,
                left: outer_pad,
            });

        let below_height = if dense_desktop && !has_file {
            Length::Shrink
        } else {
            Length::Fill
        };
        let below_keyboard = container(
            column![staff, selection_row]
                .spacing(section_gap)
                .height(below_height),
        )
        .padding(Padding {
            top: 0.0,
            right: outer_pad,
            bottom: outer_pad,
            left: outer_pad,
        })
        .height(below_height);

        let content = column![above_keyboard, keyboard_region, below_keyboard]
            .spacing(0)
            .height(Length::Fill);

        let inner: Element<Message> = container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(APP_BG)),
                text_color: Some(TEXT_MAIN),
                ..Default::default()
            })
            .into();

        // Narrow dark-walnut rails at the far left/right edges of the shell —
        // structural framing, not a full wood background. They shrink out
        // entirely on phone-width screens (see `rail_width` above).
        let shell: Element<Message> = if rail_width > 0.0 {
            let rail = || {
                image_widget(self.chrome_assets.wood_grain.clone())
                    .width(Length::Fixed(rail_width))
                    .height(Length::Fill)
                    .content_fit(ContentFit::Cover)
                    .filter_method(FilterMethod::Linear)
                    .opacity(0.8f32)
            };
            row![rail(), inner, rail()].into()
        } else {
            inner
        };

        let centered_shell: Element<Message> = container(shell)
            .width(Length::Fill)
            .max_width(1600.0 + rail_width * 2.0)
            .height(Length::Fill)
            .into();

        // The page itself stays neutral and texture-free. Grain belongs to the
        // equipment panels above, while walnut is limited to the two side rails.
        container(centered_shell)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(APP_BG)),
                text_color: Some(TEXT_MAIN),
                ..Default::default()
            })
            .into()
    }
}

// ---------------------------------------------------------------------------
// URL settings (web build only)
//
// Knob positions and a few global toggles are mirrored into the query string
// via `history.replaceState` (no navigation, no new history entry) so a
// reload or a shared link restores them. Per-file state like track mutes
// isn't included since it's meaningless without the file itself.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod url_state {
    use super::{App, Cluster, KeyPickMode, label_for_waveform};
    use crate::synth::Waveform;
    use std::sync::atomic::Ordering;

    fn waveform_slug(w: Waveform) -> &'static str {
        match w {
            Waveform::Organ => "organ",
            Waveform::Triangle => "triangle",
            Waveform::Square => "square",
            Waveform::Saw => "saw",
            Waveform::Sine => "sine",
            Waveform::Pulse => "pulse",
            Waveform::Noise => "noise",
        }
    }

    fn waveform_from_slug(s: &str) -> Option<Waveform> {
        Some(match s {
            "triangle" => Waveform::Triangle,
            "square" => Waveform::Square,
            "saw" => Waveform::Saw,
            "sine" => Waveform::Sine,
            "pulse" => Waveform::Pulse,
            "noise" => Waveform::Noise,
            _ => return None,
        })
    }

    /// URL-safe query key for a knob, derived from its display label
    /// ("Vib Depth" → "vib_depth"), so the URL reads as self-documenting
    /// name=value pairs instead of a positional list.
    fn knob_slug(label: &str) -> String {
        label.to_lowercase().replace(' ', "_")
    }

    pub fn load(app: &mut App) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(search) = window.location().search() else {
            return;
        };
        let query = search.strip_prefix('?').unwrap_or(&search).to_string();

        let mut pairs = std::collections::HashMap::new();
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                pairs.insert(key, value);
            }
        }

        for (i, param) in crate::synth::KNOB_PARAMS.iter().enumerate() {
            let Some(raw) = pairs.get(knob_slug(param.label).as_str()) else {
                continue;
            };
            let Ok(real) = raw.parse::<f32>() else {
                continue;
            };
            let real = real.clamp(param.min, param.max);
            let pos = if param.max > param.min {
                (real - param.min) / (param.max - param.min)
            } else {
                0.0
            };
            if let Some(slot) = app.knob_values.get_mut(i) {
                *slot = pos;
            }
        }
        if let Some(&v) = pairs.get("sound") {
            app.audio_enabled.store(v != "0", Ordering::Relaxed);
        }
        if let Some(&v) = pairs.get("keys") {
            app.keyboard_hits_enabled = v != "0";
        }
        if let Some(&v) = pairs.get("drum_symbols") {
            app.drum_symbols_enabled = v != "0";
        }
        if let Some(&v) = pairs.get("compact") {
            app.compact_keyboard = v != "0";
        }
        if let Some(&v) = pairs.get("loop") {
            app.looper_enabled = v != "0";
        }
        if let Some(&v) = pairs.get("row") {
            app.key_pick_mode = match v {
                "lr" => KeyPickMode::LeftRight,
                "ud" => KeyPickMode::UpDown,
                _ => KeyPickMode::Closest,
            };
        }
        if let Some(octaves) = pairs.get("live_octave").and_then(|v| v.parse::<i8>().ok()) {
            app.live_octave = octaves.saturating_mul(12);
        }
        if let Some(octave) = pairs
            .get("midi_in_octave")
            .and_then(|value| value.parse::<i8>().ok())
        {
            app.web_midi_input_octave =
                octave.clamp(super::MIN_MIDI_INPUT_OCTAVE, super::MAX_MIDI_INPUT_OCTAVE);
        }
        if let Some(&value) = pairs.get("midi_thru") {
            app.web_midi_thru_enabled = value != "0";
        }
        if let Some(&v) = pairs.get("waveforms") {
            app.waveform_keys = v
                .split(',')
                .filter_map(waveform_from_slug)
                .filter_map(label_for_waveform)
                .filter_map(|label| {
                    app.keys
                        .iter()
                        .find(|k| k.cluster == Cluster::Nav && k.label == label)
                })
                .map(|k| k.id)
                .collect();
        }
    }

    pub fn save(app: &App) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(pathname) = window.location().pathname() else {
            return;
        };
        let Ok(history) = window.history() else {
            return;
        };

        // Only knobs actually moved from their default, and only the other
        // settings that differ from their default, make it into the URL —
        // a stock setup stays a bare, uncluttered URL.
        let mut params: Vec<String> = Vec::new();

        for (i, param) in crate::synth::KNOB_PARAMS.iter().enumerate() {
            let pos = app
                .knob_values
                .get(i)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let real = param.min + pos * (param.max - param.min);
            if (real - param.default).abs() > 0.005 {
                params.push(format!("{}={:.2}", knob_slug(param.label), real));
            }
        }

        if !app.audio_enabled.load(Ordering::Relaxed) {
            params.push("sound=0".to_string());
        }
        if app.keyboard_hits_enabled {
            params.push("keys=1".to_string());
        }
        if app.drum_symbols_enabled {
            params.push("drum_symbols=1".to_string());
        }
        if app.compact_keyboard {
            params.push("compact=1".to_string());
        }
        if app.looper_enabled {
            params.push("loop=1".to_string());
        }
        let row = match app.key_pick_mode {
            KeyPickMode::LeftRight => Some("lr"),
            KeyPickMode::UpDown => Some("ud"),
            KeyPickMode::Closest => None,
        };
        if let Some(row) = row {
            params.push(format!("row={row}"));
        }
        if app.live_octave != 0 {
            params.push(format!("live_octave={}", app.live_octave / 12));
        }
        if app.web_midi_input_octave != super::DEFAULT_MIDI_INPUT_OCTAVE {
            params.push(format!("midi_in_octave={}", app.web_midi_input_octave));
        }
        if app.web_midi_thru_enabled {
            params.push("midi_thru=1".to_string());
        }
        let waveforms = app.active_waveforms();
        if !waveforms.is_empty() {
            let slugs: Vec<&str> = waveforms.iter().map(|&w| waveform_slug(w)).collect();
            params.push(format!("waveforms={}", slugs.join(",")));
        }

        let url = if params.is_empty() {
            pathname
        } else {
            format!("{pathname}?{}", params.join("&"))
        };
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
    }
}

#[cfg(target_arch = "wasm32")]
fn apply_url_settings(app: &mut App) {
    url_state::load(app);
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_url_settings(_app: &mut App) {}

impl App {
    #[cfg(target_arch = "wasm32")]
    fn sync_url(&self) {
        url_state::save(self);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sync_url(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_control_toggles_back_to_default() {
        let key = KeyId(42);
        let mut active = HashSet::new();

        toggle_waveform_key(&mut active, key);
        assert_eq!(active, HashSet::from([key]));

        toggle_waveform_key(&mut active, key);
        assert!(active.is_empty());
    }

    #[test]
    fn choosing_another_waveform_activates_it_alongside() {
        let triangle_key = KeyId(42);
        let square_key = KeyId(43);
        let mut active = HashSet::from([triangle_key]);

        toggle_waveform_key(&mut active, square_key);
        assert_eq!(active, HashSet::from([triangle_key, square_key]));
    }

    #[test]
    fn computer_spacebar_spans_six_bottom_row_notes() {
        let layout = build_layout();
        let key = ComputerKey::Named(
            iced::keyboard::key::Named::Space,
            ComputerKeyLocation::Standard,
        );
        let mapped = mapped_computer_keys(&layout.keys, &key);
        let labels = computer_projection_labels(&layout.keys);

        assert_eq!(mapped.len(), 6);
        assert!(mapped.iter().all(|id| {
            layout
                .keys
                .iter()
                .find(|candidate| candidate.id == *id)
                .is_some_and(|candidate| candidate.row == 5.0)
        }));
        assert!(
            mapped
                .iter()
                .all(|id| labels.get(id).is_some_and(|label| label == "SPACE"))
        );
    }

    #[test]
    fn computer_numpad_zero_spans_two_drum_pads() {
        let layout = build_layout();
        let key = ComputerKey::Character("0".to_string(), ComputerKeyLocation::Numpad);
        let mapped = mapped_computer_keys(&layout.keys, &key);
        let labels = computer_projection_labels(&layout.keys);

        assert_eq!(mapped.len(), 2);
        assert!(mapped.iter().all(|id| {
            layout
                .drum_note_to_key
                .values()
                .any(|drum_key| drum_key == id)
        }));
        assert!(
            mapped
                .iter()
                .all(|id| labels.get(id).is_some_and(|label| label == "0"))
        );
    }

    #[test]
    fn selection_play_order_groups_chords_and_numbers_onsets() {
        let first = KeyId(1);
        let chord_mate = KeyId(2);
        let last = KeyId(3);
        let order = play_order_from_ticks(HashMap::from([
            (first, vec![120]),
            (chord_mate, vec![120]),
            (last, vec![360]),
        ]));

        assert_eq!(order.get(&first), Some(&vec![1]));
        assert_eq!(order.get(&chord_mate), Some(&vec![1]));
        assert_eq!(order.get(&last), Some(&vec![2]));
    }

    #[test]
    fn selection_play_order_keeps_repeated_uses_of_one_key() {
        let repeated = KeyId(1);
        let middle = KeyId(2);
        let order = play_order_from_ticks(HashMap::from([
            (repeated, vec![100, 300, 300]),
            (middle, vec![200]),
        ]));

        assert_eq!(order.get(&repeated), Some(&vec![1, 3]));
        assert_eq!(order.get(&middle), Some(&vec![2]));
    }

    #[test]
    fn midi_input_parses_note_on_note_off_and_zero_velocity() {
        assert_eq!(
            parse_midi_input(&[0x92, 64, 100]),
            Some(MidiInputAction::NoteOn {
                note: 64,
                velocity: 100,
                channel: 2
            }),
        );
        assert_eq!(
            parse_midi_input(&[0x82, 64, 0]),
            Some(MidiInputAction::NoteOff {
                note: 64,
                channel: 2
            }),
        );
        assert_eq!(
            parse_midi_input(&[0x92, 64, 0]),
            Some(MidiInputAction::NoteOff {
                note: 64,
                channel: 2
            }),
        );
    }

    #[test]
    fn default_keyboard_octave_maps_physical_corner_notes_to_their_keys() {
        let layout = build_layout();
        let key_at = |row: f32, rightmost: bool| {
            layout
                .keys
                .iter()
                .filter(|key| {
                    key.row == row
                        && matches!(key.cluster, Cluster::Alpha | Cluster::AlphaLight)
                })
                .min_by(|a, b| {
                    let ordering = a.col.total_cmp(&b.col);
                    if rightmost { ordering.reverse() } else { ordering }
                })
                .expect("alpha row must contain keys")
        };

        // Firmware octave 2 sends the raw top-right 104 as MIDI 80, and the
        // raw bottom-right 80 as MIDI 56. Undoing that hardware transpose is
        // what makes two isolated G# presses land on different physical keys.
        let (top_note, top_clipped) = midi_input_board_note(80, DEFAULT_MIDI_INPUT_OCTAVE);
        let (bottom_note, bottom_clipped) =
            midi_input_board_note(56, DEFAULT_MIDI_INPUT_OCTAVE);
        let top_right = key_at(1.0, true);
        let bottom_right = key_at(5.0, true);

        assert_eq!(top_note, 104);
        assert_eq!(bottom_note, 80);
        assert!(!top_clipped && !bottom_clipped);
        assert_eq!(layout.note_to_all_keys.get(&top_note), Some(&vec![top_right.id]));
        assert_eq!(
            layout.note_to_all_keys.get(&bottom_note).and_then(|keys| keys.last()),
            Some(&bottom_right.id),
        );
    }

    #[test]
    fn midi_input_octave_four_is_an_untransposed_visual_map() {
        assert_eq!(midi_input_board_note(104, MIDI_INPUT_REFERENCE_OCTAVE), (104, false));
    }

    #[test]
    fn midi_input_honors_channel_all_notes_off() {
        assert_eq!(
            parse_midi_input(&[0xB7, 123, 0]),
            Some(MidiInputAction::AllNotesOff { channel: 7 }),
        );
        assert_eq!(parse_midi_input(&[0xB7, 1, 64]), None);
    }
}
