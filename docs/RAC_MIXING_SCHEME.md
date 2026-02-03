# RAC (Replace And Compress) Mixing Scheme

## Overview

RAC is an alternative circuit obfuscation strategy to the butterfly-based approaches. Unlike butterfly strategies that use hierarchical tree structures with R·x·R⁻¹ wrapping, RAC operates through iterative **sequential pair replacement** followed by **parallel compression**.

## Key Difference from Butterfly

| Aspect | Butterfly Strategies | RAC Strategy |
|--------|---------------------|--------------|
| **Rounds meaning** | Hierarchical tree merging with R·x·R⁻¹ wrapping → expand → compress | Sequential Replace-Then-Compress cycles |
| **Structure** | Recursive block merging (pairwise → offset → 4-way → final) | Linear left-to-right pair processing |
| **Expansion method** | Identity structure wrapping (R·x·R⁻¹) | Gate pair taxonomy + identity circuit insertion |

## Algorithm Structure

```
main_rac_big(circuit, rounds, n, save_path):
    for each round (1 to rounds):
        result = replace_and_compress_big(circuit):
            // REPLACE phase
            chunk_circuit into K chunks (~1500 gates each)
            for each chunk:
                pass1_forward = replace_sequential_pairs(chunk)
                pass2_reverse = replace_sequential_pairs(reversed)
                pass3_forward = replace_sequential_pairs(pass2_reverse)

            // COMPRESS phase
            while not stable_for_12_passes:
                split_circuit into K chunks
                compress_big() each chunk in parallel
                merge_chunks_back()

        circuit = result
        remove_adjacent_identities(circuit)
        validate_functionality(circuit)
        save_progress(circuit)

    write_final_circuit(circuit, save_path)
```

## Phase 1: Replace (Sequential Pair Replacement)

The replacement phase walks through the circuit left-to-right examining consecutive gate pairs.

### Gate Pair Taxonomy

Each gate is represented as `[target, control1, control2]` (3 wires). The taxonomy classifies how two gates interact:

```rust
pub fn gate_pair_taxonomy(g1: &[u8; 3], g2: &[u8; 3]) -> GatePair {
    GatePair {
        a: get_collision_type(&g1, g2[0]),   // Does g2's target collide with g1?
        c1: get_collision_type(&g1, g2[1]),  // Does g2's ctrl1 collide with g1?
        c2: get_collision_type(&g1, g2[2]),  // Does g2's ctrl2 collide with g1?
    }
}
```

**Collision Types:**
- `CollisionType::OnActive` - collision on target wire
- `CollisionType::OnCtrl1` - collision on first control wire
- `CollisionType::OnCtrl2` - collision on second control wire
- `CollisionType::OnNew` - no collision (new wire)

### Identity Circuit Retrieval

When a collision is found between adjacent gates:
1. Random identity length selected: 5-7 gates
2. Identity retrieved from LMDB databases (`ids_n5`, `ids_n6`, `ids_n7`) using collision taxonomy as key
3. Identity circuits are pre-synthesized circuits that compute identity while using more gates

### Gate Shooting

When no adjacent collision is found:
- `shoot_left_vec()` moves the gate backward until finding a compatible pair
- Purpose: Find opportunities for pair replacements elsewhere in the circuit

## Phase 2: Compress (Parallel Chunking Compression)

After replacement, the circuit is compressed:

1. **Chunking**: Circuit split into random chunks (size: `k = (circuit_length + 1499) / 1500`)
2. **Parallel Processing**: Each chunk processed via `compress_big()` using Rayon
3. **Stability Check**: Continue until 12 consecutive passes yield no improvement
4. **Peephole Optimization**: Scans window-sized subcircuits for replaceable patterns

## Parameters

### RAC-Specific Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `rounds` | 3 | Number of Replace→Compress cycles |
| `n` (wires) | - | Number of wires in circuit |

### Compression Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `compression_window_size` | 100 | Gates per peephole window (normal mode) |
| `compression_window_size_sat` | 10 | Gates per peephole window (SAT mode) |
| `final_stability_threshold` | 12 | Passes until declaring stable |
| `chunk_split_base` | 1500 | Gates per chunk in final compression |
| `compression_sat_limit` | 1000 | Conflict limit for SAT solver |

### Identity Circuit Parameters

| Parameter | Range | Description |
|-----------|-------|-------------|
| Identity gate count | 5-7 | Number of gates in random identity from LMDB |

## CLI Usage

```bash
# RAC command structure
local_mixing rac \
    --path <input_circuit> \
    --rounds <num_rounds> \
    -n <num_wires> \
    --save <output_path>
```

## Key Source Files

| File | Purpose |
|------|---------|
| `src/algorithms/butterfly/mixing.rs` | RAC orchestration (`main_rac_big()`, `replace_and_compress_big()`) |
| `src/algorithms/butterfly/replace.rs` | Gate replacement logic (`replace_sequential_pairs()`, `gate_pair_taxonomy()`) |
| `src/infra/random/random_data.rs` | Subcircuit finding (`simple_find_convex_subcircuit()`, `shoot_left_vec()`) |
| `src/config.rs` | Configuration (`ObfuscationConfig` struct) |

## Statistics Tracked

During RAC execution, the following statistics are logged:
- `already_collided` - pairs found with wire collisions
- `shoot_count` - gates requiring movement for collision
- `made_left` - "shoot left" operations successful
- `traverse_left` - distance traversed backward per shot
