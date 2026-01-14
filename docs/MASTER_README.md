# Local Mixing - Codebase Guide (Current State)

This document is the consolidated reference for the `local_mixing` crate. It reflects the current code layout, data model, algorithms, CLI surface, and integration points. Use it as the primary onboarding doc for coding agents.

## 0. Scope and Intent

`local_mixing` is a Rust framework for reversible circuit obfuscation and analysis. It works with an ECA57-style 3-pin gate where the target flips when `c1 OR !c2`. Gates are self-inverse, so a circuit inverse is the gate list in reverse order.

Primary workflows:
- Build rainbow tables (SQLite + LMDB) to look up canonical equivalents.
- Obfuscate circuits using butterfly-family algorithms, optional ancilla expansion, and optional SAT-based compression.
- Measure obfuscation using heatmaps, alignment, and reducer-style compression.

## 1. Repository Map (local_mixing)

### 1.1 src/
- `local_mixing/src/main.rs`: CLI entrypoint and command wiring. Handles SQLite/LMDB loading and writes standard output artifacts (for example `recent_circuit.txt`).
- `local_mixing/src/lib.rs`: module exports.
- `local_mixing/src/config.rs`: `ObfuscationConfig` (butterfly family) and `ObfConfig` (obfuscate pipeline).
- `local_mixing/src/algorithms/butterfly/mixing.rs`: butterfly, butterfly_big, abbutterfly_big, bookendless variant, merging, final compression, kill handler.
- `local_mixing/src/algorithms/butterfly/replace.rs`: identity generation, pair replacement, random gate replacement, convex subcircuit selection, expand/compress logic, SAT/LMDB compression, timers.
- `local_mixing/src/algorithms/annealing/anneal.rs`: simulated annealing engine (moves, energy, stats). Not wired to the CLI yet.
- `local_mixing/src/algorithms/annealing/local.rs`: local mixing MVP and local reducer; used by `local-mix` CLI.
- `local_mixing/src/obfuscate/config.rs`: level presets and core config for the gadget-based pipeline.
- `local_mixing/src/obfuscate/gadgets.rs`: commutator and identity gadget generators.
- `local_mixing/src/obfuscate/mixer.rs`: random mixer generation, coverage and reducibility estimates.
- `local_mixing/src/obfuscate/passes.rs`: segmentation, gadget injection, noise, simple compression, verification.
- `local_mixing/src/reducer/budget.rs`: budgeted reducer (adjacent cancel + commute). Template matching is TODO.
- `local_mixing/src/optimize/compress_sat.rs`: SAT synthesis integration via Python (`sat_revsynth/scripts/synthesize_from_tt.py`).
- `local_mixing/src/analysis/alignment/mod.rs`: DTW alignment, distance matrix, tracing.
- `local_mixing/src/analysis/alignment/README.md`: alignment overview.
- `local_mixing/src/analysis/metrics.rs`: `ObfReport` and `BenchmarkStats`.
- `local_mixing/src/hashing/canonical.rs`: canonical window hash and energy cache for annealing.
- `local_mixing/src/infra/circuit/circuit.rs`: core data structures, serialization, encoding, rewire utilities, evaluation.
- `local_mixing/src/infra/rainbow/canonical.rs`: canonicalization of permutations, caching, and `CircuitSeq::canonicalize`.
- `local_mixing/src/infra/rainbow/README.md`: conceptual overview of canonicalization.
- `local_mixing/src/infra/random/random_data.rs`: random circuits, shooting, convex subcircuits, heatmap data, DB generation helpers.
- `local_mixing/src/infra/store/reader.rs`: TemplateDB reader for LMDB `templates_by_hash`.
- `local_mixing/src/infra/store/schema.rs`: TemplateRecord layout and parsing.
- `local_mixing/src/bin/import_rainbow.rs`: import JSON rainbow data into LMDB perm tables.

### 1.2 scripts/
See `local_mixing/docs/obfuscation_methods.md` for a full script inventory and purpose.

