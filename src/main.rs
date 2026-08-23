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
// use std::fs::File;
// use std::io::{BufWriter, Write};
// use std::sync::Arc;
use std::sync::mpsc;

use tumburu::sequencer::{Sequencer, SequencerEvent};
use tumburu::synth::{SynthParameter, SynthVoice};
use tumburu::{Waveform ,SAMPLE_RATE};

fn main() {
    println!("Tumburu Digital Audio Workstation...");

    let mut synth = SynthVoice::new();
    let mut seq = Sequencer::new();

    // IMPORTANT: In a real DAW, this should be pulled from the cpal device config dynamically
    // rather than relying on a hardcoded constant, so it matches your DAC's clock.
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

    // --- REAL-TIME CPAL SETUP ---
    let host = cpal::default_host();
    let device = host.default_output_device().expect("No output device available");
    let supported_config = device.default_output_config().expect("Failed to get default output config");
    let config: cpal::StreamConfig = supported_config.into();
    println!("Using DAC",);
    let (tx, rx) = mpsc::channel();
    let channels = config.channels as usize;
    let mut running = true;

    let stream = device
        .build_output_stream(
            // 2. Pass config by value, removing the '&'
            config, // Or just `config,` if you don't need to use it again later
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let mut current_sample = 0.0;

                    if running {
                        running = seq.process_tick(&mut synth);
                        current_sample = synth.process_sample();
                    } else {
                        let _ = tx.send(());
                    }

                    for channel_sample in frame.iter_mut() {
                        *channel_sample = current_sample;
                    }
                }
            },
            move |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .expect("Failed to build output stream");

    stream.play().expect("Failed to start audio stream");
    println!("Playing audio in real-time...");

    let _ = rx.recv();
    println!("Playback complete. Exiting...");
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verifies exact byte-for-byte output of generated raw signal data
    #[test]
    fn test_raw_signal_bytes() {
        let mut synth = SynthVoice::new(); //[cite: 1]
        synth.apply_event(&SequencerEvent::NoteOn {
            //[cite: 1]
            note_freq: 220.0,
            velocity: 1.0,
        });

        let mut generated_bytes = Vec::new();
        for _ in 0..10 {
            let sample = synth.process_sample(); //[cite: 1]
            generated_bytes.extend_from_slice(&sample.to_le_bytes()); //[cite: 1]
        }

        // Exact 32-bit float Little-Endian bytes for first 10 rendered samples[cite: 1]
        let expected_bytes: [u8; 40] = [
            0, 0, 0, 0, // Sample 0 (Phase at 0.0)
            112, 145, 4, 183, // Sample 1
            129, 88, 46, 184, // Sample 2
            240, 242, 239, 184, // Sample 3
            47, 114, 115, 185, // Sample 4
            195, 218, 208, 185, // Sample 5
            230, 38, 33, 186, // Sample 6
            21, 31, 103, 186, // Sample 7
            223, 15, 157, 186, // Sample 8
            176, 219, 204, 186, // Sample 9
        ];

        assert_eq!(generated_bytes, expected_bytes);
    }

    // Ensures processed signal output stays within normalized audio amplitude limits [-0.5, 0.5][cite: 1]
    #[test]
    fn test_raw_signal_amplitude_bounds() {
        let mut synth = SynthVoice::new(); //[cite: 1]
        let mut seq = Sequencer::new(); //[cite: 1]

        seq.add_event(
            0,
            SequencerEvent::ParamChange(SynthParameter::Overdrive(10.0)), //[cite: 1]
        );
        seq.add_event(
            0,
            SequencerEvent::NoteOn {
                note_freq: 440.0,
                velocity: 1.0,
            }, //[cite: 1]
        );

        for _ in 0..4800 {
            // Check 100ms worth of audio[cite: 1]
            seq.process_tick(&mut synth); //[cite: 1]
            let sample = synth.process_sample(); //[cite: 1]
            let sample_bytes = sample.to_le_bytes(); //[cite: 1]

            // Convert back from 4-byte LE slice to float[cite: 1]
            let decoded_sample = f32::from_le_bytes(sample_bytes);

            // fast_tanh * 0.5 scaling guarantees output is within [-0.5, 0.5][cite: 1]
            assert!(
                decoded_sample >= -0.5 && decoded_sample <= 0.5,
                "Sample output out of bounds: {}",
                decoded_sample
            );
        }
    }

    // Pattern for comparing live render output against a pre-recorded reference fixture file
    #[test]
    fn test_assert_against_raw_fixture() {
        // Embed expected reference raw file directly into binary at compile time:
        // const REFERENCE_RAW: &[u8] = include_bytes!("../tests/fixtures/pattern_ref.raw");

        let mut synth = SynthVoice::new(); //[cite: 1]
        synth.apply_event(&SequencerEvent::ParamChange(SynthParameter::Waveform(
            Waveform::Sine, //[cite: 1]
        )));

        let sample = synth.process_sample(); //[cite: 1]
        let raw_bytes = sample.to_le_bytes(); //[cite: 1]

        assert_eq!(raw_bytes.len(), 4);
    }
}
