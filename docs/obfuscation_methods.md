# Local Mixing - Methods, Experiments, and Roadmap

This document focuses on method-level details, experiment tooling, and forward plans. It complements `local_mixing/docs/MASTER_README.md` which is the primary codebase reference.

## 0. How to Use This Doc

- Read Section 1 for the current obfuscation methods and where they live in code.
- Read Section 5 for scripts and reproducible experiment patterns.
- Read Section 6 for the future roadmap distilled from `local_mixing/local_mix_notes_future_plan.md`.

## 1. Obfuscation Methods (Current State)

Each item lists the mechanics, knobs, and concrete code entry points.

### 1.1 Shooting (random gate drift)
- Mechanics: `shoot_random_gate` picks a random gate index and “shoots” it left or right as far as possible without collision using `Gate::collides_index` (commuting reorders that preserve semantics).
- Controls: `ObfuscationConfig.shooting_count` (global) and `shooting_count_inner` (per-block), CLI `--shooting`.
- Code: `local_mixing/src/infra/random/random_data.rs` (`shoot_random_gate`), used in `local_mixing/src/algorithms/butterfly/mixing.rs`.

### 1.2 Pair replacement (`replace_pairs`)
- Mechanics: iterates adjacent pairs, classifies them by `gate_pair_taxonomy`, then replaces the pair with an identity template that matches the taxonomy at the front or back (reversing if needed). Uses `CircuitSeq::unrewire_subcircuit` to map template wires to the original pair and verifies identity with `CircuitSeq::probably_equal`.
- Controls: implicit in `replace_pairs`; template source depends on `--lmdb-db` (TemplateDB) vs `random_canonical_id` (perm tables).
- Code: `local_mixing/src/algorithms/butterfly/replace.rs` (`replace_pairs`, `random_stored_id`).

### 1.3 Single gate replacement
- Mechanics: choose a gate `g`, sample a canonical identity, rewire the identity so its first gate matches `g` (`rewire_first_gate`), unrewire back, then drop the first gate and splice the remainder in place of `g`. This preserves the overall function while inflating local structure.
- Controls: `ObfuscationConfig.single_gate_mode`, `single_gate_replacements`, CLI `--single-gate`.
- Code: `local_mixing/src/algorithms/butterfly/replace.rs` (`random_gate_replacements`).

### 1.4 Ancilla expansion (big blocks)
- Mechanics: selects a convex subcircuit (`find_convex_subcircuit` + `contiguous_convex`), expands its active wire set up to 7 wires, rewires to a compact wire set, then calls `expand_lmdb` to replace with an equivalent circuit drawn from LMDB perm tables. Unrewires back to original wires.
- Controls: `ObfuscationConfig.no_ancilla_mode` disables this step.
- Code: `local_mixing/src/algorithms/butterfly/replace.rs` (`expand_big`, `expand_lmdb`).

### 1.5 Compression strategies
- `compress_lmdb`: uses SQLite or LMDB `n{N}m{M}perms` to get `(perm, shuf)` for a subcircuit, then samples replacements from LMDB `n{N}m{M}` with the canonical perm prefix (`random_perm_lmdb`), including inverse handling by reversing gates.
- `compress_big`: currently **does not** call `compress_lmdb` (subcircuit path is commented out); it mostly performs convex selection and local dedup.
- `compress_big_ancillas`: uses `compress_lmdb` after ancilla expansion.
- `compress_big_sat`: enumerates truth tables and calls `sat_revsynth/scripts/synthesize_from_tt.py`.
- `compress_big_sat_lmdb`: looks up `TemplateDB` (`templates_by_hash`) via `compute_canonical_hash` and falls back to SAT.
- Controls: `ObfuscationConfig.sat_mode`, `compression_window_size`, `compression_window_size_sat`, `compression_sat_limit`.
- Code: `local_mixing/src/algorithms/butterfly/replace.rs`, `local_mixing/src/optimize/compress_sat.rs`.

Note: `--lmdb-db` points to the TemplateDB (`collection.lmdb`). The local_mixing perm tables live in `./db` and are used by `compress_lmdb`/`expand_lmdb` and `random_canonical_id`.

