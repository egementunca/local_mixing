# Local Mixing for Reversible-Circuit Obfuscation (Codebase-Aligned Plan)

This is the implementation-ready plan for a two-stage, local-rewrite obfuscator and its attack-aligned metrics. It updates prior identity-obfuscation notes to match the current `local_mixing` codebase.

Sources: 2024-006.pdf (mixing/complexity gap) and 2309.05731v3.pdf (entropy framing, k-string dynamics). These PDFs are not in-repo; the plan below stands alone.

---

## 0) Scope and target

- Target domain is reversible circuits in the ECA57 gate basis.
- Default use-case is identity obfuscation, but the rewrite dynamics are function-preserving for any permutation.
- Primary objective is to implement a **local rewrite dynamics** that can inflate and then **knead** redundancy, and to measure success using attacker-aligned metrics.

---

## 1) Current codebase assets to reuse

- Circuit model and rewiring: `local_mixing/src/infra/circuit/circuit.rs` (`CircuitSeq`, `rewire`, `unrewire`, `probably_equal`).
- Convex subcircuit selection: `local_mixing/src/infra/random/random_data.rs` (`find_convex_subcircuit`, `simple_find_convex_subcircuit`, `contiguous_convex`).
- Canonical window key (structure-only): `local_mixing/src/hashing/canonical.rs` (`canonical_window_key`).
- Truth-table hash (functional): `local_mixing/src/optimize/compress_sat.rs` (`compute_canonical_hash`).
- Template DB reader: `local_mixing/src/infra/store/reader.rs` (`TemplateDB`, `get_by_canonical_hash`, `get_smaller_equivalent`, `get_random_identity`).
- Perm table sampling: `local_mixing/src/algorithms/butterfly/replace.rs` (`compress_lmdb`, `random_perm_lmdb` helper, LMDB `n{N}m{M}` tables).
- Local reducer MVP: `local_mixing/src/algorithms/annealing/local.rs` (`reduce_circuit`, `mix_step`).
- Skeleton and wire-coverage helpers: `local_mixing/src/algorithms/identity_growth.rs` (`SkeletonGraph`) and `local_mixing/src/infra/random/random_data.rs` (`create_skeleton`).

---

## 2) Core definitions (implementation-level)

- Circuit `C` is a `CircuitSeq` with `|C|` gates and width `n`.
- Equivalent circuits share the same permutation: `C ~ C'` if `π_C = π_{C'}`.
- A **window** is a convex, weakly connected subcircuit extracted from `C` and made contiguous by `contiguous_convex`.
- **Complexity gap** for a window `W`: `CG_hat(W) = |W| - CC_hat(W)`, where `CC_hat` is the shortest equivalent circuit found by DB/SAT within a budget.

---

## 3) Algorithm overview (two-stage)

### 3.1 Stage A: Inflation (inject redundancy)

Goal: increase local complexity gap with small windows.

Mechanics:
- Sample small convex windows `ℓ_out` (typical 2..10 gates, active wires capped).
- Replace with a longer equivalent window `ℓ_in = ℓ_out + δ`.

Backends, in order:
- Perm tables (`n{N}m{M}`) for **same function, longer length** if available.
- Identity-template insertion (TemplateDB identity or perm-table identity) embedded into the window to increase length while preserving function.
- SAT fallback for bounded synthesis of longer circuits (future; needs same-permutation constraint).

### 3.2 Stage B: Kneading (spread redundancy, fixed size)

Goal: delocalize gaps so local peeling fails, without changing global size.

Mechanics:
- Sample larger convex windows `ℓ_knd >> ℓ_in`.
- Apply **same-size** equivalent replacements, or a sequence of smaller same-size replacements inside the window, to keep `|W'| = |W|`.

Practical implementation choices:
- For small windows, use perm tables to sample an alternative circuit at the same length.
- For large windows, apply `K` in-window rewrites using smaller same-size windows (keeps total window length unchanged).
- Optional SAT-based same-size synthesis is a future extension if we add a fixed-size constraint to the solver interface.

---

## 4) Window selection (weak connectivity + convexity)

### 4.1 Gate-level skeleton graph

- Nodes are gate indices.
- Undirected edge between two gates if they share any wire.
- A window is **weakly connected** if its induced subgraph is connected.

Implementation notes:
- Build adjacency by scanning gates and recording shared-wire collisions.
- Use BFS from a random seed gate to collect `ℓ` nodes.
- This is separate from `SkeletonGraph` (wire-level) and `create_skeleton` (dependency layering) but can reuse their collision logic (`Gate::collides_index`).

### 4.2 Convexity enforcement

- Use `contiguous_convex` to bubble the selected gate set into a contiguous block.
- If convexity fails, resample.

Deliverable API:
- `WindowSelector::sample(C, ℓ, max_wires) -> Window` returning contiguous range and a mapping for local rewiring.

---

## 5) Equivalence oracle (replacement source)

The oracle standardizes how windows are replaced across inflation, kneading, and attacker modes.

Required methods:
- `get_shortest(window)` for compression and attacker.
- `get_same_size_alt(window)` for kneading.
- `get_longer_alt(window, target_len)` for inflation.

