//! HELIOS-NODE — Hardware Abstraction Layer (HAL)
//!
//! All real hardware (rppal GPIO / I2C / PWM) is gated behind
//! `#[cfg(target_os = "linux")]` so the crate builds on Windows in
//! pure-simulation mode without any native dependencies.

use crate::config::{HardwareConfig, PanelConfig};
use crate::controller::simulate_panel;

// Bring rppal types into scope only on Linux.
#[cfg(target_os = "linux")]
use rppal::{
    gpio::{Gpio, InputPin},
    i2c::I2c,
    pwm::{Channel, Polarity, Pwm},
};

// ─── INA219 register map ──────────────────────────────────────────────────────
const INA219_REG_SHUNT:  u8 = 0x01; // Shunt voltage  (i16, 10 µV / LSB)
const INA219_REG_BUS:    u8 = 0x02; // Bus voltage    (u16, bits[15:3], 4 mV / LSB)
const INA219_REG_POWER:  u8 = 0x03; // Power          (u16, depends on calibration)
const INA219_REG_CURRENT:u8 = 0x04; // Current        (i16, depends on calibration)
const INA219_REG_CAL:    u8 = 0x05; // Calibration    (u16, write once at init)

// ─── PowerSensor trait ───────────────────────────────────────────────────────

/// Universal sensor interface — implemented by both real hardware and simulator.
pub trait PowerSensor {
    /// Returns (voltage_v, current_a) for the current tick.
    fn read_telemetry(&mut self, duty_cycle: f64, config: &PanelConfig) -> (f64, f64);

    /// Feed an irradiance value into a simulated sensor.
    /// Real-hardware implementations use the default no-op.
    fn set_irradiance(&mut self, _irradiance: f64) {}
}

// ─── SimulatedSensor ─────────────────────────────────────────────────────────

pub struct SimulatedSensor {
    pub current_irradiance: f64,
}

impl SimulatedSensor {
    pub fn new() -> Self {
        Self { current_irradiance: 1.0 }
    }
}

impl PowerSensor for SimulatedSensor {
    fn read_telemetry(&mut self, duty_cycle: f64, config: &PanelConfig) -> (f64, f64) {
        simulate_panel(self.current_irradiance, duty_cycle, config)
    }
    fn set_irradiance(&mut self, irradiance: f64) {
        self.current_irradiance = irradiance;
    }
}

// ─── INA219 I2C driver (Linux only) ──────────────────────────────────────────

#[cfg(target_os = "linux")]
struct Ina219 {
    i2c:           I2c,
    current_lsb_a: f64, // Amps per LSB of the current register
    power_lsb_w:   f64, // Watts per LSB of the power register
}

#[cfg(target_os = "linux")]
impl Ina219 {
    /// Open the INA219 on `bus` at `address`, program the calibration register
    /// to achieve the target current_lsb of 0.1 mA / LSB.
    fn new(bus: u8, address: u16, shunt_ohms: f64) -> anyhow::Result<Self> {
        let mut i2c = I2c::with_bus(bus)?;
        i2c.set_slave_address(address)?;

        // Current_LSB = 0.1 mA = 0.0001 A/bit
        // Cal = trunc(0.04096 / (Current_LSB × R_shunt))
        let current_lsb_a = 0.0001_f64;
        let cal = (0.04096 / (current_lsb_a * shunt_ohms)) as u16;
        let power_lsb_w = current_lsb_a * 20.0; // INA219 spec: Power_LSB = 20 × Current_LSB

        // Write calibration: [reg_addr, high_byte, low_byte]
        let [ch, cl] = cal.to_be_bytes();
        i2c.write(&[INA219_REG_CAL, ch, cl])?;

        tracing::info!(
            "INA219 bus={} addr={:#04X} cal={:#06X} I_lsb={}µA P_lsb={}mW shunt={}Ω",
            bus, address, cal,
            current_lsb_a * 1e6,
            power_lsb_w * 1e3,
            shunt_ohms
        );

        Ok(Self { i2c, current_lsb_a, power_lsb_w })
    }

