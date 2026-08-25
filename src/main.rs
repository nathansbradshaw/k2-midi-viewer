mod drums;
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
use iced::widget::{button, checkbox, column, container, row, scrollable, slider, text};
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
    drum_note_to_key: HashMap<u8, KeyId>, // GM percussion note → drum pad key
    key_pos:          HashMap<KeyId, (f32, f32)>, // KeyId → (col, row), for nearest-key picking
    keyboard_notes:        std::collections::HashSet<u8>,
    keyboard_notes_sorted: Vec<u8>, // ascending, for nearest-key search
    highlighted:           HashMap<KeyId, usize>, // KeyId → track index

    // MIDI file
    midi_file:        Option<midi::MidiFile>,
    octave_offset:    i8,
    pitch_step:       i8,          // 1 = semitone, 12 = octave
    vertical_octave:  bool,        // false = left/right (default), true = up/down
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
        let mut keyboard_notes_sorted: Vec<u8> =
            layout.keyboard_notes.iter().copied().collect();
        keyboard_notes_sorted.sort_unstable();

        let key_pos: HashMap<KeyId, (f32, f32)> = layout.keys
            .iter()
            .map(|k| (k.id, (k.col, k.row)))
            .collect();

        App {
            keyboard_notes:        layout.keyboard_notes,
            keyboard_notes_sorted,
            keys:                  layout.keys,
            note_to_all_keys:      layout.note_to_all_keys,
            drum_note_to_key:      layout.drum_note_to_key,
            key_pos,
            highlighted:           HashMap::new(),

            midi_file:        None,
            octave_offset:    0,
            pitch_step:       12,
            vertical_octave:  false,
            show_all_notes:   false,
            all_notes_cache:  HashMap::new(),
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

/// Picks whichever of `kids` sits physically closest (row *and* column distance)
/// to the centroid of keys already in `placed`. This keyboard repeats several
/// notes across overlapping rows, so a chord's duplicate occurrences should
/// cluster together on-screen instead of one jumping to a fixed top/bottom row
/// regardless of how far that drags it from the rest of the chord. Falls back
/// to the old top/bottom preference when there's no context yet to cluster against.
fn pick_nearest_key(
    kids: &[KeyId],
    key_pos: &HashMap<KeyId, (f32, f32)>,
    placed: &HashMap<KeyId, usize>,
    vertical_octave: bool,
) -> Option<KeyId> {
    match kids {
        [] => None,
        [only] => Some(*only),
        _ => {
            let mut sum = (0.0f32, 0.0f32);
            let mut n = 0u32;
            for &kid in placed.keys() {
                if let Some(&(c, r)) = key_pos.get(&kid) {
                    sum.0 += c;
                    sum.1 += r;
                    n += 1;
                }
            }

            let Some(centroid) = (n > 0).then(|| (sum.0 / n as f32, sum.1 / n as f32)) else {
                return Some(if vertical_octave { kids[0] } else { *kids.last().unwrap() });
            };

            kids.iter().copied().min_by(|&a, &b| {
                let dist = |k: KeyId| -> f32 {
                    key_pos.get(&k).map_or(f32::MAX, |&(c, r)| {
                        (c - centroid.0).powi(2) + (r - centroid.1).powi(2)
                    })
                };
                dist(a).total_cmp(&dist(b))
            })
        }
    }
}

impl App {
    fn rebuild_all_notes_cache(&mut self) {
        self.all_notes_cache.clear();
        let Some(ref f) = self.midi_file else { return };

        // Pass 1: in-range notes (track color). Drum-channel notes go straight to
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
                if let Some(kid) = pick_nearest_key(kids, &self.key_pos, &self.all_notes_cache, self.vertical_octave) {
                    self.all_notes_cache.insert(kid, note.track);
                }
            }
        }

        // Pass 2: out-of-range melodic notes — highlight the nearest keyboard key
        // with the warning sentinel (usize::MAX - 1) only if that key isn't already lit.
        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
            if note.channel == synth::DRUM_CHANNEL { continue; }
            let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
            if self.note_to_all_keys.contains_key(&shifted) { continue; }
            if let Some(nearest) = self.nearest_keyboard_note(shifted) {
                if let Some(kids) = self.note_to_all_keys.get(&nearest) {
                    if let Some(kid) = pick_nearest_key(kids, &self.key_pos, &self.all_notes_cache, self.vertical_octave) {
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

        for note in &f.notes {
            if self.track_muted.get(note.track).copied().unwrap_or(false) { continue; }
            if !(note.start_tick < e && note.end_tick > s) { continue; }

            if note.channel == synth::DRUM_CHANNEL {
                if let Some(&kid) = self.drum_note_to_key.get(&note.midi_note) {
                    self.selection_highlight_cache.insert(kid, note.track);
                }
                continue;
            }

            let shifted = (note.midi_note as i16 + self.octave_offset as i16).clamp(0, 127) as u8;
            if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                if let Some(kid) = pick_nearest_key(kids, &self.key_pos, &self.selection_highlight_cache, self.vertical_octave) {
                    self.selection_highlight_cache.insert(kid, note.track);
                }
            } else if let Some(nearest) = self.nearest_keyboard_note(shifted) {
                if let Some(kids) = self.note_to_all_keys.get(&nearest) {
                    if let Some(kid) = pick_nearest_key(kids, &self.key_pos, &self.selection_highlight_cache, self.vertical_octave) {
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
                );
                self.playback_handle = Some(handle);
                self.midi_file = Some(file);
                self.rebuild_all_notes_cache();
                Task::none()
            }

            // ── Pitch nudge ────────────────────────────────────────────────
            Message::PitchUp => {
                self.octave_offset = self.octave_offset.saturating_add(self.pitch_step);
                self.sync_octave_offset();
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
                Task::none()
            }
            Message::PitchDown => {
                self.octave_offset = self.octave_offset.saturating_sub(self.pitch_step);
                self.sync_octave_offset();
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
                if self.show_all_notes { self.rebuild_all_notes_cache(); }
                if self.staff_selection.is_some() { self.rebuild_selection_highlight(); }
                Task::none()
            }
            Message::OctaveLayoutToggle => {
                self.vertical_octave = !self.vertical_octave;
                self.highlighted.clear();
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
                                if let Some(kids) = self.note_to_all_keys.get(&shifted) {
                                    if let Some(kid) = pick_nearest_key(
                                        kids, &self.key_pos, &self.highlighted, self.vertical_octave,
                                    ) {
                                        self.highlighted.insert(kid, track);
                                    }
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
        let layout_label = if self.vertical_octave { "UD" } else { "LR" };
        let pitch_col = column![
            button("▲").on_press_maybe(has_file.then_some(Message::PitchUp)),
            button(step_label).on_press(Message::PitchStepToggle),
            button("▼").on_press_maybe(has_file.then_some(Message::PitchDown)),
            button("↺").on_press_maybe((self.octave_offset != 0).then_some(Message::PitchReset)),
            button(layout_label).on_press(Message::OctaveLayoutToggle),
        ]
        .spacing(2)
        .align_x(Alignment::Center);

        let all_notes_label = if self.show_all_notes { "All *" } else { "All" };
        let all_notes_btn = button(all_notes_label)
            .on_press_maybe(has_file.then_some(Message::ToggleAllNotes));

        let file_row = row![open_btn, meta, pitch_col, all_notes_btn]
            .spacing(16)
            .align_y(Alignment::Center);

        // ── Row 2: transport ───────────────────────────────────────────────
        let play_btn = button("▶").on_press_maybe(
            (has_file && self.play_state != PlayState::Playing).then_some(Message::Play)
        );
        // Single contextual button: ‖ while playing → pause; ■ while paused → stop.
        let stop_pause_btn: Element<Message> = match self.play_state {
            PlayState::Playing => button("‖").on_press(Message::Pause).into(),
            PlayState::Paused  => button("■").on_press(Message::Stop).into(),
            PlayState::Stopped => button("■").into(), // disabled
        };

        let audio_label = if self.audio_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            "Snd"
        } else {
            "Mut"
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
            scrollable(row(items).spacing(12).padding(iced::Padding { bottom: 8.0, ..Default::default() }))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(6).scroller_width(6),
                ))
                .into()
        } else {
            row![].into()
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
        })
        .width(Length::Fill)
        .height(Length::Fill);

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
            text(msg).size(13).into()
        } else {
            row![].into()
        };

        let content = column![file_row, transport_row, track_row, keyboard, staff, selection_row]
            .spacing(8);

        container(content)
            .padding(BOARD_PAD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
