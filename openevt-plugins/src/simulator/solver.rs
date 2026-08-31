use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct EvsParameters {
    // --- Physical and Circuit Constants ---
    /// Gain of the inverting amplifier in the feedback path of the logarithmic amplifier.
    pub a: f32,

    /// Coupling capacitance in the front-end amplifier circuit
    pub c_c: f32,

    /// Load capacitance seen by the source follower buffer
    pub c_lsf: f32,

    /// Slope-factor for transistors operating in weak inversion (subthreshold swing factor), typically between 1.0 and 1.4
    pub zeta: f32,

    /// Thermal voltage, calculated as (k_B * \theta) / q, where \theta is absolute temperature.
    pub v_t: f32,

    /// Initial stationary drain current of the front-end transistor at time t0, matching the initial photocurrent
    pub i_d1_t0: f32,

    /// Multiplier applied to normalized video luminance before it is used as
    /// photocurrent. A value of 1.0 maps 8-bit luminance directly to [0, 1].
    pub photocurrent_scale: f32,

    /// Initial stationary drain current of the source follower transistor at time t0
    pub i_d2_t0: f32,

    /// DC bias current applied to the source follower buffer
    pub i_sf: f32,

    // --- Autoregressive & Solver Constants ---
    /// The discrete time-step used for the Forward-Euler numerical integration
    pub dt: f32,

    /// System time constant for the front-end node, determining the bandwidth of the autoregressive noise model.
    pub tau_fe: f32,

    /// System time constant for the source follower node, determining the bandwidth of the autoregressive noise model.
    pub tau_o1: f32,

    /// Dark photocurrent added to every input pixel (Eq. 4's `I_PD = I_photo + I_dark`).
    pub dark_current: f32,

    /// Stationary standard deviation of the front-end voltage noise.
    /// The corresponding white-noise input is derived from Eq. 23.
    pub noise_fe_std: f32,

    /// Stationary standard deviation of the source-follower voltage noise.
    /// The corresponding white-noise input is derived from Eq. 23.
    pub noise_o1_std: f32,

    // --- Difference Detector Constants ---
    /// The positive voltage differential required by the difference detector to trigger an ON event.
    pub threshold_on: f32,

    /// The negative voltage differential required by the difference detector to trigger an OFF event.
    pub threshold_off: f32,

    /// Shared AER arbiter throughput in event-equivalent pixels per second.
    /// Each active row consumes one full-row transfer from this budget.
    pub arbiter_throughput_events_per_second: f32,
}

impl Default for EvsParameters {
    fn default() -> Self {
        Self {
            a: 1.0,
            c_c: 1.0,
            c_lsf: 1.0,
            zeta: 1.2,
            v_t: 0.02585,
            // The bundled lightning video has an ordinary-frame luminance of
            // roughly 25/255, so a unit baseline treats nearly every pixel as
            // permanently below its operating point.
            i_d1_t0: 0.1,
            photocurrent_scale: 1.0,
            i_d2_t0: 1.0,
            i_sf: 1.0,
            dt: 0.01,
            tau_fe: 1.0,
            tau_o1: 1.0,
            dark_current: 0.0,
            noise_fe_std: 0.0001,
            noise_o1_std: 0.0001,
            threshold_on: 0.001,
            threshold_off: 0.001,
            arbiter_throughput_events_per_second: 20_000_000.0,
        }
    }
}

