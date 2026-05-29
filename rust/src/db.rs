use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use std::str::FromStr;
use sha2::{Digest, Sha256};
use chrono::Utc;

pub struct HeliosDB {
    pool: SqlitePool,
}

impl HeliosDB {
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn init(db_path: &str) -> anyhow::Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            if parent != std::path::Path::new("") {
                std::fs::create_dir_all(parent)?;
            }
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Self::apply_schema(&pool).await?;

        Ok(Self { pool })
    }

    /// In-memory DB for unit tests — WAL is swapped for in-memory journal,
    /// single connection to keep all ops in the same database instance.
    #[cfg(test)]
    pub async fn init_memory() -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Memory);

        let pool = SqlitePoolOptions::new()
            .max_connections(1) // single connection = single :memory: instance
            .connect_with(options)
            .await?;

        Self::apply_schema(&pool).await?;

        Ok(Self { pool })
    }

    /// Apply the full schema: base migration 0001 + the 0002 ALTER columns
    /// (`soc`, `irradiance_wm2`). Shared by both `init` and `init_memory` so the
    /// production and in-memory test databases never drift apart.
    async fn apply_schema(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(pool)
            .await?;

        // SQLite has no IF NOT EXISTS for ALTER TABLE; ignore "duplicate column"
        // errors so this is safe to re-run on an existing DB.
        for stmt in [
            "ALTER TABLE power_telemetry ADD COLUMN soc            REAL NOT NULL DEFAULT 0.5",
            "ALTER TABLE power_telemetry ADD COLUMN irradiance_wm2 REAL NOT NULL DEFAULT 0.0",
        ] {
            let _ = sqlx::query(stmt).execute(pool).await;
        }

        Ok(())
    }

    pub async fn append_audit(
        &self,
        component: &str,
        event_type: &str,
        payload: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let prev_hash: String =
            sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_else(|| "0".repeat(64));

        let timestamp   = Utc::now().to_rfc3339();
        let payload_str = payload.map(|p| p.to_string()).unwrap_or_default();

        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(timestamp.as_bytes());
        hasher.update(component.as_bytes());
        hasher.update(event_type.as_bytes());
        hasher.update(payload_str.as_bytes());
        let hash = hex::encode(hasher.finalize());

        sqlx::query(
            "INSERT INTO audit_log \
             (timestamp_utc, component, event_type, payload_json, hash_prev, hash) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
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

    pub async fn log_telemetry(
        &self,
        v: f64, i: f64, p: f64, duty: f64,
        soc: f64, irradiance_wm2: f64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO power_telemetry \
             (voltage, current, power, duty_cycle, soc, irradiance_wm2) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(v).bind(i).bind(p).bind(duty).bind(soc).bind(irradiance_wm2)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn log_forecast(&self, value: f64, confidence: f64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ai_forecasts (forecast_value, confidence) VALUES (?, ?)",
        )
        .bind(value).bind(confidence)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Retrieve the last `n` telemetry rows (newest first).
    /// Returns (voltage, current, power, duty_cycle, soc, irradiance_wm2).
    pub async fn get_last_n_telemetry(&self, n: i64) -> anyhow::Result<Vec<(f64, f64, f64, f64, f64, f64)>> {
        let rows = sqlx::query_as::<_, (f64, f64, f64, f64, f64, f64)>(
            "SELECT voltage, current, power, duty_cycle, soc, irradiance_wm2 \
             FROM power_telemetry ORDER BY id DESC LIMIT ?",
        )
        .bind(n)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_telemetry_writes_and_retrieves() {
        let db = HeliosDB::init_memory().await.unwrap();

        db.log_telemetry(48.0, 5.0, 240.0, 0.60, 0.80, 800.0).await.unwrap();
        db.log_telemetry(46.5, 4.5, 209.25, 0.55, 0.75, 600.0).await.unwrap();
        db.log_telemetry(44.0, 4.0, 176.0, 0.50, 0.70, 400.0).await.unwrap();

        let rows = db.get_last_n_telemetry(2).await.unwrap();

        assert_eq!(rows.len(), 2, "Should retrieve exactly 2 rows");

        // ORDER BY id DESC — most recent row is first
        let (v, i, p, d, soc, irr) = rows[0];
        assert!((v   - 44.0).abs()  < 1e-6, "voltage mismatch: {v}");
        assert!((i   -  4.0).abs()  < 1e-6, "current mismatch: {i}");
        assert!((p   - 176.0).abs() < 1e-6, "power mismatch: {p}");
        assert!((d   -  0.50).abs() < 1e-6, "duty mismatch: {d}");
        assert!((soc -  0.70).abs() < 1e-6, "soc mismatch: {soc}");
        assert!((irr - 400.0).abs() < 1e-6, "irradiance mismatch: {irr}");
    }

    #[tokio::test]
    async fn test_audit_chain_hash_changes_each_entry() {
        let db = HeliosDB::init_memory().await.unwrap();

        db.append_audit("system", "TEST_A", None).await.unwrap();
        db.append_audit("system", "TEST_B", None).await.unwrap();

        let hashes: Vec<String> =
            sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq ASC")
                .fetch_all(db.pool())
                .await
                .unwrap();

        assert_eq!(hashes.len(), 2);
        assert_ne!(hashes[0], hashes[1], "Chained hashes must differ");
        // Each hash is a 64-char lowercase hex string
        assert_eq!(hashes[0].len(), 64);
        assert!(hashes[0].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_get_last_n_returns_empty_when_no_rows() {
        let db = HeliosDB::init_memory().await.unwrap();
        let rows = db.get_last_n_telemetry(5).await.unwrap();
        assert!(rows.is_empty());
    }
}
