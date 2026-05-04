mod db;
mod controller;
mod solar_data;
mod ui;

use db::HeliosDB;
use controller::{PredictiveController, simulate_panel};
use solar_data::SolarDataset;
use std::time::Duration;
use serde_json::json;
use std::sync::{Arc, Mutex};
use ui::{HeliosDashboard, DashboardState};

/// How many real-time ticks (100ms each) represent one simulated hour.
/// 36 ticks = 3.6 seconds per simulated hour → full day in ~86 seconds.
const TICKS_PER_HOUR: u64 = 36;

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
    let state_clone = shared_state.clone();

    // Create a tokio runtime for the background tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.spawn(async move {
        if let Err(e) = run_control_loop(state_clone).await {
            tracing::error!("Control loop failed: {}", e);
        }
    });

    if run_ui {
        let native_options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
            ..Default::default()
        };
        eframe::run_native(
            "HELIOS-NODE Dashboard",
            native_options,
            Box::new(|cc| Ok(Box::new(HeliosDashboard::new(cc, shared_state)))),
        ).unwrap();
    } else {
        // Keep main thread alive if no UI
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    Ok(())
}

async fn run_control_loop(shared_state: Arc<Mutex<DashboardState>>) -> anyhow::Result<()> {
    // 1. Load solar data
    let solar = match SolarDataset::load("../data/pvgis_murcia_tmy.csv") {
        Ok(ds) => {
            println!("  ☀ PVGIS TMY: {} hours | Peak: {:.0} W/m² | Avg daylight: {:.0} W/m²",
                ds.len(), ds.peak_ghi(), ds.avg_ghi_daylight());
            Some(ds)
        }
        Err(e) => {
            println!("  ⚠ PVGIS data not available: {}", e);
            println!("  ⚠ Running with synthetic irradiance (sine curve)");
            None
        }
    };

    // 2. Initialize DB
    let db = HeliosDB::init("../data/energy_bus.sqlite").await?;
    db.append_audit("system", "NODE_STARTUP", Some(json!({
        "mode": "predictive",
        "data_source": if solar.is_some() { "pvgis_tmy" } else { "synthetic" },
    }))).await?;

    // 3. Initialize Controller
    let mut controller = PredictiveController::new();
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    
    println!("  ⚡ Controller: P&O + AI bias (100ms tick)");
    println!("  📡 AI bridge: SQLite IPC (ai_forecasts table)");
    println!("  🔒 Audit: SHA-256 chained log\n");
    println!("  Run 'python ai/agent.py --serve' in another terminal for AI forecasts.\n");

    // 4. Control Loop
    let mut tick_count: u64 = 0;
    let start_hour: usize = 6; // Start simulation at 6 AM
    
    loop {
        interval.tick().await;
        
        // Calculate simulated hour
        let sim_hour = start_hour as f64 + (tick_count as f64 / TICKS_PER_HOUR as f64);
        let sim_hour_idx = sim_hour as usize;
        
        // Get irradiance for this hour
        let irradiance = match &solar {
            Some(ds) => ds.irradiance_at(sim_hour_idx),
            None => {
                // Synthetic: sine curve peaking at noon
                let hour_of_day = (sim_hour_idx % 24) as f64;
                if hour_of_day >= 6.0 && hour_of_day <= 20.0 {
                    let t = (hour_of_day - 6.0) / 14.0 * std::f64::consts::PI;
                    t.sin() * 0.9
                } else {
                    0.0
                }
            }
        };
        
        // Add small noise to simulate real sensor jitter
        let noise = (rand::random::<f64>() - 0.5) * 0.02;
        let irradiance_noisy = (irradiance + noise).clamp(0.0, 1.2);
        
        // Simulate panel output
        let (measured_v, measured_i) = simulate_panel(irradiance_noisy, controller.state().duty_cycle);
        
        // MPPT tick
        let duty = controller.tick(measured_v, measured_i);
        let power = measured_v * measured_i;

        // Persist telemetry
        db.log_telemetry(measured_v, measured_i, power, duty).await?;

        // Read AI forecast from DB every second (10 ticks)
        if tick_count % 10 == 0 {
            if let Ok(Some(forecast)) = read_latest_forecast(&db).await {
                controller.update_forecast(forecast);
            }
        }

        // Update shared state for UI
        {
            let mut state = shared_state.lock().unwrap();
            state.power = power;
            state.voltage = measured_v;
            state.current = measured_i;
            state.duty = duty;
            state.forecast = controller.forecast();
            state.irradiance_wm2 = irradiance * 1000.0;
            state.sim_hour = sim_hour;
            
            let time_f64 = tick_count as f64 * 0.1; // 100ms per tick
            state.power_history.push_back((time_f64, power));
            if state.power_history.len() > 500 {
                state.power_history.pop_front();
            }
            
            state.forecast_history.push_back((time_f64, controller.forecast()));
            if state.forecast_history.len() > 500 {
                state.forecast_history.pop_front();
            }
        }

        // Log every 5 seconds (50 ticks)
        tick_count += 1;
        if tick_count % 50 == 0 {
            let hour_of_day = sim_hour_idx % 24;
            let irr_wm2 = irradiance * 1000.0;
            println!(
                "  {:02}:00 | GHI: {:5.0} W/m² | P: {:6.1}W | V: {:5.1}V | I: {:4.1}A | D: {:.2} | AI: {:.2}",
                hour_of_day, irr_wm2, power, measured_v, measured_i, duty, controller.forecast()
            );
            
            db.append_audit("mppt_core", "TELEMETRY_BATCH", Some(json!({
                "hour": hour_of_day,
                "ghi_wm2": irr_wm2,
                "power_w": power,
                "duty": duty,
                "forecast": controller.forecast(),
            }))).await?;
        }
    }
}

/// Read the latest AI forecast from the SQLite bridge
async fn read_latest_forecast(db: &HeliosDB) -> anyhow::Result<Option<f64>> {
    let result: Option<(f64,)> = sqlx::query_as(
        "SELECT forecast_value FROM ai_forecasts ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(db.pool())
    .await?;
    
    Ok(result.map(|r| r.0))
}
