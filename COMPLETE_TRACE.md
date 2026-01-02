# Complete Canonicalization Trace with Skeleton Graph

## Executive Summary

This document shows the **complete step-by-step trace** of how the simplifier works on a small circuit (5 gates), plus the **skeleton graph** structure that enables convex subcircuit selection.

---

## Part 1: The Circuit

### Original Random Circuit R (5 gates on 5 wires)

```
Gates: g0:[1,0,3]  g1:[1,4,2]  g2:[0,4,1]  g3:[4,2,0]  g4:[4,3,0]

Visual:
0 -●----(*)-○--○-
1 (*)(*)-○-------
2 ----○-----●----
3 -○-----------●-
4 ----●--●-(*)(*)
```

**Legend:**
- `(*)` = Active pin (wire being modified by gate)
- `●` = Control pin 1 (gate fires if this is 1)
- `○` = Control pin 2 (gate fires if this is 0)

### Inverse Circuit R⁻¹

```
Gates: g0:[4,3,0]  g1:[4,2,0]  g2:[0,4,1]  g3:[1,4,2]  g4:[1,0,3]
(Just reverse the order!)

Visual:
0 -○--○-(*)----●-
1 -------○-(*)(*)
2 ----●-----○----
3 -●-----------○-
4 (*)(*)-●--●----
```

### Combined R ⊙ R⁻¹ (10 gates - should be identity!)

```
[1,0,3] [1,4,2] [0,4,1] [4,2,0] [4,3,0] | [4,3,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
    └─────────R─────────┘               └──────────R⁻¹──────────┘

✓ Verification: This permutation IS the identity
```

---

## Part 2: Skeleton Graph

### What is a Skeleton Graph?

A **skeleton graph** is a directed acyclic graph (DAG) where:
- **Nodes** = Gates in the circuit
- **Edges** = Collisions (gate dependencies)
- **Levels** = Topological generations (non-colliding gates at same level)

### Collision Rules

Two gates **collide** if:
```
gate1.active == gate2.control_1  OR
gate1.active == gate2.control_2  OR
gate1.control_1 == gate2.active  OR
gate1.control_2 == gate2.active
```

This means they **share a wire** where one gate needs to read while the other writes.

### Skeleton Graph for Our Circuit

```
Topological Levels:

Level 0:    g0[1,0,3]    g1[1,4,2]
             │            │
             │            │
             ↓            ↓
Level 1:         g2[0,4,1]
                    │
                    │
                    ↓
Level 2:    g3[4,2,0]    g4[4,3,0]
```

**Collisions (Edges):**
- `g0 → g2`: Wire 0 (g0 control, g2 active) and Wire 1 (g0 active, g2 control)
- `g1 → g2`: Wire 1 (g1 active, g2 control)
- `g1 → g3`: Wire 4 (g1 control, g3 active)
- `g1 → g4`: Wire 4 (g1 control, g4 active)
- `g2 → g3`: Wire 0 (g2 active, g3 control) and Wire 4 (g2 control, g3 active)
- `g2 → g4`: Wire 0 (g2 active, g4 control) and Wire 4 (g2 control, g4 active)

**See [skeleton_graph.png](skeleton_graph.png) for visual representation**

### Convex Subcircuits

**Convex subcircuit** = All paths between nodes in the subgraph stay within the subgraph.

**Valid convex subcircuits:**
- `{g0, g2}` ✓
- `{g1, g2}` ✓
- `{g2, g3, g4}` ✓
- `{g0, g1, g2, g3, g4}` ✓ (entire graph)

**Invalid (non-convex):**
- `{g0, g3}` ✗ - Path g0 → g2 → g3 requires g2, which is not in the set

This is why skeleton graphs are used: they enable **safe subcircuit selection** that preserves circuit correctness!

---

## Part 3: Detailed Canonicalization Trace

### Round 1: Initial Simplification

#### Input (10 gates):
```
[1,0,3] [1,4,2] [0,4,1] [4,2,0] [4,3,0] [4,3,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
```

#### Step 1: Canonicalize

**Processing gate at position 1: [1,4,2]**
- Compare with position 0: [1,0,3]
- Not in canonical order (control pins: 4,2 vs 0,3 → 4 > 0)
- ⚡ **MOVE** [1,4,2] from position 1 → position 0

Result:
```
[1,4,2] [1,0,3] [0,4,1] [4,2,0] [4,3,0] [4,3,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
```

**Processing gate at position 2: [0,4,1]**
- Compare with position 1: [1,0,3]
- ✗ **COLLISION**: Active wire 0 conflicts with control wire 0
- Gate stays at position 2

**Processing gate at position 4: [4,3,0]**
- Compare with position 3: [4,2,0]
- Not in canonical order (control pins: 3,0 vs 2,0 → 3 > 2)
- ⚡ **MOVE** [4,3,0] from position 4 → position 3

Result:
```
[1,4,2] [1,0,3] [0,4,1] [4,3,0] [4,2,0] [4,3,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
```

**Processing gate at position 5: [4,3,0]**
- Compare with position 4: [4,2,0]
- Not in canonical order
- ⚡ **MOVE** [4,3,0] from position 5 → position 4

Result:
```
[1,4,2] [1,0,3] [0,4,1] [4,3,0] [4,3,0] [4,2,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
```

After canonicalization:
```
[1,4,2] [1,0,3] [0,4,1] [4,3,0] [4,3,0] [4,2,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
```

#### Step 2: Remove Duplicates

**Found duplicate at positions 3-4: [4,3,0]**
```
[1,4,2] [1,0,3] [0,4,1] [4,3,0] [4,3,0] [4,2,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
                        ^^^^^^^^^^^^^^^^
                        REMOVE (g ⊙ g = identity)
```
Result: 8 gates

