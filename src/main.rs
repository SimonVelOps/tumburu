use std::f32::consts::PI;
use std::fs::File;
use std::io::{BufWriter, Write};

pub const SAMPLE_RATE: f32 = 48000.0;

// ============================================================================
// 1. DATA-CARRYING ENUMS (The Rust "Union" alternative)
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Waveform { Sine, Sawtooth, Square, Triangle }

/// Notice how `Cutoff` holds an f32, and `Waveform` holds our Waveform enum.
/// In C, this would require a complicated struct/union combo.
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

/// The sequencer command set. NoteOn holds both frequency and velocity!
#[derive(Copy, Clone, Debug)]
pub enum SequencerEvent {
    NoteOn { note_freq: f32, velocity: f32 },
    NoteOff,
    ParamChange(SynthParameter),
    EndOfPattern,
    None,
}

// ============================================================================
// 2. MATH & DSP COMPONENTS (From previous steps)
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

// ============================================================================
// 3. NEW IN STEP 3: SYNTH VOICE
// ============================================================================

/// Combines all DSP modules into a single monophonic voice.
pub struct SynthVoice {
    osc: PolyBlepOscillator,
    filter: SvfFilter,
    env: Adsr,
    master_overdrive: f32,
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

    /// Mutates voice state based on inbound sequencer events.
    pub fn apply_event(&mut self, event: &SequencerEvent) {
        // Pattern matching unwraps the data safely
        match event {
            SequencerEvent::NoteOn { note_freq, velocity: _ } => {
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

    /// Computes the final audio sample for the current cycle.
    pub fn process_sample(&mut self) -> f32 {
        let env_mod = self.env.process();
        let osc_out = self.osc.process();

        let mut signal = osc_out * env_mod;
        signal = self.filter.process(signal);

        // Apply overdrive and analog saturation (soft clipping)
        signal *= self.master_overdrive;
        signal = fast_tanh(signal);

        // Output margin
        signal * 0.5
    }
}

// ============================================================================
// 4. EXECUTION
// ============================================================================

fn main() {
    println!("Step 3: Testing the unified SynthVoice...");

    let mut voice = SynthVoice::new();
    let sec = SAMPLE_RATE as usize;

    // Send some setup events using our safe Enums
    voice.apply_event(&SequencerEvent::ParamChange(SynthParameter::Waveform(Waveform::Square)));
    voice.apply_event(&SequencerEvent::ParamChange(SynthParameter::Cutoff(1200.0)));
    voice.apply_event(&SequencerEvent::ParamChange(SynthParameter::Overdrive(3.0))); // Push it hard!

    let mut file = BufWriter::new(File::create("step3_output.raw").expect("Failed to create file"));

    // Simulate a 2-second timeline manually
    for i in 0..(sec * 2) {
        // Timeline events
        if i == 0 {
            voice.apply_event(&SequencerEvent::NoteOn { note_freq: 480.0, velocity: 1.0 });
        } else if i == sec {
            voice.apply_event(&SequencerEvent::NoteOff);
        }

        let sample = voice.process_sample();
        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    println!("Audio successfully written to 'step3_output.raw'.");
}
