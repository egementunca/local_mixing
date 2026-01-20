//! Template-Seeded Identity Growth
//!
//! Build large identity circuits by placing many small identity templates
//! (limited active wires) into an empty W-wire circuit, then diffuse and
//! partially compress in repeated rounds to produce large, hard-to-reduce identities.
//!
//! See: local_mixing/docs/IDENTITY_GROWTH_PLAN.md

use rand::Rng;
use rand::seq::SliceRandom;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::algorithms::annealing::local::{ReduceConfig, reduce_circuit};
use crate::algorithms::butterfly::replace::random_canonical_id;
use crate::infra::circuit::CircuitSeq;
use crate::infra::random::random_data::{random_circuit, shoot_random_gate};
use crate::infra::store::reader::TemplateDB;

/// Source for identity templates
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum TemplateSource {
    /// Use perm tables in ./db (random_canonical_id)
    #[default]
    PermTables,
    /// Use TemplateDB (collection.lmdb) - get_random_identity
    TemplateDB,
    /// Alternate between both sources
    Mixed,
}

/// Wire selection strategy for the skeleton graph
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum SkeletonMode {
    /// Prefer low-degree wires to spread coverage
    #[default]
    Balanced,
    /// Allow overlap to create dense interaction regions
    Dense,
    /// Force disjoint subsets (early rounds)
    Sparse,
}

/// Configuration for the identity growth pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityGrowthConfig {
    /// Target number of wires (W) - final circuit width
    pub target_width: usize,
    /// Minimum template width (e.g., 3)
    pub template_width_min: usize,
    /// Maximum template width (e.g., 7)
    pub template_width_max: usize,
    /// Source for identity templates
    pub template_source: TemplateSource,
    /// Minimum template gate count (applied after optional conjugation)
    pub template_gate_count_min: usize,
    /// Maximum template gate count (applied after optional conjugation)
    pub template_gate_count_max: usize,
    /// Attempts to sample a template that meets size + hardness constraints
    pub template_attempts: usize,
    /// Min depth for conjugation wrapper (0 disables)
    pub template_conjugation_depth_min: usize,
    /// Max depth for conjugation wrapper (0 disables)
    pub template_conjugation_depth_max: usize,
    /// Passes for quick hardness check (0 disables)
    pub template_hardness_passes: usize,
    /// Minimum reducer survival ratio for accepting a template
    pub template_min_reducer_ratio: f64,
    /// Number of template placements per round
    pub placements_per_round: usize,
    /// Number of A→B→C rounds
    pub rounds: usize,
    /// Diffusion intensity (shoot_random_gate passes)
    pub diffusion_passes: usize,
    /// Maximum compression passes per round
    pub compression_budget: usize,
    /// Stop compression if ratio falls below this (e.g., 0.3)
    pub min_survival_ratio: f64,
    /// Wire selection mode
    pub skeleton_mode: SkeletonMode,
}

impl Default for IdentityGrowthConfig {
    fn default() -> Self {
        Self {
            target_width: 32,
            template_width_min: 3,
            template_width_max: 6,
            template_source: TemplateSource::PermTables,
            template_gate_count_min: 0,
            template_gate_count_max: 10_000,
            template_attempts: 20,
            template_conjugation_depth_min: 0,
            template_conjugation_depth_max: 0,
            template_hardness_passes: 0,
            template_min_reducer_ratio: 0.0,
            placements_per_round: 10,
            rounds: 5,
            diffusion_passes: 100_000,
            compression_budget: 50,
            min_survival_ratio: 0.3,
            skeleton_mode: SkeletonMode::Balanced,
        }
    }
}

