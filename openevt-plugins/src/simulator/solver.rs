use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
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

    // --- Difference Detector Constants ---
    /// The positive voltage differential required by the difference detector to trigger an ON event.
    pub threshold_on: f32,

    /// The negative voltage differential required by the difference detector to trigger an OFF event.
    pub threshold_off: f32,
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
        let d_dphi_fe = fe_coeff_1 * i_pd[i] - fe_coeff_2 * (fe_exp_scale * state.dphi_fe[i]).exp();
        state.dphi_fe[i] += d_dphi_fe * dt;

        // 2. Front-End Autoregressive Noise Update
        state.noise_fe[i] += (noise_samples_fe[i] - state.noise_fe[i]) * ar_fe_scale;

        // Superposition of signal and noise at FE node
        let noisy_fe = state.dphi_fe[i] + state.noise_fe[i];

        // 3. Source Follower ODE Integration
        let d_dphi_o1 = o1_coeff_1
            * (params.i_sf
                - params.i_d2_t0
                    * ((params.zeta * state.dphi_o1[i] - noisy_fe) / o1_exp_denom).exp());
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
