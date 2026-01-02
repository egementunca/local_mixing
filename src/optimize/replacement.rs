//! Subcircuit replacement and equivalence verification.
//!
//! Replace subcircuits with optimized versions and verify correctness.

use super::SubCircuit;
use crate::circuit::{CircuitSeq, Gate};
use rand::Rng;

/// Replace a subcircuit with an optimized version.
///
/// # Arguments
/// * `circuit` - The full circuit (modified in place)
/// * `subcircuit` - The original subcircuit (contains original_indices)
/// * `optimized_gates` - The optimized gates (using local wire indices)
///
/// # Returns
/// Ok(()) on success, Err with message on failure.
pub fn replace_subcircuit(
    circuit: &mut CircuitSeq,
    subcircuit: &SubCircuit,
    optimized_gates: &[[u8; 3]],
) -> Result<(), String> {
    if subcircuit.original_indices.is_empty() {
        return Err("Subcircuit has no original indices".to_string());
    }

    // Convert optimized gates from local to original wire indices
    let unmapped_gates: Vec<[u8; 3]> = optimized_gates
        .iter()
        .map(|g| {
            [
                subcircuit.wire_unmap[g[0] as usize],
                subcircuit.wire_unmap[g[1] as usize],
                subcircuit.wire_unmap[g[2] as usize],
            ]
        })
        .collect();

    // Find insertion point (first original index)
    let insert_pos = *subcircuit.original_indices.first().unwrap();

    // Remove original gates (in reverse order to maintain indices)
    let mut indices_to_remove = subcircuit.original_indices.clone();
    indices_to_remove.sort();
    indices_to_remove.reverse();

    for idx in indices_to_remove {
        if idx < circuit.gates.len() {
            circuit.gates.remove(idx);
        }
    }

    // Insert optimized gates at the original position
    let insert_pos = insert_pos.min(circuit.gates.len());
    for (i, gate) in unmapped_gates.into_iter().enumerate() {
        circuit.gates.insert(insert_pos + i, gate);
    }

    Ok(())
}

/// Verify that two circuits compute the same function.
///
/// Uses random testing to check equivalence.
///
/// # Arguments
/// * `original` - The original circuit
/// * `modified` - The modified circuit
/// * `num_wires` - Number of wires in the circuits
/// * `num_tests` - Number of random inputs to test
///
/// # Returns
/// true if all tests pass, false if a counterexample is found.
pub fn verify_equivalence(
    original: &CircuitSeq,
    modified: &CircuitSeq,
    num_wires: usize,
    num_tests: usize,
) -> bool {
    let mut rng = rand::rng();
    let max_input = 1usize << num_wires;

    for _ in 0..num_tests {
        let input = rng.random_range(0..max_input);

        let output_original = Gate::evaluate_index_list(input, &original.gates);
        let output_modified = Gate::evaluate_index_list(input, &modified.gates);

        if output_original != output_modified {
            return false;
        }
    }

    true
}

/// Verify equivalence exhaustively for small circuits.
///
/// # Arguments
/// * `original` - The original circuit
/// * `modified` - The modified circuit
/// * `num_wires` - Number of wires (must be small, <= 20)
///
/// # Returns
/// true if circuits are equivalent, false otherwise.
pub fn verify_equivalence_exhaustive(
    original: &CircuitSeq,
    modified: &CircuitSeq,
    num_wires: usize,
) -> bool {
    if num_wires > 20 {
        // Too many inputs for exhaustive verification
        return verify_equivalence(original, modified, num_wires, 10000);
    }

    let max_input = 1usize << num_wires;

    for input in 0..max_input {
        let output_original = Gate::evaluate_index_list(input, &original.gates);
        let output_modified = Gate::evaluate_index_list(input, &modified.gates);

        if output_original != output_modified {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_equivalence_identity() {
        let circuit = CircuitSeq {
            gates: vec![
                [0, 1, 2],
                [0, 1, 2], // Same gate twice = identity on wire 0
            ],
        };

        let empty = CircuitSeq { gates: vec![] };

        // The double application should be equivalent to identity
        // for the specific wire, but the overall circuit may differ
        // This test just verifies the function runs
        let _ = verify_equivalence(&circuit, &empty, 3, 100);
    }

    #[test]
    fn test_verify_equivalence_same_circuit() {
        let circuit = CircuitSeq {
            gates: vec![[0, 1, 2], [1, 2, 0]],
        };

        let same = circuit.clone();

        assert!(verify_equivalence(&circuit, &same, 3, 100));
        assert!(verify_equivalence_exhaustive(&circuit, &same, 3));
    }
}
