# Mixing Schemes Documentation

This document compares the primary obfuscation schemes in `local_mixing` and ties them to the current code layout.

## 1. Scheme A: Asymmetric Butterfly (current production path)
**Implementation**: `local_mixing/src/algorithms/butterfly/mixing.rs` and `local_mixing/src/algorithms/butterfly/replace.rs`

### Core architecture (current code)
0. **Optional pre-mix**: `B_{w,s}` shuffle + bit-flip stage (if enabled).
1. **Local perturbation**: `shoot_random_gate` reorders gates via commuting moves.
2. **Identity injection**: `replace_pairs` (and optional `random_gate_replacements`) substitutes adjacent pairs or single gates with identity templates.
3. **Block wrapping**: each gate is wrapped with random `R`/`R_inv` blocks (chained in `abutterfly_big` to avoid symmetry).
4. **Local compression**:
   - Non-SAT path: `expand_big` + `compress_big` (perm-table LMDB/SQLite).
   - SAT path: `compress_big_sat` or `compress_big_sat_lmdb` (TemplateDB + SAT fallback).
5. **Merge + bookends**: blocks are merged, then global bookends are added and compressed in chunks.

### Variants
- `bbutterfly`: big butterfly with optional ancilla expansion and SAT compression.
- `abbutterfly`: asymmetric chain of `R` blocks with TemplateDB/SAT compression.
- `abbutterfly --bookendless`: delays or skips some bookend effects.

### Status
- **Active**: this is the default production pipeline in the CLI.

---

## 2. Scheme B: Annealed Obfuscator (research path)
**Implementation**: `local_mixing/src/algorithms/annealing/anneal.rs` and `local_mixing/src/algorithms/annealing/local.rs`

### Core architecture
- Treats obfuscation as an optimization problem: find `C'` equivalent to `C` that is hard for a reducer to compress.
- Uses Metropolis-style acceptance with an energy function (adjacent cancels, witness hits, coverage, reducer compression).
- Move set (from `local.rs`): template insertion, commuting swaps, patch-pair insertions.

### Status
- **Partially integrated**: the annealing engine exists, but there is no CLI subcommand wired to run it yet.
- `local-mix` exposes the move set and a lightweight reducer for quick experiments.

---

## 3. Additional pipeline: Gadget-based obfuscator
**Implementation**: `local_mixing/src/obfuscate/*`

- Segmentation + commutator gadget injection + noise to target overhead.
- Conjugation and wire permutation are present but disabled to preserve semantics without wrapping.
- Used by the `obfuscate` CLI subcommand.

---

## 4. Comparison (current reality)

| Feature | Asymmetric Butterfly | Annealing | Gadget Obfuscator |
| --- | --- | --- | --- |
| **Approach** | Constructive (wrap/replace/compress) | Search-based (MCMC) | Constructive (gadgets + noise) |
| **Guidance** | Heuristic + template DB | Energy-driven | Heuristic |
| **Parallelism** | High (block-level) | Low (sequential) | Medium |
| **DB usage** | Perm tables + TemplateDB | Optional | None |
| **CLI status** | Active | Not wired | Active |

Note: `local-rewrite` is experimental and documented separately below.

---

## 5. Experimental: Local-rewrite (inflation + kneading)
**Implementation**: `local_mixing/src/algorithms/local_rewrite.rs`

- Two-stage rewrite pipeline with an attack-aligned metrics loop.
- Uses perm-table oracle when available; otherwise degrades gracefully.
- Exposed via CLI `local-rewrite`.
- Treated as an experimental research path, not a production scheme.
