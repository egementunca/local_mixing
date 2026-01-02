//! SAT-based compression for subcircuits.
//!
//! Alternative to `compress_lmdb` using SAT solver for ORNB-only circuits.

use super::{OptimizeRequest, SubCircuit, call_python_optimizer, compute_truth_tables};
use crate::circuit::CircuitSeq;
use std::collections::HashSet;

/// Compress a subcircuit using the SAT-based optimizer.
///
/// This is an alternative to `compress_lmdb` that uses the Python SAT solver
/// to find smaller equivalent circuits using only ORNB gates.
///
/// # Arguments
/// * `subcircuit` - The subcircuit to optimize
/// * `num_wires` - Number of wires in the subcircuit
/// * `time_limit` - SAT solver time limit in seconds
/// * `project_root` - Path to the project root (containing optimization/)
///
/// # Returns
/// Optimized circuit if found, original circuit otherwise.
pub fn compress_sat(
    subcircuit: &CircuitSeq,
    num_wires: usize,
    time_limit: u32,
    project_root: &str,
) -> CircuitSeq {
    if subcircuit.gates.is_empty() {
        return subcircuit.clone();
    }

    // Build a SubCircuit structure for truth table computation
    let sub = build_subcircuit(subcircuit, num_wires);

    // Compute truth tables
    let tables = compute_truth_tables(&sub);

    if tables.is_empty() {
        return subcircuit.clone();
    }

    // Call Python optimizer
    let request = OptimizeRequest {
        num_inputs: sub.num_inputs(),
        output_truth_tables: tables,
        current_num_gates: subcircuit.gates.len(),
        time_limit,
    };

    match call_python_optimizer(&request, None, project_root) {
        Ok(response) if response.success => {
            if let Some(gates) = response.gates {
                // Convert gates back to CircuitSeq with original wire indices
                let optimized = convert_gates_to_circuit(&gates, &sub);
                if optimized.gates.len() < subcircuit.gates.len() {
                    return optimized;
                }
            }
            subcircuit.clone()
        }
        _ => subcircuit.clone(),
    }
}

/// Build a SubCircuit from a CircuitSeq for truth table computation.
fn build_subcircuit(circuit: &CircuitSeq, _num_wires: usize) -> SubCircuit {
    let mut all_wires: HashSet<u8> = HashSet::new();
    for gate in &circuit.gates {
        all_wires.insert(gate[0]);
        all_wires.insert(gate[1]);
        all_wires.insert(gate[2]);
    }

    let mut sorted_wires: Vec<u8> = all_wires.into_iter().collect();
    sorted_wires.sort();

    let max_wire = *sorted_wires.last().unwrap_or(&0) as usize;
    let mut wire_remap = vec![0u8; max_wire + 1];
    for (local_idx, &orig_wire) in sorted_wires.iter().enumerate() {
        wire_remap[orig_wire as usize] = local_idx as u8;
    }

    // Determine input/output wires
    let mut written: HashSet<u8> = HashSet::new();
    let mut input_wires: Vec<u8> = Vec::new();

    for gate in &circuit.gates {
        if !written.contains(&gate[1]) && !input_wires.contains(&gate[1]) {
            input_wires.push(gate[1]);
        }
        if !written.contains(&gate[2]) && !input_wires.contains(&gate[2]) {
            input_wires.push(gate[2]);
        }
        if !written.contains(&gate[0]) && !input_wires.contains(&gate[0]) {
            input_wires.push(gate[0]);
        }
        written.insert(gate[0]);
    }

    input_wires.sort();
    let mut output_wires: Vec<u8> = written.into_iter().collect();
    output_wires.sort();

    SubCircuit {
        gates: circuit.gates.clone(),
        input_wires,
        output_wires,
        original_indices: (0..circuit.gates.len()).collect(),
        wire_remap,
        wire_unmap: sorted_wires,
    }
}

/// Convert SAT optimizer output gates back to CircuitSeq.
fn convert_gates_to_circuit(gates: &[[u8; 3]], sub: &SubCircuit) -> CircuitSeq {
    // Gates from SAT optimizer use local wire indices, convert back
    let converted: Vec<[u8; 3]> = gates
        .iter()
        .map(|g| {
            [
                sub.wire_unmap.get(g[0] as usize).copied().unwrap_or(g[0]),
                sub.wire_unmap.get(g[1] as usize).copied().unwrap_or(g[1]),
                sub.wire_unmap.get(g[2] as usize).copied().unwrap_or(g[2]),
            ]
        })
        .collect();

    CircuitSeq { gates: converted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_subcircuit() {
        let circuit = CircuitSeq {
            gates: vec![[0, 1, 2], [1, 0, 2]],
        };

        let sub = build_subcircuit(&circuit, 3);

        assert_eq!(sub.gates.len(), 2);
        assert!(sub.input_wires.len() > 0);
        assert!(sub.output_wires.len() > 0);
    }
}
