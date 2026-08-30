use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;
use web_time::Instant;
use web_sys::{
    MidiAccess, MidiInput, MidiMessageEvent, MidiOptions, MidiOutput, MidiPortDeviceState,
};

use crate::midi::{EventKind, MidiFile, tick_to_micros_abs};
use crate::synth::SoftSynth;

#[derive(Debug)]
pub enum PlayCmd {
    Play,
    Pause,
    Stop,
    SeekTo(u64),
    SetAudio(bool),
    SetTrackMuted(usize, bool),
    SetTrackChannel(usize, u8),
    SetTrackOctave(usize, i8),
    SetLoopRange(Option<(u64, u64)>),
    SetOctaveOffset(i8),
    SetWaveforms(Vec<crate::synth::Waveform>),
    SetKnob(u8, f32),
    LiveNoteOn(u8, u8, u8),
    LiveNoteOff(u8, u8),
    SetMidiOutput(Option<MidiOutputConnection>),
    /// Playback rate multiplier (1.0 = normal speed).
    SetSpeed(f32),
}

#[derive(Debug)]
pub enum PlayEvent {
    NoteOn(u8, usize, u8),
    NoteOff(u8, u8),
    Position(u64),
    Done,
}

#[derive(Clone)]
pub struct CommandSender {
    commands: Arc<Mutex<VecDeque<PlayCmd>>>,
}

impl CommandSender {
    pub fn send(&self, command: PlayCmd) -> Result<(), ()> {
        self.commands.lock().map_err(|_| ())?.push_back(command);
        Ok(())
    }
}

pub struct PlaybackHandle {
    pub cmd_tx: CommandSender,
    state: Arc<Mutex<WebPlayback>>,
}

impl PlaybackHandle {
    /// Advances browser playback from the iced animation timer. Keeping this on
    /// the UI thread avoids Web Worker/thread requirements on static hosting.
    pub fn poll(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.poll();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
}

/// Owns the browser's MIDIAccess object and the state-change callback that keeps
/// device hot-plug detection alive.
pub struct MidiAccessHandle {
    access: MidiAccess,
    ports_changed: Arc<AtomicBool>,
    _state_change: Closure<dyn FnMut()>,
}

impl MidiAccessHandle {
    pub fn new(access: MidiAccess) -> Self {
        let ports_changed = Arc::new(AtomicBool::new(true));
        let changed = Arc::clone(&ports_changed);
        let state_change = Closure::wrap(Box::new(move || {
            changed.store(true, Ordering::Relaxed);
        }) as Box<dyn FnMut()>);
        access.set_onstatechange(Some(state_change.as_ref().unchecked_ref()));
        Self { access, ports_changed, _state_change: state_change }
    }

    pub fn take_ports_changed(&self) -> bool {
        self.ports_changed.swap(false, Ordering::Relaxed)
    }

    pub fn input_ports(&self) -> Vec<MidiPortInfo> {
        let mut ports = Vec::new();
        let inputs: js_sys::Map = self.access.inputs().unchecked_into();
        inputs.for_each(&mut |value, _| {
            if let Ok(input) = value.dyn_into::<MidiInput>() {
                if input.state() != MidiPortDeviceState::Connected { return; }
                ports.push(MidiPortInfo {
                    id: input.id(),
                    name: input.name().unwrap_or_else(|| "Unnamed MIDI input".to_string()),
                });
            }
        });
        ports.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        ports
    }

    pub fn output_ports(&self) -> Vec<MidiPortInfo> {
        let mut ports = Vec::new();
        let outputs: js_sys::Map = self.access.outputs().unchecked_into();
        outputs.for_each(&mut |value, _| {
            if let Ok(output) = value.dyn_into::<MidiOutput>() {
                if output.state() != MidiPortDeviceState::Connected { return; }
                ports.push(MidiPortInfo {
                    id: output.id(),
                    name: output.name().unwrap_or_else(|| "Unnamed MIDI output".to_string()),
                });
            }
        });
        ports.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        ports
    }

