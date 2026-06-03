//! HELIOS-NODE — Predictive MPPT Controller
//! 
//! Combines classic P&O (Perturb and Observe) with AI-driven
//! irradiance forecast via SQLite IPC bridge.

use std::time::Instant;
use crate::config::{PanelConfig, EconomicConfig, BatteryConfig};

#[derive(Debug, Clone, Copy)]
pub struct PowerState {
    pub voltage: f64,    // Volts
    pub current: f64,    // Amps
    pub power: f64,      // Watts
    pub duty_cycle: f64, // PWM [0, 1]
    pub soc: f64,        // State of Charge [0, 1]
}

impl Default for PowerState {
    fn default() -> Self {
        Self {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
            duty_cycle: 0.5,
            soc: 0.5, // Start at 50%
        }
    }
}

pub struct PredictiveController {
    current_state: PowerState,
    forecast_irradiance: f64,
    last_update: Instant,
    tick_count: u64,
    /// Panel electrical specs — retained for panel-aware MPPT limits (not yet wired).
    #[allow(dead_code)]
    panel_config: PanelConfig,
    economic_config: EconomicConfig,
    battery_config: BatteryConfig,
}

impl PredictiveController {
    pub fn new(panel_config: PanelConfig, economic_config: EconomicConfig, battery_config: BatteryConfig) -> Self {
        Self {
            current_state: PowerState::default(),
            forecast_irradiance: 1.0,
            last_update: Instant::now(),
            tick_count: 0,
            panel_config,
            economic_config,
            battery_config,
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

    /// Get total tick count (telemetry helper; not consumed in the current loop).
    #[allow(dead_code)]
    pub fn ticks(&self) -> u64 {
        self.tick_count
    }

    /// Estimate current electricity tariff based on config
    pub fn estimate_tariff_chf(&self, sim_hour: f64) -> f64 {
        let hour = sim_hour % 24.0;
        if hour >= 7.0 && hour < 20.0 {
            self.economic_config.peak_price
        } else {
            self.economic_config.off_peak_price
        }
    }

    /// Main control tick
    pub fn tick(&mut self, measured_v: f64, measured_i: f64, sim_hour: f64) -> f64 {
        let new_power = measured_v * measured_i;
        let power_delta = new_power - self.current_state.power;
        
        // Update SOC (State of Charge)
        // Energy = Power * Time (0.1s)
        let energy_wh = new_power * (0.1 / 3600.0);
        let total_capacity_wh = self.battery_config.capacity_ah * self.battery_config.nominal_voltage;
        let soc_delta = energy_wh / total_capacity_wh;
        
        self.current_state.soc = (self.current_state.soc + soc_delta).clamp(0.0, 1.0);

        // Overcharge protection: if SOC > max_soc, reduce duty cycle to stop charging
        if self.current_state.soc >= self.battery_config.max_soc {
            self.current_state.duty_cycle = (self.current_state.duty_cycle - 0.05).max(0.05);
            return self.current_state.duty_cycle;
        }

        // Current tariff for economic bias
        let tariff = self.estimate_tariff_chf(sim_hour);
        
        // Base P&O step size
        let base_step = 0.01;
        
        // Predictive bias from AI forecast:
        // Only applied on the upward step — prevents low forecasts from
        // amplifying the downward step and locking duty at minimum.
        let predictive_bias = (1.0 - self.forecast_irradiance) * -0.05;
        let predictive_bias = predictive_bias.max(-base_step * 0.5); // clamp: never more than 50% of step

        // Economic bias: more aggressive if power is expensive
        let economic_bias = if tariff > 0.25 { 0.005 } else { 0.0 };

        // Perturb and Observe with AI & Economic Bias
        if power_delta > 0.0 {
            // Rising power: apply both biases (AI slows climb when forecast is low)
            self.current_state.duty_cycle += base_step + predictive_bias + economic_bias;
        } else {
            // Falling power: clean step down — no bias amplification
            self.current_state.duty_cycle -= base_step;
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BatteryConfig, EconomicConfig, PanelConfig};

    fn make_controller() -> PredictiveController {
        PredictiveController::new(
            PanelConfig {
                brand: "Test".into(), model: "Test".into(),
                voc: 48.0, isc: 10.0, pmax: 400.0,
            },
            EconomicConfig {
                peak_price: 0.25, off_peak_price: 0.15, currency: "EUR".into(),
            },
            BatteryConfig {
                chemistry: "LiFePO4".into(),
                capacity_ah: 200.0, nominal_voltage: 48.0,
                max_soc: 0.95, min_soc: 0.15,
            },
        )
    }

    #[test]
    fn test_po_increases_duty_when_power_rises() {
        let mut ctrl = make_controller();
        // Initial power is 0.0; first tick with real power → power_delta > 0
        let initial_duty = ctrl.state().duty_cycle; // 0.5
        let new_duty = ctrl.tick(48.0, 5.0, 12.0);  // 240 W at noon
        assert!(
            new_duty > initial_duty,
            "P&O must increase duty when power rises: {new_duty:.4} should be > {initial_duty:.4}"
        );
    }

    #[test]
    fn test_po_decreases_duty_when_power_falls() {
        let mut ctrl = make_controller();
        // Establish a high-power baseline
        ctrl.tick(48.0, 5.0, 12.0); // 240 W
        let mid_duty = ctrl.state().duty_cycle;
        // Simulate irradiance collapse
        let new_duty = ctrl.tick(10.0, 0.5, 12.0); // 5 W — big drop
        assert!(
            new_duty < mid_duty,
            "P&O must decrease duty when power falls: {new_duty:.4} should be < {mid_duty:.4}"
        );
    }

    #[test]
    fn test_duty_always_clamped_in_physical_range() {
        let mut ctrl = make_controller();
        for _ in 0..500 {
            let d = ctrl.tick(48.0, 5.0, 12.0);
            assert!(d >= 0.05 && d <= 0.95, "duty out of [0.05, 0.95]: {d}");
        }
    }

    #[test]
    fn test_overcharge_protection_reduces_duty() {
        let mut ctrl = make_controller();
        // Force SOC to max
        ctrl.current_state.soc = 0.96; // above max_soc=0.95
        let before = ctrl.state().duty_cycle;
        let after  = ctrl.tick(48.0, 5.0, 12.0);
        assert!(
            after < before,
            "Overcharge protection must decrease duty: {after:.4} < {before:.4}"
        );
    }

    #[test]
    fn test_forecast_update_clamps_to_unit_interval() {
        let mut ctrl = make_controller();
        ctrl.update_forecast(1.5);
        assert!((ctrl.forecast() - 1.0).abs() < 1e-9, "forecast must clamp to 1.0");
        ctrl.update_forecast(-0.3);
        assert!((ctrl.forecast() - 0.0).abs() < 1e-9, "forecast must clamp to 0.0");
    }
}

/// Simulates panel V/I output from irradiance using dynamic config parameters.
pub fn simulate_panel(irradiance: f64, duty_cycle: f64, config: &PanelConfig) -> (f64, f64) {
    // Photocurrent scales linearly with irradiance and Isc
    let i_photo = config.isc * irradiance;
    
    // Operating voltage depends on duty cycle (buck converter model)
    let v_op = config.voc * duty_cycle * (0.85 + 0.15 * irradiance);
    
    // Simple I-V curve
    let vt = 2.0; 
    let current = (i_photo * (1.0 - ((v_op - config.voc * irradiance) / vt).exp())).max(0.0);
    
    // Clamp to physical limits
    let voltage = v_op.clamp(0.0, config.voc);
    let current = current.clamp(0.0, config.isc);
    
    (voltage, current)
}
