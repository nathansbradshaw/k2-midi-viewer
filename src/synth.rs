use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::drums::{midi_note_to_drum_type, DrumSampler};

/// GM convention: MIDI channel 10 (0-indexed 9) is the percussion channel.
pub const DRUM_CHANNEL: u8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    #[default]
    Organ,
    Triangle,
    Square,
    Saw,
    Sine,
    Pulse,
    Noise,
}

impl Waveform {
    fn sample(self, phase: f32) -> f32 {
        match self {
            Waveform::Organ => {
                let a = phase * TAU;
                a.sin() + 0.35 * (2.0 * a).sin() + 0.12 * (3.0 * a).sin()
            }
            Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            Waveform::Square => if phase < 0.5 { 1.0 } else { -1.0 },
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Sine => (phase * TAU).sin(),
            // 25% duty cycle, brighter/thinner than the 50% Square above.
            Waveform::Pulse => if phase < 0.25 { 1.0 } else { -1.0 },
            // Deterministic pseudo-noise hashed from the phase, so it needs
            // no extra per-voice RNG state to stay in step with note pitch.
            Waveform::Noise => {
                let x = (phase * 12_989.8).sin() * 43_758.5453;
                2.0 * (x - x.floor()) - 1.0
            }
        }
    }
}

const MAX_VOICES:      usize = 16;
const MAX_DRUM_VOICES: usize = 12;
const ATTACK_S:   f32 = 0.005; // 5 ms
const DECAY_S:    f32 = 0.30;
const SUSTAIN:    f32 = 0.55;
const RELEASE_S:  f32 = 0.25;
const VOICE_GAIN: f32 = 0.22; // per-voice scale; keeps mix below 0 dBFS
const DRUM_GAIN:  f32 = 0.6;  // drum engine output is already near full-scale

/// One of the 12 front-panel encoder knobs. `min`/`max` describe the real
/// engine unit each knob controls (seconds, Hz, cents, ...); the knob itself
/// only ever reports a normalized 0.0..=1.0 position.
#[derive(Debug, Clone, Copy)]
pub struct KnobParam {
    pub label:   &'static str,
    pub min:     f32,
    pub max:     f32,
    pub default: f32,
}

pub const KNOB_COUNT: usize = 13;

/// Ordered to match the three 4-knob trays on the board (tone shaping,
/// envelope + mix, then modulation), plus the standalone encoder to their
/// left (`Bitcrush`).
pub const KNOB_PARAMS: [KnobParam; KNOB_COUNT] = [
    KnobParam { label: "Volume",   min: 0.0,   max: 1.5,   default: 1.0 },
    KnobParam { label: "Cutoff",   min: 0.05,  max: 1.0,   default: 0.30 },
    KnobParam { label: "Attack",   min: 0.001, max: 1.0,   default: ATTACK_S },
    KnobParam { label: "Release",  min: 0.01,  max: 3.0,   default: RELEASE_S },
    KnobParam { label: "Decay",    min: 0.01,  max: 2.0,   default: DECAY_S },
    KnobParam { label: "Sustain",  min: 0.0,   max: 1.0,   default: SUSTAIN },
    KnobParam { label: "Drum Vol", min: 0.0,   max: 1.5,   default: DRUM_GAIN },
    KnobParam { label: "Pan",      min: -1.0,  max: 1.0,   default: 0.0 },
    KnobParam { label: "Vib Rate", min: 0.5,   max: 10.0,  default: 5.0 },
    KnobParam { label: "Vib Depth",min: 0.0,   max: 50.0,  default: 0.0 },
    KnobParam { label: "Tremolo",  min: 0.0,   max: 1.0,   default: 0.0 },
    KnobParam { label: "Glide",    min: 0.0,   max: 0.5,   default: 0.0 },
    KnobParam { label: "Bitcrush", min: 0.0,   max: 1.0,   default: 0.0 },
];

fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage { Attack, Decay, Sustain, Release, Done }

struct Voice {
    note:      u8,
    vel:       f32, // 0–1
    phase:     f32, // 0–1
    inc:       f32, // phase increment per sample
    hz:        f32, // current (glide-smoothed) frequency
    target_hz: f32, // frequency the note actually asked for
    env:       f32, // current envelope level
    stage:     Stage,
    lp:        f32, // one-pole low-pass state
}

struct DrumVoice {
    sampler: DrumSampler,
    vel:     f32, // 0–1
}