impl EvsParameters {
    /// Validates the physical and numerical constants before simulation starts.
    pub fn validate(&self) -> Result<(), String> {
        let values = [
            ("a", self.a),
            ("c_c", self.c_c),
            ("c_lsf", self.c_lsf),
            ("zeta", self.zeta),
            ("v_t", self.v_t),
            ("i_d1_t0", self.i_d1_t0),
            ("photocurrent_scale", self.photocurrent_scale),
            ("i_d2_t0", self.i_d2_t0),
            ("i_sf", self.i_sf),
            ("dt", self.dt),
            ("tau_fe", self.tau_fe),
            ("tau_o1", self.tau_o1),
            ("dark_current", self.dark_current),
            ("noise_fe_std", self.noise_fe_std),
            ("noise_o1_std", self.noise_o1_std),
            ("threshold_on", self.threshold_on),
            ("threshold_off", self.threshold_off),
            (
                "arbiter_throughput_events_per_second",
                self.arbiter_throughput_events_per_second,
            ),
        ];
        if let Some((name, _)) = values.iter().find(|(_, value)| !value.is_finite()) {
            return Err(format!("simulation parameter `{name}` must be finite"));
        }
        for (name, value) in [
            ("c_c", self.c_c),
            ("c_lsf", self.c_lsf),
            ("zeta", self.zeta),
            ("v_t", self.v_t),
            ("i_d1_t0", self.i_d1_t0),
            ("photocurrent_scale", self.photocurrent_scale),
            ("i_d2_t0", self.i_d2_t0),
            ("i_sf", self.i_sf),
            ("dt", self.dt),
            ("tau_fe", self.tau_fe),
            ("tau_o1", self.tau_o1),
            ("threshold_on", self.threshold_on),
            ("threshold_off", self.threshold_off),
            (
                "arbiter_throughput_events_per_second",
                self.arbiter_throughput_events_per_second,
            ),
        ] {
            if value <= 0.0 {
                return Err(format!("simulation parameter `{name}` must be positive"));
            }
        }
        for (name, value) in [
            ("dark_current", self.dark_current),
            ("noise_fe_std", self.noise_fe_std),
            ("noise_o1_std", self.noise_o1_std),
        ] {
            if value < 0.0 {
                return Err(format!(
                    "simulation parameter `{name}` must be non-negative"
                ));
            }
        }
        if self.dt > self.tau_fe || self.dt > self.tau_o1 {
            return Err("simulation time step must not exceed either noise time constant".into());
        }
        Ok(())
    }
}

pub struct EvsState {
    pub dphi_fe: Vec<f32>,
    pub dphi_o1: Vec<f32>,
    pub noise_fe: Vec<f32>,
    pub noise_o1: Vec<f32>,
    pub ref_voltage: Vec<f32>,
}

pub struct Event {
    pub timestamp: f32,
    pub pixel_index: u64,
    pub polarity: bool, // true for ON, false for OFF
}

/// Stateful event-camera simulator that consumes one photocurrent frame at a
/// time and emits the CD events generated at that frame's timestamp.
pub struct EvsSimulator {
    params: EvsParameters,
    state: EvsState,
    noise: NoiseGenerator,
    noise_samples: NoiseSamples,
}

/// Small deterministic Gaussian source. A fixed seed makes generated data
/// reproducible while still exercising the paper's temporal noise model.
struct NoiseGenerator {
    state: u64,
    spare: Option<f32>,
}

#[derive(Default)]
struct NoiseSamples {
    fe: Vec<f32>,
    o1: Vec<f32>,
}

impl NoiseGenerator {
    fn new() -> Self {
        Self {
            state: 0x8e3f_2a91_5c77_b4d1,
            spare: None,
        }
    }

    fn uniform(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state as f32 / u64::MAX as f32).clamp(f32::MIN_POSITIVE, 1.0)
    }

    fn standard_normal(&mut self) -> f32 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        let radius = (-2.0 * self.uniform().ln()).sqrt();
        let angle = 2.0 * std::f32::consts::PI * self.uniform();
        self.spare = Some(radius * angle.sin());
        radius * angle.cos()
    }
}

#[hotpath::measure_all]
impl EvsSimulator {
    /// Creates a simulator for a sensor containing `num_pixels` pixels.
    pub fn new(num_pixels: u64, params: EvsParameters) -> Result<Self, String> {
        params.validate()?;
        let num_pixels = pixel_count_to_capacity(num_pixels)?;
        Ok(Self {
            params,
            state: EvsState::with_capacity(num_pixels),
            noise: NoiseGenerator::new(),
            noise_samples: NoiseSamples::default(),
        })
    }

    /// Resets the circuit and noise state before replaying from a new time.
    pub fn reset(&mut self) {
        self.state = EvsState::with_capacity(self.state.dphi_fe.len());
        self.noise = NoiseGenerator::new();
        self.noise_samples = NoiseSamples::default();
    }

    /// Advances the model by one Forward-Euler step using one photocurrent
    /// frame and returns the generated CD events.
    pub fn process_frame(
        &mut self,
        photocurrents: &[f32],
        timestamp: f32,
    ) -> Result<Vec<Event>, String> {
        self.process_step(photocurrents, timestamp)
    }

    /// Holds a video frame for its wall-clock duration and advances the model
    /// at the paper's numerical time-step. This is the frame upsampling needed
    /// to avoid making the ODE dynamics depend on the encoded video FPS.
    pub fn process_frame_over_interval(
        &mut self,
        photocurrents: &[f32],
        timestamp_us: f32,
        duration_s: f32,
    ) -> Result<Vec<Event>, String> {
        if !duration_s.is_finite() || duration_s <= 0.0 {
            return Err("frame duration must be finite and positive".into());
        }
        let steps = (duration_s / self.params.dt).ceil() as usize;
        let mut events = Vec::new();
        for step in 0..steps.max(1) {
            let step_time_us = ((step + 1) as f32 * self.params.dt).min(duration_s) * 1_000_000.0;
            events.extend(self.process_step(photocurrents, timestamp_us + step_time_us)?);
        }
        Ok(events)
    }

