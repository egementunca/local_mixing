//! Local Mixing MVP: 64-wire identity obfuscation
//!
//! # Overview
//! - **Mixer**: Generates large identity circuits by embedding templates and applying local moves.
//! - **Reducer**: Attempts to compress circuits by local cancellation and template matching.

use crate::infra::circuit::{CircuitSeq, Gate};
use crate::infra::random::random_data::random_circuit;
use crate::algorithms::butterfly::replace::random_canonical_id;
use rand::Rng;
use std::collections::{HashMap, HashSet};

/// Configuration for mixing
#[derive(Clone, Debug)]
pub struct MixConfig {
    pub num_wires: usize,
    pub num_rounds: usize,
    pub template_size_max: usize, // Max wires for templates
}

/// Configuration for reducer
#[derive(Clone, Debug)]
pub struct ReduceConfig {
    pub max_passes: usize,
    pub max_stall: usize,
}

// =========================================================================
// 1. Core Primitives
// =========================================================================

/// Check if two gates commute based on user-specified rules.
///
/// Rules:
/// For G1 (t1, c1a, c1b) and G2 (t2, c2a, c2b):
/// 1. If t1 is in {c2a, c2b} -> Non-commute (G1 target is control of G2)
/// 2. If t2 is in {c1a, c1b} -> Non-commute (G2 target is control of G1)
/// 3. Else -> Commute
///
/// Note: Same-target gates (t1 == t2) commute if neither controls the other.
/// Since a gate usually has distinct pins (t != c), (t1 == t2) implies (t1 != c2) etc.
pub fn commutes(g1: &Gate, g2: &Gate) -> bool {
    // Gate pins: [t, c1, c2]

    // Rule 1: t1 in {c2a, c2b}
    if g1.pins[0] == g2.pins[1] || g1.pins[0] == g2.pins[2] {
        return false;
    }

    // Rule 2: t2 in {c1a, c1b}
    if g2.pins[0] == g1.pins[1] || g2.pins[0] == g1.pins[2] {
        return false;
    }

    true
}

/// Returns the inverse of a gate.
/// For ECA57 (and most reversible gates like Toffoli), the gate is self-inverse.
pub fn inverse(g: &Gate) -> Gate {
    *g // Copy
}

/// Embeds a template (identity on k wires) into a larger wire space (n wires).
///
/// - `template`: Circuit seq on k wires (indices 0..k-1).
/// - `target_wires`: A subset of n wires to map to. Length must specify mapping for used wires in template.
pub fn embed_template(template: &CircuitSeq, mapping: &[usize]) -> CircuitSeq {
    let mut new_gates = Vec::with_capacity(template.gates.len());

    for gate in &template.gates {
        // Assume gate wires are within 0..mapping.len()
        // If gate uses a wire >= mapping.len(), we panic or handle safely.

        let t = gate[0] as usize;
        let c1 = gate[1] as usize;
        let c2 = gate[2] as usize;

        let new_t = mapping[t];
        let new_c1 = mapping[c1];
        let new_c2 = mapping[c2];

        new_gates.push([new_t as u8, new_c1 as u8, new_c2 as u8]);
    }

    CircuitSeq { gates: new_gates }
}

// =========================================================================
// 2. Templates (Source)
// =========================================================================

/// Get a random identity template.
/// Attempts to use DB if provided, else falls back to synthetic P . P^-1.
pub fn get_identity_template<R: Rng>(
    num_wires: usize, // "k" for this template (e.g. 3..8)
    rng: &mut R,
    env: Option<&lmdb::Environment>,
    conn: Option<&rusqlite::Connection>,
) -> CircuitSeq {
    // Try DB first if available
    if let (Some(env), Some(conn)) = (env, conn) {
        // We use random_canonical_id which returns a canonical ID circuit
        // It might be larger than we want, or specific size?
        // random_canonical_id takes `n`.
        if let Ok(c) = random_canonical_id(env, conn, num_wires) {
            return c;
        }
    }

    // Fallback: Generate P . P^-1
    // Generate random circuit P
    // We want a non-trivial P.
    let depth = rng.random_range(5..20);
    let p = random_circuit(num_wires as u8, depth);
    let p_inv = p.inverse();

    p.concat(&p_inv)
}

// =========================================================================
// 3. Mixing Logic
// =========================================================================

