mod drums;
mod key;
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

use iced::widget::canvas::Canvas;
use iced::widget::{button, checkbox, column, container, row, scrollable, slider, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Size, Subscription, Task, Theme, Vector};

use key::{Cluster, Key, KeyId};
use layout::build_layout;
use playback::{PlayCmd, PlayEvent, PlaybackHandle};
use render::BoardCanvas;
use staff::StaffCanvas;

const SEEK_STEP: f32 = 0.0001;

const APP_BG: Color = Color::from_rgb(0.035, 0.035, 0.075);
const PANEL_BG: Color = Color::from_rgb(0.075, 0.065, 0.125);
const PANEL_BORDER: Color = Color::from_rgb(0.25, 0.17, 0.32);
const TEXT_MAIN: Color = Color::from_rgb(0.95, 0.86, 0.72);
const TEXT_MUTED: Color = Color::from_rgb(0.63, 0.53, 0.68);
const ACCENT: Color = Color::from_rgb(0.96, 0.34, 0.42);

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

fn panel_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_BG)),
        border: Border { color: PANEL_BORDER, width: 1.0, radius: 10.0.into() },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    }
}

fn control_style(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, border_color) = match status {
        button::Status::Active => (
            Color::from_rgb(0.13, 0.105, 0.18),
            TEXT_MAIN,
            Color::from_rgb(0.32, 0.22, 0.40),
        ),
        button::Status::Hovered => (
            Color::from_rgb(0.24, 0.14, 0.30),
            Color::WHITE,
            Color::from_rgb(0.54, 0.29, 0.53),
        ),
        button::Status::Pressed => (
            Color::from_rgb(0.075, 0.06, 0.12),
            TEXT_MAIN,
            ACCENT,
        ),
        button::Status::Disabled => (
            Color::from_rgb(0.085, 0.075, 0.12),
            TEXT_MUTED,
            Color::from_rgb(0.16, 0.12, 0.20),
        ),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { color: border_color, width: 1.0, radius: 7.0.into() },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, if status == button::Status::Pressed { 0.0 } else { 1.0 }),
            blur_radius: 2.0,
        },
        snap: false,
    }
}

fn accent_style(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb(1.0, 0.45, 0.48),
        button::Status::Pressed => Color::from_rgb(0.70, 0.18, 0.31),
        button::Status::Disabled => Color::from_rgb(0.24, 0.13, 0.20),
        button::Status::Active => ACCENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: if status == button::Status::Disabled { TEXT_MUTED } else { Color::WHITE },
        border: Border { color: Color::from_rgb(1.0, 0.53, 0.45), width: 1.0, radius: 7.0.into() },
        shadow: Shadow {
            color: Color::from_rgba(1.0, 0.20, 0.42, 0.28),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
        snap: false,
    }
}

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let app = iced::application(App::default, App::update, App::view)
        .title("K2 MIDI Viewer")
        .theme(app_theme_for)
        .subscription(App::subscription);
    #[cfg(not(target_arch = "wasm32"))]
    let app = app.window_size(Size::new(1520.0, 900.0));
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
                    "~" => "`", "!" => "1", "@" => "2", "#" => "3",
                    "$" => "4", "%" => "5", "^" => "6", "&" => "7",
                    "*" => "8", "(" => "9", ")" => "0", "_" => "-",
                    "+" => "=", "{" => "[", "}" => "]", "|" => "\\",
                    ":" => ";", "\"" => "'", "<" => ",", ">" => ".",
                    "?" => "/", other => other,
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
        iced::Event::Window(iced::window::Event::Unfocused) => {
            Some(Message::ReleaseComputerKeys)
        }
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
}

struct App {
    window_size: Size,

    // keyboard
    keys:             Vec<Key>,
    note_to_all_keys: HashMap<u8, Vec<KeyId>>,
    drum_note_to_key: HashMap<u8, KeyId>, // GM percussion note → drum pad key
    key_pos:          HashMap<KeyId, (f32, f32)>, // KeyId → (col, row), for nearest-key picking
    keyboard_notes:        std::collections::HashSet<u8>,
    keyboard_notes_sorted: Vec<u8>, // ascending, for nearest-key search
    highlighted:           HashMap<KeyId, usize>, // KeyId → track index
    waveform:              synth::Waveform,
    waveform_key:          Option<KeyId>,
    pressed_keys:          HashSet<KeyId>,
    keyboard_hits_enabled: bool,
    computer_keys_down:    HashMap<ComputerKey, Vec<KeyId>>,
    computer_key_labels:   HashMap<KeyId, String>,

