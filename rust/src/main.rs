mod db;
mod controller;

use db::HeliosDB;
use controller::PredictiveController;
use std::time::Duration;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║  HELIOS-NODE — Predictive Power Control   ║");
    println!("  ║  High-Speed MPPT + NIR Cloud Forecast     ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    // 1. Inicializar DB
    let db = HeliosDB::init("../data/energy_bus.sqlite").await?;
    db.append_audit("system", "NODE_STARTUP", Some(json!({"mode": "predictive"}))).await?;

    // 2. Inicializar Controlador
    let mut controller = PredictiveController::new();
    let mut interval = tokio::time::interval(Duration::from_millis(100)); // Log cada 100ms
    
    tracing::info!("Controlador HELIOS-NODE iniciado y auditado.");

    // 3. Bucle de Control y Persistencia
    let mut tick_count = 0;
    loop {
        interval.tick().await;
        
        // Simulación de lectura de sensores
        let measured_v = 48.0 + (rand::random::<f64>() * 0.5);
        let measured_i = 5.0 + (rand::random::<f64>() * 0.2);
        
        // El controlador ejecuta su lógica de 100 microsegundos internamente
        let duty = controller.tick(measured_v, measured_i);
        let power = measured_v * measured_i;

        // Persistencia de telemetría
        db.log_telemetry(measured_v, measured_i, power, duty).await?;

        tick_count += 1;
        if tick_count % 50 == 0 {
            tracing::info!("P={:.2}W | V={:.2}V | I={:.2}A | Duty={:.2}", power, measured_v, measured_i, duty);
            db.append_audit("mppt_core", "TELEMETRY_BATCH", Some(json!({"p_avg": power}))).await?;
        }
    }
}
