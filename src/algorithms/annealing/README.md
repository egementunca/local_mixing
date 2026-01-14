# Annealing (Simulated) - Current Status

This module contains a simulated-annealing obfuscator draft plus the local-mixing MVP.
The annealing engine in `anneal.rs` compiles and has unit tests, but it is not wired to the CLI yet.
The only annealing-related path currently exposed in the CLI is the `local-mix` command from `local.rs`.

## Status at a glance

- `anneal.rs`: full annealing loop exists, but is not used by any CLI or scripts.
- `local.rs`: MVP local-mixing and reducer are wired to `local-mix`.
- Template DB integration in `anneal.rs` is a placeholder (no LMDB queries yet).
- Energy model in `anneal.rs` is partially stubbed (witness hits are not computed).

## What is implemented in `anneal.rs`

- Temperature schedule: exponential from `t0` to `t_end`.
- Metropolis acceptance with `accept_move`.
- Move set:
  - Insert template: identity gate pairs (or adversarially selected gate pairs).
  - Commute swap: uses `reducer::gates_commute`.
  - Patch pair: inserts a random patch and its inverse at a different position.
- Energy functions:
  - `energy_fast`: adjacent-cancel rate + wire coverage (witness hits are set to 0).
  - `energy_slow`: uses `reducer::reduce_budget` compression ratio + wire coverage.
- Stats: accept/reject counts, move counts, checkpointed energy and temperature traces.

## What is missing or stubbed in `anneal.rs`

- No CLI subcommand or JSON output for runs.
- `lmdb_path` exists in `AnnealParams` but is unused.
- Insert-template move does not query TemplateDB; it uses synthetic identity pairs.
- `witness_hits` scoring is not implemented.
- No integration with `analysis/metrics`, DTW, or trace-based metrics.

## Related components (currently used)

- `local.rs` (local-mix):
  - `mix_step` inserts templates (LMDB/SQLite when available, else synthetic `P . P^-1`),
    commuting swaps, and patch-pair insertions.
  - `reduce_circuit` runs cancel + commute + naive small-window identity checks.
- `reducer/budget.rs`:
  - Used by `anneal.rs` for slow energy evaluation.

## Suggested integration steps (if you want to finish it)

1. Add a CLI subcommand that calls `anneal_run` and writes a JSON report.
2. Wire TemplateDB lookups into Move A, and implement witness-hits scoring.
3. Decide whether to use `reduce_budget` or the local reducer as the primary attacker model.
4. Add experiment scripts that run `anneal_run` directly (instead of approximating with `abbutterfly`).

## Background papers

- Quantum Circuit Unoptimization: https://arxiv.org/pdf/2311.03805
- Reversible Circuit Rewriting with Simulated Annealing: https://msoeken.github.io/papers/2015_vlsisoc.pdf
