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
use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, slider, text,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow, Size, Subscription, Task, Theme,
    Vector,
};

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

#[cfg(not(target_arch = "wasm32"))]
fn app_icon() -> iced::window::Icon {
    iced::window::icon::from_rgba(
        include_bytes!("../assets/desktop/k2-app-icon-v3.rgba").to_vec(),
        256,
        256,
    )
    .expect("the embedded K2 app icon must be 256x256 RGBA")
}

fn panel_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_BG)),
        border: Border {
            color: PANEL_BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
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
        button::Status::Pressed => (Color::from_rgb(0.075, 0.06, 0.12), TEXT_MAIN, ACCENT),
        button::Status::Disabled => (
            Color::from_rgb(0.085, 0.075, 0.12),
            TEXT_MUTED,
            Color::from_rgb(0.16, 0.12, 0.20),
        ),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 7.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(
                0.0,
                if status == button::Status::Pressed {
                    0.0
                } else {
                    1.0
                },
            ),
            blur_radius: 2.0,
        },
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
    (1..=16u8).map(|channel| ChannelOption { prefix, channel }).collect()
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

fn channel_pick_list_style(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let (background, border_color) = match status {
        pick_list::Status::Active => (
            Color::from_rgb(0.13, 0.105, 0.18),
            Color::from_rgb(0.32, 0.22, 0.40),
        ),
        pick_list::Status::Hovered => (
            Color::from_rgb(0.24, 0.14, 0.30),
            Color::from_rgb(0.54, 0.29, 0.53),
        ),
        pick_list::Status::Opened { .. } => (Color::from_rgb(0.075, 0.06, 0.12), ACCENT),
    };
    pick_list::Style {
        text_color: TEXT_MAIN,
        placeholder_color: TEXT_MUTED,
        handle_color: TEXT_MUTED,
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 7.0.into(),
        },
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
        text_color: if status == button::Status::Disabled {
            TEXT_MUTED
        } else {
            Color::WHITE
        },
        border: Border {
            color: Color::from_rgb(1.0, 0.53, 0.45),
            width: 1.0,
            radius: 7.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(1.0, 0.20, 0.42, 0.28),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
        snap: false,
    }
}

/// Styles an "on/active" indicator (a connected port, an enabled toggle) —
/// visually distinct from [`accent_style`] so those states don't compete
/// with the actual primary-action buttons (Play, Open MIDI) for attention.
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
            radius: 7.0.into(),
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
}

struct App {
    window_size: Size,

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
        "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "⌫",
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
        "NUM", "/", "*", "−", "7", "8", "9", "+", "4", "5", "6", "+", "1", "2", "3", "ENTER", "0",
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
            self.web_midi_status = Some("MIDI access granted — no devices found".to_string());
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

