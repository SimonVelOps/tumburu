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
use crate::MAX_EVENTS;
use crate::synth::{SynthParameter, SynthVoice};

#[derive(Copy, Clone, Debug)]
pub enum SequencerEvent {
    NoteOn { note_freq: f32, velocity: f32 },
    NoteOff,
    ParamChange(SynthParameter),
    EndOfPattern,
    None,
}

#[derive(Copy, Clone, Debug)]
pub struct EventNode {
    pub timestamp: usize,
    pub event: SequencerEvent,
}

// Gives us a safe, empty default to fill the array with at initialization.
impl Default for EventNode {
    fn default() -> Self {
        EventNode {
            timestamp: 0,
            event: SequencerEvent::None,
        }
    }
}

pub struct Sequencer {
    // A fixed-size array stored entirely on the stack. No heap allocation!
    events: [EventNode; MAX_EVENTS],
    event_count: usize,
    current_index: usize,
    sample_counter: usize,
}
impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequencer {
    pub fn new() -> Self {
        Self {
            events: [EventNode::default(); MAX_EVENTS],
            event_count: 0,
            current_index: 0,
            sample_counter: 0,
        }
    }

    pub fn add_event(&mut self, timestamp: usize, event: SequencerEvent) {
        if self.event_count < MAX_EVENTS {
            self.events[self.event_count] = EventNode { timestamp, event };
            self.event_count += 1;
        }
    }

    pub fn process_tick(&mut self, voice: &mut SynthVoice) -> bool {
        let mut active = true;

        // Execute all events scheduled for the exact current sample index.
        while self.current_index < self.event_count
            && self.events[self.current_index].timestamp == self.sample_counter
        {
            let ev = &self.events[self.current_index].event;
            if let SequencerEvent::EndOfPattern = ev {
                active = false; // Stop the loop when the pattern is done
            } else {
                voice.apply_event(ev);
            }
            self.current_index += 1;
        }

        self.sample_counter += 1;
        active
    }
}