pub struct SoftSynth {
    voices:      Vec<Voice>,
    drum_voices: Vec<DrumVoice>,
    sr:          f32,
    channels:    usize,
    /// Waveforms currently layered together (summed and averaged each
    /// sample). Empty means "nothing selected", which falls back to Organ —
    /// matching the original single-waveform default.
    active_waveforms: Vec<Waveform>,
    master_volume: f32,
    cutoff:        f32,
    attack_s:      f32,
    decay_s:       f32,
    sustain:       f32,
    release_s:     f32,
    drum_gain:     f32,
    pan:           f32,
    vibrato_rate:  f32,
    vibrato_depth: f32,
    tremolo_depth: f32,
    glide_s:       f32,
    lfo_phase:     f32,
    /// 0.0 (off) .. 1.0 (max) — drives both quantization depth and the
    /// sample-and-hold decimation rate in `render`'s final bitcrush stage.
    bitcrush:      f32,
    crush_phase:   u32,
    /// Last held output frame for the sample-and-hold decimator, one slot
    /// per channel.
    crush_hold:    Vec<f32>,
}

impl SoftSynth {
    pub fn new(sr: f32, channels: usize) -> Self {
        Self {
            voices: Vec::new(),
            drum_voices: Vec::new(),
            sr,
            channels,
            active_waveforms: Vec::new(),
            master_volume: KNOB_PARAMS[0].default,
            cutoff:        KNOB_PARAMS[1].default,
            attack_s:      KNOB_PARAMS[2].default,
            release_s:     KNOB_PARAMS[3].default,
            decay_s:       KNOB_PARAMS[4].default,
            sustain:       KNOB_PARAMS[5].default,
            drum_gain:     KNOB_PARAMS[6].default,
            pan:           KNOB_PARAMS[7].default,
            vibrato_rate:  KNOB_PARAMS[8].default,
            vibrato_depth: KNOB_PARAMS[9].default,
            tremolo_depth: KNOB_PARAMS[10].default,
            glide_s:       KNOB_PARAMS[11].default,
            lfo_phase: 0.0,
            bitcrush:    KNOB_PARAMS[12].default,
            crush_phase: 0,
            crush_hold:  vec![0.0; channels.max(1)],
        }
    }

    /// Replaces the full set of active (layered) waveforms. An empty Vec
    /// means "nothing selected" and falls back to Organ.
    pub fn set_active_waveforms(&mut self, waveforms: Vec<Waveform>) {
        self.active_waveforms = waveforms;
    }

