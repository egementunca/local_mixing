# Compression Strategy: Three Paths

This document maps every compression/reduction path in the codebase, which CLI commands use each, and what databases they require.

## Path A — Butterfly Compression (`replace.rs`)

**Used by**: `abbutterfly`, `rac`, `compress` CLI commands

**Entry points**:
- `compress_big()` — LMDB-based, with identity detection
- `compress_big_ancillas()` — Same but extends subcircuits with ancilla wires
- `compress_big_sat()` — SAT solver compression (spawns Python)
- `compress_big_sat_lmdb()` — Hybrid: LMDB first, SAT fallback

**Flow**:
```
Circuit
  → find_convex_subcircuit() [3-7 active wires]
  → [Optional: extend with ancilla wires]
  → rewire to minimal wire indices
  → Identity check: ids_index::ids_rev_contains()
    ├─ YES → delete entire subcircuit (it's identity)
    └─ NO  → ids_index::ids_prefilter_hit()
              ├─ YES → remove_identity_window() [extract sub-identity]
              └─ NO  → continue
  → compress_lmdb() [query perm tables for shorter equivalent]
    ├─ Fast path: prepared SQL for n=7,m=4 and n=6,m=5
    └─ General: LMDB lookup in n{n}m{m} databases
  → unrewire back to original layout
  → splice replacement into circuit
  → deduplicate consecutive identical gates
```

**DB requirements**:
- LMDB at `./db` with: perm_w{3-7}, n{N}m{M}perms, ids_rev, ids_wit_prefilter
- SQLite at `./db/circuits.db` with rainbow tables
- **These are the 63GB databases in `local_mixing/db/`**

**Works?**: YES — this is the main compression and it uses the full LMDB.

---

## Path B — Annealing Reducer (`annealing/local.rs`)

**Used by**: `local-mix`, `lmdb` CLI commands

**Entry points**:
- `reduce_circuit()` which calls:
  1. `pass_cancel()` — adjacent identical gate removal
  2. `pass_commute_expose()` — random commuting swaps + cancel
  3. `pass_template_delete()` — windowed identity detection

**Flow**:
```
Circuit
  → Loop (max_passes, stop after max_stall with no progress):
    → pass_cancel: scan for g[i] == g[i+1], remove pairs
    → pass_commute_expose: random swaps of commuting gates, then cancel
    → pass_template_delete:
        → slide windows [4, 6, 8, 10, 12, 16]
        → count active wires per window
        → if active_wires ≤ 6:
            → brute-force 2^k state simulation
            → if identity: delete window
        → repeat up to 10 iterations
```

**DB requirements**:
- Function signature accepts `env: Option<&lmdb::Environment>` and `conn: Option<&rusqlite::Connection>`
- **BUT BOTH ARE UNUSED** (prefixed with underscore `_env`, `_conn`)
- Only brute-force checking is done

**Works?**: PARTIALLY — cancellation and commute-expose work, but template deletion is
limited to small windows (≤6 active wires) with no DB lookup. Large identities are invisible.

**Bug**: The `_env` and `_conn` parameters should be wired to `ids_index::ids_rev_contains()`
for DB-backed identity detection, exactly as Path A does it.

---

## Path C — Budgeted Reducer (`reducer/budget.rs`)

**Used by**: `local-rewrite` CLI command (attack phase)

**Entry points**:
- `reduce_budget()` with `ReducerConfig` (max_steps, window_sizes)

**Flow**:
```
Circuit
  → Loop (until no changes or budget exhausted):
    → Pass 1: adjacent identical gate cancellation
    → Pass 2: commute swap if it enables left or right cancellation
    → Pass 3: // TODO: Template-based reduction (requires template DB)
```

**DB requirements**:
- NONE — no database parameters accepted
- Pass 3 is a TODO stub

**Works?**: PARTIALLY — cancellation and targeted commute-swap work, but template
matching is entirely unimplemented.

---

## Comparison

| Feature | Path A (Butterfly) | Path B (Annealing) | Path C (Budget) |
|---------|-------------------|-------------------|-----------------|
| Adjacent cancel | via dedup | `pass_cancel` | Pass 1 |
| Commute expose | No | Random swaps | Targeted swaps |
| Identity DB lookup | YES (ids_index) | NO (unused params) | NO (not implemented) |
| Perm table rewrite | YES (LMDB) | NO | NO |
| Ancilla extension | YES | NO | NO |
| SAT compression | YES (optional) | NO | NO |
| Window sizes | Convex 3-7 wires | Fixed [4-16] | Configurable |
| Active wire limit | 3-7 | ≤6 | N/A |
| Budget control | Trial count | Pass/stall count | Step count |

---

## Fix Plan

### Fix B: Wire `pass_template_delete` to LMDB

1. Remove underscore from `_env` → `env`, `_conn` → `conn`
2. Open `ids_rev` and `ids_wit_prefilter` databases from `env`
3. Before brute-force, call `ids_rev_contains(env, db, window_gates)`
4. For windows with >6 active wires, use DB-only mode (skip brute force)
5. Expand window sizes to `[4, 6, 8, 10, 12, 16, 20, 24]` when DB available

### Fix C: Implement `reduce_budget` Pass 3

1. Add `env: Option<&lmdb::Environment>` parameter
2. In the loop, after Pass 2, slide windows from `config.window_sizes`
3. Canonicalize each window, call `ids_rev_contains()`
4. If identity found: drain gates, increment `template_hits`
5. Set `changed = true` to continue outer loop