    // MIDI file
    midi_file:        Option<midi::MidiFile>,
    octave_offset:    i8,
    pitch_step:       i8,          // 1 = semitone, 12 = octave
    key_pick_mode:    KeyPickMode, // which duplicate key to light when a note repeats across rows
    /// Closest mode's precomputed answer, shared by every highlight path (live
    /// playback, the selection view, and the all-notes overlay) so a given
    /// note always lands on the same key everywhere instead of live playback
    /// re-deciding greedily — and losing context — every time a note re-fires.
    closest_key_for_note: HashMap<u8, KeyId>,
    show_all_notes:   bool,        // overlay every note in the file on the keyboard
    all_notes_cache:  HashMap<KeyId, usize>, // precomputed for show_all_notes
    skipped_notes:   usize,
    track_muted:     Vec<bool>,
    load_error:      Option<String>,

    // playback
    playback_handle: Option<PlaybackHandle>,
    play_state:      PlayState,
    position_tick:   u64,
    audio_enabled:   Arc<AtomicBool>,
    playback_events: Arc<Mutex<VecDeque<PlayEvent>>>,
    soft_synth:      Option<Arc<Mutex<synth::SoftSynth>>>,
    _audio_stream:   Option<cpal::Stream>,
    audio_error:     Option<String>,

    // MIDI output
    midi_port_names: Vec<String>,
    midi_port_idx:   usize,

    // staff selection
    staff_selection:          Option<(u64, u64)>,
    selection_highlight_cache: HashMap<KeyId, usize>,
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
        let mut keyboard_notes_sorted: Vec<u8> =
            layout.keyboard_notes.iter().copied().collect();
        keyboard_notes_sorted.sort_unstable();

        let key_pos: HashMap<KeyId, (f32, f32)> = layout.keys
            .iter()
            .map(|k| (k.id, (k.col, k.row)))
            .collect();
        let computer_key_labels = computer_projection_labels(&layout.keys);

        App {
            window_size: Size::new(1520.0, 900.0),

            keyboard_notes:        layout.keyboard_notes,
            keyboard_notes_sorted,
            keys:                  layout.keys,
            note_to_all_keys:      layout.note_to_all_keys,
            drum_note_to_key:      layout.drum_note_to_key,
            key_pos,
            highlighted:           HashMap::new(),
            waveform:              synth::Waveform::default(),
            waveform_key:          None,
            pressed_keys:          HashSet::new(),
            keyboard_hits_enabled: false,
            computer_keys_down:    HashMap::new(),
            computer_key_labels,

            midi_file:        None,
            octave_offset:    0,
            pitch_step:       12,
            key_pick_mode:    KeyPickMode::LeftRight,
            show_all_notes:   false,
            closest_key_for_note: HashMap::new(),
            all_notes_cache:  HashMap::new(),
            skipped_notes:   0,
            track_muted:     Vec::new(),
            load_error:      None,

            playback_handle: None,
            play_state:      PlayState::Stopped,
            position_tick:   0,
            audio_enabled:   Arc::new(AtomicBool::new(true)),
            playback_events: Arc::new(Mutex::new(VecDeque::new())),
            soft_synth,
            _audio_stream: audio_stream,
            audio_error,

            midi_port_idx:   0,
            midi_port_names,

            staff_selection:           None,
            selection_highlight_cache: HashMap::new(),
        }
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
    ToggleKeyboardHits,
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
    ToggleAllNotes,
    // tracks
    TrackMuted(usize, bool),
    // transport
    Play,
    Pause,
    Stop,
    SeekTo(f32),   // 0.0..=1.0 progress
    PollPlayback,
    // audio
    ToggleAudio,
    // port
    NextPort,
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
        _ => Some(if mode == KeyPickMode::UpDown { kids[0] } else { *kids.last().unwrap() }),
    }
}

fn toggle_waveform(
    current_key: Option<KeyId>,
    pressed_key: KeyId,
    selected: synth::Waveform,
) -> (Option<KeyId>, synth::Waveform) {
    if current_key == Some(pressed_key) {
        (None, synth::Waveform::default())
    } else {
        (Some(pressed_key), selected)
    }
}

fn key_range(keys: &[Key], row: f32, start: usize, end: usize) -> Vec<KeyId> {
    let mut row_keys: Vec<&Key> = keys.iter()
        .filter(|key| {
            matches!(key.cluster, Cluster::Alpha | Cluster::AlphaLight)
                && key.row == row
        })
        .collect();
    row_keys.sort_by(|a, b| a.col.total_cmp(&b.col));
    row_keys.get(start..=end)
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
    let mut numpad: Vec<&Key> = keys.iter()
        .filter(|key| key.cluster == Cluster::Numpad)
        .collect();
    numpad.sort_by(|a, b| a.row.total_cmp(&b.row).then(a.col.total_cmp(&b.col)));
    indices.iter().filter_map(|&index| numpad.get(index).map(|key| key.id)).collect()
}

