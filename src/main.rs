use std::f32::consts::PI;
use std::fs::File;
use std::io::{BufWriter, Write};

pub const SAMPLE_RATE: f32 = 48000.0;
pub const MAX_EVENTS: usize = 2048;

// ============================================================================
// 1. DATA-CARRYING ENUMS
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Waveform { Sine, Sawtooth, Square, Triangle }

#[derive(Copy, Clone, Debug)]
pub enum SynthParameter {
    Waveform(Waveform), Cutoff(f32), Resonance(f32), Overdrive(f32),
    Attack(f32), Decay(f32), Sustain(f32), Release(f32),
}

#[derive(Copy, Clone, Debug)]
pub enum SequencerEvent {
    NoteOn { note_freq: f32, velocity: f32 },
    NoteOff,
    ParamChange(SynthParameter),
    EndOfPattern,
    None,
}

// ============================================================================
// 2. MATH & DSP COMPONENTS
// ============================================================================

fn fast_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let numerator = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let denominator = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    let mut res = numerator / denominator;
    if res > 1.0 { res = 1.0; }
    if res < -1.0 { res = -1.0; }
    res
}

fn poly_blep(mut t: f32, dt: f32) -> f32 {
    if t < dt { t /= dt; t + t - t * t - 1.0 }
    else if t > 1.0 - dt { t = (t - 1.0) / dt; t * t + t + t + 1.0 }
    else { 0.0 }
}

#[derive(Clone, Copy)]
pub struct PolyBlepOscillator { pub waveform: Waveform, phase: f32, phase_inc: f32 }
impl PolyBlepOscillator {
    pub fn new() -> Self { Self { waveform: Waveform::Sawtooth, phase: 0.0, phase_inc: 0.0 } }
    pub fn set_frequency(&mut self, freq: f32) { self.phase_inc = freq / SAMPLE_RATE; }
    pub fn process(&mut self) -> f32 {
        let t = self.phase; let dt = self.phase_inc;
        let out = match self.waveform {
            Waveform::Sine => (t * 2.0 * PI).sin(),
            Waveform::Sawtooth => { 2.0 * t - 1.0 - poly_blep(t, dt) }
            Waveform::Square => { (if t < 0.5 { 1.0 } else { -1.0 }) + poly_blep(t, dt) - poly_blep((t + 0.5) % 1.0, dt) }
            Waveform::Triangle => { if t < 0.5 { 4.0 * t - 1.0 } else { 3.0 - 4.0 * t } }
        };
        self.phase += self.phase_inc;
        if self.phase >= 1.0 { self.phase -= 1.0; }
        out
    }
}

