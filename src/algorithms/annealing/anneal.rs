//! Annealed Obfuscator - Simulated annealing for circuit obfuscation
//!
//! Uses Metropolis-style acceptance to explore semantically equivalent circuits,
//! biasing toward circuits that are harder to reduce.

use crate::infra::circuit::CircuitSeq;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for the annealing process
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnnealParams {
    /// Initial temperature (high = accept many worse moves)
    pub t0: f64,
    /// Final temperature (low = near greedy)
    pub t_end: f64,
    /// Total number of annealing steps
    pub steps: usize,
    /// Move selection probabilities
    pub move_probs: MoveProbabilities,
    /// Energy function weights
    pub energy_weights: EnergyWeights,
    /// How often to compute E_slow and checkpoint
    pub checkpoint_interval: usize,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Number of wires in the circuit
    pub n_wires: usize,
    /// Budget for reducer calls (steps)
    pub reducer_budget: usize,
    /// Optional path to LMDB template database for Move A
    pub lmdb_path: Option<String>,
    /// Enable adversarial template selection (stickiest template)
    pub adversarial_mode: bool,
    /// Number of candidate templates to sample in adversarial mode
    pub k_candidates: usize,
    /// Window size for stickiness scoring
    pub stickiness_window: usize,
}

impl Default for AnnealParams {
    fn default() -> Self {
        Self {
            t0: 0.5,
            t_end: 0.01,
            steps: 5000, // Start small for dev
            move_probs: MoveProbabilities::default(),
            energy_weights: EnergyWeights::default(),
            checkpoint_interval: 200,
            seed: 42,
            n_wires: 64,
            reducer_budget: 10000,
            lmdb_path: None,
            adversarial_mode: true, // Enable by default
            k_candidates: 10,
            stickiness_window: 5,
        }
    }
}

/// Move selection probabilities (must sum to 1.0)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveProbabilities {
    /// Move A: Insert identity template
    pub insert_template: f64,
    /// Move C: Local commuting swap
    pub commute_swap: f64,
    /// Move B: Patch pair insertion (P, inv(P))
    pub patch_pair: f64,
}

impl Default for MoveProbabilities {
    fn default() -> Self {
        Self {
            insert_template: 0.25,
            commute_swap: 0.65,
            patch_pair: 0.10,
        }
    }
}

impl MoveProbabilities {
    pub fn normalize(&mut self) {
        let sum = self.insert_template + self.commute_swap + self.patch_pair;
        if sum > 0.0 {
            self.insert_template /= sum;
            self.commute_swap /= sum;
            self.patch_pair /= sum;
        }
    }
}

/// Energy function weights
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnergyWeights {
    /// Weight for adjacent cancel penalty
    pub adjacent_cancel: f64,
    /// Weight for witness hits penalty
    pub witness_hits: f64,
    /// Weight for coverage bonus (negative = reward coverage)
    pub coverage: f64,
    /// Weight for compression ratio in E_slow
    pub compression: f64,
}

impl Default for EnergyWeights {
    fn default() -> Self {
        Self {
            adjacent_cancel: 0.5,
            witness_hits: 0.5,
            coverage: -0.2, // Negative = reward high coverage
            compression: 1.0,
        }
    }
}

/// Statistics from an annealing run
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnnealStats {
    pub best_energy: f64,
    pub final_energy: f64,
    pub accept_count: usize,
    pub reject_count: usize,
    pub move_counts: HashMap<String, usize>,
    pub energy_trace: Vec<f64>,
    pub temperature_trace: Vec<f64>,
}

impl AnnealStats {
    pub fn accept_rate(&self) -> f64 {
        let total = self.accept_count + self.reject_count;
        if total == 0 {
            0.0
        } else {
            self.accept_count as f64 / total as f64
        }
    }
}

/// Result of an annealing run
#[derive(Clone, Debug)]
pub struct ObfRun {
    /// Final circuit after annealing
    pub c_final: CircuitSeq,
    /// Best circuit found during annealing (lowest E_slow)
    pub c_best: CircuitSeq,
    /// Statistics from the run
    pub stats: AnnealStats,
    /// The seed used
    pub seed: u64,
}

/// Types of moves available
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MoveType {
    InsertTemplate,
    CommuteSwap,
    PatchPair,
}

impl MoveType {
    pub fn name(&self) -> &'static str {
        match self {
            MoveType::InsertTemplate => "insert_template",
            MoveType::CommuteSwap => "commute_swap",
            MoveType::PatchPair => "patch_pair",
        }
    }
}