**Found duplicate at positions 3-4: [4,2,0]**
```
[1,4,2] [1,0,3] [0,4,1] [4,2,0] [4,2,0] [0,4,1] [1,4,2] [1,0,3]
                        ^^^^^^^^^^^^^^^^
                        REMOVE
```
Result: 6 gates

**Found duplicate at positions 2-3: [0,4,1]**
```
[1,4,2] [1,0,3] [0,4,1] [0,4,1] [1,4,2] [1,0,3]
                ^^^^^^^^^^^^^^^^
                REMOVE
```
Result: 4 gates

#### Round 1 Summary:
```
Before:  10 gates
After:   4 gates
Removed: 6 gates (60% reduction!)
```

---

### Round 2: Final Simplification

#### Input (4 gates):
```
[1,4,2] [1,0,3] [1,4,2] [1,0,3]
```

#### Step 1: Canonicalize

**Processing gate at position 2: [1,4,2]**
- Compare with position 1: [1,0,3]
- Not in canonical order (control pins: 4,2 vs 0,3)
- ⚡ **MOVE** [1,4,2] from position 2 → position 1

Result:
```
[1,4,2] [1,4,2] [1,0,3] [1,0,3]
```

Now adjacent duplicates are exposed!

#### Step 2: Remove Duplicates

**Found duplicate at positions 0-1: [1,4,2]**
```
[1,4,2] [1,4,2] [1,0,3] [1,0,3]
^^^^^^^^^^^^^^^^
REMOVE
```
Result: 2 gates

**Found duplicate at positions 0-1: [1,0,3]**
```
[1,0,3] [1,0,3]
^^^^^^^^^^^^^^^^
REMOVE
```
Result: 0 gates

#### Round 2 Summary:
```
Before:  4 gates
After:   0 gates
Removed: 4 gates (100% reduction!)
```

---

## Part 4: Why Canonicalization Works

### The Key Insight

Canonicalization **reorders gates** so that duplicates become **adjacent**:

```
Before:  g₁ g₂ g₃ g₂ g₄ g₄ g₁
After:   g₁ g₁ g₂ g₂ g₃ g₄ g₄
         ^^^^^ ^^^^^ ^^^^^
         (adjacent duplicates - easy to remove!)
```

### The Cascading Effect

Removing duplicates **exposes new duplicates**:

```
Round 1: 10 → 4 gates (removed [4,3,0], [4,2,0], [0,4,1] pairs)
Round 2:  4 → 0 gates (removed [1,4,2], [1,0,3] pairs)
```

Each round "peels away" a layer of the identity circuit!

---

## Part 5: Butterfly Method Connection

### Why This Matters

In the butterfly method:
```
Block 1: R⁻¹ g₁ R
Block 2: R⁻¹ g₂ R

Merged: R⁻¹ g₁ [R R⁻¹] g₂ R
                └──┬──┘
                Identity!
```

**Without simplification:**
- Each butterfly round: 10x gate blowup
- 2 rounds: 1000x blowup
- Circuit becomes **unusable**!

**With simplification:**
- Identity R ⊙ R⁻¹ reduces to 0 gates
- Circuit size stays **manageable**
- Obfuscation becomes **practical**!

---

## Part 6: Key Takeaways

### 1. Skeleton Graphs Enable Safe Selection
- **Nodes** = Gates
- **Edges** = Dependencies (collisions)
- **Convex subgraphs** = Safe to extract and replace

### 2. Canonicalization Exposes Structure
- Reorders gates by pin indices
- Makes duplicates adjacent
- Enables efficient removal

### 3. Simplification is Iterative
- Each round removes some gates
- **Cascading effect**: removal exposes new duplicates
- Converges to minimal representation (identity → 0 gates)

### 4. Critical for Butterfly Method
- **Symmetric butterfly**: R ⊙ R⁻¹ must simplify to 0 gates
- **Without simplification**: exponential blowup
- **With simplification**: manageable circuit size

---

## Summary Statistics

```
╔═══════════════════════════════════════════════════════════╗
║  COMPLETE SIMPLIFICATION SUMMARY                         ║
╚═══════════════════════════════════════════════════════════╝

Circuit:         5 wires, 5 gates
Identity:        R ⊙ R⁻¹ (10 gates)
Skeleton levels: 3 (non-colliding gate groups)
Collisions:      6 directed edges

Simplification:
  Start:   10 gates (R ⊙ R⁻¹)
  Round 1: 10 → 4 gates (60% reduction)
  Round 2:  4 → 0 gates (100% reduction)
  Result:  IDENTITY (0 gates)

Time complexity: O(log N) rounds for N gates
```

---

## Files Generated

1. **[detailed_trace](examples/detailed_trace.rs)** - Rust program showing full trace
2. **[skeleton_graph.png](skeleton_graph.png)** - Visual skeleton graph
3. **[visualize_skeleton.py](visualize_skeleton.py)** - Python visualization script
4. **This document** - Complete explanation

Run the trace yourself:
```bash
cargo run --example detailed_trace --release
```

---

## Further Reading

- **[SIMPLIFICATION_RESULTS.md](SIMPLIFICATION_RESULTS.md)** - Analysis of simplification algorithm
- **[circuit_diagram.txt](circuit_diagram.txt)** - ASCII diagrams and detailed explanations
- **[obfuscated-circuits/build-skeleton.py](../obfuscated-circuits/build-skeleton.py)** - Python skeleton graph library

---

*Generated by detailed trace run on December 16, 2025*