#[derive(Clone, Copy)]
pub struct SvfFilter { pub cutoff: f32, pub resonance: f32, ic1eq: f32, ic2eq: f32 }
impl SvfFilter {
    pub fn new() -> Self { Self { cutoff: 1000.0, resonance: 0.1, ic1eq: 0.0, ic2eq: 0.0 } }
    pub fn set_params(&mut self, cutoff: f32, res: f32) {
        self.cutoff = cutoff.clamp(20.0, SAMPLE_RATE * 0.49);
        self.resonance = res.clamp(0.0, 1.0);
    }
    pub fn process(&mut self, input: f32) -> f32 {
        let g = (PI * self.cutoff / SAMPLE_RATE).tan();
        let k = 2.0 - 2.0 * self.resonance;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1; let a3 = g * a2;
        let v3 = input - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v2
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AdsrState { Idle, Attack, Decay, Sustain, Release }
#[derive(Clone, Copy)]
pub struct Adsr { state: AdsrState, pub attack_time: f32, pub decay_time: f32, pub sustain_lvl: f32, pub release_time: f32, current_val: f32 }
impl Adsr {
    pub fn new() -> Self { Self { state: AdsrState::Idle, attack_time: 0.01, decay_time: 0.1, sustain_lvl: 0.7, release_time: 0.2, current_val: 0.0 } }
    pub fn trigger_on(&mut self) { self.state = AdsrState::Attack; }
    pub fn trigger_off(&mut self) { self.state = AdsrState::Release; }
    pub fn process(&mut self) -> f32 {
        let dt = 1.0 / SAMPLE_RATE;
        match self.state {
            AdsrState::Idle => { self.current_val = 0.0; }
            AdsrState::Attack => {
                self.current_val += (1.0 / self.attack_time) * dt;
                if self.current_val >= 1.0 { self.current_val = 1.0; self.state = AdsrState::Decay; }
            }
            AdsrState::Decay => {
                self.current_val -= ((1.0 - self.sustain_lvl) / self.decay_time) * dt;
                if self.current_val <= self.sustain_lvl { self.current_val = self.sustain_lvl; self.state = AdsrState::Sustain; }
            }
            AdsrState::Sustain => {}
            AdsrState::Release => {
                self.current_val -= (self.sustain_lvl / self.release_time) * dt;
                if self.current_val <= 0.0 { self.current_val = 0.0; self.state = AdsrState::Idle; }
            }
        }
        self.current_val
    }
}

pub struct SynthVoice {
    osc: PolyBlepOscillator, filter: SvfFilter, env: Adsr, master_overdrive: f32,
}
impl SynthVoice {
    pub fn new() -> Self { Self { osc: PolyBlepOscillator::new(), filter: SvfFilter::new(), env: Adsr::new(), master_overdrive: 1.0 } }
    pub fn apply_event(&mut self, event: &SequencerEvent) {
        match event {
            SequencerEvent::NoteOn { note_freq, velocity: _ } => { self.osc.set_frequency(*note_freq); self.env.trigger_on(); }
            SequencerEvent::NoteOff => { self.env.trigger_off(); }
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

#[derive(Copy, Clone, Debug)]
pub struct EventNode {
    pub timestamp: usize,
    pub event: SequencerEvent,
}

// Gives us a safe, empty default to fill the array with at initialization.
impl Default for EventNode {
    fn default() -> Self {
        EventNode { timestamp: 0, event: SequencerEvent::None }
    }
}

pub struct Sequencer {
    // A fixed-size array stored entirely on the stack. No heap allocation!
    events: [EventNode; MAX_EVENTS],
    event_count: usize,
    current_index: usize,
    sample_counter: usize,
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
        while self.current_index < self.event_count &&
              self.events[self.current_index].timestamp == self.sample_counter {

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

// ============================================================================
// 4. FINAL EXECUTION CONTEXT
// ============================================================================

fn main() {
    println!("Step 4: Running the Stack-Allocated Synthesizer...");

    let mut synth = SynthVoice::new();
    let mut seq = Sequencer::new();
    let sec = SAMPLE_RATE as usize;

    // Setup harsh analog tone
    seq.add_event(0, SequencerEvent::ParamChange(SynthParameter::Waveform(Waveform::Sawtooth)));
    seq.add_event(0, SequencerEvent::ParamChange(SynthParameter::Cutoff(1800.0)));
    seq.add_event(0, SequencerEvent::ParamChange(SynthParameter::Resonance(0.85)));
    seq.add_event(0, SequencerEvent::ParamChange(SynthParameter::Overdrive(5.0)));

    // Musical phrase
    seq.add_event(sec / 10, SequencerEvent::NoteOn { note_freq: 110.0, velocity: 1.0 }); // A2
    seq.add_event(sec, SequencerEvent::NoteOff);

    seq.add_event(sec + (sec / 10), SequencerEvent::NoteOn { note_freq: 220.0, velocity: 1.0 }); // A3
    seq.add_event(sec * 2, SequencerEvent::NoteOff);

    // Filter sweep modification during phrase
    seq.add_event(sec * 2 + (sec / 10), SequencerEvent::ParamChange(SynthParameter::Cutoff(400.0)));
    seq.add_event(sec * 2 + (sec / 10), SequencerEvent::NoteOn { note_freq: 55.0, velocity: 1.0 }); // A1
    seq.add_event(sec * 3, SequencerEvent::NoteOff);

    seq.add_event(sec * 4, SequencerEvent::EndOfPattern);

    // Rendering Loop
    let mut file = BufWriter::new(File::create("final_output.raw").expect("Failed to create file"));
    let mut running = true;

    while running {
        // The sequencer tells us if we should keep running
        running = seq.process_tick(&mut synth);
        let sample = synth.process_sample();

        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    println!("Audio successfully written to 'final_output.raw'.");
    println!("Playback via bash: aplay -f FLOAT_LE -r 48000 -c 1 final_output.raw");
}
