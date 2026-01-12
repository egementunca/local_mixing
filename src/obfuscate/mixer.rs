//! Random mixer circuit generation for global conjugation.

use crate::infra::circuit::CircuitSeq;
use rand::Rng;
use std::collections::HashSet;

/// Generate a random mixer circuit for conjugation.
///
/// The mixer should:
/// - Touch many wires (high coverage)
/// - Not be easily reducible by local rules
/// - Have controllable depth and locality
pub fn random_mixer<R: Rng>(
    num_wires: usize,
    depth: usize,
    locality: MixerLocality,
    rng: &mut R,
) -> CircuitSeq {
    let mut gates = Vec::with_capacity(depth);
    let n = num_wires as u8;

    for i in 0..depth {
        let gate = match locality {
            MixerLocality::Local => generate_local_gate(n, i, rng),
            MixerLocality::Global => generate_global_gate(n, rng),
            MixerLocality::Mixed => {
                if rng.random_bool(0.5) {
                    generate_local_gate(n, i, rng)
                } else {
                    generate_global_gate(n, rng)
                }
            }
        };
        gates.push(gate);
    }

    CircuitSeq { gates }
}

/// Locality mode for mixer generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerLocality {
    /// Gates prefer nearby wires.
    Local,
    /// Gates can use any wires.
    Global,
    /// Mix of local and global gates.
    Mixed,
}

impl Default for MixerLocality {
    fn default() -> Self {
        MixerLocality::Mixed
    }
}

/// Generate a gate with nearby wires (local connectivity).
fn generate_local_gate<R: Rng>(n: u8, layer: usize, rng: &mut R) -> [u8; 3] {
    let base = (layer % (n as usize)) as u8;
    let t = base;
    let c1 = (base + 1) % n;
    let mut c2 = (base + 2) % n;

    // Add some randomness to avoid patterns
    if rng.random_bool(0.3) {
        c2 = rng.random_range(0..n);
        while c2 == t || c2 == c1 {
            c2 = rng.random_range(0..n);
        }
    }

    [t, c1, c2]
}

/// Generate a gate with random wires (global connectivity).
fn generate_global_gate<R: Rng>(n: u8, rng: &mut R) -> [u8; 3] {
    let t = rng.random_range(0..n);
    let mut c1 = rng.random_range(0..n);
    while c1 == t {
        c1 = rng.random_range(0..n);
    }
    let mut c2 = rng.random_range(0..n);
    while c2 == t || c2 == c1 {
        c2 = rng.random_range(0..n);
    }
    [t, c1, c2]
}

/// Compute coverage score: fraction of wires touched by the circuit.
pub fn coverage_score(circuit: &CircuitSeq, num_wires: usize) -> f32 {
    let mut touched: HashSet<u8> = HashSet::new();
    for gate in &circuit.gates {
        touched.insert(gate[0]);
        touched.insert(gate[1]);
        touched.insert(gate[2]);
    }
    touched.len() as f32 / num_wires as f32
}

/// Compute a rough reducibility estimate based on adjacent pair patterns.
/// Lower is better (less reducible).
pub fn reducibility_estimate(circuit: &CircuitSeq) -> f32 {
    if circuit.gates.len() < 2 {
        return 0.0;
    }

    let mut adjacent_pairs = 0;
    let mut near_duplicates = 0;

    for i in 0..circuit.gates.len() - 1 {
        let g1 = &circuit.gates[i];
        let g2 = &circuit.gates[i + 1];

        // Check for identical adjacent gates (immediate cancellation)
        if g1 == g2 {
            adjacent_pairs += 1;
        }

        // Check for gates with same target (potential reduction opportunity)
        if g1[0] == g2[0] {
            near_duplicates += 1;
        }
    }

    let pair_ratio = adjacent_pairs as f32 / (circuit.gates.len() - 1) as f32;
    let dup_ratio = near_duplicates as f32 / (circuit.gates.len() - 1) as f32;

    pair_ratio * 2.0 + dup_ratio * 0.5
}

/// Generate a high-quality mixer with good coverage and low reducibility.
pub fn generate_quality_mixer<R: Rng>(
    num_wires: usize,
    depth: usize,
    max_attempts: usize,
    rng: &mut R,
) -> CircuitSeq {
    let mut best_mixer = random_mixer(num_wires, depth, MixerLocality::Mixed, rng);
    let mut best_score =
        coverage_score(&best_mixer, num_wires) - reducibility_estimate(&best_mixer);

    for _ in 1..max_attempts {
        let candidate = random_mixer(num_wires, depth, MixerLocality::Mixed, rng);
        let score = coverage_score(&candidate, num_wires) - reducibility_estimate(&candidate);

        if score > best_score {
            best_score = score;
            best_mixer = candidate;
        }
    }

    best_mixer
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_coverage() {
        let mut rng = StdRng::seed_from_u64(42);
        let mixer = random_mixer(16, 50, MixerLocality::Global, &mut rng);
        let coverage = coverage_score(&mixer, 16);
        assert!(coverage > 0.8, "Coverage should be high: {}", coverage);
    }

    #[test]
    fn test_no_adjacent_pairs() {
        let mut rng = StdRng::seed_from_u64(42);
        let mixer = generate_quality_mixer(8, 20, 5, &mut rng);

        for i in 0..mixer.gates.len() - 1 {
            assert_ne!(
                mixer.gates[i],
                mixer.gates[i + 1],
                "Quality mixer should not have adjacent identical gates"
            );
        }
    }
}
