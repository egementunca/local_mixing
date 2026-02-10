# Obfuscation Parameters (Current)

This document catalogs the parameters that affect obfuscation and compression. It separates:
- Configurable values (in config structs and CLI overrides).
- Hardcoded values embedded in the butterfly/replace pipeline.

Paths reflect the current layout (`src/algorithms/*`, `src/infra/*`).

## 1. Configurable parameters (ObfuscationConfig)

Defined in `local_mixing/src/config.rs` and loadable via `--config` for `bbutterfly`/`abbutterfly`.

| Field | Default | Used by | Notes |
| --- | --- | --- | --- |
| `structure_block_size_min` | 10 | `butterfly_big`, `abbutterfly_big` | Min length for random `R` circuits. |
| `structure_block_size_max` | 30 | `butterfly_big`, `abbutterfly_big` | Max length for random `R` circuits. |
| `single_gate_replacements` | 500 | `random_gate_replacements` | Count of single-gate replacements when enabled. |
| `shooting_count` | 500000 | `shoot_random_gate` | Global shooting at start of rounds. |
| `shooting_count_inner` | 0 | `shoot_random_gate` | Shooting inside each block. |
| `rounds` | 3 | `main_butterfly_big`, `main_butterfly_big_bookendsless` | Number of obfuscation rounds. |
| `sat_mode` | true | `compress_big_sat`, `compress_big_sat_lmdb` | SAT-based compression in block pipeline. |
| `no_ancilla_mode` | false | `expand_big` | Disables ancilla expansion when true. |
| `single_gate_mode` | false | `random_gate_replacements` | Enables single-gate replacement pass. |
| `skip_compression` | false | `compress_big`, bookendless final pass | Inflation-only runs. |
| `compression_window_size` | 100 | `expand_big`, `compress_big` | Trial count / window budget for non-SAT compression. |
| `compression_window_size_sat` | 10 | `compress_big_sat`, `compress_big_sat_lmdb` | Trial count / window budget for SAT compression. |
| `compression_sat_limit` | 1000 | SAT scripts | Timeout/conflict limit for SAT synthesis. |
| `final_stability_threshold` | 12 | final compression loops | Passes with no change before stopping. |
| `chunk_split_base` | 1500 | final compression | Chunk size base for parallel compression. |
| `shuffle_bitflip.enabled` | false | `abbutterfly_big` | Enables pre-mix `B_{w,s}` stage. |
| `shuffle_bitflip.flip_mode` | `none` | `shuffle_bitflip` | `none`, `separate` (Style A), `embedded` (Style B). |
| `shuffle_bitflip.flip_scope` | `global` | `abbutterfly_big` | Only `global` is wired; `per-stage` parsed but not implemented. |
| `shuffle_bitflip.seed` | `None` | `abbutterfly_big` | RNG seed for reproducibility. |
| `shuffle_bitflip.gadget_library_path` | `None` | `shuffle_bitflip` | JSON gadget library for Style B. |
| `shuffle_bitflip.flip_probability` | `0.0` | `shuffle_bitflip` | Flip probability for random masks (CLI default `0.5`). |
| `reducer_active_wire_limit` | 6 | (unused) | Declared but not wired in current reducers. |
| `reducer_window_sizes` | [4,6,8,10,12,16] | (unused) | Declared but not wired in current reducers. |
| `lmdb_path` | `db` | (unused) | Local LMDB path; not wired to CLI yet. |
| `mix_prob_template` | 0.40 | (unused) | Declared but not used in current pipeline. |
| `mix_prob_swaps` | 0.50 | (unused) | Declared but not used in current pipeline. |
| `mix_prob_patch` | 0.10 | (unused) | Declared but not used in current pipeline. |

## 2. Hardcoded parameters in butterfly/replace pipeline

These values are embedded in code and are easy to miss during tuning.

| Parameter | Default | Location | Notes |
| --- | --- | --- | --- |
| `butterfly` R size | 3..=25 | `local_mixing/src/algorithms/butterfly/mixing.rs` (`butterfly`) | Not tied to `ObfuscationConfig`. |
| `obfuscate` R size | 3..=25 | `local_mixing/src/algorithms/butterfly/replace.rs` (`obfuscate`) | Used by `main_mix`. |
| `replace_pairs` width | 3..=5 | `local_mixing/src/algorithms/butterfly/replace.rs` | Random template width for pair replacement. |
| TemplateDB gate-count filter | 6..=40 | `local_mixing/src/algorithms/butterfly/replace.rs` (`random_stored_id`) | Limits which template sizes are sampled. |
| `random_gate_replacements` width | 3..=7 | `local_mixing/src/algorithms/butterfly/replace.rs` | Random identity width for single-gate replacement. |
| `expand_big` max wires | 7 | `local_mixing/src/algorithms/butterfly/replace.rs` | Upper bound for ancilla expansion. |
| `expand_big` random wire range | 3..=7 | `local_mixing/src/algorithms/butterfly/replace.rs` | Convex subcircuit wire budget. |
| `compress_big` random wire range | 5..=7 | `local_mixing/src/algorithms/butterfly/replace.rs` | Convex subcircuit wire budget. |
| `compress_big_sat` random wire range | 3..=6 | `local_mixing/src/algorithms/butterfly/replace.rs` | SAT window wire budget. |
| `random_subcircuit` length | 1,2,4,8 | `local_mixing/src/algorithms/butterfly/replace.rs` | Log-scale subcircuit length selection. |

## 3. Local mixing parameters (annealing MVP)

Defined in `local_mixing/src/algorithms/annealing/local.rs` and wired to the `local-mix` CLI:

| Parameter | Default | Notes |
| --- | --- | --- |
| `MixConfig.template_size_max` | 8 | Upper bound for template width. |
| Move probabilities | 40% insert, 50% swap, 10% patch | Hardcoded in `mix_step`. |
| Swap burst count | 10..100 | Hardcoded in `mix_step` (Move B). |
| Patch width | 3..=8 | Hardcoded in `mix_step` (Move D). |
| `ReduceConfig.max_passes` | 200 | Set in `main.rs` for `local-mix`. |
| `ReduceConfig.max_stall` | 10 | Set in `main.rs` for `local-mix`. |
| Reducer window sizes | [4,6,8,10,12,16] | Hardcoded in `pass_template_delete`. |

## 4. Gadget obfuscator parameters (obfuscate pipeline)

Defined in `local_mixing/src/obfuscate/config.rs` via level presets 1–5:
- `segments`, `gadget_size`, `mixer_depth`, `target_overhead`.
- `min_survival_ratio`, `reducer_probe`, `max_attempts`, `reducer_budget`, `verify`.
- `seed`, `num_wires`.

The gadget sizes are further scaled inside `local_mixing/src/obfuscate/passes.rs`:
- Boundary gadget size: `cfg.gadget_size / 2` (min 3).
- Noise gadget size: `cfg.gadget_size / 3` (min 2).

## 5. CLI defaults and overrides

These are defaults defined directly in `local_mixing/src/main.rs` (not config):
- `bbutterfly --shooting` default: 500000.
- `bbutterfly/abbutterfly --n` default: 32.
- `abbutterfly --rounds` default: uses config if omitted.
- `obfuscate --level` default: 3.
- `obfuscate --wires` default: 64.