/// Metrics from the growth process
#[derive(Debug, Clone, Default)]
pub struct GrowthMetrics {
    /// Gate count after each round
    pub gates_per_round: Vec<usize>,
    /// Reducer ratio after each round (after bounded compression)
    pub reducer_ratios_per_round: Vec<f64>,
    /// Wire coverage (unique wires touched / total wires)
    pub wire_coverage: f64,
    /// Final gate count
    pub final_gates: usize,
    /// Number of templates placed
    pub templates_placed: usize,
    /// Number of placements skipped (no suitable template)
    pub templates_skipped: usize,
}

/// Tracks wire-to-wire interaction for placement guidance
#[derive(Debug, Clone)]
pub struct SkeletonGraph {
    num_wires: usize,
    /// Edge weights: (w1, w2) -> count (w1 < w2)
    edges: HashMap<(usize, usize), usize>,
    /// Degree of each wire
    degrees: Vec<usize>,
}

impl SkeletonGraph {
    pub fn new(num_wires: usize) -> Self {
        Self {
            num_wires,
            edges: HashMap::new(),
            degrees: vec![0; num_wires],
        }
    }

    /// Update the skeleton from a circuit's gates
    pub fn update_from_circuit(&mut self, circuit: &CircuitSeq) {
        for gate in &circuit.gates {
            let wires = [gate[0] as usize, gate[1] as usize, gate[2] as usize];
            // Add edges for all pairs
            for i in 0..3 {
                for j in (i + 1)..3 {
                    let (a, b) = if wires[i] < wires[j] {
                        (wires[i], wires[j])
                    } else {
                        (wires[j], wires[i])
                    };
                    *self.edges.entry((a, b)).or_insert(0) += 1;
                    self.degrees[wires[i]] += 1;
                    self.degrees[wires[j]] += 1;
                }
            }
        }
    }

    /// Select a subset of wires for template placement
    pub fn select_subset<R: Rng>(
        &self,
        size: usize,
        mode: SkeletonMode,
        rng: &mut R,
    ) -> Vec<usize> {
        let mut wires: Vec<usize> = (0..self.num_wires).collect();

        match mode {
            SkeletonMode::Balanced => {
                // Sort by degree (ascending) to prefer low-degree wires
                wires.sort_by_key(|&w| self.degrees[w]);
                // Take the first `size` lowest-degree wires, with some randomness
                let pool_size = (size * 2).min(wires.len());
                let pool = &mut wires[..pool_size];
                pool.shuffle(rng);
                pool[..size].to_vec()
            }
            SkeletonMode::Dense => {
                // Random selection (allow overlap)
                wires.shuffle(rng);
                wires[..size].to_vec()
            }
            SkeletonMode::Sparse => {
                // Prefer wires with degree 0 (untouched)
                let untouched: Vec<usize> = wires
                    .iter()
                    .copied()
                    .filter(|&w| self.degrees[w] == 0)
                    .collect();
                if untouched.len() >= size {
                    let mut subset = untouched;
                    subset.shuffle(rng);
                    subset[..size].to_vec()
                } else {
                    // Fall back to balanced if not enough untouched
                    self.select_subset(size, SkeletonMode::Balanced, rng)
                }
            }
        }
    }

    /// Get wire coverage as fraction
    pub fn coverage(&self) -> f64 {
        let touched = self.degrees.iter().filter(|&&d| d > 0).count();
        touched as f64 / self.num_wires as f64
    }
}

#[derive(Debug, Clone)]
struct SampledTemplate {
    circuit: CircuitSeq,
    width: usize,
}

fn normalized_range(min: usize, max: usize) -> (usize, usize) {
    if min <= max {
        (min, max)
    } else {
        (max, min)
    }
}

fn select_template_width<R: Rng>(config: &IdentityGrowthConfig, rng: &mut R) -> usize {
    let (min_raw, max_raw) = normalized_range(config.template_width_min, config.template_width_max);
    let max = max_raw.min(config.target_width);
    let min = min_raw.min(max);
    if min == max {
        min
    } else {
        rng.random_range(min..=max)
    }
}