/// Temperature at step t using exponential schedule
pub fn temperature(t: usize, params: &AnnealParams) -> f64 {
    if params.steps <= 1 {
        return params.t0;
    }
    let ratio = params.t_end / params.t0;
    let exponent = t as f64 / (params.steps - 1) as f64;
    params.t0 * ratio.powf(exponent)
}

/// Metropolis acceptance criterion
pub fn accept_move<R: Rng>(delta_e: f64, temp: f64, rng: &mut R) -> bool {
    if delta_e <= 0.0 {
        true // Always accept improvements
    } else if temp <= 0.0 {
        false // At T=0, never accept worse
    } else {
        let prob = (-delta_e / temp).exp().min(1.0);
        rng.random::<f64>() < prob
    }
}

/// Count adjacent inverse pairs (gates that cancel)
pub fn count_adjacent_cancels(c: &CircuitSeq) -> usize {
    let mut count = 0;
    for i in 0..c.gates.len().saturating_sub(1) {
        if c.gates[i] == c.gates[i + 1] {
            count += 1;
        }
    }
    count
}

/// Calculate wire coverage (fraction of wires touched)
pub fn wire_coverage(c: &CircuitSeq, n_wires: usize) -> f64 {
    if n_wires == 0 {
        return 0.0;
    }
    let used = c.count_used_wires();
    used as f64 / n_wires as f64
}

/// Fast energy calculation (no reducer call)
pub fn energy_fast(c: &CircuitSeq, params: &AnnealParams) -> f64 {
    let len = c.gates.len().max(1) as f64;
    let cancels = count_adjacent_cancels(c) as f64 / len;
    let coverage = wire_coverage(c, params.n_wires);

    // witness_hits would need template DB lookup - skip for now
    let witness_hits = 0.0;

    let w = &params.energy_weights;
    w.adjacent_cancel * cancels + w.witness_hits * witness_hits + w.coverage * coverage
}

/// Slow energy calculation (uses reducer)
pub fn energy_slow(c: &CircuitSeq, params: &AnnealParams) -> f64 {
    use crate::reducer::{ReducerConfig, reduce_budget};

    let config = ReducerConfig {
        max_steps: params.reducer_budget,
        ..Default::default()
    };

    let report = reduce_budget(c, &config, None);
    let compression = 1.0 - report.compression_ratio; // Lower compression = harder to reduce = lower energy
    let coverage = wire_coverage(c, params.n_wires);

    let w = &params.energy_weights;
    w.compression * compression + w.coverage * coverage
}

/// Select a move type based on probabilities
pub fn select_move<R: Rng>(probs: &MoveProbabilities, rng: &mut R) -> MoveType {
    let r: f64 = rng.random();
    if r < probs.insert_template {
        MoveType::InsertTemplate
    } else if r < probs.insert_template + probs.commute_swap {
        MoveType::CommuteSwap
    } else {
        MoveType::PatchPair
    }
}

/// Score a template by how many anti-commuting pairs it creates with neighbors
/// Higher score = template is "stickier" and harder to reduce
pub fn score_template_stickiness(
    c: &CircuitSeq,
    template: &[[u8; 3]],
    pos: usize,
    window: usize,
) -> usize {
    use crate::reducer::gates_commute;

    let mut score = 0;
    let start = pos.saturating_sub(window);
    let end = (pos + template.len() + window).min(c.gates.len());

    // Count anti-commuting pairs between template gates and circuit gates in window
    for t_gate in template {
        for i in start..pos.min(c.gates.len()) {
            if !gates_commute(t_gate, &c.gates[i]) {
                score += 1;
            }
        }
        // Also check gates after insertion point
        for i in pos..end.min(c.gates.len()) {
            if !gates_commute(t_gate, &c.gates[i]) {
                score += 1;
            }
        }
    }
    score
}

/// Generate K candidate templates and select the stickiest one
pub fn select_adversarial_template<R: Rng>(
    c: &CircuitSeq,
    pos: usize,
    n_wires: usize,
    k_candidates: usize,
    window: usize,
    rng: &mut R,
) -> Vec<[u8; 3]> {
    let mut best_template: Vec<[u8; 3]> = Vec::new();
    let mut best_score = 0;

    for _ in 0..k_candidates {
        // Generate a random identity pair (gate + same gate)
        let target = rng.random_range(0..n_wires as u8);
        let mut ctrl1 = rng.random_range(0..n_wires as u8);
        while ctrl1 == target {
            ctrl1 = rng.random_range(0..n_wires as u8);
        }
        let mut ctrl2 = rng.random_range(0..n_wires as u8);
        while ctrl2 == target || ctrl2 == ctrl1 {
            ctrl2 = rng.random_range(0..n_wires as u8);
        }

        let gate = [target, ctrl1, ctrl2];
        let candidate = vec![gate, gate]; // Identity pair

        let score = score_template_stickiness(c, &candidate, pos, window);

        if score > best_score || best_template.is_empty() {
            best_score = score;
            best_template = candidate;
        }
    }

    best_template
}

