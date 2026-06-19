# ADR-0003: Park HELIOS — usable by others (the core stands alone; PQC is optional)

**Status:** Accepted
**Date:** 2026-06-19
**Deciders:** Antonio (QuantumDrizzy)

## Context

I don't have a physical solar array right now, so I'm **not actively running HELIOS** — and
it's being removed from the dev Desktop (re-clone when needed). But HELIOS is genuinely
useful to the people on the **solar-panel course** (they mount and wire real panels), and at
least one person (a Linux/HPC person) has already saved the repo. So the goal for the park:
**leave HELIOS clonable and usable by an outsider, standalone.**

One thing blocks naive standalone use: `helios-sentinel` (the optional PQC daemon) has a
local path dependency `bastion = { path = "../../Bastion" }`, and **Bastion is a private
repo.** Anyone cloning HELIOS and running `cargo build` inside `helios-sentinel/` would fail.
But that crate is *not* the core: the microgrid controller + forecasting + dashboard that a
solar course actually wants live in `helios-core` (`rust/`) and `ai/` (Python), which have
**no Bastion dependency** and are launched by `run.sh` / `run.ps1`.

## Decision

**Keep the core standalone-usable; make the core-vs-optional split explicit; do not entangle
the private PQC dependency into the core build.**

- The **core** (`run.sh` → `ai/` forecasting + `rust/` control loop + egui dashboard) builds
  and runs from a fresh clone with **no Bastion, no internet, no retraining** (model + PVGIS
  data are committed — "batteries-included"). This is the path for the course / any outsider.
- `helios-sentinel` (PQC trust anchors over local IPC) is an **optional sovereignty daemon**,
  not part of the microgrid controller. It depends on the **private** Bastion crate; to build
  it, clone Bastion adjacent to HELIOS (`../../Bastion`). Outsiders can ignore it entirely.
- The three crates are intentionally **independent** (no workspace root), so building the core
  never touches the sentinel or Bastion.

## Options Considered

### Option A: Make Bastion public + use a git dependency
**Rejected (his call, and not needed).** Bastion is a from-scratch PQC/security crate kept
private on purpose; publishing it to satisfy an *optional* HELIOS daemon is the wrong reason.
The core doesn't need it.

### Option B: Feature-gate the Bastion PQC behind a cargo feature
**Deferred.** Cleaner long-term (PQC opt-in), but it's Rust surgery on the sentinel that
needs a verified build to land safely — out of scope for a park, and unnecessary because the
core is *already* independent of the sentinel.

### Option C: Document the core-vs-optional split clearly (CHOSEN)
**Chosen.** Zero risk, immediate. The core was already standalone; the only real gap was that
the README implied Bastion was part of the build. Making the split explicit closes it.

## Consequences

- **Easier:** an outsider (course / the person who saved the repo) clones, `pip install`,
  `./run.sh`, and gets the predictive microgrid controller + dashboard — no private deps.
- **Harder:** nothing. The sovereignty path (sentinel + PQC) still works *for me* with Bastion
  cloned adjacent; it's just clearly marked optional.
- **Revisit:** if HELIOS ever needs the PQC in the core for an outsider, feature-gate it
  (Option B) and decide on Bastion's visibility (Option A) then.

## Action Items
1. [x] README: mark `helios-sentinel`/Bastion as **optional + private-dep**; state the core
   builds standalone (no Bastion). 
2. [x] Commit the pending Cargo.lock changes (reproducible builds).
3. [ ] (When I have real panels / am in Switzerland) wire real ADC/GPIO (INA219) per the
   README's "what's missing"; run the hardware-in-the-loop test.
4. [ ] (Optional, later) feature-gate the sentinel's PQC so even it builds without Bastion.

## References
- ADR-0002 (adopt Bastion for PQC) — the dependency this ADR scopes as optional.
- README "run" / "what's missing"; `run.sh`; `rust/` (helios-core, standalone).
