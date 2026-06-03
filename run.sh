#!/usr/bin/env bash
# HELIOS-NODE — one-command launcher (data pipeline + run). Idempotent: each step
# is skipped if its output already exists, so re-running is cheap. Only the PVGIS
# fetch needs internet, and only the first time.
#
#   Usage:  ./run.sh           # full pipeline (if needed) + agent + dashboard
#           ./run.sh --no-ui   # headless: control loop without the egui window
set -euo pipefail
cd "$(dirname "$0")"   # repo root, regardless of where it's called from

UI_FLAG="--ui"
[ "${1:-}" = "--no-ui" ] && UI_FLAG=""

mkdir -p data

# ── 1. Data + model (each step skipped if already present) ────────────────────
if [ ! -f data/pvgis_murcia_tmy.csv ]; then
  echo "[HELIOS] Fetching PVGIS irradiance data (needs internet, runs once)..."
  python ai/pvgis_client.py
fi
if [ ! -f data/train_sequences.pt ]; then
  echo "[HELIOS] Generating training dataset (cloud perturbations)..."
  python ai/dataset_generator.py
fi
if [ ! -f data/helios_predictor.pt ]; then
  echo "[HELIOS] Training CNN-LSTM (~5 min on CPU)..."
  python ai/train.py --epochs 50
fi

# ── 2. Run: AI agent (background) + Rust core/dashboard (foreground) ───────────
echo "[HELIOS] Starting AI agent (IPC bridge)..."
python ai/agent.py --serve &
AGENT_PID=$!
trap 'kill $AGENT_PID 2>/dev/null || true' EXIT

echo "[HELIOS] Starting Rust core + dashboard..."
( cd rust && cargo run --release -- $UI_FLAG )
