# HELIOS-NODE

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

### 1. Prepare Data & Train Model

```bash
# Fetch real PVGIS data for Aljucer, Murcia
python ai/pvgis_client.py

# Generate training dataset with cloud perturbations
python ai/dataset_generator.py

# Train the LSTM model
python ai/train.py --epochs 50
```

### 2. Run the System

Open two terminals.

**Terminal 1 (AI Agent):**
```bash
python ai/agent.py --serve
```

**Terminal 2 (Rust Core + Dashboard):**
```bash
cd rust/
cargo run --release -- --ui
```

The system will start simulating days using the real PVGIS irradiance data, and you'll see the dashboard rendering the power telemetry and the AI forecasts in real-time.

---

## what's missing

- real ADC/GPIO reads replacing the PVGIS-simulated V/I sensor in `main.rs` (can be easily wired to an INA219).
- actual ML-DSA signature in `helios-sentinel: sign_checkpoint()` (mock returns 64 zero bytes).
- end-to-end integration test / CI.

---

## license

unlicensed / private research. contact: Antonio Rodríguez (QuantumDrizzy)