### 1.3 docs/
- `local_mixing/docs/README.md`: documentation index.
- `local_mixing/docs/MASTER_README.md`: this document (primary reference).
- `local_mixing/docs/obfuscation_methods.md`: methods, experiments, and roadmap.
- `local_mixing/docs/obfuscation_metrics.md`: proposed metrics for quantifying obfuscation quality.
- `local_mixing/docs/obfuscation_parameters.md`: parameter catalog (defaults and hardcoded values).
- `local_mixing/docs/DATABASES.md`: data schema details and compressor/database mapping.
- `local_mixing/docs/SCHEMES.md`: high-level scheme comparison.
- `local_mixing/docs/CHANGELOG.md`: summary of changes vs the bin branch.
- `local_mixing/docs/IDENTITY_GROWTH_PLAN.md`: plan for template-seeded identity growth and mixing.
- `local_mixing/docs/ARCHITECTURE.md`: historical architecture notes (legacy).

### 1.4 other files
- `local_mixing/local_mix_notes_future_plan.md`: roadmap notes for reducer/obfuscator co-design.
- `local_mixing/test.circuit`: example circuit.

## 2. Circuit Model and File Formats

### 2.1 Gate semantics
- Gate representation: `[target, c1, c2]` (u8 pins).
- ECA57 rule: `target ^= c1 | !c2`.
- Self-inverse: the inverse of a circuit is the gate list reversed.

### 2.2 Circuit encoding
- `CircuitSeq::repr()` encodes a circuit into a compact ASCII string with `;` separators.
- Wire encoding uses a base-83 alphabet: `0-9 a-z A-Z ! @ # $ % ^ & * ( ) - _ = + [ ] { } < > ?`.
- For wire indices >= 83, `~` prefixes are used as overflow (each `~` adds 83).
- `CircuitSeq::from_string()` parses the compact form.
- `CircuitSeq::to_string(num_wires)` renders an ASCII diagram plus the compact string.

### 2.3 Common file outputs
- `recent_circuit.txt`: last obfuscated circuit (compact format).
- `start.txt`: original circuit before obfuscation (compact format).
- `butterfly_recent.txt`, `butterfly.txt`: concatenated before:after pairs for butterfly runs.
- `good_id.txt`: appended list of successful identities from `mix`/`butterfly`.
- `compressed.txt`: output from `compress` command.
- `wire_plot.png`: output from `wiredot` command.

## 3. Storage and Databases

### 3.1 SQLite `circuits.db`
Used for enumeration and canonicalization lookups.

Dynamic table per `(N, M)`: `n{N}m{M}`
- `circuit BLOB` (3 bytes per gate)
- `perm BLOB` (canonical permutation table)
- `shuf BLOB` (wire relabeling used to reach canonical form)

### 3.2 LMDB (local_mixing perm tables)
Stored under `./db` by default.

Key tables:
- `perm_tables_n{N}`: permutation -> list of gate counts (`m`) that share the same permutation.
- `n{N}m{M}`: perm||circuit keys used to enumerate or check equivalences.
- `n{N}m{M}perms`: circuit -> perm||shuffle (used by `compress_lmdb`).

These are built via:
- `local_mixing/src/bin/import_rainbow.rs` (JSON import).
- `local_mixing_bin lmdb` and `local_mixing_bin lmdbcounts` (SQLite -> LMDB conversions).

### 3.3 LMDB TemplateDB (sat_revsynth schema)
This is a separate schema used for template lookups in `compress_big_sat_lmdb` and `replace_pairs`.

- DB name: `templates_by_hash` inside `collection.lmdb`.
- Key (36 bytes): `basis_id (u8)` + `width (u8)` + `gate_count (u16 LE)` + `canonical_hash (32 bytes)`.
- Value layout: see `local_mixing/src/infra/store/schema.rs` (`TemplateRecord`).

### 3.4 Compressor/DB mapping (quick reference)
- `compress` / `compress_exhaust`: SQLite `circuits.db` tables `n{N}m{M}`.
- `compress_lmdb` / `expand_lmdb`: local_mixing LMDB `n{N}m{M}` for replacements, plus `n{N}m{M}perms` or SQLite for perm/shuf.
- `compress_big`: currently does **not** call `compress_lmdb` (subcircuit path commented out).
- `compress_big_ancillas`: uses `compress_lmdb` (local_mixing LMDB).
- `compress_big_sat`: SAT only (no DB).
- `compress_big_sat_lmdb`: TemplateDB `templates_by_hash` (collection.lmdb) with SAT fallback.
- `replace_pairs`: TemplateDB when `--lmdb-db` is provided; otherwise uses `random_canonical_id` (local_mixing LMDB).

