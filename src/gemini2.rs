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

/// ### Rust behaviors
/// Pure numerical function executed entirely on the stack. Employs inline conditional hard-clipping bounds to prevent IEEE 754 float overflow/NaN outputs under extreme drives[cite: 1].
///
/// ### Arguments/return describe
/// * `x` (`f32`): Unbounded drive sample input[cite: 1].
/// * **Returns** (`f32`): Soft-clipped dynamic signal constrained within the open interval $[-1.0, 1.0]$[cite: 1].
///
/// ### Mathematical explanation
/// Evaluates a high-order Padé approximant $[7/6]$ of $\tanh(x)$:
/// $$\tanh(x) \approx \frac{x(135135 + 17325x^2 + 378x^4 + x^6)}{135135 + 62370x^2 + 3150x^4 + 28x^6}$$
///
/// ### Describe sound processing
/// Introduces odd-harmonic saturation (3rd, 5th, 7th harmonics) to emulate analog tube driving while maintaining smooth saturation knee compression without calling expensive system math instructions (`std::f32::tanh`).
///
/// ### Sound processing use cases
/// Master channel limiting, synth overdrive, warm tape/tube simulation, and preventing filter self-oscillation runaway[cite: 1].
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

/// ### Rust behaviors
/// Branch-based scalar helper function. Takes mutable phase state passed by value (`mut t: f32`) for localized mutation[cite: 1]. Zero allocation, safe for real-time threads[cite: 1].
///
/// ### Arguments/return describe
/// * `t` (`f32`): Current phase step normalised in $t \in [0.0, 1.0)$[cite: 1].
/// * `dt` (`f32`): Phase increment equivalent to $f / f_s$[cite: 1].
/// * **Returns** (`f32`): Continuous residual offset added to raw naive waveform[cite: 1].
///
/// ### Mathematical explanation
/// Computes Poly-BLEP (Polynomial Band-Limited Step) residual correction polynomials to smooth sharp discontinuities. Near $t=0$:
/// $$p(t) = 2t - t^2 - 1$$
/// Near $t=1$:
/// $$p(t) = t^2 + 2t + 1$$
///
/// ### Describe sound processing
/// Replaces mathematical step-discontinuities with continuous 2nd-order polynomial splines, removing high-frequency spectral folding (aliasing) above the Nyquist limit ($f_s / 2$).
///
/// ### Sound processing use cases
/// Aliasing-free subtractive synthesis signal sources (Sawtooth, Square waves) generated in real-time.
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
    /// ### Rust behaviors
    /// Constructor allocating value directly on stack frame[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Default initialized PolyBlepOscillator[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Sets initial phase $\phi = 0.0$ and phase delta $\Delta\phi = 0.0$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Instantiates silent voice signal generator awaiting frequency assignments[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Initializing polyphonic voice pools in audio engine.
    pub fn new() -> Self {
        Self {
            waveform: Waveform::Sawtooth,
            phase: 0.0,
            phase_inc: 0.0,
        }
    }

    /// ### Rust behaviors
    /// Mutates encapsulated internal struct parameters in-place[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `freq` (`f32`): Target oscillator pitch frequency in Hertz (Hz)[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Computes phase step normalized across sample rate:
    /// $$\Delta\phi = \frac{f_{\text{target}}}{f_s}$$
    ///
    /// ### Describe sound processing
    /// Converts musical fundamental frequency into digital frame increment parameter.
    ///
    /// ### Sound processing use cases
    /// Portamento pitch slides, vibrato LFO modulation, MIDI key tracking.
    pub fn set_frequency(&mut self, freq: f32) {
        self.phase_inc = freq / SAMPLE_RATE;
    }

    /// ### Rust behaviors
    /// State-modifying function advancing internal phase accumulator per sample cycle[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`f32`): Generated single audio sample normalized to $[-1.0, 1.0]$[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Evaluates band-limited oscillator equation based on target waveform state and wraps phase modulo 1.0:
    /// $$\phi_{n+1} = (\phi_n + \Delta\phi) \pmod{1.0}$$
    ///
    /// ### Describe sound processing
    /// Generates audio time-series values matching selected geometric or sinusoidal vector functions with alias suppression applied[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Subtractive synth primary tone source.
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
    /// ### Rust behaviors
    /// Stack constructor assigning default resonant filter behavior[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Default lowpass filter instance[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Sets initial state registers $ic_{1eq} = 0.0, ic_{2eq} = 0.0$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Instantiates non-linear topology preserved filter component.
    ///
    /// ### Sound processing use cases
    /// Initializing channel strip tone shaping units.
    pub fn new() -> Self {
        Self {
            cutoff: 1000.0,
            resonance: 0.1,
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }

    /// ### Rust behaviors
    /// In-place parameter modification applying numerical safety clamps[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `cutoff` (`f32`): Corner frequency in Hz (clamped between 20Hz and Nyquist limit $0.49 \cdot f_s$)[cite: 1].
    /// * `res` (`f32`): Filter Q factor bound to $[0.0, 1.0]$[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Clamps input range to ensure linear filter stability prior to Bilinear Transform mapping[cite: 1]:
    /// $$f_c \in [20.0, 0.49 f_s]$$
    ///
    /// ### Describe sound processing
    /// Prevents filter explosion or infinite recursive feedback by restricting operational frequency boundary safely inside Nyquist space[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Safe automation mapping for user filter control knobs and envelope modulations[cite: 1].
    pub fn set_params(&mut self, cutoff: f32, res: f32) {
        self.cutoff = cutoff.clamp(20.0, SAMPLE_RATE * 0.49);
        self.resonance = res.clamp(0.0, 1.0);
    }

    /// ### Rust behaviors
    /// Modifies internal state storage variables (`ic1eq`, `ic2eq`) per sample frame[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `input` (`f32`): Raw incoming audio sample[cite: 1].
    /// * **Returns** (`f32`): Lowpass filtered audio sample[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Implements Andrew Simper's State Variable Filter via Trapezoidal Integration (Bilinear Transform)[cite: 1]:
    /// $$g = \tan\left(\frac{\pi f_c}{f_s}\right), \quad k = 2 - 2r$$
    /// $$v_1 = a_1 ic_{1eq} + a_2 (x - ic_{2eq}), \quad v_2 = ic_{2eq} + a_2 ic_{1eq} + a_3 (x - ic_{2eq})$$
    ///
    /// ### Describe sound processing
    /// Provides zero-delay feedback (ZDF) topology, preventing frequency warping artifacts and unstable self-oscillation at low/high cutoff thresholds.
    ///
    /// ### Sound processing use cases
    /// Classic analog-style lowpass filtering, dynamic sweeps, resonant synthesis sound shaping.
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
    /// ### Rust behaviors
    /// Creates a default baseline ADSR envelope state machine on the stack[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Default ADSR settings initialized to `Idle`[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Assigns time periods $T_A = 10\text{ms}, T_D = 100\text{ms}, L_S = 0.7, T_R = 200\text{ms}$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Instantiates dynamic envelope controller for amplitude/filter contouring[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Default setup for envelope controls.
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

    /// ### Rust behaviors
    /// Mutates envelope state directly to `Attack` stage[cite: 1].
    ///
    /// ### Arguments/return describe
    /// Void state update[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Sets state machine index trigger transition sequence[cite: 1].
    ///
    /// ### Describe sound processing
    /// Begins dynamic gain ramp upon receiving key press or gate signal[cite: 1].
    ///
    /// ### Sound processing use cases
    /// MIDI NoteOn handling[cite: 1].
    pub fn trigger_on(&mut self) {
        self.state = AdsrState::Attack;
    }

    /// ### Rust behaviors
    /// Mutates envelope state directly to `Release` stage[cite: 1].
    ///
    /// ### Arguments/return describe
    /// Void state update[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Switches interpolation target trajectory toward absolute zero ($0.0$)[cite: 1].
    ///
    /// ### Describe sound processing
    /// Initiates exponential or linear decay toward silence upon key release[cite: 1].
    ///
    /// ### Sound processing use cases
    /// MIDI NoteOff handling[cite: 1].
    pub fn trigger_off(&mut self) {
        self.state = AdsrState::Release;
    }

    /// ### Rust behaviors
    /// State machine frame step evaluator mutating `current_val` and state transitions[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`f32`): Current amplitude control scalar in range $[0.0, 1.0]$[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Evaluates discrete linear increment steps per sample interval $\Delta t = \frac{1}{f_s}$:
    /// $$\Delta y_{\text{attack}} = \frac{\Delta t}{T_A}, \quad \Delta y_{\text{decay}} = \frac{(1 - L_S)\Delta t}{T_D}, \quad \Delta y_{\text{release}} = \frac{L_S \Delta t}{T_R}$$
    ///
    /// ### Describe sound processing
    /// Generates piece-wise linear control signals used to dynamically modulate gain or filter cutoffs over sound duration.
    ///
    /// ### Sound processing use cases
    /// Dynamic volume contouring (anti-pop ramping), filter sweep generation[cite: 1].
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
    /// ### Rust behaviors
    /// Compositional structure constructor assembling sub-DSP modules on the stack[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Complete initialized monophonic synthesizer engine instance[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Aggregates linear system state equations across submodules[cite: 1].
    ///
    /// ### Describe sound processing
    /// Builds full subtractive processing chain: Oscillator -> Envelope -> Filter -> Overdrive[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Instantiating voice channels inside polyphonic voice allocators.
    pub fn new() -> Self {
        Self {
            osc: PolyBlepOscillator::new(),
            filter: SvfFilter::new(),
            env: Adsr::new(),
            master_overdrive: 1.0,
        }
    }

    /// ### Rust behaviors
    /// Implements safe pattern matching on event enum structures, mutating internal module properties without dynamic memory allocations[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `event` (`&SequencerEvent`): Read-only reference to target sequencer command node[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Updates scalar coefficients instantly across filter and envelope difference equations[cite: 1].
    ///
    /// ### Describe sound processing
    /// Controls pitch, parameters, envelope states, and timbre parameters dynamically in audio thread real-time[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Responding to MIDI messages, parameter automation, and sequencer playback commands[cite: 1].
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

    /// ### Rust behaviors
    /// Monotonic per-sample calculation pipeline executing inline sequential mutable mutations[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`f32`): Rendered output audio sample[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Evaluates compound transfer function:
    /// $$y[n] = 0.5 \cdot \tanh\left( \mathcal{H}_{\text{SVF}}\Big( \mathcal{S}_{\text{osc}}[n] \cdot \mathcal{E}_{\text{ADSR}}[n] \Big) \cdot \alpha_{\text{drive}} \right)$$
    ///
    /// ### Describe sound processing
    /// Executes full subtractive synth rendering pipeline: phase generation, amplitude envelope scaling, state-variable filtering, soft saturation clipping, and global headroom attenuation ($0.5x$).
    ///
    /// ### Sound processing use cases
    /// Main sample processing loop inside real-time audio callback buffers (`cpal` / `JACK` / `portaudio`)[cite: 1].
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

impl Default for EventNode {
    /// ### Rust behaviors
    /// Implements standard `Default` trait allowing stack array pre-initialization without dynamic vector allocation (`Vec`)[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Zero-initialized event container node[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Zeroes out event timestamp counter ($t = 0$)[cite: 1].
    ///
    /// ### Describe sound processing
    /// Acts as inert placeholder inside sequencer event array[cite: 1].
    ///
    /// ### Sound processing use cases
    /// Fixed memory buffer allocation for deterministic real-time audio playback engines[cite: 1].
    fn default() -> Self {
        EventNode {
            timestamp: 0,
            event: SequencerEvent::None,
        }
    }
}

pub struct Sequencer {
    events: [EventNode; MAX_EVENTS],
    event_count: usize,
    current_index: usize,
    sample_counter: usize,
}

impl Sequencer {
    /// ### Rust behaviors
    /// Stack-allocates fixed size array memory (`[EventNode; 2048]`), satisfying zero-allocation realtime requirements[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * **Returns** (`Self`): Clear sequencer initialized at zero ticks[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Resets discrete time sample counter $n = 0$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Pre-allocates deterministic memory grid for timing accurate MIDI style sequencing.
    ///
    /// ### Sound processing use cases
    /// Initializing DAW pattern sequencers and playback engines.
    pub fn new() -> Self {
        Self {
            events: [EventNode::default(); MAX_EVENTS],
            event_count: 0,
            current_index: 0,
            sample_counter: 0,
        }
    }

    /// ### Rust behaviors
    /// Bounds-checked insertion function mutating array slots up to `MAX_EVENTS` without dynamic expansion[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `timestamp` (`usize`): Target playback frame time index[cite: 1].
    /// * `event` (`SequencerEvent`): Payload event enum to execute[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Appends event tuple at temporal position $t = \text{timestamp}$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Schedules note triggers and parameter updates at sample-accurate time locations.
    ///
    /// ### Sound processing use cases
    /// Recording user inputs, loading patterns from disk, scheduling automation clips.
    pub fn add_event(&mut self, timestamp: usize, event: SequencerEvent) {
        if self.event_count < MAX_EVENTS {
            self.events[self.event_count] = EventNode { timestamp, event };
            self.event_count += 1;
        }
    }

    /// ### Rust behaviors
    /// Sample-accurate process tick evaluator that mutates active sequencer indices and calls external `SynthVoice` state[cite: 1].
    ///
    /// ### Arguments/return describe
    /// * `voice` (`&mut SynthVoice`): Reference to target synth engine to control[cite: 1].
    /// * **Returns** (`bool`): `true` if pattern active, `false` upon reaching `EndOfPattern`[cite: 1].
    ///
    /// ### Mathematical explanation
    /// Increments sample clock discrete index $n \leftarrow n + 1$ and compares against event timestamps $t_i = n$[cite: 1].
    ///
    /// ### Describe sound processing
    /// Guarantees sample-accurate event triggering without timing jitter or buffer block alignment latency errors.
    ///
    /// ### Sound processing use cases
    /// Main loop event dispatch engine for DAW playback channels.
    pub fn process_tick(&mut self, voice: &mut SynthVoice) -> bool {
        let mut active = true;

        while self.current_index < self.event_count
            && self.events[self.current_index].timestamp == self.sample_counter
        {
            let ev = &self.events[self.current_index].event;
            if let SequencerEvent::EndOfPattern = ev {
                active = false;
            } else {
                voice.apply_event(ev);
            }
            self.current_index += 1;
        }

        self.sample_counter += 1;
        active
    }
}

/// ### Rust behaviors
/// Entry point function orchestrating test loop render to disk using buffered I/O stream (`BufWriter`)[cite: 1].
///
/// ### Arguments/return describe
/// * **Returns**: Execution status (`()`)[cite: 1].
///
/// ### Mathematical explanation
/// Converts 32-bit floating point output values directly into 4-byte Little-Endian raw binary PCM byte arrays[cite: 1].
///
/// ### Describe sound processing
/// Offline sample-by-sample audio rendering pipeline writing 32-bit float audio file to disk[cite: 1].
///
/// ### Sound processing use cases
/// Offline DAW audio rendering, bounce-to-disk functionality, testing unit audio outputs.
fn main() {
    println!("VelOps POC substractive synthetiser...");

    let mut synth = SynthVoice::new();
    let mut seq = Sequencer::new();
    let sec = SAMPLE_RATE as usize;

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

    seq.add_event(
        sec / 10,
        SequencerEvent::NoteOn {
            note_freq: 110.0,
            velocity: 1.0,
        },
    );
    seq.add_event(sec, SequencerEvent::NoteOff);

    seq.add_event(
        sec + (sec / 10),
        SequencerEvent::NoteOn {
            note_freq: 180.0,
            velocity: 1.0,
        },
    );
    seq.add_event(sec * 2, SequencerEvent::NoteOff);

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
    );
    seq.add_event(sec * 3, SequencerEvent::NoteOff);

    seq.add_event(sec * 4, SequencerEvent::EndOfPattern);

    let mut file = BufWriter::new(File::create("pattern.raw").expect("Failed to create file"));
    let mut running = true;

    while running {
        running = seq.process_tick(&mut synth);
        let sample = synth.process_sample();

        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    println!("Audio successfully written to 'pattern.raw'.");
    println!("Playback via bash: aplay -f FLOAT_LE -r 48000 -c 1 pattern.raw");
}
