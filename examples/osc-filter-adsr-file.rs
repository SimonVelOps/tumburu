// Tumburu Digital Audio Workstation
// Copyright (C) 2026  Simon ANDRE <simon.andre+velops.eu>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
use std::fs::File;
use std::io::{BufWriter, Write};
use tumburu::sequencer::{Sequencer, SequencerEvent};
use tumburu::synth::{SynthParameter, SynthVoice};

use tumburu::{Waveform, SAMPLE_RATE};

/// # Rust behaviors
/// Executable entry point. Instantiates modules, executes off-line audio rendering loop, and writes raw PCM float bytes to disk via `BufWriter`.
///
/// # Arguments/return describe
/// * `Return: ()` - Executable entry point.
///
/// # Mathematical explanations
/// Renders `N = 4 * SAMPLE_RATE` total samples across 4 timeline seconds.
///
/// # Describe sound processing
/// Offline rendering loop generating raw 32-bit floating-point PCM audio data (`FLOAT_LE`).
///
/// # Sound processing use cases
/// DAW local export feature.

fn main() {
    println!("VelOps POC substractive synthetiser voice...");

    let mut synth = SynthVoice::new();
    let mut seq = Sequencer::new();
    let sec = SAMPLE_RATE as usize;

    // Setup harsh analog tone
    seq.add_event(
        0,
        SequencerEvent::ParamChange(SynthParameter::Waveform(Waveform::Sine)),
    );
    seq.add_event(
        0,
        SequencerEvent::ParamChange(SynthParameter::Cutoff(1800.0)),
    );
    seq.add_event(
        0,
        SequencerEvent::ParamChange(SynthParameter::Resonance(0.85)),
    );
    seq.add_event(
        0,
        SequencerEvent::ParamChange(SynthParameter::Overdrive(5.0)),
    );

    // Musical phrase
    seq.add_event(
        sec / 10,
        SequencerEvent::NoteOn {
            note_freq: 110.0,
            velocity: 1.0,
        },
    ); // A2
    seq.add_event(sec, SequencerEvent::NoteOff);

    seq.add_event(
        sec + (sec / 10),
        SequencerEvent::NoteOn {
            note_freq: 180.0,
            velocity: 1.0,
        },
    ); // A3
    seq.add_event(sec * 2, SequencerEvent::NoteOff);

    // Filter sweep modification during phrase
    seq.add_event(
        sec * 2 + (sec / 10),
        SequencerEvent::ParamChange(SynthParameter::Cutoff(400.0)),
    );
    seq.add_event(
        sec * 2 + (sec / 10),
        SequencerEvent::NoteOn {
            note_freq: 150.0,
            velocity: 1.0,
        },
    ); // A1
    seq.add_event(sec * 3, SequencerEvent::NoteOff);

    seq.add_event(sec * 4, SequencerEvent::EndOfPattern);

    // Rendering Loop
    let mut file = BufWriter::new(File::create("/tmp/pattern.raw").expect("Failed to create file"));
    let mut running = true;

    while running {
        // The sequencer tells us if we should keep running
        running = seq.process_tick(&mut synth);
        let sample = synth.process_sample();

        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    println!("Audio successfully written to 'pattern.raw'.");
    println!("Playback via bash: aplay -f FLOAT_LE -r 48000 -c 1 pattern.raw");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden master audio fixture generated for `4 * SAMPLE_RATE` timeline seconds[cite: 1].
    const GOLDEN_PCM_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/pattern.raw");
    const SAMPLE_RATE: usize = 48000;
    const MAX_ALLOWED_AMPLITUDE: f32 = 0.5;
    const MIN_ALLOWED_AMPLITUDE: f32 = -0.5;

    /// Renders the 4-second reference offline pattern deterministically into a heapless/allocated test buffer.
    fn render_offline_sequence() -> Vec<f32> {
        let mut synth = SynthVoice::new();
        let mut seq = Sequencer::new();
        let sec = SAMPLE_RATE;

        // Sequence configuration identical to main offline bounce sequence[cite: 1]
        seq.add_event(
            0,
            SequencerEvent::ParamChange(SynthParameter::Waveform(Waveform::Sine)),
        );
        seq.add_event(
            0,
            SequencerEvent::ParamChange(SynthParameter::Cutoff(1800.0)),
        );
        seq.add_event(
            0,
            SequencerEvent::ParamChange(SynthParameter::Resonance(0.85)),
        );
        seq.add_event(
            0,
            SequencerEvent::ParamChange(SynthParameter::Overdrive(5.0)),
        );

        seq.add_event(
            sec / 10,
            SequencerEvent::NoteOn {
                note_freq: 110.0,
                velocity: 1.0,
            },
        );
        seq.add_event(sec, SequencerEvent::NoteOff);

        seq.add_event(
            sec + (sec / 10),
            SequencerEvent::NoteOn {
                note_freq: 180.0,
                velocity: 1.0,
            },
        );
        seq.add_event(sec * 2, SequencerEvent::NoteOff);

        seq.add_event(
            sec * 2 + (sec / 10),
            SequencerEvent::ParamChange(SynthParameter::Cutoff(400.0)),
        );
        seq.add_event(
            sec * 2 + (sec / 10),
            SequencerEvent::NoteOn {
                note_freq: 150.0,
                velocity: 1.0,
            },
        );
        seq.add_event(sec * 3, SequencerEvent::NoteOff);

        seq.add_event(sec * 4, SequencerEvent::EndOfPattern);

        let mut buffer = Vec::with_capacity(sec * 4);
        let mut running = true;

        while running {
            running = seq.process_tick(&mut synth);
            let sample = synth.process_sample();
            buffer.push(sample);
        }

        buffer
    }

    #[test]
    fn test_audio_bounds_and_clamping() {
        let rendered_samples = render_offline_sequence();

        assert!(
            !rendered_samples.is_empty(),
            "Rendered buffer should contain non-zero sample count."
        );

        for (idx, &sample) in rendered_samples.iter().enumerate() {
            assert!(
                !sample.is_nan() && !sample.is_infinite(),
                "DSP Instability Detected: Non-finite sample payload at index {idx}: {sample}"
            );
            assert!(
                (MIN_ALLOWED_AMPLITUDE..=MAX_ALLOWED_AMPLITUDE).contains(&sample),
                "DSP Peak Violation: Sample at index {idx} exceeded peak output limits: {sample}"
            );
        }
    }

    #[test]
    fn test_pcm_little_endian_regression_exact() {
        let rendered_samples = render_offline_sequence();
        let mut rendered_bytes = Vec::with_capacity(rendered_samples.len() * 4);

        for sample in rendered_samples {
            rendered_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        assert_eq!(
            rendered_bytes.len(),
            GOLDEN_PCM_FIXTURE.len(),
            "Audio output length mismatch against repository asset 'pattern.raw'."
        );

        // Bit-exact LE byte stream regression verification
        assert_eq!(
            rendered_bytes, GOLDEN_PCM_FIXTURE,
            "Bit-exact audio regression detected: DSP pipeline output diverges from stored golden master."
        );
    }
}