    /// Write register pointer, then read 2 bytes as big-endian u16.
    fn read_u16(&mut self, reg: u8) -> anyhow::Result<u16> {
        let mut buf = [0u8; 2];
        self.i2c.write_read(&[reg], &mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    /// Write register pointer, then read 2 bytes as big-endian i16.
    fn read_i16(&mut self, reg: u8) -> anyhow::Result<i16> {
        Ok(self.read_u16(reg)? as i16)
    }

    /// Bus voltage in Volts.
    /// Raw bits [15:3] × 4 mV/LSB; bits [2:1] are status flags.
    fn bus_voltage_v(&mut self) -> anyhow::Result<f64> {
        let raw = self.read_u16(INA219_REG_BUS)?;
        Ok(((raw >> 3) as f64) * 4.0e-3)
    }

    /// Current in Amps (signed, scaled by programmed current_lsb).
    fn current_a(&mut self) -> anyhow::Result<f64> {
        let raw = self.read_i16(INA219_REG_CURRENT)?;
        Ok(raw as f64 * self.current_lsb_a)
    }

    /// Power in Watts (unsigned, scaled by programmed power_lsb).
    #[allow(dead_code)]
    fn power_w(&mut self) -> anyhow::Result<f64> {
        let raw = self.read_u16(INA219_REG_POWER)?;
        Ok(raw as f64 * self.power_lsb_w)
    }
}

// ─── RealHardwareSensor — Linux ───────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub struct RealHardwareSensor {
    ina219:           Ina219,
    panel_stc_watts:  f64,
    last_irr_norm:    f64, // cached irradiance proxy for the dashboard
}

#[cfg(target_os = "linux")]
impl RealHardwareSensor {
    pub fn new(cfg: &HardwareConfig) -> anyhow::Result<Self> {
        let ina219 = Ina219::new(cfg.i2c_bus, cfg.ina219_address, cfg.ina219_shunt_ohms)?;
        Ok(Self {
            ina219,
            panel_stc_watts: cfg.panel_stc_watts,
            last_irr_norm: 0.0,
        })
    }

    /// Last measured power normalised to STC — used as irradiance proxy by main.
    pub fn last_irradiance_norm(&self) -> f64 {
        self.last_irr_norm
    }
}

#[cfg(target_os = "linux")]
impl PowerSensor for RealHardwareSensor {
    fn read_telemetry(&mut self, _duty_cycle: f64, _config: &PanelConfig) -> (f64, f64) {
        let v = self.ina219.bus_voltage_v().unwrap_or_else(|e| {
            tracing::warn!("INA219 bus-V read error: {e}");
            0.0
        });
        let i = self.ina219.current_a().unwrap_or_else(|e| {
            tracing::warn!("INA219 current read error: {e}");
            0.0
        });
        self.last_irr_norm = if self.panel_stc_watts > 0.0 {
            ((v * i) / self.panel_stc_watts).clamp(0.0, 1.2)
        } else {
            0.0
        };
        (v, i)
    }
    // set_irradiance → uses default no-op (hardware drives itself)
}

// ─── RealHardwareSensor — non-Linux fallback (delegates to SimulatedSensor) ──

#[cfg(not(target_os = "linux"))]
pub struct RealHardwareSensor {
    inner: SimulatedSensor,
}

#[cfg(not(target_os = "linux"))]
impl RealHardwareSensor {
    pub fn new(_cfg: &HardwareConfig) -> anyhow::Result<Self> {
        Ok(Self { inner: SimulatedSensor::new() })
    }
    pub fn last_irradiance_norm(&self) -> f64 {
        self.inner.current_irradiance
    }
}

#[cfg(not(target_os = "linux"))]
impl PowerSensor for RealHardwareSensor {
    fn read_telemetry(&mut self, duty_cycle: f64, config: &PanelConfig) -> (f64, f64) {
        self.inner.read_telemetry(duty_cycle, config)
    }
    fn set_irradiance(&mut self, irradiance: f64) {
        self.inner.set_irradiance(irradiance);
    }
}

// ─── ProtectionsMonitor — Linux ───────────────────────────────────────────────
//
// When `active = true` the struct holds real GPIO InputPins (pull-up).
// When `active = false` (simulation mode on Linux) the Options are None
// and both checks return true, so the safety branch is never tripped.

#[cfg(target_os = "linux")]
pub struct ProtectionsMonitor {
    pin_spd:     Option<InputPin>,
    pin_breaker: Option<InputPin>,
}

#[cfg(target_os = "linux")]
impl ProtectionsMonitor {
    pub fn new(cfg: &HardwareConfig, active: bool) -> anyhow::Result<Self> {
        if active {
            let gpio = Gpio::new()?;
            let pin_spd     = gpio.get(cfg.spd_pin)?.into_input_pullup();
            let pin_breaker = gpio.get(cfg.breaker_pin)?.into_input_pullup();
            tracing::info!(
                "ProtectionsMonitor: SPD=GPIO{}, Breaker=GPIO{}",
                cfg.spd_pin, cfg.breaker_pin
            );
            Ok(Self {
                pin_spd:     Some(pin_spd),
                pin_breaker: Some(pin_breaker),
            })
        } else {
            Ok(Self { pin_spd: None, pin_breaker: None })
        }
    }

