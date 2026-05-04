//! HELIOS-NODE — Predictive MPPT Controller
//! 
//! Combines classic P&O (Perturb and Observe) with AI-driven
//! irradiance forecast via SQLite IPC bridge.

use std::time::Instant;

/// Standard 72-cell solar panel parameters
pub const PANEL_VOC: f64 = 48.0;   // Open-circuit voltage [V]
pub const PANEL_ISC: f64 = 10.0;   // Short-circuit current [A]
pub const PANEL_PMAX: f64 = 380.0; // Max power point [W]

#[derive(Debug, Clone, Copy)]
pub struct PowerState {
    pub voltage: f64,    // Volts
    pub current: f64,    // Amps
    pub power: f64,      // Watts
    pub duty_cycle: f64, // PWM [0, 1]
}

impl Default for PowerState {
    fn default() -> Self {
        Self {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
            duty_cycle: 0.5,
        }
    }
}

pub struct PredictiveController {
    current_state: PowerState,
    forecast_irradiance: f64,
    last_update: Instant,
    tick_count: u64,
}

impl PredictiveController {
    pub fn new() -> Self {
        Self {
            current_state: PowerState::default(),
            forecast_irradiance: 1.0,
            last_update: Instant::now(),
            tick_count: 0,
        }
    }

    /// Update the irradiance forecast from the Python AI Agent (via SQLite)
    pub fn update_forecast(&mut self, forecast: f64) {
        self.forecast_irradiance = forecast.clamp(0.0, 1.0);
    }

    /// Get current forecast value
    pub fn forecast(&self) -> f64 {
        self.forecast_irradiance
    }

    /// Get current power state
    pub fn state(&self) -> PowerState {
        self.current_state
    }

    /// Get total tick count
    pub fn ticks(&self) -> u64 {
        self.tick_count
    }

    /// Main control tick
    /// 
    /// Implements Perturb & Observe with predictive AI bias:
    /// - If AI predicts a cloud (low forecast), preemptively shift
    ///   the impedance point to avoid voltage collapse
    /// - Step size adapts to forecast confidence
    pub fn tick(&mut self, measured_v: f64, measured_i: f64) -> f64 {
        let new_power = measured_v * measured_i;
        let power_delta = new_power - self.current_state.power;
        
        // Base P&O step size
        let base_step = 0.01;
        
        // Predictive bias from AI forecast:
        // forecast = 1.0 → clear sky → no bias
        // forecast = 0.0 → full cloud → maximum preemptive shift
        let predictive_bias = (1.0 - self.forecast_irradiance) * -0.05;

        // Perturb and Observe with AI Bias
        if power_delta > 0.0 {
            self.current_state.duty_cycle += base_step + predictive_bias;
        } else {
            self.current_state.duty_cycle -= base_step - predictive_bias;
        }

        self.current_state.duty_cycle = self.current_state.duty_cycle.clamp(0.05, 0.95);
        self.current_state.voltage = measured_v;
        self.current_state.current = measured_i;
        self.current_state.power = new_power;
        self.last_update = Instant::now();
        self.tick_count += 1;

        self.current_state.duty_cycle
    }
}

/// Simulates panel V/I output from irradiance using simplified single-diode model.
///
/// irradiance: normalized [0, 1] (1.0 = 1000 W/m² STC)
/// duty_cycle: PWM duty [0, 1] — maps to operating point on I-V curve
///
/// Returns (voltage, current)
pub fn simulate_panel(irradiance: f64, duty_cycle: f64) -> (f64, f64) {
    // Photocurrent scales linearly with irradiance
    let i_photo = PANEL_ISC * irradiance;
    
    // Operating voltage depends on duty cycle (buck converter model)
    let v_op = PANEL_VOC * duty_cycle * (0.85 + 0.15 * irradiance);
    
    // Simple I-V curve: I = Iph * (1 - exp((V - Voc) / Vt))
    let vt = 2.0; // Thermal voltage equivalent (simplified)
    let current = (i_photo * (1.0 - ((v_op - PANEL_VOC * irradiance) / vt).exp())).max(0.0);
    
    // Clamp to physical limits
    let voltage = v_op.clamp(0.0, PANEL_VOC);
    let current = current.clamp(0.0, PANEL_ISC);
    
    (voltage, current)
}
