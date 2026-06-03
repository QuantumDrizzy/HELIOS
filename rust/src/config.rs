//! HELIOS-NODE — Configuration Loader

use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct HeliosConfig {
    pub station:    StationConfig,
    pub panel:      PanelConfig,
    pub battery:    BatteryConfig,
    pub controller: ControllerConfig,
    pub economic:   EconomicConfig,
    #[serde(default)]
    pub hardware:   HardwareConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StationConfig {
    pub id:              String,
    pub location:        String,
    /// Station elevation — part of the TOML schema; not consumed by the sim yet.
    #[allow(dead_code)]
    pub elevation_m:     f64,
    pub solar_data_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PanelConfig {
    /// Panel brand — metadata, part of the TOML schema; not consumed by the sim.
    #[allow(dead_code)]
    pub brand: String,
    pub model: String,
    pub voc:   f64,
    pub isc:   f64,
    pub pmax:  f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BatteryConfig {
    pub chemistry:       String,
    pub capacity_ah:     f64,
    pub nominal_voltage: f64,
    pub max_soc:         f64,
    pub min_soc:         f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ControllerConfig {
    pub tick_rate_ms:   u64,
    pub mode:           String,
    pub mppt_algorithm: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EconomicConfig {
    pub peak_price:     f64,
    pub off_peak_price: f64,
    pub currency:       String,
}

/// Hardware peripheral configuration (Raspberry Pi / Linux target).
/// All fields have safe defaults so the section is optional in the TOML
/// for simulation-only deployments.
#[derive(Debug, Deserialize, Clone)]
pub struct HardwareConfig {
    /// Linux I2C bus number — /dev/i2c-{i2c_bus}
    pub i2c_bus:           u8,
    /// INA219 I2C address (default 0x40)
    pub ina219_address:    u16,
    /// Shunt resistor value in ohms (sets current measurement range)
    pub ina219_shunt_ohms: f64,
    /// Panel STC rated power in watts (used to normalise irradiance proxy)
    pub panel_stc_watts:   f64,
    /// BCM GPIO pin for Surge Protection Device status (active HIGH = ok)
    pub spd_pin:           u8,
    /// BCM GPIO pin for DC breaker/relay closed status (active HIGH = closed)
    pub breaker_pin:       u8,
    /// BCM GPIO pin for hardware PWM output (18 = PWM0, 19 = PWM1)
    pub pwm_pin:           u8,
    /// PWM carrier frequency in Hz (25 kHz is typical for DC-DC converters)
    pub pwm_frequency_hz:  u32,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            i2c_bus:           1,
            ina219_address:    0x40,
            ina219_shunt_ohms: 0.1,
            panel_stc_watts:   300.0,
            spd_pin:           17,
            breaker_pin:       27,
            pwm_pin:           18,
            pwm_frequency_hz:  25_000,
        }
    }
}

impl HeliosConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: HeliosConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid TOML without a [hardware] section — defaults must apply.
    const TOML_NO_HW: &str = r#"
[station]
id = "TEST-01"
location = "Test City"
elevation_m = 10.0
solar_data_path = ""

[panel]
brand = "TestBrand"
model = "TestModel"
voc = 48.0
isc = 10.0
pmax = 400.0

[battery]
chemistry = "LiFePO4"
capacity_ah = 100.0
nominal_voltage = 48.0
max_soc = 0.95
min_soc = 0.15

[controller]
mode = "simulation"
tick_rate_ms = 100
mppt_algorithm = "perturb_and_observe"

[economic]
peak_price = 0.25
off_peak_price = 0.15
currency = "EUR"
"#;

    #[test]
    fn test_load_real_config_file() {
        // Cargo runs tests from the crate root (rust/), so ../helios_config.toml
        // resolves to the project root.
        let cfg = HeliosConfig::load("../helios_config.toml")
            .expect("helios_config.toml must parse");

        assert_eq!(cfg.station.location, "Murcia, Spain");
        assert!(cfg.panel.pmax > 0.0, "panel pmax must be positive");
        assert_eq!(cfg.controller.mppt_algorithm, "perturb_and_observe");
        assert_eq!(cfg.controller.tick_rate_ms, 100);
        assert!(cfg.battery.max_soc > cfg.battery.min_soc);
    }

    #[test]
    fn test_hardware_defaults_when_section_absent() {
        let cfg: HeliosConfig = toml::from_str(TOML_NO_HW)
            .expect("TOML_NO_HW must parse");

        let hw = &cfg.hardware;
        assert_eq!(hw.i2c_bus, 1);
        assert_eq!(hw.ina219_address, 0x40);
        assert!((hw.ina219_shunt_ohms - 0.1).abs() < 1e-9);
        assert!((hw.panel_stc_watts - 300.0).abs() < 1e-9);
        assert_eq!(hw.spd_pin, 17);
        assert_eq!(hw.breaker_pin, 27);
        assert_eq!(hw.pwm_pin, 18);
        assert_eq!(hw.pwm_frequency_hz, 25_000);
    }

    #[test]
    fn test_hardware_section_overrides_defaults() {
        let toml_with_hw = format!(
            "{}\n[hardware]\ni2c_bus = 2\nina219_address = 0x41\npanel_stc_watts = 450.0\n\
             ina219_shunt_ohms = 0.01\nspd_pin = 22\nbreaker_pin = 23\npwm_pin = 12\npwm_frequency_hz = 20000",
            TOML_NO_HW
        );
        let cfg: HeliosConfig = toml::from_str(&toml_with_hw)
            .expect("TOML with [hardware] must parse");

        assert_eq!(cfg.hardware.i2c_bus, 2);
        assert_eq!(cfg.hardware.ina219_address, 0x41);
        assert!((cfg.hardware.panel_stc_watts - 450.0).abs() < 1e-9);
        assert_eq!(cfg.hardware.pwm_frequency_hz, 20_000);
    }
}
