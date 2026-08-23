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
use crate::SAMPLE_RATE;

#[derive(Clone, Copy)]
pub struct SvfFilter {
    pub cutoff: f32,
    pub resonance: f32,
    ic1eq: f32,
    ic2eq: f32,
}
impl Default for SvfFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SvfFilter {
    pub fn new() -> Self {
        Self {
            cutoff: 1000.0,
            resonance: 0.1,
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }
    pub fn set_params(&mut self, cutoff: f32, res: f32) {
        self.cutoff = cutoff.clamp(20.0, SAMPLE_RATE * 0.49);
        self.resonance = res.clamp(0.0, 1.0);
    }
    pub fn process(&mut self, input: f32) -> f32 {
        let g = (PI * self.cutoff / SAMPLE_RATE).tan();
        let k = 2.0 - 2.0 * self.resonance;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = input - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v2
    }
}
