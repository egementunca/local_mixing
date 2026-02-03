# Template-Seeded Identity Growth Plan

## 0. Goal
Build large identity circuits by placing many small identity templates (limited active wires) into an empty W-wire circuit, then mix and partially compress in repeated rounds to produce large, hard-to-reduce identities.

This plan is meant to align with existing local_mixing building blocks and databases, while adding a new top-level workflow.

## 1. Existing Building Blocks to Reuse

Identity sources:
- `random_canonical_id` (perm tables in `./db`): builds a nontrivial identity by concatenating two circuits with the same permutation and reversing one half.
- `TemplateDB::get_random_identity` via `random_stored_id` (TemplateDB `collection.lmdb`).

Mixing and diffusion:
- `shoot_random_gate` for commuting drift (`infra/random/random_data.rs`).
- `mix_step` in `algorithms/annealing/local.rs` for local mixing moves.

Compression:
- `compress_big`, `compress_big_sat`, `compress_big_sat_lmdb` in `algorithms/butterfly/replace.rs`.
- `reduce_circuit` in `algorithms/annealing/local.rs` for a reducer-style hardness estimate.

Data models:
- `CircuitSeq` + `rewire` utilities in `infra/circuit/circuit.rs`.
- Template DB reader in `infra/store/reader.rs`.

## 2. Template Inventory Check (DB Audit)

Before building the generator, validate what identity templates are available and in what widths.

Minimum report per DB:
- widths present
- gate-count buckets
- counts of identity records per width and gate count

Sources to check:
- Local perm tables in `./db` (identity permutations for `n{N}m{M}`).
- TemplateDB `collection.lmdb` (identity records by canonical hash).

Suggested audit steps:
1. Use `local_mixing/scripts/inspect_lmdb.py` to enumerate TemplateDB widths and gate counts.
2. Add a small helper (or extend an existing script) to scan LMDB perm tables to confirm that identity permutations exist for target widths and provide counts per `(N, M)`.
3. Decide the template width range to target (for example 3..7) based on actual counts.

Deliverable: a short CSV or JSON summary of identity template availability.

## 3. Core Pipeline (Template-Seeded Growth)

### 3.1 Stage A: Seed placement
Inputs:
- target width `W`
- template width range `m_min..m_max`
- placement count or target gate budget

Algorithm:
1. Start with an empty `CircuitSeq` and a `SkeletonGraph` (see Section 4).
2. For each placement:
   - choose a template width `m`
   - sample an identity template `T` from either TemplateDB or perm tables, with gate-count constraints
   - optionally wrap `T` with conjugation `R . T . R^-1` (random `R`) to inflate and harden
   - run a quick reducer pass; reject templates below a minimum survival ratio
   - choose a wire subset `S` of size `m` using the skeleton heuristic
   - rewire `T` onto `S`
   - insert `T` at a random or scheduled position in the big circuit
   - update the skeleton graph with gates from `T`

This yields a large identity because each inserted template is an identity on its own, and the concatenation of identities remains identity.

### 3.2 Stage B: Mixing and diffusion
After seeding a batch:
- apply `shoot_random_gate` to drift gates and interleave templates
- optionally run a few `mix_step` rounds (template insertion + commuting swaps) to break contiguous blocks

Goal: diffuse local structure so templates are not trivially visible.

### 3.3 Stage C: Bounded compression
Run compression with a small reducer budget to remove only the most obvious cancellations, but avoid collapsing to zero.

Practical guardrails:
- cap compression window counts (`compression_window_size`, `compression_window_size_sat`)
- stop if the compression ratio falls below a survival threshold
- optionally skip the strongest passes (SAT) for early rounds

### 3.4 Stage D: Repeat
Repeat A -> B -> C for `R` rounds, increasing the gate budget each round or adjusting template sizes.

## 4. Skeleton Graph (Placement Guidance)

The skeleton graph captures how wires interact:
- node = wire
- edge weight increments for each gate that touches both wires

Use it to guide placement:
- prefer wire subsets that increase connectivity of low-degree wires
- avoid repeatedly using the same pairs (keeps interaction graph rich)
- target a minimum connected component size (avoid isolated wire groups)

Heuristic examples:
- pick `S` by sampling low-degree wires first, then add neighbors to grow a connected subgraph
- if density is too high early, bias towards disjoint subsets; later rounds, bias towards overlap

## 5. Metrics (Hardness and Growth)

Track at least:
- total gate count and growth per round
- wire coverage (percent of wires touched)
- skeleton graph stats (avg degree, component sizes)
- reducer compression ratio: `len_after / len_before` using `reduce_circuit`
- number of template deletions found by the reducer (optional)
- template skip rate (failed hardness/size checks)

## 6. Proposed Configuration Knobs

New config or CLI options (proposal):
- `identity_growth.rounds`
- `identity_growth.target_gate_count` or `placements_per_round`
- `identity_growth.template_width_min/max`
- `identity_growth.template_gate_count_min/max`
- `identity_growth.template_source` (perm_tables, template_db, mixed)
- `identity_growth.template_attempts`
- `identity_growth.template_conjugation_depth_min/max`
- `identity_growth.template_hardness_passes`
- `identity_growth.template_min_reducer_ratio`
- `identity_growth.mix_passes`
- `identity_growth.compression_budget`
- `identity_growth.min_survival_ratio`
- `identity_growth.skeleton_mode` (balanced, dense, sparse)

## 7. Implementation Plan (Phased)

Phase 0: DB audit
- Implement the identity-template inventory report.
- Decide default width range based on actual counts.

Phase 1: Core generator
- Add a new module (for example `algorithms/identity_growth.rs`).
- Implement `seed_identity(W, opts) -> CircuitSeq`.
- Add wire-subset selection with skeleton graph guidance.

Phase 2: Mix + compress loop
- Integrate mixing (`shoot_random_gate` and/or `mix_step`).
- Integrate bounded compression with survival thresholds.

Phase 3: CLI + metrics
- Add a new CLI subcommand (for example `grow-identity`).
- Output a summary report (gate counts, coverage, reducer ratio).

Phase 4: Iterative tuning
- Adjust placement heuristics based on reducer performance.
- Try schedule variants (small templates first, then larger).

## 8. Risks and Mitigations

Risk: template database lacks enough identity records in target width range.
- Mitigation: widen the width range, or import more identity templates.

Risk: compression collapses to zero.
- Mitigation: enforce min-survival ratio and limit SAT passes.

Risk: poor wire coverage leads to reducible subcircuits.
- Mitigation: skeleton-guided placement that forces coverage.

Risk: DB confusion (TemplateDB vs perm tables).
- Mitigation: make the source explicit in config and log it at runtime.