/// Applies one round of mixing to the circuit.
pub fn mix_step<R: Rng>(
    circuit: &mut CircuitSeq,
    config: &MixConfig,
    rng: &mut R,
    env: Option<&lmdb::Environment>,
    conn: Option<&rusqlite::Connection>,
) {
    let move_type = rng.random_range(0..100);

    if move_type < 40 {
        // MOVE A: Insert Template (40% chance)
        // Pick random wire subset size k
        let k = rng.random_range(3..=config.template_size_max.min(config.num_wires));
        let template = get_identity_template(k, rng, env, conn);

        // Pick k distinct wires from num_wires
        let mut wires: Vec<usize> = (0..config.num_wires).collect();
        // seq::SliceRandom::shuffle is in prelude? we imported rand::Rng, need SliceRandom
        use rand::seq::SliceRandom;
        wires.shuffle(rng);
        let target_wires = &wires[0..k];

        let embedded = embed_template(&template, target_wires);

        // Insert at random position
        let pos = if circuit.gates.is_empty() {
            0
        } else {
            rng.random_range(0..=circuit.gates.len())
        };
        circuit.splice(pos, &embedded);
    } else if move_type < 90 {
        // MOVE B: Adjacent Commuting Swaps (50% chance)
        // Perform a burst of swaps to diffuse locally
        if circuit.gates.len() < 2 {
            return;
        }

        let num_swaps = rng.random_range(10..100);
        for _ in 0..num_swaps {
            let idx = rng.random_range(0..circuit.gates.len() - 1);
            let g1 = Gate {
                pins: [
                    circuit.gates[idx][0] as usize,
                    circuit.gates[idx][1] as usize,
                    circuit.gates[idx][2] as usize,
                ],
            };
            let g2 = Gate {
                pins: [
                    circuit.gates[idx + 1][0] as usize,
                    circuit.gates[idx + 1][1] as usize,
                    circuit.gates[idx + 1][2] as usize,
                ],
            };

            if commutes(&g1, &g2) {
                circuit.gates.swap(idx, idx + 1);
            }
        }
    } else {
        // MOVE D: Patch-and-Cancel (10% chance)
        // Insert R ... R_inv at distance
        let k = rng.random_range(3..=8.min(config.num_wires));
        // Only synthetic R for now, as R doesn't need to be identity
        let r_depth = rng.random_range(3..10);
        let r = random_circuit(k as u8, r_depth);
        let r_inv = r.inverse();

        // Pick k wires
        let mut wires: Vec<usize> = (0..config.num_wires).collect();
        use rand::seq::SliceRandom;
        wires.shuffle(rng);
        let target_wires = &wires[0..k];

        let r_emb = embed_template(&r, target_wires);
        let r_inv_emb = embed_template(&r_inv, target_wires);

        let pos1 = if circuit.gates.is_empty() {
            0
        } else {
            rng.random_range(0..=circuit.gates.len())
        };
        // Insert R
        circuit.splice(pos1, &r_emb);

        // Insert R_inv somewhere else (after pos1 effectively, but splice shifts things)
        // New length is old_len + r_emb.len()
        let pos2 = rng.random_range(0..=circuit.gates.len());
        // We want to ensure they are not immediately cancelable? Maybe.
        circuit.splice(pos2, &r_inv_emb);
    }
}

pub fn mix_circuit(
    circuit: &mut CircuitSeq,
    config: &MixConfig,
    env: Option<&lmdb::Environment>,
    conn: Option<&rusqlite::Connection>,
) {
    let mut rng = rand::rng();
    for _ in 0..config.num_rounds {
        mix_step(circuit, config, &mut rng, env, conn);
    }
}

// =========================================================================
// 4. Reducer Logic (Attacker)
// =========================================================================

/// Pass 1: Cancel adjacent inverse pairs.
/// Returns true if changes made.
fn pass_cancel(circuit: &mut CircuitSeq) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < circuit.gates.len() {
        // Gate is self-inverse: G == inv(G)
        // So check if g1 == g2
        // Note: Gate struct equality checks pins.

        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..i + 2);
            changed = true;
            // Backtrack one step to catch new adjacencies
            if i > 0 {
                i -= 1;
            }
        } else {
            i += 1;
        }
    }
    changed
}

/// Pass 2: Commute-to-expose.
/// Randomly swaps commuting gates to try and bring inverses together.
/// Note: This is stochastic.
fn pass_commute_expose<R: Rng>(circuit: &mut CircuitSeq, rng: &mut R) -> bool {
    // Attempt N random swaps
    let mut exposed = false;
    let n_swaps = circuit.gates.len() * 2; // Heuristic budget

    // We can't easily detect "bringing inverses together" without full search.
    // MVP: Just shuffle locally and run cancel pass after.
    // This "Simulated Annealing" style is decent.

    if circuit.gates.len() < 2 {
        return false;
    }

    let mut swapped_any = false;
    for _ in 0..n_swaps {
        let idx = rng.random_range(0..circuit.gates.len() - 1);
        let g1 = Gate {
            pins: [
                circuit.gates[idx][0] as usize,
                circuit.gates[idx][1] as usize,
                circuit.gates[idx][2] as usize,
            ],
        };
        let g2 = Gate {
            pins: [
                circuit.gates[idx + 1][0] as usize,
                circuit.gates[idx + 1][1] as usize,
                circuit.gates[idx + 1][2] as usize,
            ],
        };

        if commutes(&g1, &g2) {
            circuit.gates.swap(idx, idx + 1);
            swapped_any = true;
        }
    }

    // After shuffling, try cancelling
    if pass_cancel(circuit) {
        exposed = true;
    } else if swapped_any {
        // We moved things but didn't cancel.
        // We return true only if we reduced length?
        // Or if we changed state?
        // For distinct "passes", usually return true if *reduction* happened.
        // But "Commute" might be neutral.
        // Let's return false if size didn't change.
    }

    exposed
}

