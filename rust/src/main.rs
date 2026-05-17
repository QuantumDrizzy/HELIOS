mod db;
mod controller;
mod solar_data;
mod hal;
mod config;
mod ui;

use db::HeliosDB;
use controller::PredictiveController;
use hal::{
    PowerSensor, SimulatedSensor, RealHardwareSensor,
    ProtectionsMonitor, PwmOutput,
    GenericModbusInverter, Inverter,
};
use config::HeliosConfig;
use solar_data::SolarDataset;
use std::time::Duration;
use serde_json::json;
use std::sync::{Arc, Mutex};
use ui::{HeliosDashboard, DashboardState};

/// How many real-time ticks (100 ms each) represent one simulated hour.
const TICKS_PER_HOUR: u64 = 36;
const CONFIG_PATH: &str = "../helios_config.toml";

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let run_ui = args.contains(&"--ui".to_string());

    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║  HELIOS-NODE — Predictive Power Control   ║");
    println!("  ║  PVGIS Data · LSTM Forecast · MPPT        ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    let shared_state = Arc::new(Mutex::new(DashboardState::default()));
    let state_clone  = shared_state.clone();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.spawn(async move {
        if let Err(e) = run_control_loop(state_clone).await {
            tracing::error!("Control loop failed: {}", e);
        }
    });

    if run_ui {
        let native_options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([800.0, 600.0]),
            ..Default::default()
        };
        eframe::run_native(
            "HELIOS-NODE Dashboard",
            native_options,
            Box::new(|cc| Ok(Box::new(HeliosDashboard::new(cc, shared_state)))),
        )
        .unwrap();
    } else {
        loop { std::thread::sleep(Duration::from_secs(1)); }
    }

    Ok(())
}

// ─── Hardware-mode selection ─────────────────────────────────────────────────
//
// `config.controller.mode = "hardware"` enables real I2C / GPIO / PWM on Linux.
// On any non-Linux host the function always returns false so the simulation path
// is taken regardless of what the config says.

fn select_hw_mode(mode: &str) -> bool {
    if mode != "hardware" {
        return false;
    }
    #[cfg(target_os = "linux")]
    { return true; }
    #[cfg(not(target_os = "linux"))]
    {
        println!("  [WARN] mode=hardware requested but not running on Linux — using simulation");
        false
    }
}

// ─── Control loop ─────────────────────────────────────────────────────────────

