//! Local rewrite obfuscation: two-stage (inflation + kneading) with attack-aligned metrics.
//!
//! This module is method-agnostic: concrete techniques (pairwise replace, identity insertion,
//! perm-table sampling, SAT, etc.) are used through the EquivalenceOracle interface.

use crate::hashing::canonical::canonical_window_key;
use crate::infra::circuit::{CircuitSeq, Permutation};
use crate::infra::random::random_data::{contiguous_convex, find_convex_subcircuit, get_canonical, random_circuit};
use itertools::Itertools;
use lmdb::{Cursor, Database, Environment, Transaction};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// -----------------------------------------------------------------------------
// Config
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LocalityBudget {
    pub max_len: usize,
    pub max_wires: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InflationConfig {
    pub steps: usize,
    pub len_min: usize,
    pub len_max: usize,
    pub delta_min: usize,
    pub delta_max: usize,
    pub max_active_wires: usize,
    pub attempts_per_step: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KneadingConfig {
    pub steps: usize,
    pub window_schedule: Vec<usize>,
    pub max_active_wires: usize,
    pub inner_rewrites: usize,
    pub inner_len_min: usize,
    pub inner_len_max: usize,
    pub attempts_per_step: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackConfig {
    pub max_passes: usize,
    pub trials_per_pass: usize,
    pub window_len_min: usize,
    pub window_len_max: usize,
    pub max_active_wires: usize,
    pub out_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub gap_window_sizes: Vec<usize>,
    pub samples_per_size: usize,
    pub entropy_window_size: usize,
    pub entropy_samples: usize,
    pub entropy_max_count: usize,
    pub sector_seed_count: usize,
    pub sector_sample_windows: usize,
    pub sector_window_len: usize,
    pub max_active_wires: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRewriteConfig {
    pub num_wires: usize,
    pub base_depth: usize,
    pub inflation: InflationConfig,
    pub kneading: KneadingConfig,
    pub attack: AttackConfig,
    pub metrics: MetricsConfig,
    pub seed: u64,
}

impl Default for LocalRewriteConfig {
    fn default() -> Self {
        Self {
            num_wires: 64,
            base_depth: 40,
            inflation: InflationConfig {
                steps: 400,
                len_min: 4,
                len_max: 10,
                delta_min: 2,
                delta_max: 6,
                max_active_wires: 7,
                attempts_per_step: 5,
            },
            kneading: KneadingConfig {
                steps: 400,
                window_schedule: vec![32, 64],
                max_active_wires: 7,
                inner_rewrites: 2,
                inner_len_min: 4,
                inner_len_max: 10,
                attempts_per_step: 5,
            },
            attack: AttackConfig {
                max_passes: 50,
                trials_per_pass: 200,
                window_len_min: 4,
                window_len_max: 10,
                max_active_wires: 7,
                out_len: 6,
            },
            metrics: MetricsConfig {
                gap_window_sizes: vec![4, 8, 16, 32, 64],
                samples_per_size: 200,
                entropy_window_size: 6,
                entropy_samples: 100,
                entropy_max_count: 1000,
                sector_seed_count: 5,
                sector_sample_windows: 200,
                sector_window_len: 6,
                max_active_wires: 7,
            },
            seed: 0,
        }
    }
}

// -----------------------------------------------------------------------------
// Oracle (perm tables)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleStats {
    pub queries: usize,
    pub hits: usize,
    pub same_size_hits: usize,
    pub shorter_hits: usize,
    pub longer_hits: usize,
}

impl Default for OracleStats {
    fn default() -> Self {
        Self {
            queries: 0,
            hits: 0,
            same_size_hits: 0,
            shorter_hits: 0,
            longer_hits: 0,
        }
    }
}

pub struct PermTableOracle {
    env: Environment,
    dbs: HashMap<(usize, usize), Database>, // (width, gate_count)
    bit_shufs: HashMap<usize, Vec<Vec<usize>>>,
    pub stats: OracleStats,
}

impl PermTableOracle {
    pub fn open<P: AsRef<Path>>(path: P, max_width: usize, max_len: usize) -> Option<Self> {
        if !path.as_ref().exists() {
            return None;
        }

        let env = Environment::new()
            .set_max_dbs(200)
            .set_map_size(700 * 1024 * 1024 * 1024)
            .open(path.as_ref())
            .ok()?;

        let mut dbs = HashMap::new();
        for w in 1..=max_width {
            for m in 1..=max_len {
                let db_name = format!("n{}m{}", w, m);
                if let Ok(db) = env.open_db(Some(&db_name)) {
                    dbs.insert((w, m), db);
                }
            }
        }

        let mut bit_shufs = HashMap::new();
        for w in 3..=max_width {
            let perms: Vec<Vec<usize>> = (0..w).permutations(w).collect();
            let shuf = perms.into_iter().skip(1).collect::<Vec<_>>();
            bit_shufs.insert(w, shuf);
        }

        Some(Self {
            env,
            dbs,
            bit_shufs,
            stats: OracleStats::default(),
        })
    }

    pub fn available_lengths(&self, width: usize) -> Vec<usize> {
        let mut lens: Vec<usize> = self
            .dbs
            .keys()
            .filter(|(w, _)| *w == width)
            .map(|(_, m)| *m)
            .collect();
        lens.sort_unstable();
        lens
    }

    fn canonical_perm(&self, window: &CircuitSeq, width: usize) -> Option<(Permutation, Permutation)> {
        if width < 3 {
            return None;
        }
        let bit_shuf = self.bit_shufs.get(&width)?;
        let perm = window.permutation(width);
        let canon = get_canonical(&perm, bit_shuf);
        Some((canon.perm, canon.shuffle))
    }

    fn random_perm_lmdb(txn: &lmdb::RoTransaction, db: Database, prefix: &[u8]) -> Option<Vec<u8>> {
        let mut cursor = txn.open_ro_cursor(db).ok()?;
        let mut rng = rand::rng();
        let mut chosen: Option<Vec<u8>> = None;
        let mut count = 0;

        for (key, _) in cursor.iter_from(prefix) {
            if !key.starts_with(prefix) {
                break;
            }
            count += 1;
            if rng.random_range(0..count) == 0 {
                chosen = Some(key[prefix.len()..].to_vec());
            }
        }
        chosen
    }

    fn lookup_with_len(
        &mut self,
        width: usize,
        gate_count: usize,
        perm_blob: &[u8],
    ) -> Option<CircuitSeq> {
        let db = *self.dbs.get(&(width, gate_count))?;
        let txn = self.env.begin_ro_txn().ok()?;
        let res = Self::random_perm_lmdb(&txn, db, perm_blob)?;
        Some(CircuitSeq::from_blob(&res))
    }

    pub fn get_same_size_alt(
        &mut self,
        window: &CircuitSeq,
        width: usize,
        attempts: usize,
    ) -> Option<CircuitSeq> {
        self.stats.queries += 1;
        let len = window.gates.len();
        let (canon_perm, canon_shuf) = self.canonical_perm(window, width)?;
        let perm_blob = canon_perm.repr_blob();

        for _ in 0..attempts.max(1) {
            if let Some(mut repl) = self.lookup_with_len(width, len, &perm_blob) {
                // Avoid identical replacement
                if repl.repr_blob() == window.repr_blob() {
                    continue;
                }
                // Map from canonical wires back to window wires
                repl.rewire(&canon_shuf.invert(), width);

                self.stats.hits += 1;
                self.stats.same_size_hits += 1;
                return Some(repl);
            }
        }
        None
    }

    pub fn get_shortest_leq(
        &mut self,
        window: &CircuitSeq,
        width: usize,
        max_len: usize,
        attempts_per_len: usize,
    ) -> Option<CircuitSeq> {
        self.stats.queries += 1;
        let len = window.gates.len();
        if len <= 1 {
            return None;
        }

        let (canon_perm, canon_shuf) = self.canonical_perm(window, width)?;
        let perm_blob = canon_perm.repr_blob();

        let upper = max_len.min(len.saturating_sub(1));
        for target_len in 1..=upper {
            for _ in 0..attempts_per_len.max(1) {
                if let Some(mut repl) = self.lookup_with_len(width, target_len, &perm_blob) {
                    repl.rewire(&canon_shuf.invert(), width);
                    self.stats.hits += 1;
                    self.stats.shorter_hits += 1;
                    return Some(repl);
                }
            }
        }
        None
    }

    pub fn get_longer_at_least(
        &mut self,
        window: &CircuitSeq,
        width: usize,
        min_len: usize,
        max_len: usize,
        attempts_per_len: usize,
    ) -> Option<CircuitSeq> {
        self.stats.queries += 1;
        let (canon_perm, canon_shuf) = self.canonical_perm(window, width)?;
        let perm_blob = canon_perm.repr_blob();

        let start_len = min_len.max(window.gates.len() + 1);
        for target_len in start_len..=max_len {
            for _ in 0..attempts_per_len.max(1) {
                if let Some(mut repl) = self.lookup_with_len(width, target_len, &perm_blob) {
                    repl.rewire(&canon_shuf.invert(), width);
                    self.stats.hits += 1;
                    self.stats.longer_hits += 1;
                    return Some(repl);
                }
            }
        }
        None
    }

    pub fn count_alternatives(
        &mut self,
        window: &CircuitSeq,
        width: usize,
        len: usize,
        max_count: usize,
    ) -> Option<usize> {
        let (canon_perm, _canon_shuf) = self.canonical_perm(window, width)?;
        let perm_blob = canon_perm.repr_blob();
        let db = *self.dbs.get(&(width, len))?;
        let txn = self.env.begin_ro_txn().ok()?;
        let mut cursor = txn.open_ro_cursor(db).ok()?;
        let mut count = 0usize;
        for (key, _) in cursor.iter_from(&perm_blob) {
            if !key.starts_with(&perm_blob) {
                break;
            }
            count += 1;
            if count >= max_count {
                break;
            }
        }
        Some(count)
    }
}

// -----------------------------------------------------------------------------
// Window selection
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Window {
    pub start: usize,
    pub end: usize, // exclusive
    pub used_wires: Vec<u8>,
    pub rewired: CircuitSeq,
}

fn shares_wire(a: &[u8; 3], b: &[u8; 3]) -> bool {
    a.iter().any(|w| b.contains(w))
}

fn is_connected(circuit: &CircuitSeq, indices: &[usize]) -> bool {
    if indices.is_empty() {
        return false;
    }

    let mut stack = vec![indices[0]];
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(indices[0]);

    while let Some(idx) = stack.pop() {
        let gate = &circuit.gates[idx];
        for &j in indices {
            if seen.contains(&j) {
                continue;
            }
            if shares_wire(gate, &circuit.gates[j]) {
                seen.insert(j);
                stack.push(j);
            }
        }
    }

    seen.len() == indices.len()
}

fn sample_window<R: Rng>(
    circuit: &mut CircuitSeq,
    num_wires: usize,
    len: usize,
    max_wires: usize,
    rng: &mut R,
) -> Option<Window> {
    if circuit.gates.len() < len || len < 2 {
        return None;
    }

    let (mut indices, _tries) = find_convex_subcircuit(len, max_wires, num_wires, circuit, rng);
    if indices.is_empty() {
        return None;
    }

    indices.sort_unstable();
    if !is_connected(circuit, &indices) {
        return None;
    }

    let (start, end) = contiguous_convex(circuit, &mut indices, num_wires)?;
    let end_exclusive = end + 1;
    if end_exclusive <= start {
        return None;
    }

    let mut used: HashSet<u8> = HashSet::new();
    for gate in &circuit.gates[start..end_exclusive] {
        used.insert(gate[0]);
        used.insert(gate[1]);
        used.insert(gate[2]);
    }
    let mut used_wires: Vec<u8> = used.into_iter().collect();
    used_wires.sort_unstable();

    let sub_indices: Vec<usize> = (start..end_exclusive).collect();
    let rewired = CircuitSeq::rewire_subcircuit(circuit, &sub_indices, &used_wires);

    Some(Window {
        start,
        end: end_exclusive,
        used_wires,
        rewired,
    })
}

// -----------------------------------------------------------------------------
// Rewriter helpers
// -----------------------------------------------------------------------------

fn splice_replacement(
    circuit: &mut CircuitSeq,
    window: &Window,
    replacement: &CircuitSeq,
) {
    let unrewired = CircuitSeq::unrewire_subcircuit(replacement, &window.used_wires);
    circuit
        .gates
        .splice(window.start..window.end, unrewired.gates);
}

fn build_identity_padding<R: Rng>(width: usize, depth: usize, rng: &mut R) -> CircuitSeq {
    let p = random_circuit(width as u8, depth);
    let p_inv = p.inverse();
    p.concat(&p_inv)
}

// -----------------------------------------------------------------------------
// Stages
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageStats {
    pub steps: usize,
    pub windows_tried: usize,
    pub replacements: usize,
}

fn run_inflation<R: Rng>(
    circuit: &mut CircuitSeq,
    cfg: &InflationConfig,
    oracle: &mut PermTableOracle,
    num_wires: usize,
    rng: &mut R,
) -> StageStats {
    let mut stats = StageStats {
        steps: cfg.steps,
        windows_tried: 0,
        replacements: 0,
    };

    for _ in 0..cfg.steps {
        let len = rng.random_range(cfg.len_min..=cfg.len_max);
        let mut applied = false;

        for _ in 0..cfg.attempts_per_step.max(1) {
            let window = match sample_window(circuit, num_wires, len, cfg.max_active_wires, rng) {
                Some(w) => w,
                None => continue,
            };

            stats.windows_tried += 1;
            let active = window.used_wires.len();

            let delta = rng.random_range(cfg.delta_min..=cfg.delta_max);
            let target_len = len + delta;
            let max_len = target_len + cfg.delta_max;

            if let Some(repl) = oracle.get_longer_at_least(&window.rewired, active, target_len, max_len, 3) {
                splice_replacement(circuit, &window, &repl);
                stats.replacements += 1;
                applied = true;
                break;
            }

            // Fallback: identity padding
            let pad_depth = rng.random_range(cfg.delta_min..=cfg.delta_max);
            let padding = build_identity_padding(active, pad_depth, rng);
            let mut repl = window.rewired.clone();
            repl.gates.extend(padding.gates);
            splice_replacement(circuit, &window, &repl);
            stats.replacements += 1;
            applied = true;
            break;
        }

        if !applied {
            continue;
        }
    }

    stats
}

fn run_kneading<R: Rng>(
    circuit: &mut CircuitSeq,
    cfg: &KneadingConfig,
    oracle: &mut PermTableOracle,
    num_wires: usize,
    rng: &mut R,
) -> StageStats {
    let mut stats = StageStats {
        steps: cfg.steps,
        windows_tried: 0,
        replacements: 0,
    };

    if cfg.window_schedule.is_empty() {
        return stats;
    }

    for step in 0..cfg.steps {
        let len = cfg.window_schedule[step % cfg.window_schedule.len()];
        let mut applied = false;

        for _ in 0..cfg.attempts_per_step.max(1) {
            let window = match sample_window(circuit, num_wires, len, cfg.max_active_wires, rng) {
                Some(w) => w,
                None => continue,
            };

            stats.windows_tried += 1;
            let active = window.used_wires.len();

            if let Some(repl) = oracle.get_same_size_alt(&window.rewired, active, 3) {
                splice_replacement(circuit, &window, &repl);
                stats.replacements += 1;
                applied = true;
                break;
            }

            // If no same-size replacement exists, try inner rewrites
            if cfg.inner_rewrites > 0 {
                for _ in 0..cfg.inner_rewrites {
                    let inner_len = rng.random_range(cfg.inner_len_min..=cfg.inner_len_max);
                    if let Some(inner) = sample_window(circuit, num_wires, inner_len, cfg.max_active_wires, rng) {
                        let inner_active = inner.used_wires.len();
                        if let Some(inner_repl) = oracle.get_same_size_alt(&inner.rewired, inner_active, 2) {
                            splice_replacement(circuit, &inner, &inner_repl);
                            stats.replacements += 1;
                            applied = true;
                        }
                    }
                }
            }
        }

        if !applied {
            continue;
        }
    }

    stats
}

// -----------------------------------------------------------------------------
// Attacker
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackReport {
    pub passes: usize,
    pub total_replacements: usize,
    pub total_gate_reduction: usize,
}

fn attack_peel<R: Rng>(
    circuit: &mut CircuitSeq,
    cfg: &AttackConfig,
    oracle: &mut PermTableOracle,
    num_wires: usize,
    rng: &mut R,
) -> AttackReport {
    let mut report = AttackReport {
        passes: 0,
        total_replacements: 0,
        total_gate_reduction: 0,
    };

    for _pass in 0..cfg.max_passes {
        let mut replaced_this_pass = 0;
        let len_before = circuit.gates.len();

        for _ in 0..cfg.trials_per_pass {
            let len = rng.random_range(cfg.window_len_min..=cfg.window_len_max);
            let window = match sample_window(circuit, num_wires, len, cfg.max_active_wires, rng) {
                Some(w) => w,
                None => continue,
            };

            let active = window.used_wires.len();
            let repl = oracle.get_shortest_leq(&window.rewired, active, cfg.out_len, 2);
            if let Some(repl) = repl {
                let original_len = window.rewired.gates.len();
                let new_len = repl.gates.len();
                if new_len < original_len {
                    splice_replacement(circuit, &window, &repl);
                    replaced_this_pass += 1;
                }
            }
        }

        let len_after = circuit.gates.len();
        report.passes += 1;
        report.total_replacements += replaced_this_pass;
        if len_after < len_before {
            report.total_gate_reduction += len_before - len_after;
        }

        if replaced_this_pass == 0 {
            break;
        }
    }

    report
}

// -----------------------------------------------------------------------------
// Metrics
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapStats {
    pub window_len: usize,
    pub samples: usize,
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub frac_positive: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntropyStats {
    pub window_len: usize,
    pub samples: usize,
    pub mean_shannon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectoringStats {
    pub seeds: usize,
    pub bucket_count: usize,
    pub avg_pairwise_l1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRewriteReport {
    pub base_len: usize,
    pub after_inflation_len: usize,
    pub after_kneading_len: usize,
    pub inflation: StageStats,
    pub kneading: StageStats,
    pub attack: AttackReport,
    pub gap_curve: Vec<GapStats>,
    pub entropy: Option<EntropyStats>,
    pub sectoring: Option<SectoringStats>,
    pub oracle: OracleStats,
}

impl LocalRewriteReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

fn compute_gap_curve<R: Rng>(
    circuit: &mut CircuitSeq,
    cfg: &MetricsConfig,
    oracle: &mut PermTableOracle,
    num_wires: usize,
    rng: &mut R,
) -> Vec<GapStats> {
    let mut results = Vec::new();

    for &len in &cfg.gap_window_sizes {
        let mut gaps: Vec<f64> = Vec::new();
        let mut positives = 0usize;

        for _ in 0..cfg.samples_per_size {
            let window = match sample_window(circuit, num_wires, len, cfg.max_active_wires, rng) {
                Some(w) => w,
                None => continue,
            };
            let active = window.used_wires.len();
            let shortest = oracle.get_shortest_leq(&window.rewired, active, len, 1);
            if let Some(repl) = shortest {
                let gap = window.rewired.gates.len() as f64 - repl.gates.len() as f64;
                gaps.push(gap);
                if gap > 0.0 {
                    positives += 1;
                }
            }
        }

        if gaps.is_empty() {
            results.push(GapStats {
                window_len: len,
                samples: 0,
                mean: 0.0,
                median: 0.0,
                p95: 0.0,
                frac_positive: 0.0,
            });
            continue;
        }

        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let sum: f64 = gaps.iter().sum();
        let mean = sum / gaps.len() as f64;
        let median = gaps[gaps.len() / 2];
        let p95 = gaps[(((gaps.len() as f64) * 0.95).floor() as usize).min(gaps.len() - 1)];
        let frac_positive = positives as f64 / gaps.len() as f64;

        results.push(GapStats {
            window_len: len,
            samples: gaps.len(),
            mean,
            median,
            p95,
            frac_positive,
        });
    }

    results
}

fn compute_entropy<R: Rng>(
    circuit: &mut CircuitSeq,
    cfg: &MetricsConfig,
    oracle: &mut PermTableOracle,
    num_wires: usize,
    rng: &mut R,
) -> Option<EntropyStats> {
    let mut values = Vec::new();
    for _ in 0..cfg.entropy_samples {
        let window = match sample_window(
            circuit,
            num_wires,
            cfg.entropy_window_size,
            cfg.max_active_wires,
            rng,
        ) {
            Some(w) => w,
            None => continue,
        };
        let active = window.used_wires.len();
        let count = match oracle.count_alternatives(
            &window.rewired,
            active,
            cfg.entropy_window_size,
            cfg.entropy_max_count,
        ) {
            Some(c) => c,
            None => continue,
        };
        let s_hat = (count as f64 + 1.0).log2();
        values.push(s_hat);
    }

    if values.is_empty() {
        return None;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Some(EntropyStats {
        window_len: cfg.entropy_window_size,
        samples: values.len(),
        mean_shannon: mean,
    })
}

fn compute_sectoring<R: Rng>(
    base: &CircuitSeq,
    cfg: &MetricsConfig,
    num_wires: usize,
    oracle: &mut PermTableOracle,
    base_cfg: &LocalRewriteConfig,
    rng: &mut R,
) -> Option<SectoringStats> {
    if cfg.sector_seed_count < 2 {
        return None;
    }

    let bucket_count = 256usize;
    let mut sketches = Vec::new();

    for i in 0..cfg.sector_seed_count {
        let mut circuit = base.clone();
        let mut local_rng = StdRng::seed_from_u64(base_cfg.seed.wrapping_add(i as u64 + 1));

        let _ = run_inflation(&mut circuit, &base_cfg.inflation, oracle, num_wires, &mut local_rng);
        let _ = run_kneading(&mut circuit, &base_cfg.kneading, oracle, num_wires, &mut local_rng);

        let mut sketch = vec![0u32; bucket_count];
        for _ in 0..cfg.sector_sample_windows {
            if let Some(window) = sample_window(
                &mut circuit,
                num_wires,
                cfg.sector_window_len,
                cfg.max_active_wires,
                rng,
            ) {
                let key = canonical_window_key(&window.rewired.gates);
                let bucket = (key % bucket_count as u64) as usize;
                sketch[bucket] += 1;
            }
        }
        sketches.push(sketch);
    }

    if sketches.len() < 2 {
        return None;
    }

    let mut total = 0.0;
    let mut pairs = 0.0;
    for i in 0..sketches.len() {
        for j in (i + 1)..sketches.len() {
            let mut dist = 0u64;
            for b in 0..bucket_count {
                let a = sketches[i][b] as i64;
                let c = sketches[j][b] as i64;
                dist += (a - c).abs() as u64;
            }
            total += dist as f64;
            pairs += 1.0;
        }
    }

    Some(SectoringStats {
        seeds: sketches.len(),
        bucket_count,
        avg_pairwise_l1: if pairs > 0.0 { total / pairs } else { 0.0 },
    })
}

// -----------------------------------------------------------------------------
// Main entry
// -----------------------------------------------------------------------------

pub fn run_local_rewrite(
    cfg: &LocalRewriteConfig,
    oracle: &mut PermTableOracle,
) -> (CircuitSeq, LocalRewriteReport) {
    let seed = if cfg.seed == 0 { rand::random::<u64>() } else { cfg.seed };
    let mut rng = StdRng::seed_from_u64(seed);

    let base = random_circuit(cfg.num_wires as u8, cfg.base_depth);
    let base = base.concat(&base.inverse());

    let base_len = base.gates.len();
    let mut circuit = base.clone();

    let inflation_stats = run_inflation(&mut circuit, &cfg.inflation, oracle, cfg.num_wires, &mut rng);
    let after_inflation_len = circuit.gates.len();

    let kneading_stats = run_kneading(&mut circuit, &cfg.kneading, oracle, cfg.num_wires, &mut rng);
    let after_kneading_len = circuit.gates.len();

    let mut attack_circuit = circuit.clone();
    let attack_report = attack_peel(&mut attack_circuit, &cfg.attack, oracle, cfg.num_wires, &mut rng);

    let gap_curve = compute_gap_curve(&mut circuit, &cfg.metrics, oracle, cfg.num_wires, &mut rng);
    let entropy = compute_entropy(&mut circuit, &cfg.metrics, oracle, cfg.num_wires, &mut rng);
    let stats_snapshot = oracle.stats.clone();
    let sectoring = compute_sectoring(&base, &cfg.metrics, cfg.num_wires, oracle, cfg, &mut rng);
    oracle.stats = stats_snapshot;

    let report = LocalRewriteReport {
        base_len,
        after_inflation_len,
        after_kneading_len,
        inflation: inflation_stats,
        kneading: kneading_stats,
        attack: attack_report,
        gap_curve,
        entropy,
        sectoring,
        oracle: oracle.stats.clone(),
    };

    (circuit, report)
}
