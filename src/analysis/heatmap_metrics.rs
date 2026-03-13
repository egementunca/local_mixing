use crate::infra::circuit::CircuitSeq;
use rand::Rng;

/// Gradient distance heatmap: measures the *change* in Hamming distance
/// between adjacent positions.
///
/// Dist(i,j) = HD(Ĉ_i(x), C_j(x)) - HD(Ĉ_{i-1}(x), C_{j-1}(x))
///
/// Values in [-1, 1]. Positive = diverging, negative = converging.
/// More informative than raw HD when states are close.
/// (Issue #44, Ran Canetti)
pub fn gradient_distance_heatmap(
    circuit_one: &CircuitSeq,
    circuit_two: &CircuitSeq,
    num_wires: usize,
    num_inputs: usize,
    canonicalize: bool,
) -> Vec<[f64; 3]> {
    let mut c1 = circuit_one.clone();
    let mut c2 = circuit_two.clone();
    if canonicalize {
        c1.canonicalize();
        c2.canonicalize();
    }
    let len1 = c1.gates.len();
    let len2 = c2.gates.len();

    // Output grid is (len1+1) x (len2+1), same as standard heatmap.
    // At (0,*) and (*,0) we report the raw HD as the "gradient from nothing".
    let grid_size = (len1 + 1) * (len2 + 1);
    let mut result = vec![[0f64; 3]; grid_size];
    for i in 0..=len1 {
        for j in 0..=len2 {
            let idx = i * (len2 + 1) + j;
            result[idx][0] = i as f64;
            result[idx][1] = j as f64;
        }
    }

    let mut rng = rand::rng();

    for _ in 0..num_inputs {
        let input: usize = if num_wires < usize::BITS as usize {
            rng.random_range(0..(1usize << num_wires))
        } else {
            rng.random_range(0..=usize::MAX)
        };

        let evo1 = c1.evaluate_evolution(input);
        let evo2 = c2.evaluate_evolution(input);

        for i in 0..=len1 {
            for j in 0..=len2 {
                let hd_curr = (evo1[i] ^ evo2[j]).count_ones() as f64;
                let hd_prev = if i > 0 && j > 0 {
                    (evo1[i - 1] ^ evo2[j - 1]).count_ones() as f64
                } else {
                    0.0
                };
                let gradient = (hd_curr - hd_prev) / num_wires as f64;
                let idx = i * (len2 + 1) + j;
                result[idx][2] += gradient / num_inputs as f64;
            }
        }
    }

    result
}

/// Hamming weight difference heatmap: measures |HW(Ĉ_i(x)) - HW(C_j(x))| / n.
///
/// Shuffles (wire permutations) preserve Hamming weight, so this metric
/// is 0 in regions where the only transformation is a shuffle.
/// Non-zero values indicate bit flips or non-linear gates.
/// (Issue #44, Ran Canetti: "shuffles alone, without bit flips, preserve
/// the Hamming weight of the respective states")
pub fn hamming_weight_heatmap(
    circuit_one: &CircuitSeq,
    circuit_two: &CircuitSeq,
    num_wires: usize,
    num_inputs: usize,
    canonicalize: bool,
) -> Vec<[f64; 3]> {
    let mut c1 = circuit_one.clone();
    let mut c2 = circuit_two.clone();
    if canonicalize {
        c1.canonicalize();
        c2.canonicalize();
    }
    let len1 = c1.gates.len();
    let len2 = c2.gates.len();

    let grid_size = (len1 + 1) * (len2 + 1);
    let mut result = vec![[0f64; 3]; grid_size];
    for i in 0..=len1 {
        for j in 0..=len2 {
            let idx = i * (len2 + 1) + j;
            result[idx][0] = i as f64;
            result[idx][1] = j as f64;
        }
    }

    let mut rng = rand::rng();

    for _ in 0..num_inputs {
        let input: usize = if num_wires < usize::BITS as usize {
            rng.random_range(0..(1usize << num_wires))
        } else {
            rng.random_range(0..=usize::MAX)
        };

        let evo1 = c1.evaluate_evolution(input);
        let evo2 = c2.evaluate_evolution(input);

        // Pre-compute Hamming weights for all positions
        let hw1: Vec<f64> = evo1.iter().map(|s| s.count_ones() as f64).collect();
        let hw2: Vec<f64> = evo2.iter().map(|s| s.count_ones() as f64).collect();

        for i in 0..=len1 {
            for j in 0..=len2 {
                let hw_diff = (hw1[i] - hw2[j]).abs() / num_wires as f64;
                let idx = i * (len2 + 1) + j;
                result[idx][2] += hw_diff / num_inputs as f64;
            }
        }
    }

    result
}

