-- HELIOS-NODE: Initial Schema — Predictive DC-Microgrid
-- SQLite with WAL mode
--
-- Tables:
--   1. power_telemetry     — High-frequency V, I, P, Duty Cycle data
--   2. ai_forecasts        — Irradiance predictions from DRL agent
--   3. material_states     — Quantum material simulation results (mobility, temp)
--   4. system_events       — Config changes, alerts, safety triggers
--   5. audit_log           — SHA-256 chained integrity log

-- ═══════════════════════════════════════════════════════════════
-- 1. POWER TELEMETRY — High-speed MPPT data
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS power_telemetry (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_utc   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    voltage         REAL    NOT NULL,  -- Volts
    current         REAL    NOT NULL,  -- Amps
    power           REAL    NOT NULL,  -- Watts
    duty_cycle      REAL    NOT NULL,  -- PWM [0, 1]
    mppt_mode       TEXT    DEFAULT 'predictive_drl'
);

-- ═══════════════════════════════════════════════════════════════
-- 2. AI FORECASTS — DRL Agent Irradiance Predictions
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS ai_forecasts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_utc       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    forecast_value      REAL    NOT NULL,  -- Normalized [0, 1]
    confidence          REAL    DEFAULT 0.0,
    inference_time_ms   REAL,
    model_version       TEXT    DEFAULT 'v1_nir_cnn_lstm'
);

-- ═══════════════════════════════════════════════════════════════
-- 3. MATERIAL STATES — Quantum simulation output
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS material_states (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_utc       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    temp_celsius        REAL    NOT NULL,
    electron_mobility   REAL    NOT NULL,
    lattice_stability   REAL    NOT NULL,
    quantum_efficiency  REAL    NOT NULL
);

-- ═══════════════════════════════════════════════════════════════
-- 4. AUDIT LOG — SHA-256 chained (tamper-evident)
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS audit_log (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_utc   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    component       TEXT    NOT NULL CHECK(component IN ('mppt_core','ai_agent','cuda_sim','system')),
    event_type      TEXT    NOT NULL,
    payload_json    TEXT,
    hash_prev       TEXT    NOT NULL,
    hash            TEXT    NOT NULL
);

-- ═══════════════════════════════════════════════════════════════
-- 5. INDICES
-- ═══════════════════════════════════════════════════════════════

CREATE INDEX IF NOT EXISTS idx_power_ts     ON power_telemetry(timestamp_utc);
CREATE INDEX IF NOT EXISTS idx_forecast_ts  ON ai_forecasts(timestamp_utc);
CREATE INDEX IF NOT EXISTS idx_audit_comp   ON audit_log(component);
