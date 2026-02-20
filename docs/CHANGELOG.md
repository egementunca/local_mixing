# Changelog: current branch vs original `bin`

This document summarizes the major architectural and functional changes introduced since the original `bin` branch. It reflects the current layout under `local_mixing/src/`.

## 1. New obfuscation engines and pipelines

### A. Asymmetric big butterfly
- Expanded from the original butterfly to include identity injection (`replace_pairs`), shooting, ancilla expansion, and chunked final compression.
- Supports SAT-based compression (`compress_big_sat`) and LMDB-first SAT (`compress_big_sat_lmdb`).
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` and `local_mixing/src/algorithms/butterfly/replace.rs`.

### B. Annealed obfuscator (research)
- Simulated annealing engine with move probabilities and energy model.
- Move set defined in `local_mixing/src/algorithms/annealing/local.rs`.
- Engine lives in `local_mixing/src/algorithms/annealing/anneal.rs` but is **not wired to CLI** yet.

### C. Gadget-based obfuscator
- Segmentation + commutator gadget injection + noise-based inflation.
- CLI: `obfuscate` subcommand.
- Code: `local_mixing/src/obfuscate/*`.

### D. Local-rewrite (experimental)
- Two-stage local rewrite (inflation + kneading) with attack-aligned metrics.
- CLI: `local-rewrite` (experimental).
- Code: `local_mixing/src/algorithms/local_rewrite.rs`.

## 2. Infrastructure upgrades

### A. Database abstractions
- TemplateDB reader and schema for `collection.lmdb` (sat_revsynth format).
- Perm-table LMDB generation and lookup tooling for local_mixing.
- Code: `local_mixing/src/infra/store/*`, `local_mixing/src/main.rs` helpers.

### B. Canonicalization and hashing
- Canonical permutation computation and caching in `local_mixing/src/infra/rainbow/canonical.rs`.
- Window hashing for memoization in `local_mixing/src/hashing/canonical.rs`.

### C. Analysis & verification
- DTW alignment (`align` command) and heatmap generation (`heatmap`).
- Wire activity visualization (`wiredot`).
- Code: `local_mixing/src/analysis/*` and CLI in `local_mixing/src/main.rs`.

## 3. Configuration and parameterization
- Centralized `ObfuscationConfig` for butterfly pipelines.
- `ObfConfig` for the gadget-based obfuscator.
- JSON config loading for `bbutterfly` / `abbutterfly`.
- Added `shuffle_bitflip` config and CLI flags for optional `B_{w,s}` pre-mix.

## 4. Test ergonomics
- DB/slow/artifact tests are now gated by env vars:
  - `LOCAL_MIXING_DB_TESTS`
  - `LOCAL_MIXING_SLOW_TESTS`
  - `LOCAL_MIXING_ARTIFACT_TESTS`

## 5. CLI surface changes

New/expanded commands:
- `bbutterfly`, `abbutterfly`, `local-mix`, `obfuscate`, `align`, `heatmap`, `compress`, `wiredot`.

Notable mismatches:
- `anneal` is **not** currently exposed as a CLI subcommand.
- `explore` and `binload` are declared in the CLI but have no handler in `main.rs`.

## 6. Scripts and experiments
- Added a suite of Python scripts under `local_mixing/scripts/` for experiments, plotting, and database inspection.