/// Move C: Try to swap two adjacent commuting gates
pub fn move_commute_swap<R: Rng>(c: &mut CircuitSeq, rng: &mut R) -> bool {
    use crate::reducer::gates_commute;

    if c.gates.len() < 2 {
        return false;
    }

    // Pick a random position
    let pos = rng.random_range(0..c.gates.len() - 1);

    if gates_commute(&c.gates[pos], &c.gates[pos + 1]) {
        c.gates.swap(pos, pos + 1);
        true
    } else {
        false
    }
}

/// Move A: Insert an identity template (pair of identical gates)
/// For now, use simple self-inverse gate pairs at random positions
pub fn move_insert_template<R: Rng>(c: &mut CircuitSeq, n_wires: usize, rng: &mut R) -> bool {
    if n_wires < 3 {
        return false;
    }

    // Generate a random gate
    let target = rng.random_range(0..n_wires as u8);
    let mut ctrl1 = rng.random_range(0..n_wires as u8);
    while ctrl1 == target {
        ctrl1 = rng.random_range(0..n_wires as u8);
    }
    let mut ctrl2 = rng.random_range(0..n_wires as u8);
    while ctrl2 == target || ctrl2 == ctrl1 {
        ctrl2 = rng.random_range(0..n_wires as u8);
    }

    let gate = [target, ctrl1, ctrl2];

    // Insert the gate and its inverse (which is itself for Toffoli-like gates)
    let pos = if c.gates.is_empty() {
        0
    } else {
        rng.random_range(0..=c.gates.len())
    };

    // Insert two identical gates (they cancel to identity)
    // But insert them at different positions to create mixing opportunity
    c.gates.insert(pos, gate);

    // Insert the second gate somewhere else (not adjacent)
    let pos2 = if c.gates.len() <= 2 {
        c.gates.len()
    } else {
        let min_dist = 2.min(c.gates.len() - 1);
        let range_start = (pos + min_dist).min(c.gates.len());
        if range_start >= c.gates.len() {
            c.gates.len()
        } else {
            rng.random_range(range_start..=c.gates.len())
        }
    };
    c.gates.insert(pos2, gate);

    true
}

/// Move A (Adversarial): Insert identity template with maximum stickiness
/// Samples K candidates and picks the one with most anti-commuting neighbors
pub fn move_insert_template_adversarial<R: Rng>(
    c: &mut CircuitSeq,
    n_wires: usize,
    k_candidates: usize,
    window: usize,
    rng: &mut R,
) -> bool {
    if n_wires < 3 {
        return false;
    }

    // Choose insertion position
    let pos = if c.gates.is_empty() {
        0
    } else {
        rng.random_range(0..=c.gates.len())
    };

    // Select stickiest template from K candidates
    let template = select_adversarial_template(c, pos, n_wires, k_candidates, window, rng);

    if template.is_empty() {
        return false;
    }

    // Insert template gates
    for (i, &gate) in template.iter().enumerate() {
        c.gates.insert(pos + i, gate);
    }

    // Insert the inverse part at a non-adjacent position
    let pos2 = if c.gates.len() <= template.len() + 2 {
        c.gates.len()
    } else {
        let range_start = (pos + template.len() + 2).min(c.gates.len());
        if range_start >= c.gates.len() {
            c.gates.len()
        } else {
            rng.random_range(range_start..=c.gates.len())
        }
    };

    // Insert the same gates reversed (inverses)
    for (i, &gate) in template.iter().rev().enumerate() {
        c.gates.insert(pos2 + i, gate);
    }

    true
}