Note: `--lmdb-db` points to the TemplateDB (`collection.lmdb`). The local_mixing LMDB tables are opened from `./db` in current CLI code; `ObfuscationConfig.lmdb_path` is not wired yet.

### 3.5 Data generation pipeline
- Enumeration: `obfuscated-circuits/go-proj` can generate JSON rainbow tables.
- Import: `local_mixing/src/bin/import_rainbow.rs` ingests JSON into LMDB tables.
- SQLite table generation: `local_mixing/scripts/generate_rainbow_tables.py` runs `init` and `load`.

## 4. Algorithms (Current)

### 4.1 Butterfly family (obfuscation + compression)
- `butterfly`: wraps each gate with a fixed random identity and compresses outward.
- `butterfly_big` (bbutterfly):
  - `replace_pairs` and optional `random_gate_replacements`.
  - `shoot_random_gate` (global and block-level).
  - per-gate blocks `R_inv · g · R`, optional ancilla expansion, local compression.
  - merge blocks, add bookends, final compression loop.
- `abbutterfly_big`: asymmetric chain of `R` values per gate, plus block compression and merge. Supports SAT mode and LMDB-first SAT.
- `abbutterfly_big_bookendsless`: delays bookends and compresses midstream, then optional final compression.

### 4.2 Compression and expansion
- `expand_big`: rewires subcircuits to use extra wires (ancillas) before compression.
- `compress_big`: convex subcircuit selection; currently uses a placeholder `subcircuit.clone()` (LMDB path is commented out).
- `compress_lmdb`: full LMDB/SQLite compression using canonicalization and `n{N}m{M}perms` tables.
- `compress_big_sat`: SAT-based subcircuit synthesis (Python script).
- `compress_big_sat_lmdb`: LMDB lookup first, SAT fallback.
- `compress_big_ancillas`: ancilla expansion plus `compress_lmdb`.
- `compress`: small subcircuit compression using SQLite lookups.

### 4.3 Pair replacement and shooting
- `replace_pairs`: taxonomy-based replacement of adjacent gate pairs with identity templates (TemplateDB or `random_canonical_id`).
- `random_gate_replacements`: optional single-gate expansion using identity templates.
- `shoot_random_gate`: random gate insertions to add noise.

### 4.4 Local mixing (annealing/local.rs)
- `mix_circuit`: repeated local moves (template insertion, commuting swaps, patch pairs).
- `reduce_circuit`: cancellation + commuting swaps + small-window identity checks.

### 4.5 Annealed obfuscator (annealing/anneal.rs)
Defines a full simulated annealing engine (moves, energy functions, stats), but it is not wired to CLI entrypoints yet.

### 4.6 Obfuscate pipeline (obfuscate/*)
- Segmentation -> gadget injection -> noise -> simple compression -> verification.
- Conjugation and wire permutation are disabled in code because they change the global function without additional wrapping.

### 4.7 Reducers
- `reducer/budget.rs`: budgeted cancellation + commute (template matching TODO).
- `algorithms/annealing/local.rs`: includes a window identity detector for small active wire sets.

### 4.8 Hashing and canonicalization
- `hashing/canonical.rs`: canonical window hash and an LRU-ish energy cache.
- `infra/rainbow/canonical.rs`: permutation canonicalization (`fast_canon` + `brute_canonical`), caching, `CircuitSeq::canonicalize`.

### 4.9 Analysis and metrics
- Heatmap generation: `generate_heatmap_data` (normalized Hamming distance across gate positions).
- Alignment: DTW in `analysis/alignment/mod.rs` with a distance matrix over traced states.
- Metrics: `ObfReport` and `BenchmarkStats` for obfuscation evaluation.

## 5. CLI Reference (local_mixing_bin)

