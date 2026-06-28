mod key;
mod layout;
mod midi;
mod playback;
mod render;
mod staff;
mod synth;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use iced::widget::canvas::Canvas;
use iced::widget::{button, checkbox, column, container, row, slider, text};
use iced::{Alignment, Background, Color, Element, Length, Size, Subscription, Task, Theme};

use key::{Key, KeyId};
use layout::build_layout;
use playback::{PlayCmd, PlayEvent, PlaybackHandle};
use render::BoardCanvas;
use staff::StaffCanvas;

const BOARD_PAD: f32 = 16.0;

fn main() -> iced::Result {
    iced::application("K2 MIDI Viewer", App::update, App::view)
        .theme(|_| Theme::Dark)
        .window_size(Size::new(1520.0, 850.0))
        .subscription(App::subscription)
        .run()
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

struct App {
    // keyboard
    keys:             Vec<Key>,
    note_to_all_keys: HashMap<u8, Vec<KeyId>>,
    keyboard_notes:   std::collections::HashSet<u8>,
    highlighted:      HashMap<KeyId, usize>, // KeyId → track index

    // MIDI file
    midi_file:        Option<midi::MidiFile>,
    octave_offset:    i8,
    pitch_step:       i8,          // 1 = semitone, 12 = octave
    vertical_octave:  bool,        // false = left/right (default), true = up/down
    skipped_notes:   usize,
    track_muted:     Vec<bool>,
    load_error:      Option<String>,

    // playback
    playback_handle: Option<PlaybackHandle>,
    play_state:      PlayState,
    position_tick:   u64,
    audio_enabled:   Arc<AtomicBool>,
    playback_events: Arc<Mutex<VecDeque<PlayEvent>>>,

    // MIDI output
    midi_port_names: Vec<String>,
    midi_port_idx:   usize,
}

impl Default for App {
    fn default() -> Self {
        let layout = build_layout();
        let midi_port_names = playback::list_output_ports();
        App {
            keyboard_notes:   layout.keyboard_notes,
            keys:             layout.keys,
            note_to_all_keys: layout.note_to_all_keys,
            highlighted:      HashMap::new(),

            midi_file:        None,
            octave_offset:    0,
            pitch_step:       12,
            vertical_octave:  false,
            skipped_notes:   0,
            track_muted:     Vec::new(),
            load_error:      None,

            playback_handle: None,
            play_state:      PlayState::Stopped,
            position_tick:   0,
            audio_enabled:   Arc::new(AtomicBool::new(true)),
            playback_events: Arc::new(Mutex::new(VecDeque::new())),

            midi_port_idx:   0,
            midi_port_names,
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    // keyboard
    Toggle(KeyId),
    // file
    OpenFile,
    FileChosen(Option<PathBuf>),
    MidiLoaded(Result<midi::MidiFile, String>),
    // pitch nudge
    PitchUp,
    PitchDown,
    PitchStepToggle,
    PitchReset,
    OctaveLayoutToggle,
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
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Keyboard ──────────────────────────────────────────────────
            Message::Toggle(id) => {
                if self.highlighted.remove(&id).is_none() {
                    self.highlighted.insert(id, usize::MAX); // manual = no track colour
                }
                Task::none()
            }

            // ── File loading ──────────────────────────────────────────────
            Message::OpenFile => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("MIDI", &["mid", "midi"])
                        .pick_file()
                        .await
                        .map(|h| h.path().to_path_buf())
                },
                Message::FileChosen,
            ),

            Message::FileChosen(None) => Task::none(),
            Message::FileChosen(Some(path)) => Task::perform(
                async move { midi::load(path) },
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
                );
                self.playback_handle = Some(handle);
                self.midi_file = Some(file);
                Task::none()
            }

            // ── Pitch nudge ────────────────────────────────────────────────
            Message::PitchUp   => { self.octave_offset  = self.octave_offset.saturating_add(self.pitch_step); Task::none() }
            Message::PitchDown => { self.octave_offset  = self.octave_offset.saturating_sub(self.pitch_step); Task::none() }
            Message::PitchStepToggle => {
                self.pitch_step = if self.pitch_step == 12 { 1 } else { 12 };
                Task::none()
            }
            Message::PitchReset => {
                self.octave_offset = 0;
                Task::none()
            }
            Message::OctaveLayoutToggle => {
                self.vertical_octave = !self.vertical_octave;
                self.highlighted.clear();
                Task::none()
            }

            // ── Tracks ─────────────────────────────────────────────────────
            Message::TrackMuted(idx, muted) => {
                if let Some(s) = self.track_muted.get_mut(idx) { *s = muted; }
                if let Some(ref h) = self.playback_handle {
                    h.cmd_tx.send(PlayCmd::SetTrackMuted(idx, muted)).ok();
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
                self.play_state    = PlayState::Stopped;
                self.position_tick = 0;
                self.highlighted.clear();
                Task::none()
            }
            Message::SeekTo(progress) => {
                if let Some(ref f) = self.midi_file {
                    let tick = (progress.clamp(0.0, 1.0) * f.total_ticks as f32) as u64;
                    if let Some(ref h) = self.playback_handle {
                        h.cmd_tx.send(PlayCmd::SeekTo(tick)).ok();
                    }
                    self.position_tick = tick;
                }
                Task::none()
            }

            // ── Poll playback events (fired by subscription every 16 ms) ──
            Message::PollPlayback => {
                let events: Vec<PlayEvent> = {
                    let mut q = self.playback_events.lock().unwrap();
                    q.drain(..).collect()
                };
                for evt in events {
                    match evt {
                        PlayEvent::NoteOn(note, track) => {
                            let shifted = (note as i16 + self.octave_offset as i16)
                                .clamp(0, 127) as u8;
                            if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                                // vertical_octave: pick topmost row (first in list);
                                // horizontal (default): pick bottommost row (last in list).
                                let kid = if self.vertical_octave {
                                    kids.first()
                                } else {
                                    kids.last()
                                };
                                if let Some(&kid) = kid {
                                    self.highlighted.insert(kid, track);
                                }
                            }
                        }
                        PlayEvent::NoteOff(note) => {
                            let shifted = (note as i16 + self.octave_offset as i16)
                                .clamp(0, 127) as u8;
                            // Remove all positions — the note could be lit on any row.
                            if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                                for &kid in kids {
                                    self.highlighted.remove(&kid);
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
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.playback_handle.is_some() {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::PollPlayback)
        } else {
            Subscription::none()
        }
    }

    // ---------------------------------------------------------------------------
    // View
    // ---------------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let has_file = self.midi_file.is_some();

        // ── Row 1: file + metadata ──────────────────────────────────────────
        let open_btn = button("Open MIDI").on_press(Message::OpenFile);

        let meta: Element<Message> = if let Some(ref e) = self.load_error {
            text(format!("Error: {e}")).into()
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
                "{}/{}  {:.0} BPM  {}:{:02}  {}{}",
                f.time_sig.0, f.time_sig.1, bpm, mins, secs, offset_label, skip
            ))
            .into()
        } else {
            text("No file loaded").into()
        };

        let step_label = if self.pitch_step == 12 { "OCT" } else { "ST" };
        let layout_label = if self.vertical_octave { "↕" } else { "↔" };
        let pitch_col = column![
            button("▲").on_press_maybe(has_file.then_some(Message::PitchUp)),
            button(step_label).on_press(Message::PitchStepToggle),
            button("▼").on_press_maybe(has_file.then_some(Message::PitchDown)),
            button("↺").on_press_maybe((self.octave_offset != 0).then_some(Message::PitchReset)),
            button(layout_label).on_press(Message::OctaveLayoutToggle),
        ]
        .spacing(2)
        .align_x(Alignment::Center);

        let file_row = row![open_btn, meta, pitch_col]
            .spacing(16)
            .align_y(Alignment::Center);

        // ── Row 2: transport ───────────────────────────────────────────────
        let play_btn = button("▶").on_press_maybe(
            (has_file && self.play_state != PlayState::Playing).then_some(Message::Play)
        );
        // Single contextual button: ⏸ while playing → pause; ⏹ while paused → stop.
        let stop_pause_btn: Element<Message> = match self.play_state {
            PlayState::Playing => button("⏸").on_press(Message::Pause).into(),
            PlayState::Paused  => button("⏹").on_press(Message::Stop).into(),
            PlayState::Stopped => button("⏹").into(), // disabled
        };

        let audio_label = if self.audio_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            "🔊"
        } else {
            "🔇"
        };
        let audio_btn = button(audio_label).on_press(Message::ToggleAudio);

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
                .width(Length::Fill)
                .into()
        } else {
            slider(0.0f32..=1.0, 0.0f32, |_| Message::SeekTo(0.0))
                .width(Length::Fill)
                .into()
        };

        let port_label = self.midi_port_names
            .get(self.midi_port_idx)
            .map(|s| s.as_str())
            .unwrap_or("No MIDI out");
        let port_btn = button(text(format!("Port: {port_label}"))).on_press(Message::NextPort);

        let transport_row = row![
            play_btn, stop_pause_btn, audio_btn,
            scrubber,
            text(time_str),
            port_btn,
        ]
        .spacing(8)
        .align_y(Alignment::Center);

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
                    checkbox(label, muted).on_toggle(move |v| Message::TrackMuted(i, v))
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .into()
            }).collect();
            row(items).spacing(12).into()
        } else {
            row![].into()
        };

        // ── Keyboard canvas ────────────────────────────────────────────────
        let keyboard = Canvas::new(BoardCanvas {
            keys:        &self.keys,
            highlighted: &self.highlighted,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        // ── Staff canvas ───────────────────────────────────────────────────
        let staff = Canvas::new(StaffCanvas {
            midi_file:     self.midi_file.as_ref(),
            position_tick: self.position_tick,
            track_muted:   &self.track_muted,
            octave_offset: self.octave_offset,
        })
        .width(Length::Fill)
        .height(staff::STAFF_HEIGHT);

        let content = column![file_row, transport_row, track_row, keyboard, staff].spacing(8);

        container(content)
            .padding(BOARD_PAD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