### 1.6 Bookendless mode
- Mechanics: `abutterfly_big_delay_bookends` delays global bookend insertion; `main_butterfly_big_bookendsless` currently does not reattach begin/end bookends (code is commented) and optionally runs a final `compress_big` pass on halves and the full circuit.
- Controls: CLI `--bookendless`, plus standard `ObfuscationConfig` knobs.
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs`.

### 1.7 Reverse pipeline
- Mechanics: reverses gate order; since gates are self-inverse, this is the circuit inverse.
- Code: `local_mixing/src/main.rs` (`reverse`).

### 1.8 Pre-mix Shuffle + Bit-Flip (B_{w,s}) (optional)
- Mechanics: prepend `B_{w,s}` before mixing and append its inverse after mixing.
- Modes:
  - `flip-mode=none`: disable stage.
  - `flip-mode=separate`: Style A (shuffle then explicit X layer).
  - `flip-mode=embedded`: Style B (swap-with-flip gadgets).
- Controls: `--flip-mode`, `--flip-scope` (only `global` wired), `--shuffle-seed`, `--gadget-library`, `--flip-probability`.
- Code: `local_mixing/src/algorithms/shuffle_bitflip.rs` and `abbutterfly_big` in `local_mixing/src/algorithms/butterfly/mixing.rs`.

## 2. Butterfly Variants and Local Mixing

### 2.1 `butterfly`
- Pipeline: `random_id` generates a single `R` and `R_inv`, each gate is compressed via `outward_compress`, then blocks are merged, bookends are added, and final compression runs until stable.
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` (`butterfly`, `outward_compress`, `main_butterfly`).

### 2.2 `bbutterfly` (big butterfly)
- Pipeline: `replace_pairs` (perm-table identities) -> optional `random_gate_replacements` -> global `shoot_random_gate` -> per-gate blocks `R_inv · g · R` -> optional `expand_big` -> `compress_big` (or SAT if enabled) -> merge -> bookends -> final chunked compression.
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` (`butterfly_big`, `main_butterfly_big`).

### 2.3 `abbutterfly` (asymmetric big butterfly)
- Pipeline: chain of random `R` values per gate to break symmetry (`prev_r_inv`), then block compression (SAT or LMDB-first SAT), merge, add bookends `first_r` and `prev_r_inv`, and run chunked final compression.
- Template sourcing: `replace_pairs` uses TemplateDB when `--lmdb-db` is provided; otherwise falls back to perm tables.
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` (`abutterfly_big`).

