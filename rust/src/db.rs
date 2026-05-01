use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use sha2::{Digest, Sha256};
use chrono::Utc;
use serde_json::json;

pub struct HeliosDB {
    pool: SqlitePool,
}

impl HeliosDB {
    pub async fn init(db_path: &str) -> anyhow::Result<Self> {
        // Asegurar que el directorio de datos existe
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        // Run migrations
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    pub async fn append_audit(&self, component: &str, event_type: &str, payload: Option<serde_json::Value>) -> anyhow::Result<()> {
        let prev_hash: String = sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or_else(|| "0".repeat(64));

        let timestamp = Utc::now().to_rfc3339();
        let payload_str = payload.map(|p| p.to_string()).unwrap_or_default();

        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(timestamp.as_bytes());
        hasher.update(component.as_bytes());
        hasher.update(event_type.as_bytes());
        hasher.update(payload_str.as_bytes());
        let hash = hex::encode(hasher.finalize());

        sqlx::query("INSERT INTO audit_log (timestamp_utc, component, event_type, payload_json, hash_prev, hash) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(timestamp)
            .bind(component)
            .bind(event_type)
            .bind(payload_str)
            .bind(prev_hash)
            .bind(hash)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn log_telemetry(&self, v: f64, i: f64, p: f64, duty: f64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO power_telemetry (voltage, current, power, duty_cycle) VALUES (?, ?, ?, ?)")
            .bind(v)
            .bind(i)
            .bind(p)
            .bind(duty)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn log_forecast(&self, value: f64, confidence: f64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO ai_forecasts (forecast_value, confidence) VALUES (?, ?)")
            .bind(value)
            .bind(confidence)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
