use crate::infra::circuit::circuit::CircuitSeq;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct SatRequest {
    num_inputs: usize,
    output_truth_tables: Vec<String>,
    current_num_gates: usize,
    time_limit: usize,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SatResponse {
    success: bool,
    gates: Option<Vec<Vec<usize>>>,
    error: Option<String>,
}

/// Compute the truth table hash for a subcircuit (for LMDB lookup).
/// Returns a 32-byte hash of the circuit's truth table.
pub fn compute_canonical_hash(subcircuit: &CircuitSeq, num_wires: usize) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let n = num_wires;
    let rows = 1 << n;

    // Compute truth table
    let mut truth_table = Vec::with_capacity(rows * n);

    for r in 0..rows {
        let mut state = r;

        for gate in &subcircuit.gates {
            let target = gate[0] as usize;
            let c1 = gate[1] as usize;
            let c2 = gate[2] as usize;

            let val_c1 = (state >> c1) & 1;
            let val_c2 = (state >> c2) & 1;
            let activation = val_c1 | ((!val_c2) & 1);

            if activation == 1 {
                state ^= 1 << target;
            }
        }

        // Store output bits
        for i in 0..n {
            truth_table.push(((state >> i) & 1) as u8);
        }
    }

    // Hash the truth table
    let mut hasher = DefaultHasher::new();
    truth_table.hash(&mut hasher);
    let hash1 = hasher.finish();

    // Create a second hash for more bits
    let mut hasher2 = DefaultHasher::new();
    hash1.hash(&mut hasher2);
    truth_table.len().hash(&mut hasher2);
    let hash2 = hasher2.finish();

    // Combine into 32 bytes (simplified - not cryptographic)
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&hash1.to_le_bytes());
    result[8..16].copy_from_slice(&hash2.to_le_bytes());
    result[16..24].copy_from_slice(&hash1.to_be_bytes());
    result[24..32].copy_from_slice(&hash2.to_be_bytes());

    result
}

/// Parallel SAT compression: processes multiple subcircuits concurrently.
/// Returns a list of (original_index, optimized_circuit) pairs.
pub fn compress_sat_run_batch(
    subcircuits: Vec<(usize, CircuitSeq, usize)>, // (start_idx, circuit, num_wires)
    timeout_secs: u64,
) -> Vec<(usize, CircuitSeq)> {
    subcircuits
        .into_par_iter()
        .filter_map(|(idx, subcircuit, num_wires)| {
            let result = compress_sat_run(&subcircuit, num_wires, timeout_secs)?;
            if result.gates.len() < subcircuit.gates.len() {
                Some((idx, result))
            } else {
                None
            }
        })
        .collect()
}

pub fn compress_sat_run(
    subcircuit: &CircuitSeq,
    num_wires: usize,
    timeout_secs: u64,
) -> Option<CircuitSeq> {
    // 1. Generate truth tables for the subcircuit (using ORNB logic)
    let n = num_wires;
    let rows = 1 << n;
    let mut outputs = vec![String::with_capacity(rows); n];

    for r in 0..rows {
        // Initial state is the row index (inputs x0..xn)
        let mut state = r;

        // Apply circuit
        for gate in &subcircuit.gates {
            // Gate is [target, c1, c2]
            // Logic: target ^= (c1 | !c2)
            let target = gate[0] as usize;
            let c1 = gate[1] as usize;
            let c2 = gate[2] as usize;

            let val_c1 = (state >> c1) & 1;
            let val_c2 = (state >> c2) & 1;

            // ORNB: c1 OR (NOT c2)
            // (!val_c2 & 1) handles the NOT for a single bit
            let activation = val_c1 | ((!val_c2) & 1);

            if activation == 1 {
                state ^= 1 << target;
            }
        }

        // Record output state bits
        for i in 0..n {
            let bit = (state >> i) & 1;
            outputs[i].push(if bit == 1 { '1' } else { '0' });
        }
    }

    // 2. Prepare JSON request
    let request = SatRequest {
        num_inputs: n,
        output_truth_tables: outputs,
        current_num_gates: subcircuit.gates.len(),
        time_limit: timeout_secs as usize,
    };

    let json_req = serde_json::to_string(&request).unwrap();

    // 3. Call Python script
    // We assume the script is at "../sat_revsynth/scripts/synthesize_from_tt.py" relative to current dir
    // This works if running from research-group/local_mixing/
    let mut child = std::process::Command::new("python3")
        .arg("../sat_revsynth/scripts/synthesize_from_tt.py")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    {
        let stdin = child.stdin.as_mut()?;
        std::io::Write::write_all(stdin, json_req.as_bytes()).ok()?;
    }

    // Wait for output
    // Note: Python script should flush stdout or just finish
    let output = child.wait_with_output().ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("SAT Script Failed: {}", stderr);
        return None;
    }

    let result: SatResponse = serde_json::from_slice(&output.stdout).ok()?;

    if result.success {
        if let Some(gates_data) = result.gates {
            // Convert Vec<Vec<usize>> to CircuitSeq (Vec<[u8; 3]>)
            let mut seq = CircuitSeq { gates: Vec::new() };
            for g in gates_data {
                if g.len() == 3 {
                    seq.gates.push([g[0] as u8, g[1] as u8, g[2] as u8]);
                } else if g.len() >= 3 {
                    // Should be [t, c1, c2]
                    seq.gates.push([g[0] as u8, g[1] as u8, g[2] as u8]);
                } else {
                    // Invalid gate format?
                    return None;
                }
            }
            return Some(seq);
        }
    }

    None
}
