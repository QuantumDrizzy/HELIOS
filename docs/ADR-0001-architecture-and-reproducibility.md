# ADR-0001: HELIOS-NODE architecture baseline + reproducibility contract

**Status:** Accepted
**Date:** 2026-06-03
**Deciders:** Antonio (QuantumDrizzy)

---

## Context

HELIOS-NODE is a predictive DC-microgrid controller for solar arrays: a Rust MPPT
control core, a PyTorch CNN-LSTM irradiance forecaster, real PVGIS data, an egui
dashboard, and post-quantum trust anchors over local IPC. It is "fully functional
end-to-end" and is also used as a **demonstrable artifact** — it must be replicable
by third parties (course instructors, a new machine in Zürich, an offline laptop)
with zero friction: `clone → run`, anywhere, anytime.

This ADR records (a) the architecture as built, and (b) the **reproducibility
contract** that makes it portable, after a hardening pass on 2026-06-03.

### Why this matters
A system that only runs on the author's machine is a liability, not an asset. The
goal is the opposite: anyone can clone it and watch it run, on Windows or Linux,
with or without internet, without re-training.

## Decision

**Keep the language-by-domain architecture, and guarantee portability via a
batteries-included repo + a single idempotent launcher.**

### Architecture (each language where it fits)
| Layer | Language | Role |
|-------|----------|------|
| Control core | **Rust** (`rust/`, crate `helios-core`) | MPPT (P&O + AI bias), 100 ms tick, async telemetry, egui dashboard, SHA-256 chained audit log |
| AI forecaster | **Python** (`ai/`) | CNN-LSTM irradiance predictor (PyTorch), PVGIS client, dataset generator, training |
| Trust anchors | **Rust** (`helios-sentinel/`, `helios-pqc-python/`) | post-quantum (ML-KEM / ML-DSA) trust daemon — optional add-on, independent crates |
| IPC bus | **SQLite (WAL)** | the Rust core and the Python agent exchange telemetry/forecasts through `data/energy_bus.sqlite` |

The three Rust crates are **independent** (no workspace): the demo is self-contained
in `rust/` — `helios-sentinel` / `helios-pqc-python` are optional and build separately.

### Reproducibility contract
1. **Batteries-included data.** The trained model (`data/helios_predictor.pt`) and
   the PVGIS input (`data/pvgis_murcia_tmy.csv`) are committed → a fresh clone runs
   **offline, no re-training**. Regenerable intermediates (`*_sequences.pt`,
   `training_loss.png`) and runtime DB files (`*.sqlite*`) are git-ignored.
2. **One-command launcher.** `run.sh` / `run.ps1` are idempotent: regenerate any
   missing artifact, then start the AI agent (background) + Rust core (foreground).
   They enforce the correct working directory so relative paths resolve.
3. **Pinned, portable deps.** `requirements.txt` pins stable ranges (CPU torch
   suffices; GPU optional). No machine-specific/nightly wheels.
4. **No hardcoded paths.** The Python AI resolves data via `__file__`-relative paths
   (cwd-independent); the Rust core resolves `../data` from `rust/` (enforced by the
   launcher). A machine-specific orphan consumer was removed.

## Options Considered

### Option A: Author-machine-only (status quo before this ADR)
Hardcoded absolute paths, unpinned deps, untracked model, manual multi-terminal
startup. **Rejected** — not replicable; breaks on any other machine.

### Option B: Clean repo, regenerate everything on first run
Git-ignore *all* artifacts; first run fetches PVGIS + trains. Clean, but the first
run needs internet + ~5 min training — fragile for a live demo on unknown wifi.
**Partially adopted** (intermediates are ignored) but not for the model/input.

### Option C: Batteries-included + idempotent launcher  ✅ CHOSEN
Ship the trained model + input data; one command runs it offline. Best serves the
"runs anywhere, anytime" goal. Small cost: a model binary lives in git (acceptable
for a demo repo; intermediates stay ignored).

## Consequences

**Easier:** clone-and-run offline on any OS; instructors can replicate or modify it
as-is; no "works on my machine".
**Harder:** the committed model must be refreshed if the architecture changes
(re-run the pipeline + commit). Documented in the launcher.
**Revisit when:** a real hardware-in-the-loop (ADC/GPIO, INA219) test replaces the
simulated sensor — the one honest gap (`main.rs` simulates V/I; see README).

## Action Items
1. [x] Remove machine-specific orphan (`a stale orphan script`).
2. [x] Pin `requirements.txt` to portable stable ranges.
3. [x] Add `run.sh` / `run.ps1` idempotent launchers.
4. [x] Batteries-included `data/` (ship model + CSV; ignore intermediates + runtime).
5. [x] Document the architecture + reproduction in the README.
6. [ ] (Future) Hardware-in-the-loop test with real INA219 sensor.
