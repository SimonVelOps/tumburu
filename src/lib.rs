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

/// Expose the crate internal modules
pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod sequencer;
pub mod synth;

pub const SAMPLE_RATE: f32 = 48000.0;
pub const MAX_EVENTS: usize = 2048;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Waveform {
    Sine,
    Sawtooth,
    Square,
    Triangle,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AdsrState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}


pub fn fast_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let numerator = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let denominator = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    let mut res = numerator / denominator;
    if res > 1.0 {
        res = 1.0;
    }
    if res < -1.0 {
        res = -1.0;
    }
    res
}

pub fn poly_blep(mut t: f32, dt: f32) -> f32 {
    if t < dt {
        t /= dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}