async fn run_control_loop(shared_state: Arc<Mutex<DashboardState>>) -> anyhow::Result<()> {

    // 0. Load configuration
    let config = match HeliosConfig::load(CONFIG_PATH) {
        Ok(c) => {
            println!("  Config: {} ({})", c.station.id, c.station.location);
            c
        }
        Err(e) => {
            println!("  [ERR] Config load failed: {e}");
            return Err(e);
        }
    };

    let hw_mode = select_hw_mode(&config.controller.mode);

    // 1. Load PVGIS solar data (used for simulation irradiance + AI bias)
    let solar = match SolarDataset::load(&config.station.solar_data_path) {
        Ok(ds) => {
            println!("  Solar data: {} | {} h | peak {:.0} W/m²",
                config.station.solar_data_path, ds.len(), ds.peak_ghi());
            Some(ds)
        }
        Err(e) => {
            println!("  [WARN] Solar data unavailable ({e}) — using sine fallback");
            None
        }
    };

    // 2. Database
    let db = HeliosDB::init("../data/energy_bus.sqlite").await?;
    db.append_audit("system", "NODE_STARTUP", Some(json!({
        "mode":   config.controller.mode,
        "region": config.station.location,
        "panel":  config.panel.model,
        "hw":     hw_mode,
    }))).await?;

    // 3. Controller
    let mut controller = PredictiveController::new(
        config.panel.clone(),
        config.economic.clone(),
        config.battery.clone(),
    );

    // 4. Sensor HAL  ─────────────────────────────────────────────────────────
    //    In hardware mode: RealHardwareSensor reads INA219 over I2C.
    //    In simulation mode (or non-Linux): SimulatedSensor uses PVGIS data.
    let mut sensor: Box<dyn PowerSensor + Send> = if hw_mode {
        println!("  Sensor: INA219 via I2C bus {} addr {:#04X}",
            config.hardware.i2c_bus, config.hardware.ina219_address);
        Box::new(RealHardwareSensor::new(&config.hardware)?)
    } else {
        println!("  Sensor: SimulatedSensor (PVGIS irradiance model)");
        Box::new(SimulatedSensor::new())
    };

    // 5. Protections monitor (GPIO SPD + breaker) ────────────────────────────
    //    Opens GPIO pins only when hw_mode = true on Linux.
    let protections = ProtectionsMonitor::new(&config.hardware, hw_mode)?;

    // 6. PWM output ───────────────────────────────────────────────────────────
    //    Created only when hw_mode = true; None in simulation mode.
    let pwm_output: Option<PwmOutput> = if hw_mode {
        println!("  PWM: BCM{} @ {} Hz",
            config.hardware.pwm_pin, config.hardware.pwm_frequency_hz);
        Some(PwmOutput::new(&config.hardware)?)
    } else {
        None
    };

    // 7. Modbus inverter (stub — always created, returns constants until wired)
    let inverter = GenericModbusInverter { address: "192.168.1.15".to_string() };

    let mut interval = tokio::time::interval(
        Duration::from_millis(config.controller.tick_rate_ms)
    );

    println!("  MPPT: {} tick={}ms hw={}",
        config.controller.mppt_algorithm, config.controller.tick_rate_ms, hw_mode);
    println!("  Inverter: {}", inverter.get_status());
    println!("  AI bridge: SQLite IPC (ai_forecasts table)");
    println!();

    // ─── Main control loop ───────────────────────────────────────────────────
    let mut tick_count: u64 = 0;
    let start_hour: usize   = 6; // simulation starts at 06:00

    loop {
        interval.tick().await;

        // Safety gate — both checks are no-ops in simulation mode
        if !protections.is_spd_ok() || !protections.is_breaker_closed() {
            tracing::error!("CRITICAL: hardware protection tripped — MPPT halted");
            continue;
        }

        // ── Simulated time & PVGIS irradiance (used in simulation mode and
        //    for the AI-forecast time index in both modes) ───────────────────
        let sim_hour     = start_hour as f64 + (tick_count as f64 / TICKS_PER_HOUR as f64);
        let sim_hour_idx = sim_hour as usize;

        let pvgis_irradiance = match &solar {
            Some(ds) => ds.irradiance_at(sim_hour_idx),
            None => {
                let h = (sim_hour_idx % 24) as f64;
                if h >= 6.0 && h <= 20.0 {
                    ((h - 6.0) / 14.0 * std::f64::consts::PI).sin() * 0.9
                } else {
                    0.0
                }
            }
        };

        // In simulation mode feed irradiance into the sensor model.
        // In hardware mode this call is a no-op (real V/I comes from INA219).
        let noise = (rand::random::<f64>() - 0.5) * 0.02;
        sensor.set_irradiance((pvgis_irradiance + noise).clamp(0.0, 1.2));

        // ── Read voltage + current via HAL ────────────────────────────────────
        let (measured_v, measured_i) =
            sensor.read_telemetry(controller.state().duty_cycle, &config.panel);
        let power = measured_v * measured_i;

        // ── MPPT tick ─────────────────────────────────────────────────────────
        let duty = controller.tick(measured_v, measured_i, sim_hour);

        // ── Send duty cycle to hardware PWM (no-op if pwm_output is None) ────
        if let Some(ref pwm) = pwm_output {
            if let Err(e) = pwm.set_duty_cycle(duty) {
                tracing::warn!("PWM write error: {e}");
            }
        }

        // ── Persist telemetry ─────────────────────────────────────────────────
        db.log_telemetry(
            measured_v, measured_i, power, duty,
            controller.state().soc,
            pvgis_irradiance * 1000.0,   // PVGIS normalized [0,1] → W/m²
        ).await?;

        // ── Poll AI forecast every ~1 s (10 ticks × 100 ms) ──────────────────
        if tick_count % 10 == 0 {
            if let Ok(Some(fc)) = read_latest_forecast(&db).await {
                controller.update_forecast(fc);
            }
        }

        // ── Irradiance value shown in the dashboard ───────────────────────────
        //    Simulation: from PVGIS data.
        //    Hardware:   derived from measured power (irradiance proxy).
        let displayed_irradiance_wm2 = if hw_mode && config.hardware.panel_stc_watts > 0.0 {
            power / config.hardware.panel_stc_watts * 1000.0
        } else {
            pvgis_irradiance * 1000.0
        };

        // ── Update shared UI state ────────────────────────────────────────────
        {
            let mut state = shared_state.lock().unwrap();
            state.power           = power;
            state.voltage         = measured_v;
            state.current         = measured_i;
            state.duty            = duty;
            state.forecast        = controller.forecast();
            state.irradiance_wm2  = displayed_irradiance_wm2;
            state.sim_hour        = sim_hour;
            state.pmax            = config.panel.pmax;
            state.location        = config.station.location.clone();
            state.currency        = config.economic.currency.clone();
            state.battery_type    = config.battery.chemistry.clone();
            state.soc             = controller.state().soc;
            state.inverter_v_ac   = inverter.read_output_ac_voltage().unwrap_or(0.0);
            state.inverter_eff    = inverter.read_efficiency().unwrap_or(0.0);
            state.protections_ok  = protections.is_spd_ok() && protections.is_breaker_closed();

            let t = tick_count as f64 * 0.1;
            state.power_history.push_back((t, power));
            if state.power_history.len() > 500 {
                state.power_history.pop_front();
            }
            state.forecast_history.push_back((t, controller.forecast()));
            if state.forecast_history.len() > 500 {
                state.forecast_history.pop_front();
            }
        }

        // ── Console log every 5 s (50 ticks × 100 ms) ────────────────────────
        tick_count += 1;
        if tick_count % 50 == 0 {
            let h = sim_hour_idx % 24;
            println!(
                "  {:02}:00 | GHI: {:5.0} W/m² | P: {:6.1}W | V: {:5.1}V | I: {:4.2}A | D: {:.3} | AI: {:.3}",
                h, displayed_irradiance_wm2, power, measured_v, measured_i, duty, controller.forecast()
            );
            db.append_audit("mppt_core", "TELEMETRY_BATCH", Some(json!({
                "hour":     h,
                "ghi_wm2":  displayed_irradiance_wm2,
                "power_w":  power,
                "duty":     duty,
                "forecast": controller.forecast(),
                "hw_mode":  hw_mode,
            }))).await?;
        }
    }
}

/// Read the most recent AI forecast from the SQLite IPC bridge.
async fn read_latest_forecast(db: &HeliosDB) -> anyhow::Result<Option<f64>> {
    let row: Option<(f64,)> = sqlx::query_as(
        "SELECT forecast_value FROM ai_forecasts ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(|r| r.0))
}