    /// Applies a front-panel knob move. `value` is already scaled into the
    /// knob's real engine unit (see `KNOB_PARAMS`), not the raw 0.0..=1.0
    /// dial position.
    pub fn set_knob(&mut self, index: u8, value: f32) {
        let Some(param) = KNOB_PARAMS.get(index as usize) else { return };
        let value = value.clamp(param.min, param.max);
        match index {
            0 => self.master_volume = value,
            1 => self.cutoff = value,
            2 => self.attack_s = value,
            3 => self.release_s = value,
            4 => self.decay_s = value,
            5 => self.sustain = value,
            6 => self.drum_gain = value,
            7 => self.pan = value,
            8 => self.vibrato_rate = value,
            9 => self.vibrato_depth = value,
            10 => self.tremolo_depth = value,
            11 => self.glide_s = value,
            12 => self.bitcrush = value,
            _ => {}
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8, channel: u8) {
        if channel == DRUM_CHANNEL {
            let Some(drum_type) = midi_note_to_drum_type(note) else { return };

            if self.drum_voices.len() >= MAX_DRUM_VOICES {
                let pos = self.drum_voices.iter().position(|d| d.sampler.is_finished())
                    .unwrap_or(0);
                self.drum_voices.remove(pos);
            }
            let mut sampler = DrumSampler::new();
            sampler.trigger_at(drum_type, self.sr);
            self.drum_voices.push(DrumVoice { sampler, vel: velocity as f32 / 127.0 });
            return;
        }

        let target_hz = midi_to_hz(note);

        // Retrigger existing voice so the same key doesn't build up
        if let Some(v) = self.voices.iter_mut()
            .find(|v| v.note == note && v.stage != Stage::Done)
        {
            v.vel       = velocity as f32 / 127.0;
            v.stage     = Stage::Attack;
            v.target_hz = target_hz;
            if self.glide_s <= 0.0 { v.hz = target_hz; }
            return;
        }
        // Steal the quietest-or-oldest voice if polyphony is full
        if self.voices.len() >= MAX_VOICES {
            let pos = self.voices.iter().position(|v| v.stage == Stage::Done)
                .unwrap_or(0);
            self.voices.remove(pos);
        }
        // Portamento: start from whatever the most recently active voice was
        // playing instead of jumping straight to the new pitch.
        let start_hz = if self.glide_s > 0.0 {
            self.voices.iter().rev()
                .find(|v| v.stage != Stage::Done)
                .map(|v| v.hz)
                .unwrap_or(target_hz)
        } else {
            target_hz
        };
        self.voices.push(Voice {
            note,
            vel:       velocity as f32 / 127.0,
            phase:     0.0,
            inc:       start_hz / self.sr,
            hz:        start_hz,
            target_hz,
            env:       0.0,
            stage:     Stage::Attack,
            lp:        0.0,
        });
    }

    pub fn note_off(&mut self, note: u8, channel: u8) {
        // Drum hits are one-shots — GM percussion ignores note-off.
        if channel == DRUM_CHANNEL { return; }

        for v in &mut self.voices {
            if v.note == note && matches!(v.stage, Stage::Attack | Stage::Decay | Stage::Sustain) {
                v.stage = Stage::Release;
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.stage != Stage::Done { v.stage = Stage::Release; }
        }
        self.drum_voices.clear();
    }

    pub fn render(&mut self, data: &mut [f32]) {
        data.fill(0.0);

        let sr = self.sr;
        let ch = self.channels.max(1);
        let sustain      = self.sustain;
        let attack_rate  = 1.0 / (self.attack_s * sr);
        let decay_rate   = (1.0 - sustain) / (self.decay_s * sr);
        let release_rate = sustain / (self.release_s * sr);
        let lp_coeff     = self.cutoff;
        let active_waveforms = &self.active_waveforms;

        // Glide eases `hz` toward `target_hz` on each sample; larger glide_s
        // means a slower approach. Vibrato/tremolo share one LFO phase,
        // recomputed per-sample from a fixed base so voices agree on it
        // without needing a mutable running phase inside this loop.
        let glide_coeff    = if self.glide_s <= 0.0001 { 1.0 } else { (1.0 / (self.glide_s * sr)).min(1.0) };
        let vib_depth_ratio = 2f32.powf(self.vibrato_depth / 1200.0) - 1.0;
        let lfo_inc         = self.vibrato_rate / sr;
        let tremolo_depth   = self.tremolo_depth;
        let (pan_l, pan_r) = if self.pan <= 0.0 {
            (1.0, 1.0 + self.pan)
        } else {
            (1.0 - self.pan, 1.0)
        };

        for v in &mut self.voices {
            if v.stage == Stage::Done { continue; }
            let gain = v.vel * VOICE_GAIN;

            for (i, frame) in data.chunks_exact_mut(ch).enumerate() {
                let lfo_phase = (self.lfo_phase + i as f32 * lfo_inc).fract();

                // Waveform selection only applies to melodic voices. Drum
                // voices are rendered by their dedicated sampler below.
                v.hz = v.hz + (v.target_hz - v.hz) * glide_coeff;
                let vibrato = 1.0 + vib_depth_ratio * (lfo_phase * TAU).sin();
                v.inc = (v.hz * vibrato).max(0.0) / sr;
                v.phase = (v.phase + v.inc).fract();
                let wave = if active_waveforms.is_empty() {
                    Waveform::Organ.sample(v.phase)
                } else {
                    active_waveforms.iter().map(|w| w.sample(v.phase)).sum::<f32>()
                        / active_waveforms.len() as f32
                };

                // One-pole low-pass filter (soften high harmonics)
                v.lp += lp_coeff * (wave - v.lp);

                // ADSR envelope
                v.env = match v.stage {
                    Stage::Attack  => {
                        let e = v.env + attack_rate;
                        if e >= 1.0 { v.stage = Stage::Decay; 1.0 } else { e }
                    }
                    Stage::Decay   => {
                        let e = v.env - decay_rate;
                        if e <= sustain { v.stage = Stage::Sustain; sustain } else { e }
                    }
                    Stage::Sustain => sustain,
                    Stage::Release => {
                        let e = v.env - release_rate;
                        if e <= 0.0 { v.stage = Stage::Done; 0.0 } else { e }
                    }
                    Stage::Done    => 0.0,
                };

                let tremolo = 1.0 - tremolo_depth * 0.5 * (1.0 - (lfo_phase * TAU).cos());
                let s = v.lp * v.env * gain * tremolo;
                for (ci, samp) in frame.iter_mut().enumerate() {
                    let g = match ci { 0 => pan_l, 1 => pan_r, _ => 1.0 };
                    *samp += s * g;
                }
            }
        }
        self.lfo_phase = (self.lfo_phase + (data.len() / ch) as f32 * lfo_inc).fract();

        for d in &mut self.drum_voices {
            let gain = d.vel * self.drum_gain;
            for frame in data.chunks_exact_mut(ch) {
                let s = d.sampler.next_value() * gain;
                for samp in frame.iter_mut() { *samp += s; }
            }
        }
        self.drum_voices.retain(|d| !d.sampler.is_finished());

        // Master volume, then soft clip to prevent digital distortion
        for s in data.iter_mut() { *s = (*s * self.master_volume).clamp(-1.0, 1.0); }

        // Bitcrush: lo-fi degradation on the finished mix — quantizes to a
        // reduced bit depth and decimates via sample-and-hold, both scaled
        // by the one knob so it goes from transparent to fully crushed.
        if self.bitcrush > 0.001 {
            let levels = 2f32.powf(16.0 - self.bitcrush * 13.0);
            let hold_len = 1 + (self.bitcrush * 24.0) as u32;
            for frame in data.chunks_exact_mut(ch) {
                if self.crush_phase == 0 {
                    for (hold, samp) in self.crush_hold.iter_mut().zip(frame.iter()) {
                        *hold = (samp * levels).round() / levels;
                    }
                }
                for (samp, &hold) in frame.iter_mut().zip(self.crush_hold.iter()) {
                    *samp = hold;
                }
                self.crush_phase = (self.crush_phase + 1) % hold_len;
            }
        }

        self.voices.retain(|v| v.stage != Stage::Done);
    }
}

// ---------------------------------------------------------------------------
// Audio stream startup
// ---------------------------------------------------------------------------

/// Starts a cpal output stream backed by the soft synth.
/// Returns a descriptive error when no output device or supported stream is available.
pub fn start_soft_synth() -> Result<(Arc<Mutex<SoftSynth>>, cpal::Stream), String> {
    let host   = cpal::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| "Audio output is unavailable".to_string())?;

    // CPAL's Web Audio backend reports a synthetic "best" default with 32
    // channels. Browsers commonly reject or fail to route that layout to a
    // stereo destination, leaving the app with no synth at all. Request the
    // actual layout K2 renders instead.
    #[cfg(target_arch = "wasm32")]
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(44_100),
        buffer_size: cpal::BufferSize::Default,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let config: cpal::StreamConfig = device.default_output_config()
        .map_err(|error| format!("audio configuration failed: {error}"))?
        .into();

    let sr  = config.sample_rate.0 as f32;
    let ch  = config.channels as usize;

    let synth    = Arc::new(Mutex::new(SoftSynth::new(sr, ch)));
    let synth_cb = Arc::clone(&synth);

    // Only f32 streams are needed; CoreAudio (macOS) always supports f32.
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            match synth_cb.try_lock() {
                Ok(mut s) => s.render(data),
                Err(_)    => data.fill(0.0), // don't block audio thread
            }
        },
        |err| eprintln!("soft synth audio error: {err}"),
        None,
    ).map_err(|error| format!("audio stream creation failed: {error}"))?;

    stream.play()
        .map_err(|error| format!("audio start failed: {error}"))?;
    Ok((synth, stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }

    #[test]
    fn selectable_waveforms_have_the_expected_shape() {
        close(Waveform::Triangle.sample(0.0), -1.0);
        close(Waveform::Triangle.sample(0.5), 1.0);
        close(Waveform::Square.sample(0.25), 1.0);
        close(Waveform::Square.sample(0.75), -1.0);
        close(Waveform::Saw.sample(0.0), -1.0);
        close(Waveform::Saw.sample(0.5), 0.0);
        close(Waveform::Sine.sample(0.25), 1.0);
        close(Waveform::Sine.sample(0.75), -1.0);
        close(Waveform::Pulse.sample(0.1), 1.0);
        close(Waveform::Pulse.sample(0.5), -1.0);
        assert!(Waveform::Noise.sample(0.37).abs() <= 1.0);
    }

    #[test]
    fn waveform_selection_does_not_replace_the_drum_engine() {
        let mut synth = SoftSynth::new(48_000.0, 2);
        synth.note_on(36, 100, DRUM_CHANNEL);
        synth.set_active_waveforms(vec![Waveform::Saw]);

        assert!(synth.voices.is_empty());
        assert_eq!(synth.drum_voices.len(), 1);
    }

    #[test]
    fn live_melodic_notes_follow_press_and_release() {
        let mut synth = SoftSynth::new(48_000.0, 2);
        synth.note_on(60, 108, 0);

        assert_eq!(synth.voices.len(), 1);
        assert!(matches!(synth.voices[0].stage, Stage::Attack));

        synth.note_off(60, 0);
        assert!(matches!(synth.voices[0].stage, Stage::Release));
    }
}
