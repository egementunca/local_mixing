# Triplet Replacement for RAC Mixing

## Context: Extending Gate Pair Replacement

The RAC mixing scheme's Replace phase currently operates on **adjacent gate pairs**. Each pair is classified using `GatePair` taxonomy, matched against an identity database, and replaced with a longer equivalent circuit.

This document proposes extending replacement to **triplets** (3 consecutive gates), enabling:
- Larger replacement windows → more obfuscation per operation
- Richer structural patterns → better identity matching
- Reduced iteration count → fewer Replace→Compress cycles needed

---

## The Problem: GatePair Doesn't Scale to Triplets

### Current GatePair Taxonomy

For two gates $(G_1, G_2)$, we classify how $G_2$'s wires relate to $G_1$:

```rust
pub struct GatePair {
    a: CollisionType,   // G2's target vs G1
    c1: CollisionType,  // G2's ctrl1 vs G1
    c2: CollisionType,  // G2's ctrl2 vs G1
}

pub enum CollisionType {
    OnActive,  // Matches G1's target
    OnCtrl1,   // Matches G1's ctrl1
    OnCtrl2,   // Matches G1's ctrl2
    OnNew,     // No match
}
```

This gives $4^3 = 64$ possible pair taxonomies.

### Why This Breaks for Triplets

For a triplet $(G_1, G_2, G_3)$, we need to describe:
- $G_2$ relative to $G_1$ (one GatePair)
- $G_3$ relative to $G_1$ (another GatePair)
- $G_3$ relative to $G_2$ (yet another GatePair)

But these aren't independent. If $G_2$'s target hits $G_1$'s ctrl1, and $G_3$'s target also hits $G_1$'s ctrl1, then $G_2$ and $G_3$ share their target wire. We'd need compound enums like:

```rust
// This gets ugly fast
enum G3_Target_Collision {
    OnG1Active,
    OnG1Ctrl1,
    OnG1Ctrl2,
    OnG2Active,
    OnG2Ctrl1,
    OnG2Ctrl2,
    OnG1Active_And_OnG2Active,  // Same wire hits both
    OnG1Ctrl1_And_OnG2Active,
    // ... exponential explosion
    OnNew,
}
```

This is unmaintainable and the constraint validation becomes complex.

---

## The Solution: First-Appearance Wire Labeling

Instead of relative enums, we label wires by the order they first appear when scanning left-to-right through the triplet.

### Algorithm

```
Input: Triplet [(a1,c1,c2), (a2,c3,c4), (a3,c5,c6)]
Output: Normalized key with canonical wire indices

1. Start with empty mapping φ and counter k=0
2. For each gate, for each wire (target, ctrl1, ctrl2):
   - If wire not in φ: assign φ[wire] = k, increment k
   - Record normalized index
3. Return sequence of normalized gates
```

### Concrete Example

**Input triplet:**
| Gate | Target | Ctrl1 | Ctrl2 |
|------|--------|-------|-------|
| $G_1$ | 10 | 20 | 30 |
| $G_2$ | 20 | 40 | 50 |
| $G_3$ | 10 | 50 | 60 |

**Normalization trace:**

```
G1: [10, 20, 30]
    10 → 0 (new)
    20 → 1 (new)
    30 → 2 (new)
    Normalized: [0, 1, 2]

G2: [20, 40, 50]
    20 → 1 (seen in G1)
    40 → 3 (new)
    50 → 4 (new)
    Normalized: [1, 3, 4]

G3: [10, 50, 60]
    10 → 0 (seen in G1)
    50 → 4 (seen in G2)
    60 → 5 (new)
    Normalized: [0, 4, 5]
```

**Triplet Key:** `[[0,1,2], [1,3,4], [0,4,5]]`

---

## Reading Collisions from the Key

The normalized key encodes all collision information:

| Interaction | How to Read | This Example |
|-------------|-------------|--------------|
| $G_2$ vs $G_1$ | Which indices in $G_2$ appear in $G_1 = [0,1,2]$? | Index 1 (target) → $G_2$ target hits $G_1$ ctrl1 |
| $G_3$ vs $G_1$ | Which indices in $G_3$ appear in $G_1 = [0,1,2]$? | Index 0 (target) → $G_3$ target hits $G_1$ target |
| $G_3$ vs $G_2$ | Which indices in $G_3$ appear in $G_2 = [1,3,4]$? | Index 4 (ctrl1) → $G_3$ ctrl1 hits $G_2$ ctrl2 |

**Collision summary for this triplet:**
- $G_1.ctrl1 = G_2.target$ (wire 20)
- $G_1.target = G_3.target$ (wire 10)
- $G_2.ctrl2 = G_3.ctrl1$ (wire 50)

---

## Relationship to GatePair

First-appearance normalization generalizes GatePair. For the pair $(G_1, G_2)$:

| Normalized Index | Equivalent CollisionType |
|------------------|--------------------------|
| 0 | `OnActive` |
| 1 | `OnCtrl1` |
| 2 | `OnCtrl2` |
| ≥ 3 | `OnNew` |

The first gate always normalizes to `[0, 1, 2]`. The second gate's normalized form directly maps to a GatePair.

---

## Implementation

### Core Normalization

```rust
use std::collections::HashMap;

/// Normalizes a triplet to its canonical form.
/// Returns (normalized_key, wire_mapping).
pub fn normalize_triplet(triplet: &[[u8; 3]; 3]) -> ([[u8; 3]; 3], HashMap<u8, u8>) {
    let mut mapping: HashMap<u8, u8> = HashMap::new();
    let mut next_id: u8 = 0;
    let mut result = [[0u8; 3]; 3];

    for (i, gate) in triplet.iter().enumerate() {
        for (j, &wire) in gate.iter().enumerate() {
            result[i][j] = *mapping.entry(wire).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
        }
    }

    (result, mapping)
}

/// Computes just the key for database lookup.
pub fn triplet_key(triplet: &[[u8; 3]; 3]) -> [[u8; 3]; 3] {
    normalize_triplet(triplet).0
}
```

