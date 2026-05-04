//! HELIOS — Predictive MPPT Controller
//! 
//! This module implements the microsecond-scale control loop.
//! It combines classic P&O (Perturb and Observe) with the 
//! AI-driven irradiance forecast.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct PowerState {
    pub voltage: f64,    // Volts
    pub current: f64,    // Amps
    pub power: f64,      // Watts
    pub duty_cycle: f64, // PWM [0, 1]
}

pub struct PredictiveController {
    current_state: PowerState,
    forecast_irradiance: f64, // From DRL Agent [0, 1]
    last_update: Instant,
}

impl PredictiveController {
    pub fn new() -> Self {
        Self {
            current_state: PowerState {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
                duty_cycle: 0.5,
            },
            forecast_irradiance: 1.0,
            last_update: Instant::now(),
        }
    }

    /// Update the irradiance forecast from the Python DRL Agent
    pub fn update_forecast(&mut self, forecast: f64) {
        self.forecast_irradiance = forecast;
    }

    /// Main control tick (Target: 100 microseconds)
    pub fn tick(&mut self, measured_v: f64, measured_i: f64) -> f64 {
        let new_power = measured_v * measured_i;
        let power_delta = new_power - self.current_state.power;
        
        // Predictive Adjustment:
        // If the AI predicts a cloud (low forecast), we preemptively
        // shift the impedance point to avoid a voltage collapse.
        let base_step = 0.01;
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

        self.current_state.duty_cycle
    }
}

pub async fn run_control_loop() {
    let mut controller = PredictiveController::new();
    let mut interval = tokio::time::interval(Duration::from_micros(100));

    loop {
        interval.tick().await;
        // In reality, this would read from ADC and write to PWM via GPIO/SPI
        let (v, i) = (48.0, 5.0); // Synthetic measurement
        let _new_duty = controller.tick(v, i);
        
        // Tracing at lower frequency for debugging
        if v as i32 % 10 == 0 {
            // tracing::debug!("MPPT Tick: Power={:.2}W, Forecast={:.2}", v*i, controller.forecast_irradiance);
        }
    }
}
