use std::f32::consts::PI;
use std::fs::File;
use std::io::{BufWriter, Write};

pub const SAMPLE_RATE: f32 = 48000.0;
pub const MAX_EVENTS: usize = 2048;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Waveform {
    Sine,
    Sawtooth,
    Square,
    Triangle,
}

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

#[derive(Copy, Clone, Debug)]
pub enum SequencerEvent {
    NoteOn { note_freq: f32, velocity: f32 },
    NoteOff,
    ParamChange(SynthParameter),
    EndOfPattern,
    None,
}

/// # Rust behaviors
/// Pure, stack-allocated calculation without heap allocations or side effects.
/// Bounded branching applies hard limits to prevent output divergence.
///
/// # Arguments/return describe
/// * `x: f32` - Input audio signal amplitude or gain-scaled drive value.
/// * `Return: f32` - Symmetrically saturated signal output clamped within `[-1.0, 1.0]`.
///
/// # Mathematical explanations
/// Evaluates a [7/8] Padé rational approximant for the hyperbolic tangent function:
///
/// `tanh(x) ≈ x * (135135 + 17325*x^2 + 378*x^4 + x^6) / (135135 + 62370*x^2 + 3150*x^4 + 28*x^6 + x^8)`
///
/// Provides near-zero error relative to `std::f32::tanh` while executing significantly faster without transcendental calls.
///
/// # Describe sound processing
/// Soft-knee waveshaping distortion. Adds odd-order harmonics (3rd, 5th, 7th) to warm the audio spectrum and compress dynamic range smoothly without harsh digital hard-clipping.
///
/// # Sound processing use cases
/// Analog tape/tube saturation modeling, master bus brickwall limiting protection, and synth voice overdrive stages.
fn fast_tanh(x: f32) -> f32 {
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

/// # Rust behaviors
/// Branching evaluation based on normalized phase offset `t`. Modifies mutable local binding `t` internally without heap allocations or side effects.
///
/// # Arguments/return describe
/// * `t: f32` - Normalized phase in range `[0.0, 1.0)`.
/// * `dt: f32` - Phase increment per sample step (`frequency / sample_rate`).
/// * `Return: f32` - Smooth residual correction value to add/subtract at step discontinuities.
///
/// # Mathematical explanations
/// Computes the 2nd-order Polynomial Band-Limited Step (PolyBLEP) residual:
/// * If `t < dt`: `t' = t / dt`, residual = `2 * t' - (t')^2 - 1.0`
/// * If `t > 1.0 - dt`: `t' = (t - 1.0) / dt`, residual = `(t')^2 + 2 * t' + 1.0`
/// * Otherwise: `0.0`
///
/// # Describe sound processing
/// Anti-aliasing correction function. Smooths sharp wave transition edges across phase resets to attenuate high-frequency foldback aliasing according to the Nyquist-Shannon sampling theorem.
///
/// # Sound processing use cases
/// Virtual analog synth oscillators generating band-limited sawtooth and square waveforms.
fn poly_blep(mut t: f32, dt: f32) -> f32 {
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

#[derive(Clone, Copy)]
pub struct PolyBlepOscillator {
    pub waveform: Waveform,
    phase: f32,
    phase_inc: f32,
}
impl PolyBlepOscillator {
    /// # Rust behaviors
    /// Associated constructor returning a stack-allocated struct instance with zeroed phase state.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - PolyBLEP oscillator initialized to `Waveform::Sawtooth` with zero phase.
    ///
    /// # Mathematical explanations
    /// Sets initial phase state `phase = 0.0` and normalized frequency delta `phase_inc = 0.0`.
    ///
    /// # Describe sound processing
    /// Instantiates digital generator memory prior to audio stream rendering.
    ///
    /// # Sound processing use cases
    /// Subtractive synth oscillator initialization during voice creation.
    pub fn new() -> Self {
        Self {
            waveform: Waveform::Sawtooth,
            phase: 0.0,
            phase_inc: 0.0,
        }
    }

    /// # Rust behaviors
    /// In-place state mutation via mutable pointer `&mut self`.
    ///
    /// # Arguments/return describe
    /// * `freq: f32` - Target frequency in Hertz (Hz).
    /// * `Return: ()` - Mutates internal `phase_inc` in-place.
    ///
    /// # Mathematical explanations
    /// Converts continuous frequency `f` into normalized discrete-time phase delta:
    ///
    /// `phase_inc = freq / SAMPLE_RATE`
    ///
    /// # Describe sound processing
    /// Updates oscillator speed per audio sample frame to track target pitch.
    ///
    /// # Sound processing use cases
    /// Pitch tracking, MIDI note updates, pitch bend, and vibrato LFO modulation.
    pub fn set_frequency(&mut self, freq: f32) {
        self.phase_inc = freq / SAMPLE_RATE;
    }

    /// # Rust behaviors
    /// Mutates internal `phase` state on every execution step and wraps phase bound using modulo arithmetic.
    ///
    /// # Arguments/return describe
    /// * `Return: f32` - Normalized band-limited output sample value in range `[-1.0, 1.0]`.
    ///
    /// # Mathematical explanations
    /// Generates naive oscillator signals and applies PolyBLEP residual corrections `B(t, dt)`:
    /// * Sawtooth: `2t - 1.0 - B(t, dt)`
    /// * Square: `sign(t - 0.5) + B(t, dt) - B((t + 0.5) % 1.0, dt)`
    /// * Triangle: Continuous linear integration `4t - 1.0` (or `3.0 - 4t`).
    ///
    /// # Describe sound processing
    /// Real-time sample generator for basic audio waveforms with suppressed digital aliasing.
    ///
    /// # Sound processing use cases
    /// Primary audio signal generation in subtractive synthesizer engines.
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

#[derive(Clone, Copy)]
pub struct SvfFilter {
    pub cutoff: f32,
    pub resonance: f32,
    ic1eq: f32,
    ic2eq: f32,
}
impl SvfFilter {
    /// # Rust behaviors
    /// Stack-allocated struct constructor initializing internal state memory variables to zero.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - Filter struct initialized with cutoff at 1000 Hz and resonance at 0.1.
    ///
    /// # Mathematical explanations
    /// Zero-initializes state variable delay registers `ic1eq = 0.0` and `ic2eq = 0.0`.
    ///
    /// # Describe sound processing
    /// Allocates an unfiltered, neutral 2-pole State Variable Filter topology.
    ///
    /// # Sound processing use cases
    /// Voice filter initialization in DAW channels or plugin instances.
    pub fn new() -> Self {
        Self {
            cutoff: 1000.0,
            resonance: 0.1,
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }

    /// # Rust behaviors
    /// Mutates parameters in-place; applies `f32::clamp` to guarantee filter numerical stability.
    ///
    /// # Arguments/return describe
    /// * `cutoff: f32` - Cutoff frequency in Hz, clamped to `[20.0, SAMPLE_RATE * 0.49]`.
    /// * `res: f32` - Resonance parameter, clamped to `[0.0, 1.0]`.
    /// * `Return: ()` - Updates parameters in-place.
    ///
    /// # Mathematical explanations
    /// Restricts `cutoff` below Nyquist limit (`f_s / 2`) to ensure bilinear transform domain stability.
    ///
    /// # Describe sound processing
    /// Configures filter boundary controls to prevent infinite feedback or numerical overflow.
    ///
    /// # Sound processing use cases
    /// Handling cutoff knob sweeps, envelope tracking, and dynamic filter automation.
    pub fn set_params(&mut self, cutoff: f32, res: f32) {
        self.cutoff = cutoff.clamp(20.0, SAMPLE_RATE * 0.49);
        self.resonance = res.clamp(0.0, 1.0);
    }

    /// # Rust behaviors
    /// Mutates internal state registers (`ic1eq`, `ic2eq`) during sample rendering tick.
    ///
    /// # Arguments/return describe
    /// * `input: f32` - Single incoming audio sample.
    /// * `Return: f32` - Lowpass-filtered audio sample output (`v2`).
    ///
    /// # Mathematical explanations
    /// Implements Andrew Simper's Topology-Preserving Transform (TPT) State Variable Filter discretization:
    /// * `g = tan(PI * cutoff / SAMPLE_RATE)`
    /// * `k = 2.0 - 2.0 * resonance`
    /// * `a1 = 1.0 / (1.0 + g * (g + k))`, `a2 = g * a1`, `a3 = g * a2`
    /// * `v3 = input - ic2eq`
    /// * `v1 = a1 * ic1eq + a2 * v3`
    /// * `v2 = ic2eq + a2 * ic1eq + a3 * v3`
    /// * Updates: `ic1eq = 2*v1 - ic1eq`, `ic2eq = 2*v2 - ic2eq`
    ///
    /// # Describe sound processing
    /// 12 dB/octave (2-pole) resonant low-pass filter preserving analog phase structure under fast modulation.
    ///
    /// # Sound processing use cases
    /// Timbral sculpting, high-frequency dampening, and classic synth filter sweeps.
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

#[derive(Clone, Copy, PartialEq)]
enum AdsrState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}
#[derive(Clone, Copy)]
pub struct Adsr {
    state: AdsrState,
    pub attack_time: f32,
    pub decay_time: f32,
    pub sustain_lvl: f32,
    pub release_time: f32,
    current_val: f32,
}
impl Adsr {
    /// # Rust behaviors
    /// Associated constructor function returning a default ADSR envelope struct on the stack.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - Envelope generator initialized to `AdsrState::Idle` with default timings.
    ///
    /// # Mathematical explanations
    /// Initial value `current_val = 0.0`. Default time parameters: `attack = 10ms`, `decay = 100ms`, `sustain = 0.7`, `release = 200ms`.
    ///
    /// # Describe sound processing
    /// Instantiates envelope state machine for control voltage/gain generation.
    ///
    /// # Sound processing use cases
    /// Envelope module setup for synthesizer voices.
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

    /// # Rust behaviors
    /// In-place state transition setting `state` variant to `Attack`.
    ///
    /// # Arguments/return describe
    /// * `Return: ()` - Modifies `state` field in-place.
    ///
    /// # Mathematical explanations
    /// Resets envelope target progression to start linear ramp towards `1.0`.
    ///
    /// # Describe sound processing
    /// Responds to key-press gate signals (Gate On).
    ///
    /// # Sound processing use cases
    /// MIDI NoteOn signal handling.
    pub fn trigger_on(&mut self) {
        self.state = AdsrState::Attack;
    }

    /// # Rust behaviors
    /// In-place state transition setting `state` variant to `Release`.
    ///
    /// # Arguments/return describe
    /// * `Return: ()` - Modifies `state` field in-place.
    ///
    /// # Mathematical explanations
    /// Initiates exponential/linear decay from `sustain_lvl` down to `0.0`.
    ///
    /// # Describe sound processing
    /// Responds to key-release gate signals (Gate Off).
    ///
    /// # Sound processing use cases
    /// MIDI NoteOff signal handling.
    pub fn trigger_off(&mut self) {
        self.state = AdsrState::Release;
    }

    /// # Rust behaviors
    /// State-machine execution step; mutates `current_val` and transitions `state` based on thresholds.
    ///
    /// # Arguments/return describe
    /// * `Return: f32` - Envelope amplitude coefficient in range `[0.0, 1.0]`.
    ///
    /// # Mathematical explanations
    /// Performs discrete linear integration over step `dt = 1.0 / SAMPLE_RATE`:
    /// * Attack: `v[n] = v[n-1] + (1.0 / attack_time) * dt`
    /// * Decay: `v[n] = v[n-1] - ((1.0 - sustain_lvl) / decay_time) * dt`
    /// * Sustain: `v[n] = sustain_lvl`
    /// * Release: `v[n] = v[n-1] - (sustain_lvl / release_time) * dt`
    ///
    /// # Describe sound processing
    /// Generates dynamic control signals to shape note amplitude and filter dynamics over time.
    ///
    /// # Sound processing use cases
    /// Dynamic volume modulation, pluck transients, and pad fade-ins.
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

pub struct SynthVoice {
    osc: PolyBlepOscillator,
    filter: SvfFilter,
    env: Adsr,
    master_overdrive: f32,
}
impl SynthVoice {
    /// # Rust behaviors
    /// Aggregates `PolyBlepOscillator`, `SvfFilter`, and `Adsr` into a single stack struct.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - Monophonic synth voice instance.
    ///
    /// # Mathematical explanations
    /// Aggregates default module states and sets master overdrive parameter `G = 1.0`.
    ///
    /// # Describe sound processing
    /// Initializes complete monophonic audio processing signal chain.
    ///
    /// # Sound processing use cases
    /// Voice instantiation in monophonic synths or polyphonic voice pools.
    pub fn new() -> Self {
        Self {
            osc: PolyBlepOscillator::new(),
            filter: SvfFilter::new(),
            env: Adsr::new(),
            master_overdrive: 1.0,
        }
    }

    /// # Rust behaviors
    /// Pattern matches `SequencerEvent` and mutates sub-components in-place via internal mutable references.
    ///
    /// # Arguments/return describe
    /// * `event: &SequencerEvent` - Control event message reference.
    /// * `Return: ()` - Updates internal settings in-place.
    ///
    /// # Mathematical explanations
    /// Maps input parameters directly into signal chain coefficients (frequency, cutoff, resonance, overdrive gain factor $G$, ADSR times).
    ///
    /// # Describe sound processing
    /// Control plane interface translating sequencer messages into real-time DSP parameters.
    ///
    /// # Sound processing use cases
    /// Dynamic parameter updates triggered by MIDI events or timeline automation.
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

    /// # Rust behaviors
    /// Executes sequential module processing steps within a single lock-free sample rendering tick.
    ///
    /// # Arguments/return describe
    /// * `Return: f32` - Final monophonic floating-point audio sample.
    ///
    /// # Mathematical explanations
    /// Computes full voice signal flow equation:
    ///
    /// `y[n] = 0.5 * fast_tanh( SVF( Osc[n] * Env[n] ) * master_overdrive )`
    ///
    /// # Describe sound processing
    /// Subtractive synth signal flow: Osc -> Envelope Gain -> Resonant SVF Lowpass -> Overdrive Saturation -> Half-Gain Scale.
    ///
    /// # Sound processing use cases
    /// Sample generation in real-time audio thread rendering.
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
    /// # Rust behaviors
    /// Implements `Default` trait returning a zero-allocated placeholder node.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - Default `EventNode` with `timestamp = 0` and `event = SequencerEvent::None`.
    ///
    /// # Mathematical explanations
    /// Initializes timestamp register `t = 0`.
    ///
    /// # Describe sound processing
    /// Inactive placeholder event for fixed-size memory allocation.
    ///
    /// # Sound processing use cases
    /// Stack memory initialization for pre-allocated event buffers.
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

impl Sequencer {
    /// # Rust behaviors
    /// Pre-allocates fixed-size array on the stack without heap allocation or runtime re-allocations.
    ///
    /// # Arguments/return describe
    /// * `Return: Self` - Sequencer instance with `MAX_EVENTS` zeroed slots.
    ///
    /// # Mathematical explanations
    /// Sets sample clock counter `sample_counter = 0` and event pointer `current_index = 0`.
    ///
    /// # Describe sound processing
    /// Real-time sample-accurate timeline event scheduler initialization.
    ///
    /// # Sound processing use cases
    /// DAW track timeline event manager setup.
    pub fn new() -> Self {
        Self {
            events: [EventNode::default(); MAX_EVENTS],
            event_count: 0,
            current_index: 0,
            sample_counter: 0,
        }
    }

    /// # Rust behaviors
    /// Array bounds check prior to insertion; mutates internal array and counter in-place.
    ///
    /// # Arguments/return describe
    /// * `timestamp: usize` - Absolute sample index frame when event triggers.
    /// * `event: SequencerEvent` - Event payload message.
    /// * `Return: ()` - Appends event if capacity permits.
    ///
    /// # Mathematical explanations
    /// Stores node `E` at index `N`, enforcing bounds condition `N < MAX_EVENTS`.
    ///
    /// # Describe sound processing
    /// Schedules time-stamped control messages onto the audio playback timeline.
    ///
    /// # Sound processing use cases
    /// Recording MIDI patterns and timeline parameter automation.
    pub fn add_event(&mut self, timestamp: usize, event: SequencerEvent) {
        if self.event_count < MAX_EVENTS {
            self.events[self.event_count] = EventNode { timestamp, event };
            self.event_count += 1;
        }
    }

    /// # Rust behaviors
    /// Iterates over scheduled events targeting the current sample frame; mutates index and target `SynthVoice`.
    ///
    /// # Arguments/return describe
    /// * `voice: &mut SynthVoice` - Target synthesizer voice instance.
    /// * `Return: bool` - `true` if sequencer is active, `false` upon `EndOfPattern`.
    ///
    /// # Mathematical explanations
    /// Matches condition `timestamp == sample_counter`. Increments `sample_counter = sample_counter + 1`.
    ///
    /// # Describe sound processing
    /// Sample-accurate event processing ensuring zero jitter in MIDI note timing or automation.
    ///
    /// # Sound processing use cases
    /// Sample-exact timeline sequence playback in DAWs.
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
/// DAW offline export / bounce-to-disk feature.
fn main() {
    println!("VelOps POC substractive synthetiser...");

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
    let mut file = BufWriter::new(File::create("pattern.raw").expect("Failed to create file"));
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
