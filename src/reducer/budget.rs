//! Budgeted Reducer - Attacker model with step/time constraints
//!
//! Performs reduction operations (template deletion, adjacent cancellation,
//! commuting swaps) but stops after a fixed budget of operations.
//!
//! Pass 1: Adjacent identical gate cancellation (self-inverse)
//! Pass 2: Commuting swaps that enable cancellation
//! Pass 3: LMDB-backed identity window detection (canonicalize + DB lookup)

use crate::infra::circuit::CircuitSeq;
use crate::infra::ids_index::{self, IDS_REV_DB};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Report from a budgeted reduction run
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReductionReport {
    /// The reduced circuit
    pub c_reduced: CircuitSeq,
    /// Original circuit length
    pub original_len: usize,
    /// Reduced circuit length
    pub reduced_len: usize,
    /// Compression ratio (reduced_len / original_len)
    pub compression_ratio: f64,
    /// Template hits by size m
    pub template_hits: HashMap<usize, usize>,
    /// Number of adjacent cancel pairs found
    pub cancel_pairs: usize,
    /// Number of commuting swaps performed
    pub commute_swaps: usize,
    /// Steps taken before budget exhausted
    pub steps_taken: usize,
}

impl ReductionReport {
    /// Calculate the "hardness" score (higher = harder to reduce)
    pub fn hardness(&self) -> f64 {
        self.compression_ratio
    }

    /// Gates removed
    pub fn gates_removed(&self) -> usize {
        self.original_len.saturating_sub(self.reduced_len)
    }
}

/// Budgeted reducer configuration
#[derive(Clone, Debug)]
pub struct ReducerConfig {
    /// Maximum number of operations
    pub max_steps: usize,
    /// Maximum template size to check
    pub max_template_m: usize,
    /// Window sizes to try for template matching
    pub window_sizes: Vec<usize>,
}

impl Default for ReducerConfig {
    fn default() -> Self {
        Self {
            max_steps: 10000,
            max_template_m: 8,
            window_sizes: vec![3, 4, 5, 6],
        }
    }
}

/// Check if two gates commute (can be swapped without changing semantics)
/// Gates commute if they don't share the target wire
pub fn gates_commute(g1: &[u8; 3], g2: &[u8; 3]) -> bool {
    // Gates commute if:
    // - Neither target wire is a control of the other
    // - The target wires are different
    let t1 = g1[0];
    let t2 = g2[0];

    // If same target, they don't commute
    if t1 == t2 {
        return false;
    }

    // If g1's target is a control of g2, they don't commute
    if t1 == g2[1] || t1 == g2[2] {
        return false;
    }

    // If g2's target is a control of g1, they don't commute
    if t2 == g1[1] || t2 == g1[2] {
        return false;
    }

    true
}

/// Perform budgeted reduction on a circuit.
///
/// When an LMDB environment is provided, Pass 3 uses the `ids_rev` index
/// to detect and remove identity windows that are too large for brute-force.
pub fn reduce_budget(
    c: &CircuitSeq,
    config: &ReducerConfig,
    env: Option<&lmdb::Environment>,
) -> ReductionReport {
    let original_len = c.gates.len();
    let mut reduced = c.clone();
    let mut steps = 0;
    let mut cancel_pairs = 0;
    let mut commute_swaps = 0;
    let mut template_hits: HashMap<usize, usize> = HashMap::new();

    // Open ids_rev DB once for the lifetime of this reduction
    let ids_rev_db = env.and_then(|e| e.open_db(Some(IDS_REV_DB)).ok());

    let mut changed = true;

    // Iterate until no changes or budget exhausted
    while changed && steps < config.max_steps {
        changed = false;

        // Pass 1: Remove adjacent identical gates (self-inverse cancellation)
        let mut i = 0;
        while i < reduced.gates.len().saturating_sub(1) && steps < config.max_steps {
            steps += 1;
            if reduced.gates[i] == reduced.gates[i + 1] {
                reduced.gates.remove(i + 1);
                reduced.gates.remove(i);
                cancel_pairs += 1;
                changed = true;
            } else {
                i += 1;
            }
        }

        // Pass 2: Try commuting swaps to enable more cancellations
        let mut i = 0;
        while i < reduced.gates.len().saturating_sub(2) && steps < config.max_steps {
            steps += 1;
            if gates_commute(&reduced.gates[i], &reduced.gates[i + 1]) {
                let would_cancel_left = i > 0 && reduced.gates[i - 1] == reduced.gates[i + 1];
                let would_cancel_right =
                    i + 2 < reduced.gates.len() && reduced.gates[i] == reduced.gates[i + 2];

                if would_cancel_left || would_cancel_right {
                    reduced.gates.swap(i, i + 1);
                    commute_swaps += 1;
                    changed = true;
                }
            }
            i += 1;
        }

        // Pass 3: Identity window detection via LMDB lookup
        if let (Some(e), Some(db)) = (env, ids_rev_db) {
            'template_scan: for &w_len in &config.window_sizes {
                if reduced.gates.len() < w_len || steps >= config.max_steps {
                    continue;
                }
                for i in 0..=reduced.gates.len() - w_len {
                    steps += 1;
                    if steps >= config.max_steps {
                        break 'template_scan;
                    }

                    let window = &reduced.gates[i..i + w_len];

                    // Count active wires to skip trivially wide windows
                    let mut active = HashSet::new();
                    for g in window {
                        active.insert(g[0]);
                        active.insert(g[1]);
                        active.insert(g[2]);
                    }
                    if active.len() > config.max_template_m {
                        continue;
                    }

                    // Canonicalize and query DB
                    if ids_index::ids_rev_contains(e, db, window) {
                        reduced.gates.drain(i..i + w_len);
                        *template_hits.entry(w_len).or_insert(0) += 1;
                        changed = true;
                        break 'template_scan; // indices invalidated, restart
                    }
                }
            }
        }
    }

    let reduced_len = reduced.gates.len();
    let compression_ratio = if original_len == 0 {
        1.0
    } else {
        reduced_len as f64 / original_len as f64
    };

    ReductionReport {
        c_reduced: reduced,
        original_len,
        reduced_len,
        compression_ratio,
        template_hits,
        cancel_pairs,
        commute_swaps,
        steps_taken: steps,
    }
}

/// Backward-compatible wrapper for callers that don't have an LMDB environment.
pub fn reduce_budget_no_db(c: &CircuitSeq, config: &ReducerConfig) -> ReductionReport {
    reduce_budget(c, config, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gates_commute() {
        // Gates on different wires commute
        let g1 = [0, 1, 2];
        let g2 = [3, 4, 5];
        assert!(gates_commute(&g1, &g2));

        // Same target doesn't commute
        let g3 = [0, 1, 2];
        let g4 = [0, 3, 4];
        assert!(!gates_commute(&g3, &g4));

        // Target is control of other doesn't commute
        let g5 = [0, 1, 2];
        let g6 = [3, 0, 4];
        assert!(!gates_commute(&g5, &g6));
    }

    #[test]
    fn test_reduce_budget_cancellation() {
        // Create circuit with adjacent identical gates
        let c = CircuitSeq {
            gates: vec![[0, 1, 2], [0, 1, 2], [3, 4, 5]],
        };

        let config = ReducerConfig::default();
        let report = reduce_budget(&c, &config, None);

        assert_eq!(report.original_len, 3);
        assert_eq!(report.reduced_len, 1);
        assert_eq!(report.cancel_pairs, 1);
    }
}