Backend wiring:
- TemplateDB (LMDB `templates_by_hash`) using truth-table hash `compute_canonical_hash`.
- Perm tables (`n{N}m{M}`) for random sampling by permutation.
- SAT fallback (future) for size-bounded synthesis.

Important constraints:
- TemplateDB stores one record per `(width, gate_count, hash)`. Same-size alternatives are not guaranteed from this DB.
- Perm tables are the primary source for same-size alternatives when available.
- For windows wider than DB coverage, fall back to identity insertions or local commutation-based kneading.

---

## 6) Rewriter (extract, rewire, replace, splice)

Steps:
- Select a convex, connected window and extract gates.
- Rewire to a compact wire set (existing `CircuitSeq::rewire` and `unrewire` utilities).
- Query the oracle for replacement.
- Unrewire and splice back into the circuit.
- Optional correctness check with `CircuitSeq::probably_equal` for small widths.

---

## 7) Attacker model (peeling)

Define `attack_peel(C, ℓ_in, ℓ_out, trials_per_pass)`:

- Sample convex windows of length `ℓ_in`.
- Compute `CC_hat` using the oracle (`get_shortest`).
- If `CC_hat <= ℓ_out`, replace with the shortest known equivalent.
- Repeat until no improvements or a max pass count.

This attacker should be used as a **primary metric** before and after kneading.

---

## 8) Metrics and logging (minimum viable suite)

### 8.1 Gap spreading curve

For window sizes `s ∈ {4, 8, 16, 32, 64, ...}`:
- Sample windows and compute `CG_hat` where possible.
- Log mean, median, 95th percentile of `CG_hat` and fraction of windows with `CG_hat > 0`.

Expected behavior:
- After inflation: gaps appear at small `s`.
- After kneading: gaps shift to larger `s` and become harder to localize.

### 8.2 Attacker success curve

Log per pass:
- number of replacements
- total gate reduction
- time-to-stagnation

### 8.3 Reducibility power curve `M(k)`

Define `M(k)` as the fraction of gates removable using windows up to size `k`.
Lower is better for small `k`.

### 8.4 Local entropy proxy

For sampled windows at fixed size:
- `count_alternatives` = number of same-size alternatives in perm tables (approx or sampled).
- `S_hat = log2(count_alternatives + 1)`.

### 8.5 Sectoring diagnostics

Run multiple seeds from the same input; compute pairwise distances between outputs.
Feature options (all implementable locally):
- histogram of canonical window keys for small windows
- wire-triplet frequency sketches
- adjacency histogram (wire or gate-level)

---

## 9) Module plan (where code should live)

Suggested locations:
- Window selection: new module `local_mixing/src/algorithms/local_mixing/window.rs` or extend `infra/random/random_data.rs` with a gate-graph sampler.
- Equivalence oracle: new module `local_mixing/src/algorithms/local_mixing/oracle.rs`.
- Rewriter and stages: new module `local_mixing/src/algorithms/local_mixing/mixer.rs`.
- Attacker: new module `local_mixing/src/algorithms/local_mixing/attack.rs`.
- Metrics harness: new module `local_mixing/src/analysis/local_mixing_metrics.rs` and add summary to `analysis/metrics.rs`.
- CLI wiring: add a `local-mix2` or upgrade `local-mix` in `local_mixing/src/main.rs`.

---

## 10) Configuration knobs (implementation-ready)

- `inflation.window_len_min`, `inflation.window_len_max`
- `inflation.delta_min`, `inflation.delta_max`
- `inflation.max_active_wires`
- `kneading.window_schedule` (list of lengths)
- `kneading.inner_rewrites_per_window`
- `kneading.max_active_wires`
- `oracle.use_template_db`, `oracle.use_perm_tables`, `oracle.use_sat`
- `oracle.perm_table_max_width`, `oracle.template_db_max_width`
- `attacker.window_len`, `attacker.out_len`, `attacker.trials_per_pass`, `attacker.max_passes`
- `metrics.sample_count_per_size`, `metrics.seed_count`

---

## 11) Implementation milestones

1. DB audit and coverage report.
2. WindowSelector (connected + convex) with deterministic sampling and tests.
3. Oracle MVP.
4. Inflation stage using small windows and identity-template insertion.
5. Attacker `attack_peel` and gap curve metrics.
6. Kneading stage using same-size alternatives (perm tables) and in-window mixing.
7. Full metrics harness and JSON reporting.

---

## 12) Open decisions (need answers before coding)

- Which DBs are required for first milestone runs: TemplateDB only, perm tables only, or both.
- Maximum active-wire cap for window sampling, given DB coverage and SAT limits.
- Whether to keep identity-only targets for phase 1 or allow general permutations.
- Whether to store multiple same-size alternatives in a new LMDB table to improve kneading.

---

## 13) Success criteria

- Inflation alone increases `CG_hat` at small window sizes but is peelable by `attack_peel`.
- After kneading, attacker success drops materially at the same budget.
- Gap curve shifts to larger windows and does not collapse under small-window reduction.

---
