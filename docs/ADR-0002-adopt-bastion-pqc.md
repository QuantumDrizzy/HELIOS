# ADR-0002: Adopt Bastion for HELIOS's PQC trust anchors

**Status:** Accepted
**Date:** 2026-06-15
**Deciders:** Antonio (QuantumDrizzy)

## Context

HELIOS advertises **"post-quantum trust anchors"** and ships PQC in two places:
- `helios-sentinel` (Rust daemon) — uses the third-party crates **`ml-kem`** and
  **`ml-dsa`** (RustCrypto) for ML-KEM-768 + ML-DSA-65.
- `helios-pqc-python` (PyO3) — exposes the same third-party PQC to Python.

Meanwhile **Bastion** is the ecosystem's **from-scratch, owned, sovereign** PQC core
(ML-KEM-768 / FIPS 203 and ML-DSA-65 / FIPS 204), **KAT-green** and **security-audited
(2026-06-15)**. Bastion's reason to exist, verbatim: *"post-quantum trust anchors mean
nothing if the primitives are someone else's binary."*

So HELIOS's "sovereign trust anchors" currently rest on **someone else's binary** — a
direct contradiction with the stated goal, and with the ecosystem's sovereignty thesis.

## Decision

**HELIOS consumes Bastion for all PQC**, replacing `ml-kem`/`ml-dsa`. Bastion becomes a
consumed **spine** (same pattern as Spectra for spectra, tenSORS for tensors): one
owned, audited PQC core under the stack.

Bastion's API covers HELIOS's needs 1:1 (randomized runtime API behind the default
`rand` feature):

| HELIOS today (`ml-kem`/`ml-dsa`) | Bastion |
|---|---|
| `MlKem768` keygen / `encapsulate` / `decapsulate` | `bastion::kem::{keygen, encaps, decaps}` + `EK_BYTES/DK_BYTES/CT_BYTES/SS_BYTES` |
| `MlDsa65` keygen / `sign` / `verify` | `bastion::sig::{keygen, sign, verify, SIG_BYTES}` (FIPS-204 `ctx` arg) |

Dependency is by path (Bastion is private, not on crates.io):
`bastion = { path = "../Bastion" }`.

## Why

- **Fulfills HELIOS's own sovereignty claim** — trust anchors on owned primitives.
- **One audited PQC core** for the whole ecosystem (Bastion audit 2026-06-15), instead
  of trusting + tracking two external crates.
- **Coherence**: Bastion joins Spectra/tenSORS as a consumed spine; the labs/tools stay
  thin over owned cores.

## Consequences

**Easier:** a single PQC implementation to audit and own; sovereignty is real, not
nominal; two third-party crypto deps dropped.
**Harder:** Bastion is a path dependency (must be present to build HELIOS); the FIPS-204
`ctx` parameter and byte-array shapes must be matched at the call sites.
**Revisit:** if HELIOS is ever built standalone without the ecosystem checkout, vendor
Bastion or feature-gate the PQC backend.

## Migration sequence (the concrete next step)
1. `helios-sentinel/Cargo.toml`: drop `ml-kem`/`ml-dsa`, add `bastion = { path = "../Bastion" }`.
2. Rewrite `helios-sentinel/src/main.rs` PQC call sites: keygen/sign/verify →
   `bastion::sig` (empty `ctx` unless a domain string is wanted); session
   encapsulate/decapsulate → `bastion::kem`.
3. Re-run helios-sentinel tests — the ML-DSA verify test must pass against
   Bastion-produced signatures (cross-implementation check).
4. `helios-pqc-python`: back the PyO3 module with `bastion::kem`/`sig` instead of the
   RustCrypto crates.
5. Update the README "what exists" PQC rows to note the Bastion-backed, sovereign core.

## Action items
1. [ ] helios-sentinel → Bastion (KEM + SIG), tests green.
2. [ ] helios-pqc-python → Bastion-backed PyO3.
3. [ ] README/status update.