    pub fn connect_input(
        &self,
        id: &str,
        events: Arc<Mutex<VecDeque<Vec<u8>>>>,
    ) -> Result<MidiInputConnection, String> {
        let input = self.access.inputs().get(id)
            .ok_or_else(|| "MIDI input is no longer available".to_string())?;
        let _ = input.open();
        let closure = Closure::wrap(Box::new(move |event: MidiMessageEvent| {
            if let Ok(data) = event.data() {
                if let Ok(mut queue) = events.lock() {
                    queue.push_back(data);
                }
            }
        }) as Box<dyn FnMut(MidiMessageEvent)>);
        input.set_onmidimessage(Some(closure.as_ref().unchecked_ref()));
        Ok(MidiInputConnection { input, _message: closure })
    }

    pub fn connect_output(&self, id: &str) -> Result<MidiOutputConnection, String> {
        let output = self.access.outputs().get(id)
            .ok_or_else(|| "MIDI output is no longer available".to_string())?;
        let _ = output.open();
        Ok(MidiOutputConnection { output })
    }
}

impl Drop for MidiAccessHandle {
    fn drop(&mut self) {
        self.access.set_onstatechange(None);
    }
}

pub struct MidiInputConnection {
    input: MidiInput,
    _message: Closure<dyn FnMut(MidiMessageEvent)>,
}

impl Drop for MidiInputConnection {
    fn drop(&mut self) {
        self.input.set_onmidimessage(None);
        let _ = self.input.close();
    }
}

#[derive(Debug, Clone)]
pub struct MidiOutputConnection {
    output: MidiOutput,
}

impl MidiOutputConnection {
    pub fn send(&self, message: &[u8]) -> Result<(), String> {
        let bytes = Uint8Array::from(message);
        self.output.send(bytes.as_ref()).map_err(js_error)
    }

