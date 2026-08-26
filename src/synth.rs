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

fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage { Attack, Decay, Sustain, Release, Done }

struct Voice {
    note:  u8,
    vel:   f32, // 0–1
    phase: f32, // 0–1
    inc:   f32, // phase increment per sample
    env:   f32, // current envelope level
    stage: Stage,
    lp:    f32, // one-pole low-pass state
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
    waveform:    Waveform,
}

impl SoftSynth {
    pub fn new(sr: f32, channels: usize) -> Self {
        Self {
            voices: Vec::new(),
            drum_voices: Vec::new(),
            sr,
            channels,
            waveform: Waveform::default(),
        }
    }

    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
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

        // Retrigger existing voice so the same key doesn't build up
        if let Some(v) = self.voices.iter_mut()
            .find(|v| v.note == note && v.stage != Stage::Done)
        {
            v.vel   = velocity as f32 / 127.0;
            v.stage = Stage::Attack;
            return;
        }
        // Steal the quietest-or-oldest voice if polyphony is full
        if self.voices.len() >= MAX_VOICES {
            let pos = self.voices.iter().position(|v| v.stage == Stage::Done)
                .unwrap_or(0);
            self.voices.remove(pos);
        }
        self.voices.push(Voice {
            note,
            vel:   velocity as f32 / 127.0,
            phase: 0.0,
            inc:   midi_to_hz(note) / self.sr,
            env:   0.0,
            stage: Stage::Attack,
            lp:    0.0,
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
        let ch = self.channels;
        let attack_rate  = 1.0 / (ATTACK_S  * sr);
        let decay_rate   = (1.0 - SUSTAIN) / (DECAY_S  * sr);
        let release_rate = SUSTAIN / (RELEASE_S * sr);
        let lp_coeff     = 0.30f32; // ≈ 6 kHz cutoff at 44.1 kHz
        let waveform     = self.waveform;

        for v in &mut self.voices {
            if v.stage == Stage::Done { continue; }
            let gain = v.vel * VOICE_GAIN;

            for frame in data.chunks_exact_mut(ch) {
                // Waveform selection only applies to melodic voices. Drum
                // voices are rendered by their dedicated sampler below.
                v.phase = (v.phase + v.inc).fract();
                let wave = waveform.sample(v.phase);

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
                        if e <= SUSTAIN { v.stage = Stage::Sustain; SUSTAIN } else { e }
                    }
                    Stage::Sustain => SUSTAIN,
                    Stage::Release => {
                        let e = v.env - release_rate;
                        if e <= 0.0 { v.stage = Stage::Done; 0.0 } else { e }
                    }
                    Stage::Done    => 0.0,
                };

                let s = v.lp * v.env * gain;
                for samp in frame.iter_mut() { *samp += s; }
            }
        }

        for d in &mut self.drum_voices {
            let gain = d.vel * DRUM_GAIN;
            for frame in data.chunks_exact_mut(ch) {
                let s = d.sampler.next_value() * gain;
                for samp in frame.iter_mut() { *samp += s; }
            }
        }
        self.drum_voices.retain(|d| !d.sampler.is_finished());

        // Soft clip to prevent digital distortion
        for s in data.iter_mut() { *s = s.clamp(-1.0, 1.0); }

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
    }

    #[test]
    fn waveform_selection_does_not_replace_the_drum_engine() {
        let mut synth = SoftSynth::new(48_000.0, 2);
        synth.note_on(36, 100, DRUM_CHANNEL);
        synth.set_waveform(Waveform::Saw);

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
