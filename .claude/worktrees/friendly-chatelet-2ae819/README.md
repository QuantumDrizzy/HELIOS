# HELIOS

Predictive DC-microgrid controller for perovskite/graphene solar arrays. MPPT in Rust, CNN-LSTM cloud forecasting in PyTorch, post-quantum trust anchor over local IPC. ~57% complete.

---

## what exists

| component | path | status |
|-----------|------|--------|
| MPPT controller (P&O + AI bias) | `rust/src/controller.rs` | real — runs |
| async telemetry loop, 100 ms tick | `rust/src/main.rs` | real — runs |
| SHA-256 chained audit log (SQLite WAL) | `rust/src/db.rs` | real — runs |
| DB schema + migrations | `migrations/0001_initial_schema.sql` | real — applied |
| live SQLite database | `data/energy_bus.sqlite` | real — active |
| CNN-LSTM irradiance predictor | `ai/agent.py` | real — minor bug (`import time` missing line 66) |
| SHM circular frame buffer consumer | `ai/helios_drl_consumer.py` | real — runs |
| CUDA NIR bilinear-resize + normalize kernel | `cuda/nir_preprocess.cu` | real — compiles, untested end-to-end |
| helios-sentinel PQC daemon (ML-KEM, ML-DSA) | `helios-sentinel/src/` | framework real, `sign_checkpoint` returns mock signature |
| Python ↔ Sentinel bridge (PyO3) | `helios-pqc-python/src/lib.rs` | real bindings, mocked crypto layer |

sensor reads in `main.rs` are simulated (random V/I within 48 V / 5 A bounds). real ADC integration not yet wired.

---

## architecture

```
NIR camera
    │  raw uint8 frames
    ▼
nir_preprocess.cu          ← CUDA bilinear resize → float32 normalized
    │  /dev/shm/helios_nir_drl_v1  (3-buffer circular, sequence-numbered)
    ▼
helios_drl_consumer.py     ← non-blocking SHM reader
    │  latest frame tensor
    ▼
CloudPredictorNet           ← CNN (2-conv) → LSTM (128h) → sigmoid → [0,1] irradiance forecast
(ai/agent.py)
    │  forecast float
    ▼
helios-core (Rust)          ← injects forecast as predictive_bias into P&O duty-cycle step
  controller.rs             ← duty ∈ [0.05, 0.95], 100 μs target resolution (100 ms actual)
  db.rs                     ← sqlx async pool, WAL, SHA-256 chained audit entries
    │
    ▼
helios-sentinel (daemon)    ← ML-DSA signing of DB checkpoints, ML-KEM peer handshakes
    │  Unix domain socket (Windows: TCP fallback)
    ▼
helios_pqc (PyO3 wheel)     ← Python calls SentinelClient.sign_checkpoint() / authenticate()
```

---

## stack

`Rust 1.78` · `tokio` · `sqlx / SQLite WAL` · `sha2` · `ml-kem 0.2` · `ml-dsa 0.1` · `chacha20poly1305` · `pyo3 0.21` · `PyTorch 2.x` · `CUDA 11+` · `maturin`

---

## build

### helios-core (MPPT + telemetry)

```bash
cd rust/
cargo build --release
cargo run
```

requires: Rust stable, SQLite3, libssl (for sqlx).

### helios-sentinel (PQC daemon)

```bash
cd helios-sentinel/
cargo build --release
./target/release/helios-sentinel
```

key material expected at `config/keys/slh-dsa-root.vk`, `config/keys/sentinel-ml-dsa.sk`.  
peer manifests at `config/peers/*.json`.  
daemon not needed for core loop to run; controller degrades gracefully when socket absent.

### helios_pqc (Python wheel)

```bash
cd helios-pqc-python/
pip install maturin
maturin develop
python -c "from helios_pqc import SentinelClient; print('ok')"
```

### DRL agent

```bash
pip install torch numpy
python ai/agent.py
```

fix first: add `import time` at top of `ai/agent.py`.

### CUDA preprocessing (optional, Jetson / RTX)

```bash
nvcc -O3 -shared -fPIC cuda/nir_preprocess.cu -o libhelios_nir.so
```

---

## what's missing (~43%)

- real ADC/GPIO reads replacing the simulated V/I sensor in `main.rs`
- actual ML-DSA signature in `helios-sentinel: sign_checkpoint()` (mock returns 64 zero bytes)
- SHM writer side (camera acquisition process not in this repo)
- trained model weights for `CloudPredictorNet` — architecture present, no `.pt` file
- Python ↔ Rust IPC to feed forecast into core loop (currently two separate processes with no bridge)
- `eframe/wgpu` UI declared in `rust/Cargo.toml` but nothing built yet
- end-to-end integration test / CI

---

## related

- [KHAOS](https://github.com/QuantumDrizzy/KHAOS) — BCI kernel
- [CryptoTN-GPU](https://github.com/QuantumDrizzy/CryptoTN-GPU) — GPU tensor networks for quantum biology
- [Q-NAA](https://github.com/QuantumDrizzy/Q-NAA) — quantum neural attention analyzer (Rust + CUDA)

---

## license

unlicensed / private research. contact: Antonio Rodríguez (QuantumDrizzy)
