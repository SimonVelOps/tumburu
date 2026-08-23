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
use std::f32::consts::PI;
use crate::{poly_blep, Waveform, SAMPLE_RATE};

#[derive(Clone, Copy)]
pub struct PolyBlepOscillator {
    pub waveform: Waveform,
    phase: f32,
    phase_inc: f32,
}
impl Default for PolyBlepOscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl PolyBlepOscillator {
    pub fn new() -> Self {
        Self {
            waveform: Waveform::Sawtooth,
            phase: 0.0,
            phase_inc: 0.0,
        }
    }
    pub fn set_frequency(&mut self, freq: f32) {
        self.phase_inc = freq / SAMPLE_RATE;
    }
    pub fn process(&mut self) -> f32 {
        let t = self.phase;
        let dt = self.phase_inc;
        let out = match self.waveform {
            Waveform::Sine => (t * 2.0 * PI).sin(),
            Waveform::Sawtooth => 2.0 * t - 1.0 - poly_blep(t, dt),
            Waveform::Square => {
                (if t < 0.5 { 1.0 } else { -1.0 }) + poly_blep(t, dt)
                    - poly_blep((t + 0.5) % 1.0, dt)
            }
            Waveform::Triangle => {
                if t < 0.5 {
                    4.0 * t - 1.0
                } else {
                    3.0 - 4.0 * t
                }
            }
        };
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }
}

