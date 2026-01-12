use crate::infra::circuit::circuit::{CircuitSeq, Gate};
use ndarray::Array2;
use rayon::prelude::*;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AlignmentResult {
    pub c_star: f64,
    pub path: Vec<(usize, usize)>,
    pub d_matrix: Array2<f64>,
    pub dp_cost: Array2<f64>,
    pub dp_len: Array2<usize>,
}

pub fn trace_states(circuit: &CircuitSeq, inputs: &[u64]) -> Vec<Vec<u64>> {
    let m = circuit.gates.len();
    let n_inputs = inputs.len();

    // states[i][k] = state after i gates for input k
    // states has size m+1
    let mut states = vec![Vec::with_capacity(n_inputs); m + 1];

    // Initial states (i=0)
    states[0] = inputs.to_vec();

    // Compute subsequent states
    for (i, gate) in circuit.gates.iter().enumerate() {
        // Reuse the logic from Gate struct, casting appropriately.
        // On 64-bit systems, usize is u64.
        // Gate::evaluate_index takes (usize, [u8;3]) -> usize

        let prev = &states[i];
        let next: Vec<u64> = prev
            .par_iter() // Parallelize over inputs for speed
            .map(|&s| Gate::evaluate_index(s as usize, *gate) as u64)
            .collect();
        states[i + 1] = next;
    }

    states
}

pub fn distance_matrix(
    trace_a: &[Vec<u64>],
    trace_b: &[Vec<u64>],
    num_wires: usize,
) -> Array2<f64> {
    assert!(
        num_wires <= 64,
        "Alignment scoring only supports up to 64 wires"
    );
    let ma = trace_a.len();
    let mb = trace_b.len();
    let n_inputs = trace_a[0].len();

    // We want to compute D[i,j] for all i,j.
    // Parallelize the outer loop (i) to use multiple threads.
    let d_vec: Vec<f64> = (0..ma)
        .into_par_iter()
        .flat_map(|i| {
            // For each row i, compute all columns j
            (0..mb).into_par_iter().map(move |j| {
                let state_a = &trace_a[i];
                let state_b = &trace_b[j];

                // Vectorized Hamming distance over inputs
                let mut sum_dist = 0.0;
                for k in 0..n_inputs {
                    let diff = state_a[k] ^ state_b[k];
                    let hd = diff.count_ones() as f64;
                    // Correlation logic from heatmap:
                    // dist_k = hd / num_wires
                    // corr_k = |1 - 2 * dist_k|  (1=perfect/inverse, 0=random)
                    // We want distance, so we use 1 - corr_k
                    let norm_hd = hd / num_wires as f64;
                    // Simplify to just normalized Hamming distance as requested by user
                    // 0.0 = Identical
                    // 0.5 = Random
                    // 1.0 = Inverse
                    let corr = norm_hd;
                    sum_dist += corr;
                }
                // Average over inputs
                sum_dist / n_inputs as f64
            })
        })
        .collect();

    // Convert flat vector to Array2 (row-major)
    Array2::from_shape_vec((ma, mb), d_vec).expect("Shape mismatch in distance matrix")
}

pub fn dtw_alignment(d: &Array2<f64>, pen_hv: f64, pen_diag: f64) -> AlignmentResult {
    let (ma, mb) = d.dim();

    // dp_cost[i,j] stores min cumulative cost
    let mut dp_cost = Array2::<f64>::from_elem((ma, mb), f64::INFINITY);
    // dp_len[i,j] stores length of that optimal path
    let mut dp_len = Array2::<usize>::zeros((ma, mb));
    // Backpointers: 0=Diag, 1=Vert, 2=Horiz
    let mut backptr = Array2::<u8>::zeros((ma, mb));

    // Base case
    dp_cost[[0, 0]] = d[[0, 0]];
    dp_len[[0, 0]] = 1;
    // backptr[[0, 0]] = 0; // arbitrary, won't be used

    // Initialize first row (can only come from Horiz/Left)
    for j in 1..mb {
        dp_cost[[0, j]] = dp_cost[[0, j - 1]] + pen_hv + d[[0, j]];
        dp_len[[0, j]] = dp_len[[0, j - 1]] + 1;
        backptr[[0, j]] = 2; // Horizontal
    }

    // Initialize first column (can only come from Vert/Up)
    for i in 1..ma {
        dp_cost[[i, 0]] = dp_cost[[i - 1, 0]] + pen_hv + d[[i, 0]];
        dp_len[[i, 0]] = dp_len[[i - 1, 0]] + 1;
        backptr[[i, 0]] = 1; // Vertical
    }

    // Fill DP table
    for i in 1..ma {
        for j in 1..mb {
            // Calculate candidate costs purely based on previous cumulative cost + step penalty
            // D[i,j] is added afterwards
            let cost_diag = dp_cost[[i - 1, j - 1]] + pen_diag;
            let cost_vert = dp_cost[[i - 1, j]] + pen_hv;
            let cost_horiz = dp_cost[[i, j - 1]] + pen_hv;

            // Tie-breaking preference: Diag > Vert > Horiz
            let (best_prev_cost, dir) = if cost_diag <= cost_vert && cost_diag <= cost_horiz {
                (cost_diag, 0)
            } else if cost_vert <= cost_horiz {
                (cost_vert, 1)
            } else {
                (cost_horiz, 2)
            };

            dp_cost[[i, j]] = best_prev_cost + d[[i, j]];

            match dir {
                0 => dp_len[[i, j]] = dp_len[[i - 1, j - 1]] + 1,
                1 => dp_len[[i, j]] = dp_len[[i - 1, j]] + 1,
                2 => dp_len[[i, j]] = dp_len[[i, j - 1]] + 1,
                _ => unreachable!(),
            }
            backptr[[i, j]] = dir;
        }
    }

    // Backtrack to reconstruct path
    let mut path = Vec::new();
    let (mut i, mut j) = (ma - 1, mb - 1);
    path.push((i, j));

    while i > 0 || j > 0 {
        match backptr[[i, j]] {
            0 => {
                // Diag
                i -= 1;
                j -= 1;
            }
            1 => {
                // Vert
                i -= 1;
            }
            2 => {
                // Horiz
                j -= 1;
            }
            _ => unreachable!(),
        }
        path.push((i, j));
    }
    path.reverse();

    let c_star = dp_cost[[ma - 1, mb - 1]] / dp_len[[ma - 1, mb - 1]] as f64;

    AlignmentResult {
        c_star,
        path,
        d_matrix: d.clone(),
        dp_cost,
        dp_len,
    }
}
