# ABButterfly Obfuscation Reference

> **Formal, comprehensive documentation of the Asymmetric Big Butterfly (abbutterfly) reversible circuit obfuscation scheme.**

---

## 1. Mathematical Foundation

### 1.1 Problem Statement

Given a reversible circuit $C$ implementing a permutation $\pi: \mathbb{Z}_2^n \to \mathbb{Z}_2^n$, produce an obfuscated circuit $C'$ such that:

1. **Functional Equivalence**: $\pi_{C'} = \pi_C$ (identical input-output behavior)
2. **Structural Divergence**: $C'$ resists automated reduction to $C$ or any simpler form
3. **Size Bounds**: $|C'| \leq k \cdot |C|$ for some expansion factor $k$

### 1.2 Gate Semantics: ECA57

All circuits use the **ECA57** gate (Elementary Cellular Automaton Rule 57):

$$
\text{ECA57}(t, c_1, c_2): \quad x_t \mapsto x_t \oplus (x_{c_1} \lor \neg x_{c_2})
$$

**Properties:**
- Self-inverse: $\text{ECA57}^2 = I$
- Universal: Can implement any reversible function
- 3-wire gate: target $t$, positive control $c_1$, negative control $c_2$

---

## 2. ABButterfly Algorithm

### 2.1 High-Level Pipeline

```
Input Circuit C
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 0 (optional): Pre-mix        │
│  Apply B_{w,s} shuffle + bit-flip   │
│  Track (w,s) for inverse bookend    │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 1: Gate-Level Wrapping       │
│  For each gate g in C:              │
│    B_i := R_{i-1}^{-1} · g · R_i    │
│  where R_i is random reversible     │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 2: Block Compression         │
│  For each block B_i:                │
│    Find equivalent smaller form     │
│    via Rainbow Table / SAT lookup   │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 3: Hierarchical Merge        │
│  Merge adjacent compressed blocks   │
│  Add global bookends: R_0, R_n^{-1} │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 4: Final Compression         │
│  Chunked sliding-window compression │
│  until stability threshold reached  │
└─────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────┐
│  Phase 5 (optional): Post-mix       │
│  Append B_{w,s}^{-1} if applied     │
└─────────────────────────────────────┘
      │
      ▼
Output Circuit C'
```

### 2.2 Key Invariant: Telescoping Cancellation

The random circuits $R_i$ are chained so that:

$$
C' = R_0^{-1} \cdot g_1 \cdot \underbrace{R_1 \cdot R_1^{-1}}_{= I} \cdot g_2 \cdot \underbrace{R_2 \cdot R_2^{-1}}_{= I} \cdot \ldots \cdot g_n \cdot R_n
$$

When bookends $R_0$ and $R_n^{-1}$ are added:

$$
C' = R_0 \cdot R_0^{-1} \cdot g_1 \cdot g_2 \cdot \ldots \cdot g_n \cdot R_n \cdot R_n^{-1} = C
$$

**The asymmetry** comes from using *different* random $R_i$ at each boundary (vs. symmetric schemes that reuse $R$).

---

## 3. Configuration Parameters

### 3.1 Core Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `rounds` | 3 | Number of complete obfuscation iterations. Each round: wrap → compress → merge. |
| `wires` | 32 | Circuit width (CLI `--n` default). |
| `path` | required | Input circuit file path (CLI `--path`). |

Note: `initial_gates` applies to generators (e.g., `gen`), not `abbutterfly` which requires `--path`.

### 3.2 Structure Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `structure_block_size_min` | 10 | Minimum gate count for random $R_i$ circuits. |
| `structure_block_size_max` | 30 | Maximum gate count for random $R_i$ circuits. |

**Effect**: Larger ranges → more structural diversity but slower compression.

### 3.3 Pre-Processing: Shooting (Gate Reordering)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `shooting_count` | 500,000 | Gate reordering passes at **start of each round**. |
| `shooting_count_inner` | 0 | Reordering **inside each wrapped block** (usually disabled). |

#### How Shooting Works

```
shoot_random_gate(circuit, rounds):
    for _ in 0..rounds:
        1. Pick random gate index
        2. Pick random direction (left or right)
        3. "Shoot" the gate in that direction:
           - Move gate left/right one position at a time
           - Stop when hitting a COLLISION (gates that share a wire)
           - Gates that don't share wires can swap (they commute)
```

### 3.4 Pre-mix Shuffle + Bit-Flip (B_{w,s}) (optional)

When enabled, `abbutterfly_big` prepends a shuffle+bit-flip circuit `B_{w,s}` and appends its inverse after mixing.
This preserves functionality while increasing structural complexity.

Modes:
- `flip-mode=none`: no shuffle/bit-flip stage.
- `flip-mode=separate`: Style A (shuffle then explicit X layer).
- `flip-mode=embedded`: Style B (swap-with-flip gadgets).

Flags:
- `--flip-mode {none,separate,embedded}`
- `--flip-scope {global,per-stage}` (only `global` is currently wired)
- `--shuffle-seed <u64>`
- `--gadget-library <path>` (Style B)
- `--flip-probability <0.0..1.0>`

**Collision check**: Two gates collide if `g1.target ∈ g2.wires` or any control overlaps.

**Why two counts?**
- `shooting_count`: Applied to the **entire circuit** at round start. Scrambles global gate ordering before wrapping.
- `shooting_count_inner`: Applied to **each block** after wrapping (e.g., `R_{i-1}^{-1} · g · R_i`). Scrambles local structure. Usually 0 because blocks are already random.

### 3.4 Block Selection (Convex Subcircuits)

During compression, the algorithm selects **convex subcircuits**—contiguous gate sequences that can be isolated without interleaving with other gates.

```
find_convex_subcircuit(circuit, set_size, max_wires):
    1. Pick a random starting gate
    2. Grow outward (left and right) adding gates that:
       - Don't "interleave" with gates outside the selection
       - Keep total active wires ≤ max_wires
    3. A gate interleaves if it shares wires with gates
       both inside AND outside the current selection
```

**Convexity**: A subcircuit is convex if you can "bubble" all its gates together without passing through non-selected gates that share wires.

```
contiguous_convex(circuit, selected_gates):
    1. Left pass: push non-selected gates leftward (if they don't collide)
    2. Right pass: push non-selected gates rightward (if they don't collide)
    3. Result: selected gates are now contiguous in the circuit
```

### 3.5 Replacement Strategies

| Parameter | Default | Description |
|-----------|---------|-------------|
| `no_ancilla_mode` | `false` | If true, disables using extra wires for compression. |
| `single_gate_mode` | `false` | If true, runs a pass of single gate random replacements before the main loop. |
| `pair_replacement_mode` | `true` | If true, enables replacing adjacent pairs with identities (inflating/blurring). |
| `equal_replacement_mode` | `true` | If true, allows "compression" to accept replacements of equal length (blurring). |
| `sat_mode` | `true` | Uses SAT solver / large LMDB for rigorous compression (experimental). |

**Single-Gate Replacement**: For gate $g$, sample a canonical identity $I = g_1 \cdot g_2 \cdot \ldots \cdot g_k$ where $g_1 = g$. Replace $g$ with $g_2 \cdot \ldots \cdot g_k$ (the remainder after removing the matched gate).

### 3.6 Compression Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `sat_mode` | true | Use SAT solver for compression (slower but more effective). |
| `no_ancilla_mode` | false | Disable ancilla expansion during compression. |
| `skip_compression` | false | Inflation-only mode (no compression, circuit only grows). |
| `compression_window_size` | 100 | Number of random subcircuit trials per compression pass (non-SAT). |
| `compression_window_size_sat` | 10 | Number of random subcircuit trials per compression pass (SAT mode). |
| `compression_sat_limit` | 1000 | SAT solver conflict limit (higher = more thorough but slower). |
| `final_stability_threshold` | 12 | Stop final compression after N consecutive passes with no improvement. |
| `chunk_split_base` | 1500 | Chunk size for parallel final compression. |

#### How compression_window_size Works

This is **NOT** a sliding window across the circuit. It's the number of **random trials**:

```
compress(circuit, trials=compression_window_size):
    for _ in 0..trials:
        1. Pick random subcircuit (random start, random length 1/2/4/8)
        2. Canonicalize it (compute permutation, find canonical hash)
        3. Query database for smaller equivalent
        4. If found and smaller: replace
```

**SAT version** (`compression_window_size_sat`): Same loop but fewer trials because SAT is expensive. SAT is used when TemplateDB lookup fails.

#### How chunk_split_base Works

Final compression runs on the **entire merged circuit**. For large circuits, this is parallelized:

```
final_compression(circuit, chunk_split_base):
    1. Split circuit into chunks of ~chunk_split_base gates
    2. Compress each chunk independently (parallel)
    3. Merge results
    4. Repeat until stability_threshold reached
```

**Effect**: Larger chunks = more context for finding replacements, but slower per chunk.

---

## 4. Database Infrastructure

### 4.1 Overview

ABButterfly uses **two separate database systems**:

```
┌─────────────────────────────────────────────────────────────┐
│                    Local LMDB (./db)                        │
│  Purpose: Permutation-based identity construction           │
│  Used by: random_canonical_id, compress_lmdb, expand_lmdb   │
├─────────────────────────────────────────────────────────────┤
│  Tables:                                                    │
│  • perm_tables_n{N}: perm → [gate counts with that perm]    │
│  • n{N}m{M}: perm||circuit → ∅ (prefix search)              │
│  • n{N}m{M}perms: circuit → perm||shuf                      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│               TemplateDB (collection.lmdb)                  │
│  Purpose: Hash-based template lookup for SAT compression    │
│  Used by: compress_big_sat_lmdb, replace_pairs              │
├─────────────────────────────────────────────────────────────┤
│  Key: [BasisID|Width|GateCount|CanonicalHash] (36 bytes)    │
│  Value: TemplateRecord with optimized gate sequence         │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Local LMDB (`./db`) — Perm Tables

**Purpose**: Fast permutation-based lookups for identity construction and compression.

| Table | Key | Value | Usage |
|-------|-----|-------|-------|
| `perm_tables_n{N}` | Permutation vector | List of gate counts | `random_canonical_id` |
| `n{N}m{M}` | `perm \|\| circuit` | Empty | Prefix search for replacements |
| `n{N}m{M}perms` | Circuit blob | `perm \|\| shuf` | Circuit → canonical form |

**Build Commands**:
```bash
local_mixing_bin init        # Build SQLite tables
local_mixing_bin lmdb -n N -m M   # Convert to LMDB
local_mixing_bin lmdbcounts  # Build perm_tables
```

### 4.3 TemplateDB (`collection.lmdb`) — SAT Templates

**Purpose**: Store pre-computed optimal circuit implementations indexed by canonical hash.

**Key Format** (36 bytes):
```
[BasisID:1][Width:1][GateCount:2][CanonicalHash:32]
```

**Value Format** (TemplateRecord):
- `template_id`: Unique identifier
- `basis_id`: Gate set (1 = ECA57)
- `width`, `gate_count`: Circuit dimensions
- `canonical_hash`, `family_hash`: Function and structure hashes
- `gates`: Serialized gate list (3 bytes per gate: target, c1, c2)

**Lookup Flow**:
```
Subcircuit → compute_canonical_hash() → query LMDB
                                              ↓
                          If found: return smaller equivalent
                          If not: fall back to SAT synthesis
```

---

## 5. Compression Strategies

### 5.1 Compression Hierarchy

```
compress_big_sat_lmdb (preferred)
         │
         ├── Query TemplateDB by canonical hash
         │         ↓
         │   Found? → Replace with smaller template
         │         ↓
         └── Not found? → SAT synthesis fallback
                               ↓
                         compress_big_sat
                               │
                               ▼
                      SAT solver (external script)
```

### 5.2 Ancilla Expansion

When `no_ancilla_mode = false`, compression can temporarily **borrow extra wires** to find equivalences not visible in the original wire space.

#### The Problem

A 4-wire subcircuit may not have a simpler form in 4 wires. But if we **expand to 6 wires**, the same function might have a shorter implementation.

#### The Process

```
expand_big(circuit, subcircuit):
    1. Select convex subcircuit using find_convex_subcircuit()
       - Constraint: total active wires ≤ 7
    
    2. Identify active wires in the subcircuit
       Example: gates touch wires {0, 2, 5} → active_wires = [0, 2, 5]
    
    3. Rewire to compact form
       Map {0, 2, 5} → {0, 1, 2}  (contiguous small indices)
       Now it's a 3-wire circuit
    
    4. Expand wire set (ancilla)
       Try adding wire 3, 4, 5, 6... up to 7 total
       Query LMDB for each width: "Is there a smaller circuit 
       implementing this permutation in N+k wires?"
    
    5. If found: use the expanded replacement
       Unrewire back: {0, 1, 2, 3} → {0, 2, 5, unused_wire}
    
    6. If not found: keep original
```

#### Why It Works

Some permutations have shorter implementations with more wires (ancillas act as scratch space). The databases contain pre-computed optimized circuits for various wire counts.

### 5.3 Pair Replacement

For adjacent gate pairs $(g_1, g_2)$, we can replace them with an identity template that starts or ends with the same structure.

#### Gate Pair Taxonomy

The algorithm classifies pairs by the relationship between $g_2$'s wires and $g_1$:

```rust
gate_pair_taxonomy(g1, g2) → GatePair {
    a:  collision_type(g1, g2.target)    // Is g2's target on g1's wires?
    c1: collision_type(g1, g2.control1)  // Is g2's ctrl1 on g1's wires?
    c2: collision_type(g1, g2.control2)  // Is g2's ctrl2 on g1's wires?
}

CollisionType:
    OnTarget  = g2's wire is g1's target
    OnC1      = g2's wire is g1's control1
    OnC2      = g2's wire is g1's control2  
    OnNew     = g2's wire is NOT on g1 (new wire)
```

#### The Replacement Process

```
replace_pairs(circuit):
    1. Group all adjacent pairs by taxonomy
       pairs = {taxonomy → [list of pair indices]}
    
    2. For each taxonomy group:
       a. Sample random identity template (3-5 wires)
       b. Check if template's first two gates have matching taxonomy
       c. If match: rewire template to match the pair's wires
       d. Replace the 2-gate pair with the full identity template
    
    3. Can also match reversed templates (check last two gates)
```

**Key insight**: The identity template starts as $I = g_1' \cdot g_2' \cdot \ldots \cdot g_k$. If $(g_1', g_2')$ has the same taxonomy as $(g_1, g_2)$, we can rewire the template to exactly match, then the replacement preserves semantics.

---

## 6. Hardcoded Constants

These values are embedded in code and not configurable:

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| R block size (butterfly) | 3–25 | `mixing.rs` | Random structure size |
| R block size (obfuscate) | 3–25 | `replace.rs` | Alternative pipeline |
| Template width (replace_pairs) | 3–5 | `replace.rs` | Pair replacement |
| Template gate filter | 6–40 | `replace.rs` | Which templates to sample |
| Single-gate width | 3–7 | `replace.rs` | Identity template width |
| Ancilla max wires | 7 | `replace.rs` | Expansion limit |
| Compress wire range | 5–7 | `replace.rs` | Non-SAT window size |
| SAT wire range | 3–6 | `replace.rs` | SAT window size |

### Why Are These Hardcoded?

1. **Database constraints**: The perm tables and TemplateDB only exist for certain (n, m) combinations. 7 wires is the practical LMDB limit (2^7 = 128 entry permutations are manageable; 2^8 = 256 explodes storage).

2. **Performance tradeoffs**: These values represent empirically tuned sweet spots. Wider ranges would require exponentially more database entries.

3. **Future work**: Making these configurable would require either:
   - Dynamic database generation
   - Multiple pre-built databases for different ranges
   - Lazy loading based on config

The current approach prioritizes performance over flexibility.

---

## 7. Bookendless Mode

**Standard Mode**: Bookends $R_0$ and $R_n^{-1}$ are added after each round.

**Bookendless Mode** (`--bookendless`):
- Delays bookend insertion across rounds
- Runs final compression on halves and full circuit
- Experimental; may produce more aggressive mixing

---

## 8. Metrics and Verification

### 8.1 Expansion Factor

$$
\text{Expansion Factor} = \frac{|C'|}{|C|}
$$

Target: $2\times$ to $10\times$ depending on security requirements.

### 8.2 Heatmap Analysis

Visualizes divergence via normalized Hamming distance between state evolutions:

```bash
local_mixing_bin heatmap --c1 "circuit1" --c2 "circuit2" -n 8 --inputs 16
```

### 8.3 Alignment (DTW)

Dynamic Time Warping alignment between original and obfuscated circuits:

```bash
local_mixing_bin align --c1 "circuit1" --c2 "circuit2" -n 8 --inputs 16
```

---

## 9. Quick Reference: Command Line

```bash
# Standard ABButterfly obfuscation
local_mixing_bin abbutterfly --path input.gate -n 32 --rounds 3 --shooting 500000

# With SAT compression and TemplateDB
local_mixing_bin abbutterfly --path input.gate -n 32 --sat --lmdb-db ../data/collection.lmdb

# Inflation-only (no compression)
local_mixing_bin abbutterfly --path input.gate -n 32 --skip-compression

# Bookendless mode
local_mixing_bin abbutterfly --path input.gate -n 32 --bookendless

# Shuffle + bit-flip (Style B)
local_mixing_bin abbutterfly --path input.gate -n 32 -r 3 \
  --flip-mode embedded --gadget-library ../share/swap_flip_gadgets.json --shuffle-seed 42
```

---

## 10. Summary Table

| What | How | Database |
|------|-----|----------|
| Random identity construction | `random_canonical_id` | Local LMDB `perm_tables_n{N}` |
| Pair replacement | `replace_pairs` | TemplateDB or Local LMDB |
| Single-gate replacement | `random_gate_replacements` | Local LMDB |
| Ancilla expansion | `expand_lmdb` | Local LMDB `n{N}m{M}` |
| Block compression (non-SAT) | `compress_lmdb` | Local LMDB |
| Block compression (SAT) | `compress_big_sat_lmdb` | TemplateDB + SAT fallback |
| Gate reordering | `shoot_random_gate` | None (in-memory) |

---

*Document generated from source code analysis of `local_mixing` framework.*