/// Pass 3: Template Deletion (Naive Window Match).
/// Scans for windows that match known templates (modulo relabeling).
fn pass_template_delete(
    circuit: &mut CircuitSeq,
    _env: Option<&lmdb::Environment>,
    _conn: Option<&rusqlite::Connection>,
) -> bool {
    // MVP: We only detect "Identity" templates.
    // Actually, "delete matched templates" implies we replace them with empty circuit.
    // For MVP, without a fast index, this is O(L * T) where L is circuit len, T is num templates.
    // If we generate templates on fly, we don't have a "DB of all templates".
    // BUT, we can detect *self-evident* identities?
    // No, the attacker "knows" the template DB.

    // If we are using synthetic templates (P . P^-1) in mixing, the reducer should ideally "know" them?
    // Or we rely on the fact that P . P^-1 is just 1.
    // A general reducer might check small windows for Identity by function evaluation.
    // This is "bruteforce identity check on window".

    // Implementation:
    // Slide window of size W in 4..12
    // Measure active wires k.
    // If k is small (e.g. <= 5), check if window is Identity map.
    // If so, delete it.

    let windows = [4, 6, 8, 10, 12, 16]; // Window sizes
    let mut changed = false;

    // We iterate manually to handle deletions safely
    // Actually, simple strategy: try found match, if found, delete and restart.

    // Limit iterations to avoid infinite loops if something is weird
    for _iter in 0..10 {
        let mut found_this_iter = false;

        'window_loop: for &w_len in &windows {
            if circuit.gates.len() < w_len {
                continue;
            }

            for i in 0..=circuit.gates.len() - w_len {
                // Check window [i .. i+w_len]
                let window_gates = &circuit.gates[i..i + w_len];

                // Identify active wires
                let mut active = HashSet::new();
                for g in window_gates {
                    active.insert(g[0]);
                    active.insert(g[1]);
                    active.insert(g[2]);
                }

                if active.len() <= 6 {
                    // Limit checking to small active sets for speed
                    // Check identity
                    // Bruteforce check? 2^k checks.
                    // with k=6, 64 checks. Fast.

                    if is_identity_window(window_gates, &active) {
                        // Delete!
                        circuit.gates.drain(i..i + w_len);
                        changed = true;
                        found_this_iter = true;
                        break 'window_loop; // Invalidated indices, restart
                    }
                }
            }
        }

        if !found_this_iter {
            break;
        }
    }

    changed
}

fn is_identity_window(gates: &[[u8; 3]], active_wires: &HashSet<u8>) -> bool {
    let sorted_wires: Vec<u8> = {
        let mut v: Vec<u8> = active_wires.iter().cloned().collect();
        v.sort();
        v
    };

    // Create mapping wire -> bit index
    let mut map = HashMap::new();
    for (idx, &w) in sorted_wires.iter().enumerate() {
        map.insert(w, idx);
    }

    let k = sorted_wires.len();
    let num_states = 1 << k;

    for state in 0..num_states {
        let mut curr = state;
        for g in gates {
            // Map global gate to local indices
            // g[0] is target, g[1], g[2] controls
            // Logic: t ^= c1 & !c2

            let t_local = map[&g[0]];
            let c1_local = map[&g[1]];
            let c2_local = map[&g[2]];

            let v_c1 = (curr >> c1_local) & 1;
            let v_c2 = (curr >> c2_local) & 1;

            // ECA57 rule: t ^= c1 | !c2 ??
            // src/circuit/circuit.rs says:
            // let c1 = (state >> gate[1]) & 1;
            // let c2 = (state >> gate[2]) & 1;
            // state ^ (c1 | ((!c2) & 1)) << gate[0]
            // Wait, gate.rs says: `state ^ (c1 | ((!c2) & 1)) << gate[0]`
            // Yes.

            let update_bit = v_c1 | (1 - v_c2);
            if update_bit == 1 {
                curr ^= 1 << t_local;
            }
        }

        if curr != state {
            return false;
        }
    }

    true
}

pub fn reduce_circuit(
    circuit: &mut CircuitSeq,
    config: &ReduceConfig,
    env: Option<&lmdb::Environment>,
    conn: Option<&rusqlite::Connection>,
) -> usize {
    let mut passes = 0;
    let mut stall_count = 0;

    let mut rng = rand::rng();

    loop {
        if passes >= config.max_passes {
            break;
        }
        if stall_count >= config.max_stall {
            break;
        }

        let start_len = circuit.gates.len();

        // Run passes
        // Run passes
        pass_cancel(circuit);
        pass_commute_expose(circuit, &mut rng); // This includes a cancel pass inside
        pass_template_delete(circuit, env, conn);

        let end_len = circuit.gates.len();

        if end_len < start_len {
            stall_count = 0;
        } else {
            stall_count += 1;
        }

        passes += 1;
    }

    circuit.gates.len()
}