### 2.4 `abbutterfly --bookendless`
- Pipeline: `abutterfly_big_delay_bookends` in each round, then optional final compression on halves and the full circuit (if `skip_compression` is false).
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` (`abutterfly_big_delay_bookends`, `main_butterfly_big_bookendsless`).

### 2.5 `mix`
- Pipeline: `obfuscate_and_target_compress` wraps each gate with a fixed random identity `R · R_inv`, then compresses each `r_inv · g · r_next` slice before final global compression.
- Code: `local_mixing/src/algorithms/butterfly/mixing.rs` (`main_mix`, `obfuscate_and_target_compress`).

### 2.6 `local-mix`
- Pipeline: `mix_step` applies stochastic moves (template insertion, commuting swaps, patch pairs), then `reduce_circuit` measures compression ratio with a local attacker model.
- Code: `local_mixing/src/algorithms/annealing/local.rs`.

### 2.7 `obfuscate`
- Pipeline: segmentation -> commutator gadget injection -> noise to target overhead -> `simple_compress` -> optional verification.
- Code: `local_mixing/src/obfuscate/passes.rs`.

### 2.8 `local-rewrite` (experimental)
- Pipeline: two-stage local rewrite (inflation + kneading) with attack-aligned metrics.
- Uses a perm-table oracle (LMDB) when available; otherwise falls back to conservative local moves.
- Intended as an experimental, theory-aligned prototype; not part of the main production obfuscation schemes.
- Design is motivated by the local-rewrite framework and attack-based metrics in internal notes.
- Code: `local_mixing/src/algorithms/local_rewrite.rs` and CLI `local-rewrite`.

## 3. Reducer and Attacker Models

### 3.1 Budgeted reducer
- Cancellation + commute under a step budget.
- Template matching is TODO in this reducer.
- Code: `local_mixing/src/reducer/budget.rs`.

### 3.2 Local reducer (annealing MVP)
- Cancellation pass, commuting swaps, and a small-window identity detector for low active-wire windows.
- Code: `local_mixing/src/algorithms/annealing/local.rs`.

### 3.3 Obfuscate pipeline compression
- Uses `simple_compress` only (adjacent identical gates).
- `ObfConfig.reducer_probe` and `min_survival_ratio` exist but are not used in `passes.rs` yet.

## 4. Analysis and Metrics

### 4.1 Heatmap
- Generated by `generate_heatmap_data`: normalized Hamming distance between state evolutions.
- CLI: `heatmap --c1 --c2 --num_wires --inputs` prints JSON to stdout.
- Plotting: `scripts/plot_heatmap.py` expects the list-of-triplets output format.

### 4.2 Alignment (DTW)
- Uses `trace_states` and `distance_matrix` (normalized Hamming distance).
- CLI: `align --c1 --c2 -n --inputs` prints JSON with `d_matrix` and `path`.
- Plotting: `scripts/plot_alignment.py` derives similarity as `1 - distance`.

### 4.3 Wire activity and coverage
- CLI `wiredot` writes `wire_plot.png` with total and active gate counts per wire.
- Script `plot_wire_stats.py` provides a more customizable view.

### 4.4 Obfuscation reports
- `ObfReport` includes overhead, coverage, reducibility estimate, survival ratio.
- Used by `obfuscate` pipeline and can be serialized to JSON.

## 5. Experiment and Utility Scripts

All scripts live in `local_mixing/scripts/`.

Experiment runners:
- `run_experiments.py`: runs multiple `abbutterfly` configurations and plots heatmaps.
- `experiment_sat_64w.py`: SAT-mode 64-wire runs with heatmap and alignment.
- `experiment_annealed.py`: annealing-style run (currently approximated with low-shooting `abbutterfly`).
- `experiment_expansion.py`: measures size growth over rounds.
- `experiment_inflation_only.py`: inflation-only obfuscation and plots heatmap/alignment/histogram.
- `experiment_inflation_bookendless.py`: inflation-only with bookendless mode.
- `experiment_0106_config.py`: tests configuration system for SAT vs LMDB modes.
- `replicate_experiment_0106.py`: reproduces a 32-wire run with analysis plots.

Data and DB helpers:
- `generate_rainbow_tables.py`: runs `init` and `load` to create SQLite rainbow tables.
- `inspect_lmdb.py`: inspects TemplateDB (sat_revsynth schema) and prints stats.
- `check_lmdb_simple.py`: generic LMDB inspection.
- `pack_for_cluster.py`: packages an experiment for cluster execution with a qsub script.

Plotting and analysis utilities:
- `plot_heatmap.py`: plots heatmap JSON from CLI output.
- `plot_alignment.py`: plots DTW alignment JSON.
- `plot_wire_stats.py`: scatter plot of wire activity.
- `plot_histogram.py`: histogram of heatmap values (expects a matrix-like JSON; may need adaptation for current heatmap output).
- `check_activity.py`: simulates toggles using a different gate semantics (Toffoli-style), so treat results with caution.
- `gen_random.py`: generates a random compact circuit string (wire charset limited to 64 by default).

## 6. Roadmap (from local_mix_notes_future_plan.md)

These items are not fully implemented yet. They represent the intended next-stage architecture.

### 6.1 Identity construction
- Generate identity circuits as `P · inv(P)` or `P1 · inv(P2)` where `P1` and `P2` are functionally equivalent but structurally distinct.

### 6.2 Reducer pipeline
- Multi-pass reducer with:
  - Local cancellation and commuting swaps.
  - Sliding window scan with canonical window keys.
  - `m`-schedule: start with small active-wire windows and increase.

### 6.3 Template DB schema and matching
- Store `template_id`, `gate_list`, `m_active_wires`, `canonical_hash`, optional `witness_hashes`.
- Exact-match lookup by canonical hash, optional approximate matching later.

### 6.4 Canonicalization for window matching
- First-occurrence relabeling to normalize wires in a window.
- Avoid factorial search by using deterministic wire ordering.

### 6.5 Witness prefilter
- Store small witness hashes to avoid full canonicalization on most windows.

### 6.6 Metrics
- Compression ratio under reducer as primary hardness metric.
- Track deletion counts, reducer rounds, and wire-interaction statistics.

### 6.7 SAT repair extension
- Replace a window with an approximate template, then synthesize a compensator circuit so the global function returns to identity.

### 6.8 Skeleton-guided identity insertion (idea)
- For the early phase of `butterfly`/`abutterfly`, add identity blocks using a local skeleton graph so insertions target sparse wire interactions instead of random gaps.
- Heuristic sketch: build/update a wire-interaction graph over a sliding window, pick low-degree wires first, then choose identity templates whose active wires best fill the current gaps.
- Expected effect: smarter identity insertions that increase coverage and reduce obvious structure before the main mixing/compression passes.

## 7. Agent Handoff Notes

When extending the system:
- Verify which LMDB schema you are using (`perm_tables_*` vs `templates_by_hash`).
- Keep gate semantics consistent (ECA57 rule) when adding scripts or analysis.
- Update both `local_mixing/docs/MASTER_README.md` and this doc when adding or removing CLI commands.