    #[cfg(target_arch = "wasm32")]
    fn web_midi_key_for_note(&self, note: u8, channel: u8) -> Option<(KeyId, bool)> {
        if channel == synth::DRUM_CHANNEL {
            return self
                .drum_note_to_key
                .get(&note)
                .copied()
                .map(|key| (key, false));
        }
        if let Some(keys) = self.note_to_all_keys.get(&note) {
            let key = if self.key_pick_mode == KeyPickMode::Closest {
                pick_key_nearest(keys, &self.key_pos, &self.web_midi_highlighted)
            } else {
                pick_key_fixed(keys, self.key_pick_mode)
            }?;
            return Some((key, false));
        }
        let nearest = self.nearest_keyboard_note(note)?;
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

                let shifted =
                    self.shifted_note(note.midi_note, note.track);
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

                let shifted =
                    self.shifted_note(note.midi_note, note.track);
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

                let shifted =
                    self.shifted_note(note.midi_note, note.track);
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

                let shifted =
                    self.shifted_note(note.midi_note, note.track);
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
        let Some(ref h) = self.playback_handle else { return };
        let range = self.looper_enabled
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
                                if let Some(kid) = self.active_highlight_keys.remove(&(note, channel)) {
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
            text(format!("Error: {e}"))
                .color(Color::from_rgb(0.98, 0.48, 0.38))
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
            let skip = if self.skipped_notes > 0 {
                format!("  ({} skipped)", self.skipped_notes)
            } else {
                String::new()
            };
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
            button("−")
                .padding([5, 10])
                .style(control_style)
                .on_press_maybe(has_file.then_some(Message::PitchDown)),
            button(step_label)
                .padding([5, 10])
                .style(control_style)
                .on_press(Message::PitchStepToggle),
            button("+")
                .padding([5, 10])
                .style(control_style)
                .on_press_maybe(has_file.then_some(Message::PitchUp)),
            button("Reset")
                .padding([5, 10])
                .style(control_style)
                .on_press_maybe((self.octave_offset != 0).then_some(Message::PitchReset)),
            button(layout_label)
                .padding([5, 10])
                .style(control_style)
                .on_press(Message::OctaveLayoutToggle),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let all_notes_label = if self.show_all_notes {
            "All notes: on"
        } else {
            "All notes"
        };
        let all_notes_btn = button(all_notes_label)
            .padding([5, 10])
            .style(if self.show_all_notes {
                toggled_style
            } else {
                control_style
            })
            .on_press_maybe(has_file.then_some(Message::ToggleAllNotes));

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

        let live_channel_btn = pick_list(
            channel_options("PLAY CH"),
            Some(ChannelOption { prefix: "PLAY CH", channel: self.live_channel + 1 }),
            |opt: ChannelOption| Message::LiveChannel(opt.channel - 1),
        )
        .text_size(12)
        .padding([8, 12])
        .style(channel_pick_list_style);

        #[cfg(target_arch = "wasm32")]
        let web_midi_controls: Element<Message> = if self.web_midi_access.is_none() {
            let label = if self.web_midi_pending {
                "Connecting MIDI…"
            } else {
                "Connect MIDI"
            };
            let connect = button(text(label).size(12))
                .padding([8, 12])
                .style(control_style)
                .on_press_maybe((!self.web_midi_pending).then_some(Message::RequestWebMidi));
            let status = self.web_midi_status.as_deref().unwrap_or("");
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
                .on_press_maybe(
                    (!self.web_midi_inputs.is_empty() || self.web_midi_input_id.is_some())
                        .then_some(Message::NextWebMidiInput),
                );
            let output = button(text(format!("MIDI OUT · {output_name}")).size(11))
                .padding([7, 10])
                .style(if self.web_midi_output_id.is_some() {
                    toggled_style
                } else {
                    control_style
                })
                .on_press_maybe(
                    (!self.web_midi_outputs.is_empty() || self.web_midi_output_id.is_some())
                        .then_some(Message::NextWebMidiOutput),
                );
            let status = self.web_midi_status.as_deref().unwrap_or("");
            column![
                row![input, output].spacing(5),
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
        };

        let identity = column![
            text("K2").size(24).color(TEXT_MAIN),
            text("MIDI PERFORMANCE VIEWER").size(10).color(TEXT_MUTED),
        ]
        .spacing(0)
        .width(Length::Fill);

        #[cfg(not(target_arch = "wasm32"))]
        let header_top = row![identity, port_btn, live_channel_btn, open_btn]
            .spacing(row_gap)
            .align_y(Alignment::Center);
        #[cfg(target_arch = "wasm32")]
        let header_top = row![identity, web_midi_controls, live_channel_btn, open_btn]
            .spacing(row_gap)
            .align_y(Alignment::Center);

        let keyboard_hits_label = if self.keyboard_hits_enabled {
            "Computer keys: on"
        } else {
            "Computer keys: off"
        };
        let keyboard_hits_btn = button(keyboard_hits_label)
            .padding([8, 12])
            .style(if self.keyboard_hits_enabled {
                toggled_style
            } else {
                control_style
            })
            .on_press(Message::ToggleKeyboardHits);

        let drum_symbols_label = if self.drum_symbols_enabled {
            "Drum symbols: on"
        } else {
            "Drum symbols: off"
        };
        let drum_symbols_btn = button(drum_symbols_label)
            .padding([8, 12])
            .style(if self.drum_symbols_enabled {
                toggled_style
            } else {
                control_style
            })
            .on_press(Message::ToggleDrumSymbols);

        let header_bottom = row![
            container(meta).width(Length::Fill),
            pitch_controls,
            all_notes_btn,
            keyboard_hits_btn,
            drum_symbols_btn,
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
                (has_file && self.play_state != PlayState::Stopped).then_some(Message::Stop),
            );

        let looper_label = match (self.looper_enabled, self.staff_selection.is_some()) {
            (false, _) => "Loop: off",
            (true, true) => "Loop: selection",
            (true, false) => "Loop: song",
        };
        let looper_btn = button(looper_label)
            .padding([8, 12])
            .style(if self.looper_enabled {
                toggled_style
            } else {
                control_style
            })
            .on_press(Message::ToggleLooper);

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
        let audio_btn = button(text(audio_label).size(14))
            .padding([8, 12])
            .style(control_style)
            .on_press_maybe(audio_available.then_some(Message::ToggleAudio));

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

        let transport_row = container(
            row![
                play_pause_btn,
                stop_btn,
                looper_btn,
                audio_btn,
                live_octave_label,
                scrubber,
                text(time_str).size(13).color(TEXT_MUTED),
            ]
            .spacing(row_gap)
            .align_y(Alignment::Center),
        )
        .padding([panel_v, panel_h])
        .style(panel_style);

        // ── Row 3: track mutes ─────────────────────────────────────────────
        // Only worth showing once there's more than one track to mute/route —
        // for single-track files (or no file yet) it's just an empty panel.
        let track_row: Option<Element<Message>> = self
            .midi_file
            .as_ref()
            .filter(|f| f.tracks.len() > 1)
            .map(|f| {
            let items: Vec<Element<Message>> =
                f.tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let name = t.name.as_deref().unwrap_or("Track");
                        let label = format!("{}: {}", i + 1, name);
                        let muted = self.track_muted.get(i).copied().unwrap_or(false);
                        let (r, g, b) = render::TRACK_COLORS[i % render::TRACK_COLORS.len()];
                        let swatch = container(text("")).width(12).height(12).style(move |_| {
                            container::Style {
                                background: Some(Background::Color(Color::from_rgb8(r, g, b))),
                                border: iced::Border {
                                    radius: 2.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        });
                        let channel = self.track_channel.get(i).copied().unwrap_or(0);
                        let channel_picker = pick_list(
                            channel_options("CH"),
                            Some(ChannelOption { prefix: "CH", channel: channel + 1 }),
                            move |opt: ChannelOption| Message::TrackChannel(i, opt.channel - 1),
                        )
                        .text_size(11)
                        .padding([3, 6])
                        .style(channel_pick_list_style);
                        let octave = self.track_octave.get(i).copied().unwrap_or(0);
                        let octave_picker = pick_list(
                            track_octave_options(),
                            Some(TrackOctaveOption(octave)),
                            move |opt: TrackOctaveOption| Message::TrackOctave(i, opt.0),
                        )
                        .text_size(11)
                        .padding([3, 6])
                        .style(channel_pick_list_style);
                        row![
                            swatch,
                            checkbox(muted)
                                .label(label)
                                .on_toggle(move |v| Message::TrackMuted(i, v)),
                            channel_picker,
                            octave_picker,
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center)
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
            // A bit taller than the other panels — with both a channel and an
            // octave picker per track now, the row reads as cramped at the
            // same vertical padding used for single-line panels.
            .padding([panel_v + 4.0, panel_h])
            .style(panel_style)
            .into()
        });

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

        let keyboard = Canvas::new(BoardCanvas {
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
        .height(390.0);

        // ── Staff canvas (or, with nothing loaded, usage instructions) ──────
        let staff: Element<Message> = if has_file {
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
            .height(Length::Fill)
            .into()
        } else {
            instructions_panel(panel_v, panel_h, row_gap)
        };

        // ── Selection info ────────────────────────────────────────────────
        let selection_row: Element<Message> = if has_file {
            let msg = self
                .selection_summary()
                .unwrap_or_else(|| "Drag on the staff to inspect notes in a range".to_string());
            text(msg).size(12).color(TEXT_MUTED).into()
        } else {
            row![].into()
        };

        let mut content_children: Vec<Element<Message>> =
            vec![file_row.into(), transport_row.into()];
        if let Some(track_row) = track_row {
            content_children.push(track_row);
        }
        content_children.push(keyboard.into());
        content_children.push(staff);
        content_children.push(selection_row);

        let content = column(content_children)
            .spacing(section_gap)
            .height(Length::Fill);

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

/// Fills the staff area with usage instructions while no file is loaded —
/// otherwise that space is just an empty "Load a MIDI file" placeholder.
fn instructions_panel(panel_v: f32, panel_h: f32, row_gap: f32) -> Element<'static, Message> {
    let heading = |t: &'static str| text(t).size(13).color(TEXT_MAIN);
    let body = |t: &'static str| text(t).size(12).color(TEXT_MUTED);

    let sections: [(&str, &str); 9] = [
        (
            "Getting started",
            "Click \"Open MIDI\" to load a .mid file, then use Play / Pause / Stop or drag \
             the scrubber to move through it.",
        ),
        (
            "PITCH",
            "− / + nudge the song by a semitone or octave (toggle which with the ST/OCT \
             button); Reset returns to the automatic best-fit offset. The \"Rows\" button \
             picks which key lights up when a note repeats across the keyboard's overlapping \
             rows: L/R, U/D, or Closest (shortest total travel).",
        ),
        (
            "All notes",
            "Overlays every note in the file on the keyboard at once, instead of only \
             whatever is currently playing.",
        ),
        (
            "Tracks",
            "Mute or unmute individual tracks once a file is loaded — each track's color \
             matches its notes on the keyboard and staff.",
        ),
        (
            "Computer keys",
            "Toggles the ability to play the keyboard by typing on your physical computer \
             keyboard.",
        ),
        (
            "Drum symbols",
            "Shows the GM percussion instrument assigned to each drum pad. Turn it off to \
             restore the pad's numpad legends.",
        ),
        (
            "Looper",
            "Automatically restarts the loaded MIDI file when playback reaches the end.",
        ),
        (
            "Sound / MIDI OUT",
            "\"Sound on\" mutes the built-in synth or selected MIDI output. In Chrome \
             or desktop Firefox, use Connect MIDI, then cycle the separate MIDI IN and \
             MIDI OUT selectors.",
        ),
        (
            "Staff view",
            "Once a file is loaded, this area shows scrolling staff notation — drag across \
             it to inspect the notes in a time range.",
        ),
    ];

    let mut col =
        column![text("How to use K2 MIDI Viewer").size(16).color(TEXT_MAIN)].spacing(row_gap * 1.5);

    for (h, b) in sections {
        col = col.push(column![heading(h), body(b)].spacing(2));
    }

    container(scrollable(col).width(Length::Fill))
        .padding([panel_v, panel_h])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(panel_style)
        .into()
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
    fn midi_input_honors_channel_all_notes_off() {
        assert_eq!(
            parse_midi_input(&[0xB7, 123, 0]),
            Some(MidiInputAction::AllNotesOff { channel: 7 }),
        );
        assert_eq!(parse_midi_input(&[0xB7, 1, 64]), None);
    }
}