    fn process_step(
        &mut self,
        photocurrents: &[f32],
        timestamp: f32,
    ) -> Result<Vec<Event>, String> {
        if photocurrents.len() != self.state.dphi_fe.len() {
            return Err(format!(
                "frame contains {} pixels, expected {}",
                photocurrents.len(),
                self.state.dphi_fe.len()
            ));
        }
        let mut events = Vec::new();
        step_forward_euler_with_noise(
            &mut self.state,
            &self.params,
            timestamp,
            photocurrents,
            &mut self.noise,
            &mut self.noise_samples,
            &mut events,
        );
        Ok(events)
    }
}

/// Converts the desired stationary standard deviation (Eq. 23) into the
/// standard deviation of the white-noise samples used by Eq. 19.
fn autoregressive_input_std(stationary_std: f32, dt: f32, tau: f32) -> f32 {
    if stationary_std == 0.0 {
        return 0.0;
    }
    let ratio = dt / tau;
    stationary_std * (1.0 - (1.0 - ratio).powi(2)).sqrt() / ratio
}

impl EvsState {
    pub fn new(num_pixels: u64) -> Result<Self, String> {
        Ok(Self::with_capacity(pixel_count_to_capacity(num_pixels)?))
    }

    fn with_capacity(num_pixels: usize) -> Self {
        Self {
            dphi_fe: vec![0.0; num_pixels],
            dphi_o1: vec![0.0; num_pixels],
            noise_fe: vec![0.0; num_pixels],
            noise_o1: vec![0.0; num_pixels],
            ref_voltage: vec![0.0; num_pixels],
        }
    }
}

/// Executes a single microsecond Forward-Euler time step across the entire sensor array.
#[hotpath::measure]
pub fn step_forward_euler(
    state: &mut EvsState,
    params: &EvsParameters,
    current_time: f32,
    i_pd: &[f32],             // Interpolated photocurrents for this exact time step
    noise_samples_fe: &[f32], // Random normal samples e(n) ~ N(0, sigma_fe)
    noise_samples_o1: &[f32], // Random normal samples e(n) ~ N(0, sigma_o1)
    events_out: &mut Vec<Event>,
) {
    step_forward_euler_impl(
        state,
        params,
        current_time,
        i_pd,
        noise_samples_fe,
        noise_samples_o1,
        events_out,
    );
}

/// Generates the frame's noise samples serially, then updates all pixels in
/// parallel. Keeping the RNG serial preserves the simulator's deterministic
/// noise stream while leaving the expensive pixel math embarrassingly parallel.
fn step_forward_euler_with_noise(
    state: &mut EvsState,
    params: &EvsParameters,
    current_time: f32,
    photocurrents: &[f32],
    noise: &mut NoiseGenerator,
    samples: &mut NoiseSamples,
    events_out: &mut Vec<Event>,
) {
    let noise_fe_sigma = autoregressive_input_std(params.noise_fe_std, params.dt, params.tau_fe);
    let noise_o1_sigma = autoregressive_input_std(params.noise_o1_std, params.dt, params.tau_o1);
    samples.fe.resize(photocurrents.len(), 0.0);
    samples.o1.resize(photocurrents.len(), 0.0);
    for (fe, o1) in samples.fe.iter_mut().zip(samples.o1.iter_mut()) {
        *fe = noise.standard_normal() * noise_fe_sigma;
        *o1 = noise.standard_normal() * noise_o1_sigma;
    }
    step_forward_euler_impl(
        state,
        params,
        current_time,
        photocurrents,
        &samples.fe,
        &samples.o1,
        events_out,
    );
}

