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
use crate::{AdsrState, SAMPLE_RATE};


#[derive(Clone, Copy)]
pub struct Adsr {
    state: AdsrState,
    pub attack_time: f32,
    pub decay_time: f32,
    pub sustain_lvl: f32,
    pub release_time: f32,
    current_val: f32,
}
impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    pub fn new() -> Self {
        Self {
            state: AdsrState::Idle,
            attack_time: 0.01,
            decay_time: 0.1,
            sustain_lvl: 0.7,
            release_time: 0.2,
            current_val: 0.0,
        }
    }
    pub fn trigger_on(&mut self) {
        self.state = AdsrState::Attack;
    }
    pub fn trigger_off(&mut self) {
        self.state = AdsrState::Release;
    }
    pub fn process(&mut self) -> f32 {
        let dt = 1.0 / SAMPLE_RATE;
        match self.state {
            AdsrState::Idle => {
                self.current_val = 0.0;
            }
            AdsrState::Attack => {
                self.current_val += (1.0 / self.attack_time) * dt;
                if self.current_val >= 1.0 {
                    self.current_val = 1.0;
                    self.state = AdsrState::Decay;
                }
            }
            AdsrState::Decay => {
                self.current_val -= ((1.0 - self.sustain_lvl) / self.decay_time) * dt;
                if self.current_val <= self.sustain_lvl {
                    self.current_val = self.sustain_lvl;
                    self.state = AdsrState::Sustain;
                }
            }
            AdsrState::Sustain => {}
            AdsrState::Release => {
                self.current_val -= (self.sustain_lvl / self.release_time) * dt;
                if self.current_val <= 0.0 {
                    self.current_val = 0.0;
                    self.state = AdsrState::Idle;
                }
            }
        }
        self.current_val
    }
}
