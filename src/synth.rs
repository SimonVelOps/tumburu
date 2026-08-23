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
use crate::oscillator::PolyBlepOscillator;
use crate::filter::SvfFilter;
use crate::sequencer::SequencerEvent;
use crate::envelope::Adsr;
use crate::{Waveform, fast_tanh};

#[derive(Copy, Clone, Debug)]
pub enum SynthParameter {
    Waveform(Waveform),
    Cutoff(f32),
    Resonance(f32),
    Overdrive(f32),
    Attack(f32),
    Decay(f32),
    Sustain(f32),
    Release(f32),
}

pub struct SynthVoice {
    osc: PolyBlepOscillator,
    filter: SvfFilter,
    env: Adsr,
    master_overdrive: f32,
}
impl Default for SynthVoice {
    fn default() -> Self {
        Self::new()
    }
}

impl SynthVoice {
    pub fn new() -> Self {
        Self {
            osc: PolyBlepOscillator::new(),
            filter: SvfFilter::new(),
            env: Adsr::new(),
            master_overdrive: 1.0,
        }
    }
    pub fn apply_event(&mut self, event: &SequencerEvent) {
        match event {
            SequencerEvent::NoteOn {
                note_freq,
                velocity: _,
            } => {
                self.osc.set_frequency(*note_freq);
                self.env.trigger_on();
            }
            SequencerEvent::NoteOff => {
                self.env.trigger_off();
            }
            SequencerEvent::ParamChange(param) => match param {
                SynthParameter::Waveform(w) => self.osc.waveform = *w,
                SynthParameter::Cutoff(c) => self.filter.set_params(*c, self.filter.resonance),
                SynthParameter::Resonance(r) => self.filter.set_params(self.filter.cutoff, *r),
                SynthParameter::Overdrive(o) => self.master_overdrive = *o,
                SynthParameter::Attack(a) => self.env.attack_time = *a,
                SynthParameter::Decay(d) => self.env.decay_time = *d,
                SynthParameter::Sustain(s) => self.env.sustain_lvl = *s,
                SynthParameter::Release(r) => self.env.release_time = *r,
            },
            SequencerEvent::EndOfPattern | SequencerEvent::None => {}
        }
    }
    pub fn process_sample(&mut self) -> f32 {
        let env_mod = self.env.process();
        let mut signal = self.osc.process() * env_mod;
        signal = self.filter.process(signal);
        fast_tanh(signal * self.master_overdrive) * 0.5
    }
}
