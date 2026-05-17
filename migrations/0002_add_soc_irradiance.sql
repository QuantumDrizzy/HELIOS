-- Migration 0002: add soc and irradiance_wm2 to power_telemetry
-- SQLite does not support IF NOT EXISTS on ALTER TABLE ADD COLUMN.
-- These statements are run with error-ignore in db.rs (idempotent on re-run).

ALTER TABLE power_telemetry ADD COLUMN soc           REAL NOT NULL DEFAULT 0.5;
ALTER TABLE power_telemetry ADD COLUMN irradiance_wm2 REAL NOT NULL DEFAULT 0.0;