/// Move B: Insert a patch pair (P and P^-1 at different positions)
pub fn move_patch_pair<R: Rng>(c: &mut CircuitSeq, n_wires: usize, rng: &mut R) -> bool {
    if n_wires < 3 {
        return false;
    }

    // Generate a small random subcircuit (2-4 gates)
    let patch_len = rng.random_range(2..=4);
    let mut patch: Vec<[u8; 3]> = Vec::with_capacity(patch_len);

    for _ in 0..patch_len {
        let target = rng.random_range(0..n_wires as u8);
        let mut ctrl1 = rng.random_range(0..n_wires as u8);
        while ctrl1 == target {
            ctrl1 = rng.random_range(0..n_wires as u8);
        }
        let mut ctrl2 = rng.random_range(0..n_wires as u8);
        while ctrl2 == target || ctrl2 == ctrl1 {
            ctrl2 = rng.random_range(0..n_wires as u8);
        }
        patch.push([target, ctrl1, ctrl2]);
    }

    // Insert patch at one position
    let pos1 = if c.gates.is_empty() {
        0
    } else {
        rng.random_range(0..=c.gates.len())
    };
    for (i, &gate) in patch.iter().enumerate() {
        c.gates.insert(pos1 + i, gate);
    }

    // Insert inverse (reversed) patch at another position
    let pos2 = if c.gates.len() <= patch_len {
        c.gates.len()
    } else {
        rng.random_range((pos1 + patch_len).min(c.gates.len())..=c.gates.len())
    };
    for (i, &gate) in patch.iter().rev().enumerate() {
        c.gates.insert(pos2 + i, gate);
    }

    true
}

/// Main annealing loop
pub fn anneal_run(initial: CircuitSeq, params: &AnnealParams) -> ObfRun {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    let mut rng = StdRng::seed_from_u64(params.seed);
    let mut current = initial.clone();
    let mut best = initial.clone();
    let mut best_energy = energy_slow(&best, params);

    let mut stats = AnnealStats {
        best_energy,
        ..Default::default()
    };

    for step in 0..params.steps {
        let temp = temperature(step, params);

        // Propose a move
        let move_type = select_move(&params.move_probs, &mut rng);
        let mut proposed = current.clone();

        let success = match move_type {
            MoveType::CommuteSwap => move_commute_swap(&mut proposed, &mut rng),
            MoveType::InsertTemplate => {
                if params.adversarial_mode {
                    move_insert_template_adversarial(
                        &mut proposed,
                        params.n_wires,
                        params.k_candidates,
                        params.stickiness_window,
                        &mut rng,
                    )
                } else {
                    move_insert_template(&mut proposed, params.n_wires, &mut rng)
                }
            }
            MoveType::PatchPair => move_patch_pair(&mut proposed, params.n_wires, &mut rng),
        };

        if !success {
            stats.reject_count += 1;
            continue;
        }

        // Calculate energy (use fast for most steps, slow at checkpoints)
        let use_slow = step % params.checkpoint_interval == 0;
        let current_e = if use_slow {
            energy_slow(&current, params)
        } else {
            energy_fast(&current, params)
        };
        let proposed_e = if use_slow {
            energy_slow(&proposed, params)
        } else {
            energy_fast(&proposed, params)
        };

        let delta_e = proposed_e - current_e;

        // Accept or reject
        if accept_move(delta_e, temp, &mut rng) {
            current = proposed;
            stats.accept_count += 1;
            *stats
                .move_counts
                .entry(move_type.name().to_string())
                .or_insert(0) += 1;

            // Update best if using slow energy
            if use_slow && proposed_e < best_energy {
                best = current.clone();
                best_energy = proposed_e;
                stats.best_energy = best_energy;
            }
        } else {
            stats.reject_count += 1;
        }

        // Log checkpoint
        if use_slow {
            stats.energy_trace.push(current_e);
            stats.temperature_trace.push(temp);
        }
    }

    stats.final_energy = energy_slow(&current, params);

    ObfRun {
        c_final: current,
        c_best: best,
        stats,
        seed: params.seed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_schedule() {
        let params = AnnealParams {
            t0: 1.0,
            t_end: 0.01,
            steps: 100,
            ..Default::default()
        };

        let t_start = temperature(0, &params);
        let t_mid = temperature(50, &params);
        let t_end = temperature(99, &params);

        assert!((t_start - 1.0).abs() < 0.001);
        assert!(t_mid < t_start && t_mid > t_end);
        assert!((t_end - 0.01).abs() < 0.001);
    }

    #[test]
    fn test_metropolis_acceptance() {
        let mut rng = rand::rng();

        // Always accept improvements
        assert!(accept_move(-0.5, 1.0, &mut rng));

        // Never accept at T=0
        assert!(!accept_move(0.5, 0.0, &mut rng));
    }

    #[test]
    fn test_anneal_run_basic() {
        let initial = CircuitSeq { gates: vec![] };
        let params = AnnealParams {
            steps: 100,
            n_wires: 8,
            checkpoint_interval: 50,
            ..Default::default()
        };

        let result = anneal_run(initial, &params);

        // Should have grown the circuit
        assert!(result.c_final.gates.len() > 0 || result.c_best.gates.len() > 0);
        assert!(result.stats.accept_count + result.stats.reject_count == params.steps);
    }
}
