use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const MAX_VOICES: usize = 16;
const ATTACK_S:   f32 = 0.005; // 5 ms
const DECAY_S:    f32 = 0.30;
const SUSTAIN:    f32 = 0.55;
const RELEASE_S:  f32 = 0.25;
const VOICE_GAIN: f32 = 0.22; // per-voice scale; keeps mix below 0 dBFS

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

pub struct SoftSynth {
    voices:   Vec<Voice>,
    sr:       f32,
    channels: usize,
}

impl SoftSynth {
    pub fn new(sr: f32, channels: usize) -> Self {
        Self { voices: Vec::new(), sr, channels }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
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

    pub fn note_off(&mut self, note: u8) {
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
    }

    pub fn render(&mut self, data: &mut [f32]) {
        data.fill(0.0);

        let sr = self.sr;
        let ch = self.channels;
        let attack_rate  = 1.0 / (ATTACK_S  * sr);
        let decay_rate   = (1.0 - SUSTAIN) / (DECAY_S  * sr);
        let release_rate = SUSTAIN / (RELEASE_S * sr);
        let lp_coeff     = 0.30f32; // ≈ 6 kHz cutoff at 44.1 kHz

        for v in &mut self.voices {
            if v.stage == Stage::Done { continue; }
            let gain = v.vel * VOICE_GAIN;

            for frame in data.chunks_exact_mut(ch) {
                // Oscillator: fundamental + 2nd + 3rd harmonic (organ-like timbre)
                v.phase = (v.phase + v.inc).fract();
                let a = v.phase * TAU;
                let wave = a.sin()
                    + 0.35 * (2.0 * a).sin()
                    + 0.12 * (3.0 * a).sin();

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

        // Soft clip to prevent digital distortion
        for s in data.iter_mut() { *s = s.clamp(-1.0, 1.0); }

        self.voices.retain(|v| v.stage != Stage::Done);
    }
}

// ---------------------------------------------------------------------------
// Audio stream startup
// ---------------------------------------------------------------------------

/// Starts a cpal output stream backed by the soft synth.
/// Returns None if no audio device is available (rare) or format is unsupported.
pub fn start_soft_synth() -> Option<(Arc<Mutex<SoftSynth>>, cpal::Stream)> {
    let host   = cpal::default_host();
    let device = host.default_output_device()?;
    let config = device.default_output_config().ok()?;

    let sr  = config.sample_rate().0 as f32;
    let ch  = config.channels() as usize;

    let synth    = Arc::new(Mutex::new(SoftSynth::new(sr, ch)));
    let synth_cb = Arc::clone(&synth);

    // Only f32 streams are needed; CoreAudio (macOS) always supports f32.
    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            match synth_cb.try_lock() {
                Ok(mut s) => s.render(data),
                Err(_)    => data.fill(0.0), // don't block audio thread
            }
        },
        |err| eprintln!("soft synth audio error: {err}"),
        None,
    ).ok()?;

    stream.play().ok()?;
    Some((synth, stream))
}