fn mapped_computer_keys(keys: &[Key], computer_key: &ComputerKey) -> Vec<KeyId> {
    use iced::keyboard::key::Named;

    let alpha_span = match computer_key {
        ComputerKey::Character(character, ComputerKeyLocation::Standard) => {
            const ROW_1: [&str; 13] = ["`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="];
            const ROW_2: [&str; 13] = ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]", "\\"];
            const ROW_3: [&str; 11] = ["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"];
            const ROW_4: [&str; 10] = ["z", "x", "c", "v", "b", "n", "m", ",", ".", "/"];

            ROW_1.iter().position(|value| *value == character).map(|index| (1.0, index, index))
                .or_else(|| ROW_2.iter().position(|value| *value == character).map(|index| (2.0, index + 2, index + 2)))
                .or_else(|| ROW_3.iter().position(|value| *value == character).map(|index| (3.0, index + 1, index + 1)))
                .or_else(|| ROW_4.iter().position(|value| *value == character).map(|index| (4.0, index + 2, index + 2)))
        }
        ComputerKey::Named(Named::Backspace, ComputerKeyLocation::Standard) => Some((1.0, 13, 13)),
        ComputerKey::Named(Named::Tab, ComputerKeyLocation::Standard) => Some((2.0, 0, 1)),
        ComputerKey::Named(Named::CapsLock, ComputerKeyLocation::Standard) => Some((3.0, 0, 0)),
        ComputerKey::Named(Named::Enter, ComputerKeyLocation::Standard) => Some((3.0, 12, 13)),
        ComputerKey::Named(Named::Shift, ComputerKeyLocation::Left) => Some((4.0, 0, 1)),
        ComputerKey::Named(Named::Shift, ComputerKeyLocation::Right) => Some((4.0, 12, 14)),
        ComputerKey::Named(Named::Control, ComputerKeyLocation::Left) => Some((5.0, 0, 0)),
        ComputerKey::Named(Named::Fn | Named::Meta | Named::Super, ComputerKeyLocation::Left) => Some((5.0, 1, 1)),
        ComputerKey::Named(Named::Alt, ComputerKeyLocation::Left) => Some((5.0, 2, 2)),
        ComputerKey::Named(Named::Space, _) => Some((5.0, 3, 8)),
        ComputerKey::Named(Named::Alt | Named::AltGraph, ComputerKeyLocation::Right) => Some((5.0, 9, 9)),
        ComputerKey::Named(Named::Meta | Named::Super, ComputerKeyLocation::Right) => Some((5.0, 10, 10)),
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
        ComputerKey::Named(Named::Enter, ComputerKeyLocation::Numpad) => numpad_range(keys, &[15, 19]),
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

    for (index, label) in ["`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "⌫"]
        .iter().enumerate()
    {
        label_range(1.0, index, index, label);
    }

    label_range(2.0, 0, 1, "TAB");
    for (index, label) in ["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\"]
        .iter().enumerate()
    {
        label_range(2.0, index + 2, index + 2, label);
    }

    label_range(3.0, 0, 0, "CAPS");
    for (index, label) in ["A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'"]
        .iter().enumerate()
    {
        label_range(3.0, index + 1, index + 1, label);
    }
    label_range(3.0, 12, 13, "ENTER");

    label_range(4.0, 0, 1, "SHIFT");
    for (index, label) in ["Z", "X", "C", "V", "B", "N", "M", ",", ".", "/"]
        .iter().enumerate()
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
                let Some(&(c, r)) = key_pos.get(&k) else { return f32::MAX };
                placed.keys()
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
fn shortest_path_keys(
    stages: &[Vec<KeyId>],
    key_pos: &HashMap<KeyId, (f32, f32)>,
) -> Vec<KeyId> {
    let dist = |a: KeyId, b: KeyId| -> f32 {
        match (key_pos.get(&a), key_pos.get(&b)) {
            (Some(&(c1, r1)), Some(&(c2, r2))) => ((c1 - c2).powi(2) + (r1 - r2).powi(2)).sqrt(),
            _ => 0.0,
        }
    };

    // dp[i][k] = (cheapest total cost to reach stages[i][k], index into stages[i-1] that got us there)
    let mut dp: Vec<Vec<(f32, usize)>> = vec![vec![(0.0, 0); stages[0].len()]];
    for i in 1..stages.len() {
        let row = stages[i].iter().map(|&cand| {
            stages[i - 1].iter().enumerate()
                .map(|(j, &prev)| (dp[i - 1][j].0 + dist(prev, cand), j))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap()
        }).collect();
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
            }
            Err(error) => self.audio_error = Some(error),
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
    fn rebuild_closest_key_map(&mut self) {
        self.closest_key_for_note.clear();
        if self.key_pick_mode != KeyPickMode::Closest { return; }
        let Some(ref f) = self.midi_file else { return };

        let mut shifted_notes: Vec<u8> = Vec::new();
        let mut stages: Vec<Vec<KeyId>> = Vec::new();
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
            if note.channel == synth::DRUM_CHANNEL { continue; }
            let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
            if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                shifted_notes.push(shifted);
                stages.push(kids.clone());
            }
        }
        if stages.is_empty() { return; }

        for (shifted, kid) in shifted_notes.into_iter().zip(shortest_path_keys(&stages, &self.key_pos)) {
            self.closest_key_for_note.insert(shifted, kid);
        }
    }

    fn rebuild_all_notes_cache(&mut self) {
        self.all_notes_cache.clear();
        let Some(ref f) = self.midi_file else { return };

        if self.key_pick_mode == KeyPickMode::Closest {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.all_notes_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
                if let Some(&kid) = self.closest_key_for_note.get(&shifted) {
                    self.all_notes_cache.insert(kid, note.track);
                }
            }
        } else {
            // In-range notes (track color). Drum-channel notes go straight to
            // their dedicated pad — no octave shift, no nearest-key fallback.
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.all_notes_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
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
            if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
            if note.channel == synth::DRUM_CHANNEL { continue; }
            let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
            if self.note_to_all_keys.contains_key(&shifted) { continue; }
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
        let Some(ref f) = self.midi_file else { return };
        let Some((s, e)) = self.staff_selection else { return };
        let e = e.max(s + 1);
        let in_range = |note: &midi::Note| note.start_tick < e && note.end_tick > s;

        if self.key_pick_mode == KeyPickMode::Closest {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
                if !in_range(note) { continue; }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.selection_highlight_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
                if let Some(&kid) = self.closest_key_for_note.get(&shifted) {
                    self.selection_highlight_cache.insert(kid, note.track);
                }
            }
        } else {
            for note in &f.notes {
                if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
                if !in_range(note) { continue; }

                if note.channel == synth::DRUM_CHANNEL {
                    if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                        self.selection_highlight_cache.insert(kid, note.track);
                    }
                    continue;
                }

                let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
                if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                    if let Some(kid) = pick_key_fixed(kids, self.key_pick_mode) {
                        self.selection_highlight_cache.insert(kid, note.track);
                    }
                }
            }
        }

        // Out-of-range melodic notes within the selection — same nearest-keyboard
        // fallback as rebuild_all_notes_cache.
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
            if !in_range(note) { continue; }
            if note.channel == synth::DRUM_CHANNEL { continue; }
            let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
            if self.note_to_all_keys.contains_key(&shifted) { continue; }
            if let Some(nearest) = self.nearest_keyboard_note(shifted) {
                if let Some(kids) = self.note_to_all_keys.get(&nearest) {
                    let kid = if self.key_pick_mode == KeyPickMode::Closest {
                        pick_key_nearest(kids, &self.key_pos, &self.selection_highlight_cache)
                    } else {
                        pick_key_fixed(kids, self.key_pick_mode)
                    };
                    if let Some(kid) = kid {
                        self.selection_highlight_cache.entry(kid).or_insert(usize::MAX - 1);
                    }
                }
            }
        }
    }

    /// Tells the playback thread about a new octave offset, so it can tell which
    /// notes actually land on the physical keyboard and skip audio for the rest.
    fn sync_octave_offset(&self) {
        if let Some(ref h) = self.playback_handle {
            h.cmd_tx.send(PlayCmd::SetOctaveOffset(self.octave_offset)).ok();
        }
    }

    fn key_sound(&self, id: KeyId) -> Option<(u8, u8)> {
        if let Some(note) = self.keys.iter()
            .find(|key| key.id == id)
            .and_then(|key| key.midi_note)
        {
            return Some((note, 0));
        }

        self.drum_note_to_key.iter()
            .find_map(|(&note, &key_id)| (key_id == id).then_some((note, synth::DRUM_CHANNEL)))
    }

    fn press_board_key(&mut self, id: KeyId) {
        self.pressed_keys.insert(id);
        let waveform = self.keys.iter()
            .find(|key| key.id == id && key.cluster == Cluster::Nav)
            .and_then(|key| match key.label {
                "Insert" => Some(synth::Waveform::Triangle),
                "Home" => Some(synth::Waveform::Square),
                "PgUp" => Some(synth::Waveform::Saw),
                _ => None,
            });

        if let Some(waveform) = waveform {
            let (waveform_key, waveform) =
                toggle_waveform(self.waveform_key, id, waveform);
            self.waveform_key = waveform_key;
            self.waveform = waveform;
            if let Some(ref synth) = self.soft_synth {
                if let Ok(mut synth) = synth.lock() { synth.set_waveform(waveform); }
            }
            if let Some(ref h) = self.playback_handle {
                h.cmd_tx.send(PlayCmd::SetWaveform(waveform)).ok();
            }
            return;
        }

        if let Some((note, channel)) = self.key_sound(id) {
            if self.audio_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::LiveNoteOn(note, 108, channel)).ok();
                } else if let Some(ref synth) = self.soft_synth {
                    if let Ok(mut synth) = synth.lock() { synth.note_on(note, 108, channel); }
                }
            }
        }
    }

    fn release_board_key(&mut self, id: KeyId) {
        self.pressed_keys.remove(&id);
        if let Some((note, channel)) = self.key_sound(id) {
            if let Some(ref h) = self.playback_handle {
                h.cmd_tx.send(PlayCmd::LiveNoteOff(note, channel)).ok();
            } else if let Some(ref synth) = self.soft_synth {
                if let Ok(mut synth) = synth.lock() { synth.note_off(note, channel); }
            }
        }
    }

    fn release_computer_keys(&mut self) {
        let keys: Vec<KeyId> = self.computer_keys_down
            .drain()
            .flat_map(|(_, keys)| keys)
            .collect();
        for id in keys {
            self.release_board_key(id);
        }
    }

    fn nearest_keyboard_note(&self, note: u8) -> Option<u8> {
        let s = &self.keyboard_notes_sorted;
        if s.is_empty() { return None; }
        let pos = s.partition_point(|&n| n < note);
        Some(if pos == 0 {
            s[0]
        } else if pos == s.len() {
            *s.last().unwrap()
        } else {
            let below = s[pos - 1];
            let above = s[pos];
            if note - below <= above - note { below } else { above }
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

            Message::ToggleKeyboardHits => {
                if self.keyboard_hits_enabled {
                    self.release_computer_keys();
                }
                self.keyboard_hits_enabled = !self.keyboard_hits_enabled;
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
            Message::FileChosen(Some(bytes)) => Task::perform(
                async move { midi::load_bytes(&bytes) },
                Message::MidiLoaded,
            ),

            Message::MidiLoaded(Err(e)) => {
                self.load_error = Some(e);
                Task::none()
            }
            Message::MidiLoaded(Ok(file)) => {
                let (offset, covered, total) =
                    midi::best_octave_offset(&file, &self.keyboard_notes);
                self.skipped_notes  = total.saturating_sub(covered);
                self.octave_offset  = offset;
                self.track_muted    = vec![false; file.tracks.len()];
                self.load_error     = None;
                self.play_state     = PlayState::Stopped;
                self.position_tick  = 0;
                self.highlighted.clear();
                self.staff_selection = None;
                self.selection_highlight_cache.clear();

                // Drop any existing playback thread
                self.playback_handle = None;

                // Spawn a new idle playback thread ready for this file
                let conn = playback::open_output(self.midi_port_idx);
                let handle = playback::spawn(
                    Arc::new(file.clone()),
                    Arc::clone(&self.playback_events),
                    Arc::clone(&self.audio_enabled),
                    self.track_muted.clone(),
                    conn,
                    Arc::new(self.keyboard_notes.clone()),
                    self.octave_offset,
                    self.waveform,
                    self.soft_synth.as_ref().map(Arc::clone),
                );
                self.playback_handle = Some(handle);
                self.midi_file = Some(file);
                self.rebuild_closest_key_map();
                self.rebuild_all_notes_cache();
                Task::none()
            }

            // ── Pitch nudge ────────────────────────────────────────────────
            Message::PitchUp => {
                self.octave_offset = self.octave_offset.saturating_add(self.pitch_step);
                self.sync_octave_offset();
                self.rebuild_closest_key_map();
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
                Task::none()
            }
            Message::PitchDown => {
                self.octave_offset = self.octave_offset.saturating_sub(self.pitch_step);
                self.sync_octave_offset();
                self.rebuild_closest_key_map();
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
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
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
                Task::none()
            }
            Message::OctaveLayoutToggle => {
                self.key_pick_mode = self.key_pick_mode.next();
                self.highlighted.clear();
                self.rebuild_closest_key_map();
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
                Task::none()
            }
            Message::ToggleAllNotes => {
                self.show_all_notes = !self.show_all_notes;
                if self.show_all_notes {
                    self.rebuild_all_notes_cache();
                } else {
                    self.highlighted.clear();
                }
                Task::none()
            }

            // ── Tracks ─────────────────────────────────────────────────────
            Message::TrackMuted(idx, muted) => {
                if let Some(s) = self.track_muted.get_mut(idx) { *s = muted; }
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetTrackMuted(idx, muted)).ok();
                }
                self.rebuild_closest_key_map();
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
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
                self.play_state    = PlayState::Stopped;
                self.position_tick = 0;
                self.highlighted.clear();
                Task::none()
            }
            Message::SeekTo(progress) => {
                if let Some(ref f) = self.midi_file {
                    let tick = (progress.clamp(0.0, 1.0) * f.total_ticks as f32) as u64;
                    // Discard notes and position reports from the old timeline
                    // before asking the playback clock to re-anchor.
                    self.highlighted.clear();
                    self.playback_events.lock().unwrap().clear();
                    if let Some(ref h) = self.playback_handle {
                        h.cmd_tx.send(PlayCmd::SeekTo(tick)).ok();
                    }
                    self.position_tick = tick;
                }
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
                                let shifted = (note as i16 + self.octave_offset as i16)
                                    .clamp(0, 127) as u8;
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
                                let shifted = (note as i16 + self.octave_offset as i16)
                                    .clamp(0, 127) as u8;
                                if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                                    for &kid in kids {
                                        self.highlighted.remove(&kid);
                                    }
                                }
                            }
                        }
                        PlayEvent::Position(t) => {
                            self.position_tick = t;
                        }
                        PlayEvent::Done => {
                            self.play_state = PlayState::Stopped;
                            self.position_tick = 0;
                            self.highlighted.clear();
                        }
                    }
                }
                Task::none()
            }

            // ── Audio toggle ───────────────────────────────────────────────
            Message::ToggleAudio => {
                let was = self.audio_enabled.fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetAudio(!was)).ok();
                } else if was {
                    if let Some(ref synth) = self.soft_synth {
                        if let Ok(mut synth) = synth.lock() { synth.all_notes_off(); }
                    }
                }
                Task::none()
            }

            // ── Port cycling ───────────────────────────────────────────────
            Message::NextPort => {
                if !self.midi_port_names.is_empty() {
                    self.midi_port_idx = (self.midi_port_idx + 1) % self.midi_port_names.len();
                }
                Task::none()
            }

            // ── Staff selection ───────────────────────────────────────────
            Message::StaffSelectionChanged(sel) => {
                self.staff_selection = sel;
                self.rebuild_selection_highlight();
                Task::none()
            }
        }
    }

    /// Human-readable summary of the notes under the current staff selection.
    fn selection_summary(&self) -> Option<String> {
        let f = self.midi_file.as_ref()?;
        let (s, e) = self.staff_selection?;
        let e = e.max(s + 1); // a zero-width selection still catches notes at that instant

        let mut notes: Vec<&midi::Note> = f.notes.iter()
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

        let track_strs: Vec<String> = by_track.iter().map(|(t, names)| {
            let tname = f.tracks.get(*t).and_then(|ti| ti.name.as_deref()).unwrap_or("Track");
            format!("T{} {}: {}", t + 1, tname, names.join(", "))
        }).collect();

        Some(format!(
            "{} note{} · {}",
            notes.len(),
            if notes.len() == 1 { "" } else { "s" },
            track_strs.join("   |   "),
        ))
    }

    fn subscription(&self) -> Subscription<Message> {
        let playback = if self.playback_handle.is_some() {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::PollPlayback)
        } else {
            Subscription::none()
        };
        let resize = iced::window::resize_events()
            .map(|(_, size)| Message::WindowResized(size));
        let computer_keyboard = if self.keyboard_hits_enabled {
            iced::event::listen_with(computer_keyboard_event)
        } else {
            Subscription::none()
        };

        Subscription::batch([playback, resize, computer_keyboard])
    }

    // ---------------------------------------------------------------------------
    // View
    // ---------------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let has_file = self.midi_file.is_some();
        let (outer_pad, section_gap, panel_v, panel_h, row_gap, track_gap) =
            if self.window_size.width < 1200.0 {
                (4.0, 4.0, 5.0, 6.0, 5.0, 8.0)
            } else if self.window_size.width < 1600.0 {
                (8.0, 6.0, 7.0, 8.0, 7.0, 10.0)
            } else {
                (18.0, 10.0, 10.0, 12.0, 10.0, 14.0)
            };

        // ── Header: identity, file, metadata and pitch mapping ──────────────
        let open_btn = button("Open MIDI")
            .padding([8, 14])
            .style(accent_style)
            .on_press(Message::OpenFile);

        let meta: Element<Message> = if let Some(ref e) = self.load_error {
            text(format!("Error: {e}")).color(Color::from_rgb(0.98, 0.48, 0.38)).into()
        } else if let Some(ref f) = self.midi_file {
            let bpm  = midi::bpm_at(0, &f.tempo_map);
            let dur  = midi::total_duration_secs(f);
            let mins = (dur / 60.0) as u32;
            let secs = (dur % 60.0) as u32;
            let offset_label = if self.octave_offset == 0 {
                "±0".to_string()
            } else if self.octave_offset % 12 == 0 {
                format!("{:+} oct", self.octave_offset / 12)
            } else {
                format!("{:+} st", self.octave_offset)
            };
            let skip = if self.skipped_notes > 0 {
                format!("  ({} skipped)", self.skipped_notes)
            } else { String::new() };
            text(format!(
                "{}/{}   ·   {:.0} BPM   ·   {}:{:02}   ·   {}{}",
                f.time_sig.0, f.time_sig.1, bpm, mins, secs, offset_label, skip
            ))
            .size(14)
            .color(TEXT_MUTED)
            .into()
        } else {
            text("No MIDI loaded — open a file to begin")
                .size(14)
                .color(TEXT_MUTED)
                .into()
        };

        let step_label = if self.pitch_step == 12 { "OCT" } else { "ST" };
        let layout_label = self.key_pick_mode.label();
        let pitch_controls = row![
            text("PITCH").size(10).color(TEXT_MUTED),
            button("−").padding([5, 10]).style(control_style)
                .on_press_maybe(has_file.then_some(Message::PitchDown)),
            button(step_label).padding([5, 10]).style(control_style)
                .on_press(Message::PitchStepToggle),
            button("+").padding([5, 10]).style(control_style)
                .on_press_maybe(has_file.then_some(Message::PitchUp)),
            button("Reset").padding([5, 10]).style(control_style)
                .on_press_maybe((self.octave_offset != 0).then_some(Message::PitchReset)),
            button(layout_label).padding([5, 10]).style(control_style)
                .on_press(Message::OctaveLayoutToggle),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let all_notes_label = if self.show_all_notes { "All notes: on" } else { "All notes" };
        let all_notes_btn = button(all_notes_label)
            .padding([5, 10])
            .style(if self.show_all_notes { accent_style } else { control_style })
            .on_press_maybe(has_file.then_some(Message::ToggleAllNotes));

        #[cfg(not(target_arch = "wasm32"))]
        let port_label = self.midi_port_names
            .get(self.midi_port_idx)
            .map(|s| s.as_str())
            .unwrap_or("No MIDI output");
        #[cfg(not(target_arch = "wasm32"))]
        let port_btn = button(text(format!("MIDI OUT  ·  {port_label}")).size(12))
            .padding([8, 12])
            .style(control_style)
            .on_press(Message::NextPort);

        let identity = column![
            text("K2").size(24).color(TEXT_MAIN),
            text("MIDI PERFORMANCE VIEWER").size(10).color(TEXT_MUTED),
        ]
        .spacing(0)
        .width(Length::Fill);

        #[cfg(not(target_arch = "wasm32"))]
        let header_top = row![identity, port_btn, open_btn]
            .spacing(row_gap)
            .align_y(Alignment::Center);
        #[cfg(target_arch = "wasm32")]
        let header_top = row![identity, open_btn]
            .spacing(row_gap)
            .align_y(Alignment::Center);
        let header_bottom = row![
            container(meta).width(Length::Fill),
            pitch_controls,
            all_notes_btn,
        ]
        .spacing(row_gap)
        .align_y(Alignment::Center);
        let file_row = container(column![header_top, header_bottom].spacing(row_gap))
            .padding([panel_v, panel_h])
            .style(panel_style);

        // ── Row 2: transport ───────────────────────────────────────────────
        let play_pause_btn: Element<Message> = match self.play_state {
            PlayState::Playing => button("Pause")
                .padding([8, 14])
                .style(accent_style)
                .on_press(Message::Pause)
                .into(),
            PlayState::Paused | PlayState::Stopped => button("Play")
                .padding([8, 14])
                .style(accent_style)
                .on_press_maybe(has_file.then_some(Message::Play))
                .into(),
        };
        let stop_btn = button("Stop")
            .padding([8, 12])
            .style(control_style)
            .on_press_maybe(
                (has_file && self.play_state != PlayState::Stopped).then_some(Message::Stop)
            );

        let audio_label = if let Some(error) = &self.audio_error {
            format!("Sound unavailable · {error}")
        } else if self.audio_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            "Sound on".to_string()
        } else {
            "Muted".to_string()
        };
        let audio_btn = button(text(audio_label).size(14))
            .padding([8, 12])
            .style(control_style)
            .on_press_maybe(self.soft_synth.is_some().then_some(Message::ToggleAudio));

        let keyboard_hits_label = if self.keyboard_hits_enabled {
            "Computer keys: on"
        } else {
            "Computer keys: off"
        };
        let keyboard_hits_btn = button(keyboard_hits_label)
            .padding([8, 12])
            .style(if self.keyboard_hits_enabled { accent_style } else { control_style })
            .on_press(Message::ToggleKeyboardHits);

        let (progress, time_str) = if let Some(ref f) = self.midi_file {
            let p = if f.total_ticks > 0 {
                self.position_tick as f32 / f.total_ticks as f32
            } else { 0.0 };
            let cur_us = midi::tick_to_micros_abs(self.position_tick, &f.tempo_map, f.ticks_per_beat);
            let tot_us = midi::tick_to_micros_abs(f.total_ticks,     &f.tempo_map, f.ticks_per_beat);
            let fmt = |us: u64| format!("{}:{:02}", us / 60_000_000, (us % 60_000_000) / 1_000_000);
            (p, format!("{} / {}", fmt(cur_us), fmt(tot_us)))
        } else {
            (0.0f32, "0:00 / 0:00".to_string())
        };

        let scrubber: Element<Message> = if has_file {
            slider(0.0f32..=1.0, progress, Message::SeekTo)
                .step(SEEK_STEP)
                .shift_step(SEEK_STEP / 10.0)
                .width(Length::Fill)
                .into()
        } else {
            slider(0.0f32..=1.0, 0.0f32, |_| Message::SeekTo(0.0))
                .step(SEEK_STEP)
                .width(Length::Fill)
                .into()
        };

        let transport_row = container(row![
            play_pause_btn, stop_btn, audio_btn, keyboard_hits_btn,
            scrubber,
            text(time_str).size(13).color(TEXT_MUTED),
        ]
        .spacing(row_gap)
        .align_y(Alignment::Center))
        .padding([panel_v, panel_h])
        .style(panel_style);

        // ── Row 3: track mutes ─────────────────────────────────────────────
        let track_row: Element<Message> = if let Some(ref f) = self.midi_file {
            let items: Vec<Element<Message>> = f.tracks.iter().enumerate().map(|(i, t)| {
                let name  = t.name.as_deref().unwrap_or("Track");
                let label = format!("{}: {}", i + 1, name);
                let muted = self.track_muted.get(i).copied().unwrap_or(false);
                let (r, g, b) = render::TRACK_COLORS[i % render::TRACK_COLORS.len()];
                let swatch = container(text(""))
                    .width(12)
                    .height(12)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(Color::from_rgb8(r, g, b))),
                        border: iced::Border { radius: 2.0.into(), ..Default::default() },
                        ..Default::default()
                    });
                row![
                    swatch,
                    checkbox(muted).label(label).on_toggle(move |v| Message::TrackMuted(i, v))
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .into()
            }).collect();
            let tracks = scrollable(row(items).spacing(track_gap).align_y(Alignment::Center))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(6).scroller_width(6),
                ));
            container(row![text("TRACKS").size(10).color(TEXT_MUTED), tracks]
                .spacing(row_gap)
                .align_y(Alignment::Center))
                .padding([panel_v, panel_h])
                .style(panel_style)
                .into()
        } else {
            container(
                row![
                    text("TRACKS").size(10).color(TEXT_MUTED),
                    text("Track controls appear after loading a MIDI file").size(12).color(TEXT_MUTED),
                ]
                .spacing(row_gap)
                .align_y(Alignment::Center),
            )
            .padding([panel_v, panel_h])
            .style(panel_style)
            .into()
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

        let keyboard = Canvas::new(BoardCanvas {
            keys:        &self.keys,
            highlighted: highlighted_ref,
            selected_control: self.waveform_key,
            pressed: &self.pressed_keys,
            projected_labels: self.keyboard_hits_enabled.then_some(&self.computer_key_labels),
        })
        .width(Length::Fill)
        .height(390.0);

        // ── Staff canvas ───────────────────────────────────────────────────
        let staff = Canvas::new(StaffCanvas {
            midi_file:     self.midi_file.as_ref(),
            position_tick: self.position_tick,
            track_muted:   &self.track_muted,
            octave_offset: self.octave_offset,
            selection:     self.staff_selection,
            keyboard_notes:    &self.keyboard_notes,
            drum_note_to_key:  &self.drum_note_to_key,
        })
        .width(Length::Fill)
        .height(staff::STAFF_HEIGHT);

        // ── Selection info ────────────────────────────────────────────────
        let selection_row: Element<Message> = if has_file {
            let msg = self.selection_summary()
                .unwrap_or_else(|| "Drag on the staff to inspect notes in a range".to_string());
            text(msg).size(12).color(TEXT_MUTED).into()
        } else {
            row![].into()
        };

        let content = column![file_row, transport_row, track_row, keyboard, staff, selection_row]
            .spacing(section_gap);

        container(content)
            .padding(outer_pad)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(APP_BG)),
                text_color: Some(TEXT_MAIN),
                ..Default::default()
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_control_toggles_back_to_default() {
        let key = KeyId(42);
        let (active, waveform) = toggle_waveform(None, key, synth::Waveform::Triangle);
        assert_eq!(active, Some(key));
        assert_eq!(waveform, synth::Waveform::Triangle);

        let (active, waveform) = toggle_waveform(active, key, synth::Waveform::Triangle);
        assert_eq!(active, None);
        assert_eq!(waveform, synth::Waveform::default());
    }

    #[test]
    fn choosing_another_waveform_keeps_a_control_active() {
        let triangle_key = KeyId(42);
        let square_key = KeyId(43);
        let (active, waveform) =
            toggle_waveform(Some(triangle_key), square_key, synth::Waveform::Square);
        assert_eq!(active, Some(square_key));
        assert_eq!(waveform, synth::Waveform::Square);
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
        assert!(mapped.iter().all(|id| layout.keys.iter()
            .find(|candidate| candidate.id == *id)
            .is_some_and(|candidate| candidate.row == 5.0)));
        assert!(mapped.iter().all(|id| labels.get(id).is_some_and(|label| label == "SPACE")));
    }

    #[test]
    fn computer_numpad_zero_spans_two_drum_pads() {
        let layout = build_layout();
        let key = ComputerKey::Character("0".to_string(), ComputerKeyLocation::Numpad);
        let mapped = mapped_computer_keys(&layout.keys, &key);

        assert_eq!(mapped.len(), 2);
        assert!(mapped.iter().all(|id| layout.drum_note_to_key.values()
            .any(|drum_key| drum_key == id)));
    }
}
