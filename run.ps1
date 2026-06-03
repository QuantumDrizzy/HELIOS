# HELIOS-NODE - one-command launcher (data pipeline + run). Idempotent: each step
# is skipped if its output already exists, so re-running is cheap. Only the PVGIS
# fetch needs internet, and only the first time.
#
#   Usage:  .\run.ps1            # full pipeline (if needed) + agent + dashboard
#           .\run.ps1 -NoUi      # headless: control loop without the egui window
#
# NOTE: build the Rust core from an "x64 Native Tools" prompt (or run vcvars64.bat
# first) so cargo finds the MSVC linker. Python must be on PATH.
param([switch]$NoUi)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot   # repo root

$uiArg = if ($NoUi) { @() } else { @("--ui") }

New-Item -ItemType Directory -Force -Path data | Out-Null

# ── 1. Data + model (each step skipped if already present) ────────────────────
if (-not (Test-Path data/pvgis_murcia_tmy.csv)) {
  Write-Host "[HELIOS] Fetching PVGIS irradiance data (needs internet, runs once)..."
  python ai/pvgis_client.py
}
if (-not (Test-Path data/train_sequences.pt)) {
  Write-Host "[HELIOS] Generating training dataset (cloud perturbations)..."
  python ai/dataset_generator.py
}
if (-not (Test-Path data/helios_predictor.pt)) {
  Write-Host "[HELIOS] Training CNN-LSTM (~5 min on CPU)..."
  python ai/train.py --epochs 50
}

# ── 2. Run: AI agent (background) + Rust core/dashboard (foreground) ───────────
Write-Host "[HELIOS] Starting AI agent (IPC bridge)..."
$agent = Start-Process python -ArgumentList "ai/agent.py","--serve" -PassThru -NoNewWindow
try {
  Write-Host "[HELIOS] Starting Rust core + dashboard..."
  Push-Location rust
  cargo run --release -- @uiArg
} finally {
  Pop-Location
  if ($agent -and -not $agent.HasExited) { Stop-Process -Id $agent.Id -Force }
}