fn step_forward_euler_impl(
    state: &mut EvsState,
    params: &EvsParameters,
    current_time: f32,
    i_pd: &[f32],
    noise_samples_fe: &[f32],
    noise_samples_o1: &[f32],
    events_out: &mut Vec<Event>,
) {
    let dt = params.dt;

    // Precompute invariant coefficients for Equation 5
    let fe_coeff_1 = params.a / ((1.0 + params.a) * params.c_c);
    let fe_coeff_2 = (params.a * params.i_d1_t0) / ((1.0 + params.a) * params.c_c);
    let fe_exp_scale = (params.zeta + params.a) / (params.a * params.zeta * params.v_t);

    // Precompute invariant coefficients for Equation 6
    let o1_coeff_1 = 1.0 / params.c_lsf;
    let o1_exp_denom = params.zeta * params.v_t;

    // Precompute autoregressive noise coefficients (Equation 19)
    let ar_fe_scale = dt / params.tau_fe;
    let ar_o1_scale = dt / params.tau_o1;

    let mut events: Vec<Event> = state
        .dphi_fe
        .par_iter_mut()
        .zip(state.dphi_o1.par_iter_mut())
        .zip(state.noise_fe.par_iter_mut())
        .zip(state.noise_o1.par_iter_mut())
        .zip(state.ref_voltage.par_iter_mut())
        .zip(i_pd.par_iter())
        .zip(noise_samples_fe.par_iter())
        .zip(noise_samples_o1.par_iter())
        .enumerate()
        .map(
            |(
                i,
                (
                    (
                        (((((dphi_fe, dphi_o1), noise_fe), noise_o1), ref_voltage), photocurrent),
                        noise_sample_fe,
                    ),
                    noise_sample_o1,
                ),
            )| {
                let photocurrent = *photocurrent + params.dark_current;

                // 1. Logarithmic Amplifier ODE Integration
                let fe_exponent = (fe_exp_scale * *dphi_fe).clamp(-80.0, 80.0);
                let d_dphi_fe = fe_coeff_1 * photocurrent - fe_coeff_2 * fe_exponent.exp();
                *dphi_fe += d_dphi_fe * dt;

                // 2. Front-End Autoregressive Noise Update
                *noise_fe += (*noise_sample_fe - *noise_fe) * ar_fe_scale;

                // Superposition of signal and noise at FE node
                let noisy_fe = *dphi_fe + *noise_fe;

                // 3. Source Follower ODE Integration
                let d_dphi_o1 = o1_coeff_1
                    * (params.i_sf
                        - params.i_d2_t0
                            * (((params.zeta * *dphi_o1 - noisy_fe) / o1_exp_denom)
                                .clamp(-80.0, 80.0)
                                .exp()));
                *dphi_o1 += d_dphi_o1 * dt;

                // 4. Source Follower Autoregressive Noise Update
                *noise_o1 += (*noise_sample_o1 - *noise_o1) * ar_o1_scale;

                // Superposition of signal and noise at SF node
                let noisy_o1 = *dphi_o1 + *noise_o1;

                // 5. Difference Detector (Comparator) Evaluation
                let voltage_diff = noisy_o1 - *ref_voltage;

                if voltage_diff >= params.threshold_on {
                    *ref_voltage = noisy_o1;
                    Some(Event {
                        timestamp: current_time,
                        pixel_index: i as u64,
                        polarity: true,
                    })
                } else if voltage_diff <= -params.threshold_off {
                    *ref_voltage = noisy_o1;
                    Some(Event {
                        timestamp: current_time,
                        pixel_index: i as u64,
                        polarity: false,
                    })
                } else {
                    None
                }
            },
        )
        .flatten()
        .collect();

    // Model finite AER bandwidth by randomly sampling the requests that can
    // be transmitted during this integration step. The ranking is generated
    // from the event identity and timestamp rather than shared mutable RNG
    // state, so the result remains deterministic under parallel execution.
    let max_events = (params.arbiter_throughput_events_per_second * dt).floor() as usize;
    if events.len() > max_events {
        events.sort_unstable_by_key(|event| {
            let mut value = event.pixel_index as u64;
            value ^= (event.timestamp.to_bits() as u64).rotate_left(21);
            value ^= (event.polarity as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            splitmix64(value)
        });
        events.truncate(max_events);
    }
    events_out.extend(events);
}

fn pixel_count_to_capacity(num_pixels: u64) -> Result<usize, String> {
    usize::try_from(num_pixels).map_err(|_| "pixel count is too large for this platform".to_owned())
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_simulator_accepts_a_frame() {
        let mut simulator = EvsSimulator::new(2, EvsParameters::default()).unwrap();
        assert!(simulator.process_frame(&[0.0, 1.0], 42.0).is_ok());
    }

    #[test]
    fn simulator_rejects_a_frame_with_wrong_dimensions() {
        let mut simulator = EvsSimulator::new(2, EvsParameters::default()).unwrap();
        assert!(simulator.process_frame(&[1.0], 42.0).is_err());
    }
}