/// Windowed minimum heatmap: min_{i <= i' <= i+d} HD(Ĉ_{i'}(x), C_j(x)) / n.
///
/// Smooths out mixing that only has local effect but cancels itself out.
/// Different values of d reveal different scales of structure.
/// (Issue #44, Ran Canetti)
pub fn windowed_min_heatmap(
    circuit_one: &CircuitSeq,
    circuit_two: &CircuitSeq,
    num_wires: usize,
    num_inputs: usize,
    window_d: usize,
    canonicalize: bool,
) -> Vec<[f64; 3]> {
    let mut c1 = circuit_one.clone();
    let mut c2 = circuit_two.clone();
    if canonicalize {
        c1.canonicalize();
        c2.canonicalize();
    }
    let len1 = c1.gates.len();
    let len2 = c2.gates.len();

    let grid_size = (len1 + 1) * (len2 + 1);
    let mut result = vec![[0f64; 3]; grid_size];
    for i in 0..=len1 {
        for j in 0..=len2 {
            let idx = i * (len2 + 1) + j;
            result[idx][0] = i as f64;
            result[idx][1] = j as f64;
        }
    }

    let mut rng = rand::rng();

    for _ in 0..num_inputs {
        let input: usize = if num_wires < usize::BITS as usize {
            rng.random_range(0..(1usize << num_wires))
        } else {
            rng.random_range(0..=usize::MAX)
        };

        let evo1 = c1.evaluate_evolution(input);
        let evo2 = c2.evaluate_evolution(input);

        for i in 0..=len1 {
            for j in 0..=len2 {
                // Find minimum HD in window [i, min(i+d, len1)]
                let window_end = (i + window_d).min(len1);
                let mut min_hd = f64::MAX;
                for ip in i..=window_end {
                    let hd = (evo1[ip] ^ evo2[j]).count_ones() as f64
                        / num_wires as f64;
                    if hd < min_hd {
                        min_hd = hd;
                    }
                }
                let idx = i * (len2 + 1) + j;
                result[idx][2] += min_hd / num_inputs as f64;
            }
        }
    }

    result
}

/// Correlated inputs heatmap: fix n-k wires to a random base, vary only k wires.
///
/// Tests whether the obfuscated circuit has sub-circuits whose functionality
/// is similar to sub-circuits of the original.
/// (Issue #44, Ran Canetti)
pub fn correlated_inputs_heatmap(
    circuit_one: &CircuitSeq,
    circuit_two: &CircuitSeq,
    num_wires: usize,
    k: usize,
    num_bases: usize,
    canonicalize: bool,
) -> Vec<[f64; 3]> {
    let mut c1 = circuit_one.clone();
    let mut c2 = circuit_two.clone();
    if canonicalize {
        c1.canonicalize();
        c2.canonicalize();
    }
    let len1 = c1.gates.len();
    let len2 = c2.gates.len();

    assert!(k <= num_wires, "k must be <= num_wires");
    let num_varied = 1usize << k; // 2^k inputs per base
    let total_inputs = num_bases * num_varied;

    let grid_size = (len1 + 1) * (len2 + 1);
    let mut result = vec![[0f64; 3]; grid_size];
    for i in 0..=len1 {
        for j in 0..=len2 {
            let idx = i * (len2 + 1) + j;
            result[idx][0] = i as f64;
            result[idx][1] = j as f64;
        }
    }

    let mut rng = rand::rng();

    for _ in 0..num_bases {
        // Random base input
        let base: usize = if num_wires < usize::BITS as usize {
            rng.random_range(0..(1usize << num_wires))
        } else {
            rng.random_range(0..=usize::MAX)
        };

        // Choose k random wire positions to vary
        let mut wire_positions: Vec<usize> = (0..num_wires).collect();
        // Fisher-Yates shuffle to pick k positions
        for idx in 0..k {
            let swap_idx = rng.random_range(idx..num_wires);
            wire_positions.swap(idx, swap_idx);
        }
        let free_wires = &wire_positions[..k];

        // Create mask for fixed wires: base with free wire bits cleared
        let mut free_mask: usize = 0;
        for &w in free_wires {
            free_mask |= 1 << w;
        }
        let fixed_bits = base & !free_mask;

        // Iterate over all 2^k combinations of free wires
        for combo in 0..num_varied {
            let mut input = fixed_bits;
            for (bit_idx, &wire_pos) in free_wires.iter().enumerate() {
                if (combo >> bit_idx) & 1 == 1 {
                    input |= 1 << wire_pos;
                }
            }

            let evo1 = c1.evaluate_evolution(input);
            let evo2 = c2.evaluate_evolution(input);

            for i in 0..=len1 {
                for j in 0..=len2 {
                    let diff = evo1[i] ^ evo2[j];
                    let hd = diff.count_ones() as f64 / num_wires as f64;
                    let idx = i * (len2 + 1) + j;
                    result[idx][2] += hd / total_inputs as f64;
                }
            }
        }
    }

    result
}