### Database Key Serialization

```rust
/// Serialize triplet key to 9 bytes for DB indexing.
pub fn serialize_triplet_key(key: &[[u8; 3]; 3]) -> [u8; 9] {
    [
        key[0][0], key[0][1], key[0][2],
        key[1][0], key[1][1], key[1][2],
        key[2][0], key[2][1], key[2][2],
    ]
}
```

### Identity Substitution

```rust
/// Apply a normalized identity circuit to a target triplet's wire context.
pub fn apply_triplet_identity(
    identity: &[[u8; 3]],      // Normalized identity circuit
    mapping: &HashMap<u8, u8>,  // From normalize_triplet
    available_wires: &mut Vec<u8>, // Fresh wires for OnNew indices
) -> Result<Vec<[u8; 3]>, SubstitutionError> {
    // Build reverse map: normalized_index -> original_wire
    let mut reverse: HashMap<u8, u8> = mapping
        .iter()
        .map(|(&orig, &norm)| (norm, orig))
        .collect();

    // Find max index used in identity
    let max_idx = identity.iter()
        .flat_map(|g| g.iter())
        .max()
        .copied()
        .unwrap_or(0);

    // Allocate fresh wires for indices beyond mapping
    for idx in 0..=max_idx {
        if !reverse.contains_key(&idx) {
            let fresh = available_wires.pop()
                .ok_or(SubstitutionError::NoFreshWires)?;
            reverse.insert(idx, fresh);
        }
    }

    // Map identity to original wire space
    Ok(identity.iter().map(|g| {
        [reverse[&g[0]], reverse[&g[1]], reverse[&g[2]]]
    }).collect())
}
```

---

## Integration with RAC Replace Phase

### Current Flow (Pairs)

```rust
// In replace_sequential_pairs()
for each adjacent pair (left, right):
    taxonomy = gate_pair_taxonomy(left, right)
    if taxonomy.is_none():
        shoot_left()  // Find collision elsewhere
    else:
        identity = db.get_random_identity(taxonomy)
        splice(identity)
```

### Proposed Flow (Triplets)

```rust
// In replace_sequential_triplets()
for each consecutive triplet (g1, g2, g3):
    (key, mapping) = normalize_triplet([g1, g2, g3])

    if !has_any_collision(&key):
        // No wires shared between gates - skip or shoot
        continue

    identity = triplet_db.get_random_identity(key)?
    remapped = apply_triplet_identity(identity, mapping, available_wires)
    splice(remapped)  // Replace 3 gates with identity circuit
```

### Collision Check

```rust
/// Returns true if any wire is shared between gates in the triplet.
fn has_any_collision(key: &[[u8; 3]; 3]) -> bool {
    // G1 is always [0,1,2]. Check if G2 or G3 reuse indices 0,1,2
    // or if G3 reuses indices from G2
    let g1_wires: HashSet<u8> = key[0].iter().copied().collect();
    let g2_wires: HashSet<u8> = key[1].iter().copied().collect();

    let g2_hits_g1 = key[1].iter().any(|w| g1_wires.contains(w));
    let g3_hits_g1 = key[2].iter().any(|w| g1_wires.contains(w));
    let g3_hits_g2 = key[2].iter().any(|w| g2_wires.contains(w));

    g2_hits_g1 || g3_hits_g1 || g3_hits_g2
}
```

---

## Triplet Identity Database

### Schema

```
Database: triplet_ids_n{K}  (K = identity gate count, e.g., 7-11)

Key:   [u8; 9]  — serialized triplet key
Value: Vec<IdentityCircuit>  — list of equivalent circuits
```

### Population Strategy

1. **Enumerate valid triplet keys** - Not all `[[u8;3];3]` are valid; first-appearance ordering constrains the space
2. **For each key**, synthesize identity circuits that:
   - Have the same normalized prefix (first 3 gates match the key)
   - Compute identity on the involved wires
   - Use K total gates (where K > 3 for expansion)

### Key Space Size

For triplets, the key space is manageable:
- $G_1$ is always `[0,1,2]` (1 possibility)
- $G_2$ uses indices from {0,1,2,3,4,5} with constraints
- $G_3$ uses indices from {0,1,2,3,4,5,6,7,8} with constraints

Estimated: **~2,000-5,000 valid triplet keys** (vs 64 for pairs)

---

## Benefits Over Pair Replacement

| Aspect | Pair Replacement | Triplet Replacement |
|--------|------------------|---------------------|
| **Window size** | 2 gates | 3 gates |
| **Structural patterns** | 64 taxonomies | ~2,000-5,000 keys |
| **Replacement ratio** | 2 → K gates | 3 → K gates |
| **Collision coverage** | Pairwise only | All three pairwise + transitive |
| **Obfuscation depth** | Shallow | Deeper per operation |

---

## Open Questions

1. **Hybrid approach?** Run pair replacement first, then triplet replacement on remaining structure?

2. **Key filtering?** Some triplet keys may be rare in practice. Should we only pre-compute identities for common patterns?

3. **Shooting for triplets?** Current `shoot_left` finds pair collisions. How should we adapt this for finding triplet collision opportunities?

4. **Identity length distribution?** For pairs we use 5-7 gates. What range makes sense for triplets (7-11)?
