# HELIOS-NODE

[![CI](https://github.com/QuantumDrizzy/HELIOS/actions/workflows/ci.yml/badge.svg)](https://github.com/QuantumDrizzy/HELIOS/actions/workflows/ci.yml)

Predictive DC-microgrid controller for solar arrays. Features an MPPT loop in Rust, CNN-LSTM cloud forecasting in PyTorch, real PVGIS data integration, an egui dashboard, and post-quantum trust anchors over local IPC. Fully functional end-to-end.

---

## what exists

| component | path | status |
|-----------|------|--------|
| MPPT controller (P&O + AI bias) | `rust/src/controller.rs` | real — runs (100 ms tick) |
| async telemetry & UI loop | `rust/src/main.rs` | real — runs |
| egui real-time dashboard | `rust/src/ui.rs` | real — runs (`cargo run -- --ui`) |
| SHA-256 chained audit log (SQLite) | `rust/src/db.rs` | real — active |
| DB schema + migrations | `migrations/0001_initial_schema.sql` | real — applied |
| CNN-LSTM irradiance predictor | `ai/agent.py` | real — active IPC bridge via SQLite |
| PVGIS TMY data client | `ai/pvgis_client.py` | real — fetches data for Murcia |
| training dataset generator | `ai/dataset_generator.py` | real — adds cloud perturbations |
| model training pipeline | `ai/train.py` | real — generates `helios_predictor.pt` |
| helios-sentinel PQC daemon | `helios-sentinel/src/` | framework real |

---

## architecture

```
PVGIS Data (Murcia)
    │  8760 hours of real irradiance
    ▼
ai/dataset_generator.py    ← adds cloud drops and sensor noise
    │  train_sequences.pt
    ▼
ai/train.py                ← trains LSTM → helios_predictor.pt
    │
    ▼
ai/agent.py (--serve)      ← AI Agent (Python)
    │  reads telemetry from SQLite
    │  writes forecast [0,1] to ai_forecasts table
    ▼
energy_bus.sqlite          ← IPC Bridge (SQLite WAL)
    ▲
    │  reads forecast every 1s
    │  writes telemetry every 100ms
helios-core (Rust)         ← MPPT Controller
  controller.rs             ← injects forecast as predictive_bias into P&O duty-cycle step
  main.rs                   ← control loop + simulated physics
  ui.rs                     ← egui dashboard (Power Gauge, Timeline, AI Status)
```

---

## stack

`Rust 1.78` · `tokio` · `egui` · `sqlx / SQLite WAL` · `sha2` · `ml-kem 0.2` · `ml-dsa 0.1` · `PyTorch 2.x`

---

## run

### Prerequisites

- Rust 1.78+ — https://rustup.rs
- Python 3.11+ with pip
- **Windows only:** build from an *"x64 Native Tools Command Prompt for VS"* (or run
  `vcvars64.bat` first) so cargo finds the MSVC linker.

```bash
git clone https://github.com/QuantumDrizzy/HELIOS.git
cd HELIOS
pip install -r requirements.txt
```

### Quick start (one command)

The repo is **batteries-included**: the trained model (`data/helios_predictor.pt`)
and the PVGIS input (`data/pvgis_murcia_tmy.csv`) are committed, so a fresh clone
runs **offline, with no internet and no re-training**.

```bash
./run.sh          # Linux / macOS   (--no-ui for headless)
.\run.ps1         # Windows         (-NoUi for headless)
```

The launcher is idempotent: it regenerates any missing data/model, then starts the
AI agent (background) and the Rust core + egui dashboard (foreground). The SQLite
IPC bus at `data/energy_bus.sqlite` is created automatically on first run.

### Manual steps (equivalent to the launcher)

```bash
# (Re)generate data + model — only needed to RE-train (a model is already shipped)
python ai/pvgis_client.py          # fetch PVGIS TMY for Murcia (needs internet)
python ai/dataset_generator.py     # add cloud perturbations → train/val sequences
python ai/train.py --epochs 50     # train the CNN-LSTM (~5 min on CPU)

# Run — two terminals
python ai/agent.py --serve                 # terminal 1 — AI forecast agent (IPC bridge)
cd rust && cargo run --release -- --ui     # terminal 2 — control loop + egui dashboard
```

The system simulates day/night cycles using real PVGIS irradiance data; the dashboard renders power telemetry and AI forecasts in real-time.

---

## what's missing

- real ADC/GPIO reads replacing the PVGIS-simulated V/I sensor in `main.rs` (can be easily wired to an INA219).
- Unix Domain Socket server requires Linux/RPi to run (stub on Windows by design).
- full hardware-in-the-loop end-to-end test (16 Rust unit tests pass via `cargo test`; CI builds + tests on every push — what remains is a real-hardware HIL run).

---

## license

MIT — Antonio Zambudio Rodriguez (QuantumDrizzy) · drizzyrdrgz.exe@protonmail.com