    pub fn all_notes_off(&self) {
        for channel in 0..16u8 {
            let _ = self.send(&[0xB0 | channel, 0x7B, 0]);
        }
    }
}

/// Starts the browser's permission request synchronously from the button event so
/// it retains the browser's user-activation context. The returned promise is
/// awaited by an iced Task.
pub fn request_midi_access() -> Result<js_sys::Promise, String> {
    let window = web_sys::window().ok_or_else(|| "Browser window is unavailable".to_string())?;
    let options = MidiOptions::new();
    options.set_sysex(false);
    window.navigator()
        .request_midi_access_with_options(&options)
        .map_err(js_error)
}

pub async fn resolve_midi_access(promise: js_sys::Promise) -> Result<MidiAccess, String> {
    JsFuture::from(promise)
        .await
        .map_err(js_error)?
        .dyn_into::<MidiAccess>()
        .map_err(|_| "The browser returned an invalid Web MIDI access object".to_string())
}

/// Firefox desktop exposes the standard API but deliberately uses a stricter
/// site-permission add-on flow. Give users an actionable message instead of
/// surfacing the browser's opaque NotAllowedError. Firefox Android has no Web
/// MIDI implementation at all.
pub fn midi_access_error_status(error: &str) -> String {
    let user_agent = web_sys::window()
        .and_then(|window| window.navigator().user_agent().ok())
        .unwrap_or_default();
    if user_agent.contains("Firefox/") && user_agent.contains("Android") {
        "MIDI error: Firefox for Android does not support Web MIDI".to_string()
    } else if user_agent.contains("Firefox/") {
        "MIDI error: connect the device before starting Firefox, then approve its MIDI site-permission add-on"
            .to_string()
    } else {
        format!("MIDI error: {error}")
    }
}

fn js_error(value: wasm_bindgen::JsValue) -> String {
    if let Some(message) = value.as_string() {
        return message;
    }
    let message = js_sys::Reflect::get(&value, &"message".into())
        .ok()
        .and_then(|message| message.as_string());
    let name = js_sys::Reflect::get(&value, &"name".into())
        .ok()
        .and_then(|name| name.as_string());
    match (name, message) {
        (Some(name), Some(message)) if !message.is_empty() => format!("{name}: {message}"),
        (Some(name), _) => name,
        (_, Some(message)) => message,
        _ => "Web MIDI request failed".to_string(),
    }
}

pub fn list_output_ports() -> Vec<String> {
    Vec::new()
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    file: Arc<MidiFile>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    track_muted: Vec<bool>,
    track_channel: Vec<u8>,
    track_octave: Vec<i8>,
    midi_conn: Option<MidiOutputConnection>,
    keyboard_notes: Arc<HashSet<u8>>,
    octave_offset: i8,
    waveforms: Vec<crate::synth::Waveform>,
    shared_synth: Option<Arc<Mutex<SoftSynth>>>,
    speed: f32,
) -> PlaybackHandle {
    if let Some(ref synth) = shared_synth {
        if let Ok(mut synth) = synth.lock() {
            synth.set_active_waveforms(waveforms);
        }
    }

    let commands = Arc::new(Mutex::new(VecDeque::new()));
    let state = Arc::new(Mutex::new(WebPlayback {
        file,
        events_out,
        audio_enabled,
        track_muted,
        track_channel,
        track_octave,
        loop_range: None,
        keyboard_notes,
        octave_offset,
        synth: shared_synth,
        midi_conn,
        commands: Arc::clone(&commands),
        cursor: 0,
        position_tick: 0,
        playing: false,
        wall_start: Instant::now(),
        start_micros: 0,
        speed,
    }));

    PlaybackHandle {
        cmd_tx: CommandSender { commands },
        state,
    }
}

struct WebPlayback {
    file: Arc<MidiFile>,
    events_out: Arc<Mutex<VecDeque<PlayEvent>>>,
    audio_enabled: Arc<AtomicBool>,
    track_muted: Vec<bool>,
    track_channel: Vec<u8>,
    track_octave: Vec<i8>,
    /// (start_tick, end_tick) currently being repeated, or `None`.
    loop_range: Option<(u64, u64)>,
    keyboard_notes: Arc<HashSet<u8>>,
    octave_offset: i8,
    synth: Option<Arc<Mutex<SoftSynth>>>,
    midi_conn: Option<MidiOutputConnection>,
    commands: Arc<Mutex<VecDeque<PlayCmd>>>,
    cursor: usize,
    position_tick: u64,
    playing: bool,
    wall_start: Instant,
    start_micros: u64,
    speed: f32,
}

impl WebPlayback {
    fn poll(&mut self) {
        self.apply_commands();
        if !self.playing {
            return;
        }

        let elapsed = Instant::now().duration_since(self.wall_start);
        let raw_target_micros = self
            .start_micros
            .saturating_add(scaled_duration_micros(elapsed, self.speed));
        let total_micros = tick_to_micros_abs(
            self.file.total_ticks,
            &self.file.tempo_map,
            self.file.ticks_per_beat,
        );
        // A loop range wraps the transport back to its start the instant
        // playback reaches its end—so both section loops (a staff selection)
        // and whole-song loops stay sample-accurate and gapless, without
        // waiting for a Done round-trip through the UI thread.
        let loop_end_micros = self.loop_range.map(|(_, end)| {
            tick_to_micros_abs(end, &self.file.tempo_map, self.file.ticks_per_beat)
        });
        let target_micros = loop_end_micros.map_or(raw_target_micros, |end| raw_target_micros.min(end));

        while let Some(event) = self.file.events.get(self.cursor) {
            let event_micros =
                tick_to_micros_abs(event.tick, &self.file.tempo_map, self.file.ticks_per_beat);
            if event_micros > target_micros {
                break;
            }

            let event = event.clone();
            self.cursor += 1;
            self.dispatch_event(event);
        }

        self.position_tick = tick_at_micros(&self.file, target_micros.min(total_micros));
        self.publish(PlayEvent::Position(self.position_tick));

        if let (Some((loop_start, _)), Some(loop_end_micros)) = (self.loop_range, loop_end_micros) {
            if raw_target_micros >= loop_end_micros {
                self.seek_to(loop_start);
                self.clear_and_publish(PlayEvent::Position(self.position_tick));
                return;
            }
        }

        if target_micros >= total_micros {
            self.all_notes_off();
            self.playing = false;
            self.position_tick = 0;
            self.cursor = 0;
            self.publish(PlayEvent::Done);
        }
    }