Core commands:
- `init -n N`: initialize SQLite tables for width `N`.
- `load -n N -m M`: build/load rainbow tables into SQLite for `(N, M)`.
- `random -n N -m M -c COUNT` or `-C`: generate random circuits and store in SQLite.
- `gen --wires N --length M`: print a random circuit to stdout (compact format).
- `mix -r ROUNDS`: obfuscate and target-compress `initial.txt`.
- `butterfly -r ROUNDS`: standard butterfly.
- `bbutterfly`: big butterfly with CLI flags for shooting, ancillas, single-gate, config.
- `abbutterfly`: asymmetric big butterfly with `--sat`, `--bookendless`, `--lmdb-db`, `--config`.
- `compress -p PATH -n WIRES`: run final compression and write `compressed.txt`.
- `obfuscate -i INPUT -o OUTPUT`: gadget-based obfuscator, optional JSON report.
- `local-mix`: generate a 64-wire identity via local mixing and report reducer ratio.
- `heatmap --c1 --c2 --num_wires --inputs`: output heatmap JSON to stdout.
- `align --c1 --c2 -n --inputs`: output alignment JSON to stdout.
- `reverse -s SRC -d DST`: reverse gate order and write output.
- `wiredot -n --path`: generate `wire_plot.png`.
- `lmdb -n N -m M`: build LMDB tables for perm lookups.
- `lmdbcounts`: build perm_tables_n{N} DBs in LMDB.
- `string -s SRC -d DST`: pretty-print a circuit (ASCII diagram + compact form).

Declared but not implemented (CLI defined without match arm):
- `explore` (no handler).
- `binload` (no handler).

## 6. Configuration

### 6.1 ObfuscationConfig (butterfly family)
Defined in `local_mixing/src/config.rs`, supports JSON overlay via `--config`.

Key fields:
- Structure size: `structure_block_size_min`, `structure_block_size_max`.
- Move probabilities: `mix_prob_template`, `mix_prob_swaps`, `mix_prob_patch`.
- Intensity: `shooting_count`, `shooting_count_inner`, `rounds`.
- Modes: `sat_mode`, `no_ancilla_mode`, `single_gate_mode`, `skip_compression`.
- Compression: `compression_window_size`, `compression_window_size_sat`, `compression_sat_limit`, `final_stability_threshold`, `chunk_split_base`.
- Reducer: `reducer_active_wire_limit`, `reducer_window_sizes`.
- System: `lmdb_path`.

### 6.2 ObfConfig (gadget-based obfuscator)
Defined in `local_mixing/src/obfuscate/config.rs` with level presets 1-5.

Key fields:
- `segments`, `gadget_size`, `mixer_depth`.
- `target_overhead`, `min_survival_ratio`, `reducer_probe`.
- `max_attempts`, `reducer_budget`, `verify`.
- `seed`, `num_wires`.

### 6.3 MixConfig and ReduceConfig (local-mix)
Defined in `local_mixing/src/algorithms/annealing/local.rs`:
- `MixConfig`: `num_wires`, `num_rounds`, `template_size_max`.
- `ReduceConfig`: `max_passes`, `max_stall`.

## 7. Outputs and Artifacts

Common output files written by CLI tools:
- `recent_circuit.txt`: most obfuscation commands.
- `start.txt`: original circuit in butterfly family.
- `butterfly_recent.txt`, `butterfly.txt`: before:after paired strings.
- `good_id.txt`: appended identities from `mix` and `butterfly`.
- `compressed.txt`: `compress` output.
- `wire_plot.png`: `wiredot` output.

## 8. Related Modules and Integrations

- `sat_revsynth`: SAT-based synthesis and TemplateDB generation, used by `compress_sat` and `TemplateDB`.
- `obfuscated-circuits/go-proj`: enumeration and JSON export for rainbow tables.
- `identity-factory-*`: UI/API components in this repo; not directly used by `local_mixing`.

## 9. Known Gaps and Mismatches (Current)

- `compress_big` has the LMDB compression path commented out, so it currently does not reduce subcircuits beyond local dedup.
- `obfuscate/passes.rs` disables conjugation and wire permutation (function changes without wrapping).
- `annealing/anneal.rs` is not wired to any CLI command.
- CLI subcommands `explore` and `binload` are defined but have no implementation in `main.rs`.
- Some scripts use alternate gate semantics or assume different heatmap formats (see `local_mixing/docs/obfuscation_methods.md`).
