//! Subcircuit extraction from larger circuits.
//!
//! Provides functionality to extract a subset of gates as a subcircuit,
//! identifying input and output wires.

use crate::circuit::CircuitSeq;
use std::collections::HashSet;

/// A subcircuit extracted from a larger circuit.
#[derive(Debug, Clone)]
pub struct SubCircuit {
    /// The gates in this subcircuit (in order).
    pub gates: Vec<[u8; 3]>,

    /// Wires that are inputs to this subcircuit.
    /// These are wires that are read before being written.
    pub input_wires: Vec<u8>,

    /// Wires that are outputs of this subcircuit.
    /// These are wires whose final values are used after the subcircuit.
    pub output_wires: Vec<u8>,

    /// Original gate indices in the parent circuit.
    pub original_indices: Vec<usize>,

    /// Mapping from original wire indices to local indices (0..n).
    pub wire_remap: Vec<u8>,

    /// Inverse mapping from local indices to original wire indices.
    pub wire_unmap: Vec<u8>,
}

impl SubCircuit {
    /// Number of gates in the subcircuit.
    pub fn num_gates(&self) -> usize {
        self.gates.len()
    }

    /// Number of input wires.
    pub fn num_inputs(&self) -> usize {
        self.input_wires.len()
    }

    /// Number of output wires.
    pub fn num_outputs(&self) -> usize {
        self.output_wires.len()
    }

    /// Get the gates with local wire indices.
    pub fn local_gates(&self) -> Vec<[u8; 3]> {
        self.gates
            .iter()
            .map(|g| {
                [
                    self.wire_remap[g[0] as usize],
                    self.wire_remap[g[1] as usize],
                    self.wire_remap[g[2] as usize],
                ]
            })
            .collect()
    }
}

/// Extract a window of consecutive gates as a subcircuit.
///
/// # Arguments
/// * `circuit` - The full circuit
/// * `start_gate` - Index of first gate to include
/// * `num_gates` - Number of gates to include
///
/// # Returns
/// A SubCircuit with the specified gates.
pub fn extract_subcircuit(circuit: &CircuitSeq, start_gate: usize, num_gates: usize) -> SubCircuit {
    let end_gate = (start_gate + num_gates).min(circuit.gates.len());
    let gate_indices: Vec<usize> = (start_gate..end_gate).collect();

    extract_by_indices(circuit, &gate_indices)
}

/// Extract gates by their indices.
///
/// # Arguments
/// * `circuit` - The full circuit
/// * `gate_indices` - Indices of gates to extract
///
/// # Returns
/// A SubCircuit containing the specified gates.
pub fn extract_by_indices(circuit: &CircuitSeq, gate_indices: &[usize]) -> SubCircuit {
    // Collect all wires touched by these gates
    let mut all_wires: HashSet<u8> = HashSet::new();
    for &idx in gate_indices {
        let gate = circuit.gates[idx];
        all_wires.insert(gate[0]);
        all_wires.insert(gate[1]);
        all_wires.insert(gate[2]);
    }

    // Create wire remapping (original -> local)
    let mut sorted_wires: Vec<u8> = all_wires.into_iter().collect();
    sorted_wires.sort();

    let max_wire = *sorted_wires.last().unwrap_or(&0) as usize;
    let mut wire_remap = vec![0u8; max_wire + 1];
    for (local_idx, &orig_wire) in sorted_wires.iter().enumerate() {
        wire_remap[orig_wire as usize] = local_idx as u8;
    }

    // Collect gates
    let gates: Vec<[u8; 3]> = gate_indices.iter().map(|&idx| circuit.gates[idx]).collect();

    // Determine input wires (read before written in subcircuit)
    // and output wires (written in subcircuit)
    let mut written: HashSet<u8> = HashSet::new();
    let mut input_wires: Vec<u8> = Vec::new();

    for gate in &gates {
        // Control wires are read
        if !written.contains(&gate[1]) && !input_wires.contains(&gate[1]) {
            input_wires.push(gate[1]);
        }
        if !written.contains(&gate[2]) && !input_wires.contains(&gate[2]) {
            input_wires.push(gate[2]);
        }
        // Target wire is read then written
        if !written.contains(&gate[0]) && !input_wires.contains(&gate[0]) {
            input_wires.push(gate[0]);
        }
        written.insert(gate[0]);
    }

    input_wires.sort();

    // Output wires are all wires that have been written
    // (In a reversible circuit, all wires that are targets become outputs)
    let mut output_wires: Vec<u8> = written.into_iter().collect();
    output_wires.sort();

    SubCircuit {
        gates,
        input_wires,
        output_wires,
        original_indices: gate_indices.to_vec(),
        wire_remap,
        wire_unmap: sorted_wires,
    }
}

/// Extract gates that operate on a specific set of wires.
///
/// # Arguments
/// * `circuit` - The full circuit
/// * `wire_set` - Set of wire indices to include
/// * `start_gate` - Start searching from this gate index
/// * `max_gates` - Maximum number of gates to include
///
/// # Returns
/// A SubCircuit containing gates that touch the specified wires.
pub fn extract_by_wire_window(
    circuit: &CircuitSeq,
    wire_set: &HashSet<u8>,
    start_gate: usize,
    max_gates: usize,
) -> SubCircuit {
    let mut gate_indices = Vec::new();

    for (idx, gate) in circuit.gates.iter().enumerate().skip(start_gate) {
        if gate_indices.len() >= max_gates {
            break;
        }

        // Check if this gate touches any of our wires
        if wire_set.contains(&gate[0]) || wire_set.contains(&gate[1]) || wire_set.contains(&gate[2])
        {
            gate_indices.push(idx);
        }
    }

    extract_by_indices(circuit, &gate_indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_subcircuit() {
        // Create a simple circuit: 3 gates on 4 wires
        let circuit = CircuitSeq {
            gates: vec![
                [0, 1, 2], // gate 0
                [1, 2, 3], // gate 1
                [2, 0, 1], // gate 2
                [3, 1, 0], // gate 3
            ],
        };

        // Extract gates 1 and 2
        let sub = extract_subcircuit(&circuit, 1, 2);

        assert_eq!(sub.num_gates(), 2);
        assert_eq!(sub.gates[0], [1, 2, 3]);
        assert_eq!(sub.gates[1], [2, 0, 1]);
        assert_eq!(sub.original_indices, vec![1, 2]);
    }

    #[test]
    fn test_input_output_wires() {
        // Circuit where wire 0 is read before written
        let circuit = CircuitSeq {
            gates: vec![
                [1, 0, 2], // reads 0, 2; writes 1
                [2, 1, 0], // reads 1, 0; writes 2
            ],
        };

        let sub = extract_subcircuit(&circuit, 0, 2);

        // Wire 0 is read first (as control), so it's an input
        // Wire 2 is read first (as control), so it's an input
        // Wire 1 is written first, but read as target, so input
        assert!(sub.input_wires.contains(&0));
        assert!(sub.input_wires.contains(&2));

        // Wires 1 and 2 are written (targets)
        assert!(sub.output_wires.contains(&1));
        assert!(sub.output_wires.contains(&2));
    }
}