    fn apply_commands(&mut self) {
        let commands: Vec<PlayCmd> = self
            .commands
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default();

        for command in commands {
            match command {
                PlayCmd::Play => {
                    if self.position_tick >= self.file.total_ticks {
                        self.position_tick = 0;
                        self.cursor = 0;
                    }
                    self.start_micros = tick_to_micros_abs(
                        self.position_tick,
                        &self.file.tempo_map,
                        self.file.ticks_per_beat,
                    );
                    self.wall_start = Instant::now();
                    self.playing = true;
                    self.restore_held_notes();
                }
                PlayCmd::Pause => {
                    self.capture_position();
                    self.playing = false;
                    self.all_notes_off();
                }
                PlayCmd::Stop => {
                    self.playing = false;
                    self.position_tick = 0;
                    self.cursor = 0;
                    self.all_notes_off();
                    self.clear_and_publish(PlayEvent::Position(0));
                }
                PlayCmd::SeekTo(tick) => {
                    self.seek_to(tick);
                    self.clear_and_publish(PlayEvent::Position(self.position_tick));
                }
                PlayCmd::SetAudio(enabled) => {
                    self.audio_enabled.store(enabled, Ordering::Relaxed);
                    if !enabled {
                        self.all_notes_off();
                    }
                }
                PlayCmd::SetTrackMuted(index, muted) => {
                    if let Some(value) = self.track_muted.get_mut(index) {
                        *value = muted;
                    }
                    if muted {
                        self.all_notes_off();
                    }
                }
                PlayCmd::SetTrackChannel(index, channel) => {
                    if let Some(value) = self.track_channel.get_mut(index) {
                        *value = channel;
                    }
                }
                PlayCmd::SetTrackOctave(index, octave) => {
                    if let Some(value) = self.track_octave.get_mut(index) {
                        *value = octave;
                    }
                }
                PlayCmd::SetLoopRange(range) => self.loop_range = range,
                PlayCmd::SetOctaveOffset(offset) => self.octave_offset = offset,
                PlayCmd::SetWaveforms(waveforms) => {
                    if let Some(ref synth) = self.synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.set_active_waveforms(waveforms);
                        }
                    }
                }
                PlayCmd::SetKnob(index, value) => {
                    if let Some(ref synth) = self.synth {
                        if let Ok(mut synth) = synth.lock() {
                            synth.set_knob(index, value);
                        }
                    }
                }
                PlayCmd::LiveNoteOn(note, velocity, channel) => {
                    if self.audio_enabled.load(Ordering::Relaxed) {
                        self.note_on(note, velocity, channel);
                    }
                }
                PlayCmd::LiveNoteOff(note, channel) => self.note_off(note, channel),
                PlayCmd::SetMidiOutput(output) => {
                    self.all_notes_off();
                    self.midi_conn = output;
                }
                PlayCmd::SetSpeed(speed) => {
                    // Re-anchor the clock at the current musical position
                    // instead of touching the cursor/notes, so held notes
                    // keep sounding through a speed change instead of
                    // glitching off and back on.
                    if self.playing {
                        let elapsed = scaled_duration_micros(
                            Instant::now().duration_since(self.wall_start),
                            self.speed,
                        );
                        self.start_micros = self.start_micros.saturating_add(elapsed);
                        self.wall_start = Instant::now();
                    }
                    self.speed = speed.max(0.05);
                }
            }
        }
    }

    /// Re-anchors the playback clock at `tick`, restoring any notes that
    /// should already be sounding there. Shared by explicit seeks and by
    /// loop wraps, which are just a seek back to the loop start.
    fn seek_to(&mut self, tick: u64) {
        self.all_notes_off();
        self.position_tick = tick.min(self.file.total_ticks);
        self.cursor = self
            .file
            .events
            .partition_point(|event| event.tick < self.position_tick);
        self.start_micros = tick_to_micros_abs(
            self.position_tick,
            &self.file.tempo_map,
            self.file.ticks_per_beat,
        );
        self.wall_start = Instant::now();
        if self.playing {
            self.restore_held_notes();
        }
    }

    fn capture_position(&mut self) {
        if !self.playing {
            return;
        }
        let elapsed = scaled_duration_micros(
            Instant::now().duration_since(self.wall_start),
            self.speed,
        );
        self.position_tick = tick_at_micros(&self.file, self.start_micros.saturating_add(elapsed));
        self.publish(PlayEvent::Position(self.position_tick));
    }

    fn dispatch_event(&mut self, event: crate::midi::TimedEvent) {
        if self.track_muted.get(event.track).copied().unwrap_or(false) {
            return;
        }

        match event.kind {
            EventKind::NoteOn { note, velocity } => {
                self.publish(PlayEvent::NoteOn(note, event.track, event.channel));
                if self.audio_enabled.load(Ordering::Relaxed)
                    && self.fits_keyboard(note, event.channel, event.track)
                {
                    self.note_on(note, velocity, self.output_channel(event.track, event.channel));
                }
            }
            EventKind::NoteOff { note } => {
                self.publish(PlayEvent::NoteOff(note, event.channel));
                self.note_off(note, self.output_channel(event.track, event.channel));
            }
        }
    }

    /// The channel actually written to the wire/synth for a track's event —
    /// see the native implementation in `playback.rs` for the rationale.
    fn output_channel(&self, track: usize, original_channel: u8) -> u8 {
        self.track_channel.get(track).copied().unwrap_or(original_channel)
    }

    fn restore_held_notes(&mut self) {
        if self.position_tick == 0 {
            return;
        }

        let held: Vec<_> = self
            .file
            .notes
            .iter()
            .filter(|note| {
                note.start_tick < self.position_tick && note.end_tick > self.position_tick
            })
            .filter(|note| !self.track_muted.get(note.track).copied().unwrap_or(false))
            .cloned()
            .collect();
        for note in held {
            self.publish(PlayEvent::NoteOn(note.midi_note, note.track, note.channel));
            if self.audio_enabled.load(Ordering::Relaxed)
                && self.fits_keyboard(note.midi_note, note.channel, note.track)
            {
                let out_ch = self.output_channel(note.track, note.channel);
                self.note_on(note.midi_note, note.velocity, out_ch);
            }
        }
    }

    fn fits_keyboard(&self, note: u8, channel: u8, track: usize) -> bool {
        if channel == crate::synth::DRUM_CHANNEL {
            return true;
        }
        let shift = crate::midi::combined_octave_shift(self.octave_offset, &self.track_octave, track);
        let shifted = (note as i16 + shift).clamp(0, 127) as u8;
        self.keyboard_notes.contains(&shifted)
    }

    fn note_on(&mut self, note: u8, velocity: u8, channel: u8) {
        if let Some(ref output) = self.midi_conn {
            let _ = output.send(&[0x90 | (channel & 0x0F), note, velocity]);
        } else if let Some(ref synth) = self.synth {
            if let Ok(mut synth) = synth.lock() {
                synth.note_on(note, velocity, channel);
            }
        }
    }

    fn note_off(&mut self, note: u8, channel: u8) {
        if let Some(ref output) = self.midi_conn {
            let _ = output.send(&[0x80 | (channel & 0x0F), note, 0]);
        } else if let Some(ref synth) = self.synth {
            if let Ok(mut synth) = synth.lock() {
                synth.note_off(note, channel);
            }
        }
    }

    fn all_notes_off(&mut self) {
        if let Some(ref output) = self.midi_conn {
            output.all_notes_off();
        }
        if let Some(ref synth) = self.synth {
            if let Ok(mut synth) = synth.lock() {
                synth.all_notes_off();
            }
        }
    }

    fn publish(&self, event: PlayEvent) {
        if let Ok(mut events) = self.events_out.lock() {
            events.push_back(event);
        }
    }

    fn clear_and_publish(&self, event: PlayEvent) {
        if let Ok(mut events) = self.events_out.lock() {
            events.clear();
            events.push_back(event);
        }
    }
}

/// Wall-clock duration scaled by the playback speed multiplier, to get the
/// corresponding amount of musical time elapsed.
fn scaled_duration_micros(duration: Duration, speed: f32) -> u64 {
    (duration.as_secs_f64() * speed as f64 * 1_000_000.0).max(0.0) as u64
}

fn tick_at_micros(file: &MidiFile, micros: u64) -> u64 {
    let mut low = 0u64;
    let mut high = file.total_ticks;

    while low < high {
        let mid = low + (high - low + 1) / 2;
        if tick_to_micros_abs(mid, &file.tempo_map, file.ticks_per_beat) <= micros {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}