    pub fn is_spd_ok(&self) -> bool {
        self.pin_spd.as_ref().map_or(true, |p| p.is_high())
    }
    pub fn is_breaker_closed(&self) -> bool {
        self.pin_breaker.as_ref().map_or(true, |p| p.is_high())
    }
}

// ─── ProtectionsMonitor — non-Linux stub ─────────────────────────────────────

#[cfg(not(target_os = "linux"))]
pub struct ProtectionsMonitor;

#[cfg(not(target_os = "linux"))]
impl ProtectionsMonitor {
    pub fn new(_cfg: &HardwareConfig, _active: bool) -> anyhow::Result<Self> {
        Ok(Self)
    }
    pub fn is_spd_ok(&self) -> bool { true }
    pub fn is_breaker_closed(&self) -> bool { true }
}

// ─── PwmOutput — Linux ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub struct PwmOutput {
    pwm: Pwm,
}

#[cfg(target_os = "linux")]
impl PwmOutput {
    pub fn new(cfg: &HardwareConfig) -> anyhow::Result<Self> {
        let channel = match cfg.pwm_pin {
            12 | 18 => Channel::Pwm0,
            13 | 19 => Channel::Pwm1,
            p => anyhow::bail!(
                "Unsupported PWM BCM pin {p}: use 12 or 18 (PWM0) / 13 or 19 (PWM1)"
            ),
        };
        let pwm = Pwm::with_frequency(
            channel,
            cfg.pwm_frequency_hz as f64,
            0.5,              // safe 50% initial duty
            Polarity::Normal,
            true,             // enable immediately
        )?;
        tracing::info!(
            "PwmOutput: BCM{} → {:?} @ {} Hz",
            cfg.pwm_pin, channel, cfg.pwm_frequency_hz
        );
        Ok(Self { pwm })
    }

    pub fn set_duty_cycle(&self, duty: f64) -> anyhow::Result<()> {
        self.pwm.set_duty_cycle(duty.clamp(0.0, 1.0))?;
        Ok(())
    }
}

// ─── PwmOutput — non-Linux stub ──────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
pub struct PwmOutput;

#[cfg(not(target_os = "linux"))]
impl PwmOutput {
    pub fn new(_cfg: &HardwareConfig) -> anyhow::Result<Self> { Ok(Self) }

    pub fn set_duty_cycle(&self, duty: f64) -> anyhow::Result<()> {
        tracing::debug!("[PWM-stub] duty: {:.4}", duty);
        Ok(())
    }
}

// ─── Inverter trait + GenericModbusInverter ───────────────────────────────────

pub trait Inverter {
    fn read_output_ac_voltage(&self) -> anyhow::Result<f64>;
    fn read_efficiency(&self)        -> anyhow::Result<f64>;
    fn get_status(&self)             -> String;
}

pub struct GenericModbusInverter {
    pub address: String,
}

impl Inverter for GenericModbusInverter {
    fn read_output_ac_voltage(&self) -> anyhow::Result<f64> {
        // TODO: query Modbus register 0x0100 via rmodbus
        Ok(230.5)
    }
    fn read_efficiency(&self) -> anyhow::Result<f64> {
        Ok(0.97)
    }
    fn get_status(&self) -> String {
        "ONLINE".to_string()
    }
}
