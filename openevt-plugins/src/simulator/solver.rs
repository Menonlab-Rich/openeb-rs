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
}

impl Default for EvsParameters {
    fn default() -> Self {
        Self {
            a: 1.0,
            c_c: 1.0,
            c_lsf: 1.0,
            zeta: 1.2,
            v_t: 0.02585,
            i_d1_t0: 1.0,
            i_d2_t0: 1.0,
            i_sf: 1.0,
            dt: 0.01,
            tau_fe: 1.0,
            tau_o1: 1.0,
            dark_current: 0.0,
            noise_fe_std: 0.01,
            noise_o1_std: 0.01,
            threshold_on: 0.1,
            threshold_off: 0.1,
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
            ("i_d2_t0", self.i_d2_t0),
            ("i_sf", self.i_sf),
            ("dt", self.dt),
            ("tau_fe", self.tau_fe),
            ("tau_o1", self.tau_o1),
            ("threshold_on", self.threshold_on),
            ("threshold_off", self.threshold_off),
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
                return Err(format!("simulation parameter `{name}` must be non-negative"));
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
    pub pixel_index: usize,
    pub polarity: bool, // true for ON, false for OFF
}

/// Stateful event-camera simulator that consumes one photocurrent frame at a
/// time and emits the CD events generated at that frame's timestamp.
pub struct EvsSimulator {
    params: EvsParameters,
    state: EvsState,
    noise: NoiseGenerator,
}

/// Small deterministic Gaussian source. A fixed seed makes generated data
/// reproducible while still exercising the paper's temporal noise model.
struct NoiseGenerator {
    state: u64,
    spare: Option<f32>,
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

impl EvsSimulator {
    /// Creates a simulator for a sensor containing `num_pixels` pixels.
    pub fn new(num_pixels: usize, params: EvsParameters) -> Result<Self, String> {
        params.validate()?;
        Ok(Self {
            params,
            state: EvsState::new(num_pixels),
            noise: NoiseGenerator::new(),
        })
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
        let noise_fe_sigma = autoregressive_input_std(
            self.params.noise_fe_std,
            self.params.dt,
            self.params.tau_fe,
        );
        let noise_o1_sigma = autoregressive_input_std(
            self.params.noise_o1_std,
            self.params.dt,
            self.params.tau_o1,
        );
        let mut noise_samples_fe = Vec::with_capacity(photocurrents.len());
        let mut noise_samples_o1 = Vec::with_capacity(photocurrents.len());
        for _ in photocurrents {
            noise_samples_fe.push(self.noise.standard_normal() * noise_fe_sigma);
            noise_samples_o1.push(self.noise.standard_normal() * noise_o1_sigma);
        }
        let photocurrents: Vec<f32> = photocurrents
            .iter()
            .map(|current| current + self.params.dark_current)
            .collect();
        let mut events = Vec::new();
        step_forward_euler(
            &mut self.state,
            &self.params,
            timestamp,
            &photocurrents,
            &noise_samples_fe,
            &noise_samples_o1,
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
    pub fn new(num_pixels: usize) -> Self {
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
pub fn step_forward_euler(
    state: &mut EvsState,
    params: &EvsParameters,
    current_time: f32,
    i_pd: &[f32],             // Interpolated photocurrents for this exact time step
    noise_samples_fe: &[f32], // Random normal samples e(n) ~ N(0, sigma_fe)
    noise_samples_o1: &[f32], // Random normal samples e(n) ~ N(0, sigma_o1)
    events_out: &mut Vec<Event>,
) {
    let num_pixels = state.dphi_fe.len();
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

    for i in 0..num_pixels {
        // 1. Logarithmic Amplifier ODE Integration
        let fe_exponent = (fe_exp_scale * state.dphi_fe[i]).clamp(-80.0, 80.0);
        let d_dphi_fe = fe_coeff_1 * i_pd[i] - fe_coeff_2 * fe_exponent.exp();
        state.dphi_fe[i] += d_dphi_fe * dt;

        // 2. Front-End Autoregressive Noise Update
        state.noise_fe[i] += (noise_samples_fe[i] - state.noise_fe[i]) * ar_fe_scale;

        // Superposition of signal and noise at FE node
        let noisy_fe = state.dphi_fe[i] + state.noise_fe[i];

        // 3. Source Follower ODE Integration
        let d_dphi_o1 = o1_coeff_1
            * (params.i_sf
                - params.i_d2_t0
                    * (((params.zeta * state.dphi_o1[i] - noisy_fe) / o1_exp_denom)
                        .clamp(-80.0, 80.0)
                        .exp()));
        state.dphi_o1[i] += d_dphi_o1 * dt;

        // 4. Source Follower Autoregressive Noise Update
        state.noise_o1[i] += (noise_samples_o1[i] - state.noise_o1[i]) * ar_o1_scale;

        // Superposition of signal and noise at SF node
        let noisy_o1 = state.dphi_o1[i] + state.noise_o1[i];

        // 5. Difference Detector (Comparator) Evaluation
        let voltage_diff = noisy_o1 - state.ref_voltage[i];

        if voltage_diff >= params.threshold_on {
            events_out.push(Event {
                timestamp: current_time,
                pixel_index: i,
                polarity: true,
            });
            // Reset reference voltage upon firing
            state.ref_voltage[i] = noisy_o1;
        } else if voltage_diff <= -params.threshold_off {
            events_out.push(Event {
                timestamp: current_time,
                pixel_index: i,
                polarity: false,
            });
            // Reset reference voltage upon firing
            state.ref_voltage[i] = noisy_o1;
        }
    }
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