fn select_template_source<R: Rng>(
    config: &IdentityGrowthConfig,
    template_db: Option<&TemplateDB>,
    rng: &mut R,
) -> TemplateSource {
    match config.template_source {
        TemplateSource::Mixed => {
            if template_db.is_some() && rng.random_bool(0.5) {
                TemplateSource::TemplateDB
            } else {
                TemplateSource::PermTables
            }
        }
        other => other,
    }
}

fn template_gate_bounds(config: &IdentityGrowthConfig) -> (usize, usize) {
    normalized_range(config.template_gate_count_min, config.template_gate_count_max)
}

fn gate_count_in_range(count: usize, config: &IdentityGrowthConfig) -> bool {
    let (min, max) = template_gate_bounds(config);
    count >= min && count <= max
}

fn sample_template_from_perm(
    env: &lmdb::Environment,
    conn: &Connection,
    width: usize,
) -> Option<CircuitSeq> {
    random_canonical_id(env, conn, width).ok()
}

fn sample_template_from_db<R: Rng>(
    db: &TemplateDB,
    width: usize,
    config: &IdentityGrowthConfig,
    rng: &mut R,
    gate_count_cache: &mut HashMap<usize, Vec<u16>>,
) -> Option<CircuitSeq> {
    let width_u8 = u8::try_from(width).ok()?;
    let (min_gc, max_gc) = template_gate_bounds(config);
    let min_conj = config.template_conjugation_depth_min;
    let max_conj = config.template_conjugation_depth_max;
    let base_min = min_gc.saturating_sub(max_conj.saturating_mul(2));
    let base_max = max_gc.saturating_sub(min_conj.saturating_mul(2));

    let max_gc = base_max.min(u16::MAX as usize);
    let min_gc = base_min.min(max_gc);

    let counts = gate_count_cache
        .entry(width)
        .or_insert_with(|| db.get_available_gate_counts(width_u8));

    let candidates: Vec<u16> = counts
        .iter()
        .copied()
        .filter(|&gc| {
            let gc_usize = gc as usize;
            gc_usize >= min_gc && gc_usize <= max_gc
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    for _ in 0..3 {
        let idx = rng.random_range(0..candidates.len());
        let gc = candidates[idx];
        if let Some(record) = db.get_random_identity(width_u8, gc) {
            return Some(record.to_circuit_seq());
        }
    }

    None
}

fn conjugate_template<R: Rng>(
    template: &CircuitSeq,
    width: usize,
    config: &IdentityGrowthConfig,
    rng: &mut R,
) -> CircuitSeq {
    let (min_depth, max_depth) = normalized_range(
        config.template_conjugation_depth_min,
        config.template_conjugation_depth_max,
    );
    if max_depth == 0 {
        return template.clone();
    }
    let depth = if min_depth == max_depth {
        min_depth
    } else {
        rng.random_range(min_depth..=max_depth)
    };
    if depth == 0 {
        return template.clone();
    }
    let width_u8 = match u8::try_from(width) {
        Ok(w) => w,
        Err(_) => return template.clone(),
    };
    let r = random_circuit(width_u8, depth);
    let r_inv = r.inverse();
    r.concat(template).concat(&r_inv)
}

fn template_passes_hardness(template: &CircuitSeq, config: &IdentityGrowthConfig) -> bool {
    if config.template_hardness_passes == 0 || config.template_min_reducer_ratio <= 0.0 {
        return true;
    }
    let mut reduced = template.clone();
    let before = reduced.gates.len();
    if before == 0 {
        return true;
    }
    let reduce_cfg = ReduceConfig {
        max_passes: config.template_hardness_passes,
        max_stall: config.template_hardness_passes,
    };
    reduce_circuit(&mut reduced, &reduce_cfg, None, None);
    let after = reduced.gates.len();
    let ratio = after as f64 / before as f64;
    ratio >= config.template_min_reducer_ratio
}

fn sample_template<R: Rng>(
    config: &IdentityGrowthConfig,
    env: &lmdb::Environment,
    conn: &Connection,
    template_db: Option<&TemplateDB>,
    rng: &mut R,
    gate_count_cache: &mut HashMap<usize, Vec<u16>>,
) -> Option<SampledTemplate> {
    let attempts = config.template_attempts.max(1);
    for _ in 0..attempts {
        let template_width = select_template_width(config, rng);
        let source = select_template_source(config, template_db, rng);
        let mut template = match source {
            TemplateSource::PermTables => sample_template_from_perm(env, conn, template_width),
            TemplateSource::TemplateDB => template_db.and_then(|db| {
                sample_template_from_db(db, template_width, config, rng, gate_count_cache)
            }),
            TemplateSource::Mixed => unreachable!("Mixed handled in select_template_source"),
        };

        let Some(raw_template) = template.take() else {
            continue;
        };

        let inflated = conjugate_template(&raw_template, template_width, config, rng);
        if !gate_count_in_range(inflated.gates.len(), config) {
            continue;
        }
        if !template_passes_hardness(&inflated, config) {
            continue;
        }

        return Some(SampledTemplate {
            circuit: inflated,
            width: template_width,
        });
    }

    None
}

/// Rewire a small template (on wires 0..k) to use the given target wires
fn rewire_to_subset(
    template: &CircuitSeq,
    target_wires: &[usize],
    _target_width: usize,
) -> CircuitSeq {
    // Build permutation: wire i in template -> target_wires[i]
    let k = target_wires.len();
    let mut perm_data = vec![0; k];
    for i in 0..k {
        perm_data[i] = target_wires[i];
    }

    // Apply the permutation to the template
    let new_gates: Vec<[u8; 3]> = template
        .gates
        .iter()
        .map(|g| {
            [
                perm_data[g[0] as usize] as u8,
                perm_data[g[1] as usize] as u8,
                perm_data[g[2] as usize] as u8,
            ]
        })
        .collect();

    CircuitSeq { gates: new_gates }
}

fn bounded_reduce(circuit: &CircuitSeq, config: &IdentityGrowthConfig) -> (CircuitSeq, f64, bool) {
    if circuit.gates.is_empty() || config.compression_budget == 0 {
        return (circuit.clone(), 1.0, true);
    }
    let mut reduced = circuit.clone();
    let before = reduced.gates.len();

    simple_compress(&mut reduced);

    if config.compression_budget > 0 {
        let reduce_cfg = ReduceConfig {
            max_passes: config.compression_budget,
            max_stall: config.compression_budget,
        };
        reduce_circuit(&mut reduced, &reduce_cfg, None, None);
    }

    let after = reduced.gates.len();
    let ratio = if before > 0 {
        after as f64 / before as f64
    } else {
        1.0
    };
    if ratio < config.min_survival_ratio {
        (circuit.clone(), ratio, false)
    } else {
        (reduced, ratio, true)
    }
}

/// Main entry point: grow a large identity circuit
pub fn grow_identity(
    config: &IdentityGrowthConfig,
    env: &lmdb::Environment,
    conn: &Connection,
    template_db: Option<&TemplateDB>,
) -> Result<(CircuitSeq, GrowthMetrics), Box<dyn std::error::Error>> {
    if matches!(config.template_source, TemplateSource::TemplateDB) && template_db.is_none() {
        return Err("TemplateDB source requested but no TemplateDB provided".into());
    }
    if matches!(config.template_source, TemplateSource::Mixed) && template_db.is_none() {
        eprintln!("Warning: Mixed template source without TemplateDB; using perm tables only");
    }
    let mut rng = rand::rng();
    let mut circuit = CircuitSeq { gates: vec![] };
    let mut skeleton = SkeletonGraph::new(config.target_width);
    let mut metrics = GrowthMetrics::default();
    let mut gate_count_cache: HashMap<usize, Vec<u16>> = HashMap::new();

    println!(
        "Identity Growth: {} wires, {} rounds, {} placements/round",
        config.target_width, config.rounds, config.placements_per_round
    );

    for round in 0..config.rounds {
        println!("\n=== Round {}/{} ===", round + 1, config.rounds);
        let round_start_gates = circuit.gates.len();

        // --- Stage A: Seed Placement ---
        let mut placed_this_round = 0;
        let mut skipped_this_round = 0;
        for _ in 0..config.placements_per_round {
            let sampled = sample_template(
                config,
                env,
                conn,
                template_db,
                &mut rng,
                &mut gate_count_cache,
            );
            let Some(sampled) = sampled else {
                skipped_this_round += 1;
                metrics.templates_skipped += 1;
                continue;
            };

            // Select wire subset
            let target_wires =
                skeleton.select_subset(sampled.width, config.skeleton_mode, &mut rng);

            // Rewire template to target wires
            let rewired = rewire_to_subset(&sampled.circuit, &target_wires, config.target_width);

            // Insert at random position
            let pos = if circuit.gates.is_empty() {
                0
            } else {
                rng.random_range(0..=circuit.gates.len())
            };
            circuit.splice(pos, &rewired);

            // Update skeleton
            skeleton.update_from_circuit(&rewired);
            placed_this_round += 1;
            metrics.templates_placed += 1;
        }

        println!(
            "  Stage A: Placed {} templates (skipped {}), now {} gates",
            placed_this_round,
            skipped_this_round,
            circuit.gates.len()
        );

        // --- Stage B: Diffusion ---
        shoot_random_gate(&mut circuit, config.diffusion_passes);
        println!(
            "  Stage B: Diffused with {} passes",
            config.diffusion_passes
        );

        // --- Stage C: Bounded Compression ---
        let before = circuit.gates.len();
        let (compressed, ratio, accepted) = bounded_reduce(&circuit, config);
        if accepted {
            circuit = compressed;
        }
        let after = circuit.gates.len();

        println!(
            "  Stage C: Compressed {} -> {} gates (ratio: {:.2}){}",
            before,
            after,
            ratio,
            if accepted { "" } else { " [rejected]" }
        );
        if !accepted {
            println!("  Warning: Survival ratio below threshold, skipping compression");
        }

        metrics.gates_per_round.push(circuit.gates.len());
        metrics.reducer_ratios_per_round.push(ratio);
        println!(
            "  Round {} complete: {} gates (added {} this round)",
            round + 1,
            circuit.gates.len(),
            circuit.gates.len().saturating_sub(round_start_gates)
        );
    }

    metrics.final_gates = circuit.gates.len();
    metrics.wire_coverage = skeleton.coverage();

    println!("\n=== Growth Complete ===");
    println!("  Final gates: {}", metrics.final_gates);
    println!("  Templates placed: {}", metrics.templates_placed);
    println!("  Templates skipped: {}", metrics.templates_skipped);
    println!("  Wire coverage: {:.1}%", metrics.wire_coverage * 100.0);

    Ok((circuit, metrics))
}

/// Simple compression: remove adjacent duplicate gates
fn simple_compress(circuit: &mut CircuitSeq) {
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skeleton_graph_basic() {
        let mut skeleton = SkeletonGraph::new(8);
        assert_eq!(skeleton.coverage(), 0.0);

        let circuit = CircuitSeq {
            gates: vec![[0, 1, 2], [3, 4, 5]],
        };
        skeleton.update_from_circuit(&circuit);
        assert!(skeleton.coverage() > 0.0);
    }

    #[test]
    fn test_rewire_to_subset() {
        let template = CircuitSeq {
            gates: vec![[0, 1, 2]],
        };
        let target_wires = vec![10, 20, 30];
        let rewired = rewire_to_subset(&template, &target_wires, 32);
        assert_eq!(rewired.gates[0], [10, 20, 30]);
    }
}
