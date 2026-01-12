//! Identity gadget generators for obfuscation.
//!
//! All gadgets produce circuits that evaluate to identity.
//! Note: In this gate set, gates are self-inverse (G = G⁻¹).

use crate::infra::circuit::CircuitSeq;
use rand::Rng;

/// Generate a conjugate gadget: X · X⁻¹ = I
///
/// Since gates are self-inverse and inverse() reverses order,
/// X · inverse(X) = I for any circuit X.
///
/// Returns the full gadget and splits for boundary injection.
pub fn commutator<R: Rng>(
    num_wires: usize,
    x_depth: usize,
    _y_depth: usize, // ignored, kept for API compatibility
    rng: &mut R,
) -> CommutatorGadget {
    let x = random_circuit_touching(num_wires, x_depth, rng);
    let x_inv = x.inverse();

    // Full gadget: X · X⁻¹ = I
    let full = x.concat(&x_inv);

    // Split for boundary injection:
    // first_half = X (first half of circuit)
    // second_half = X⁻¹ (reversed X)
    let first_half = x;
    let second_half = x_inv;

    CommutatorGadget {
        full,
        first_half,
        second_half,
    }
}

/// A commutator gadget with split halves for boundary injection.
#[derive(Clone, Debug)]
pub struct CommutatorGadget {
    pub full: CircuitSeq,
    pub first_half: CircuitSeq,
    pub second_half: CircuitSeq,
}

/// Generate a hidden inverse gadget.
/// Uses same construction as commutator.
pub fn hidden_inverse<R: Rng>(
    num_wires: usize,
    g_depth: usize,
    h_depth: usize,
    rng: &mut R,
) -> CommutatorGadget {
    commutator(num_wires, g_depth, h_depth, rng)
}

/// Generate a simple identity gadget: G · G⁻¹ = I
pub fn simple_identity<R: Rng>(num_wires: usize, depth: usize, rng: &mut R) -> CircuitSeq {
    let g = random_circuit_touching(num_wires, depth, rng);
    let g_inv = g.inverse();
    g.concat(&g_inv)
}

/// Generate a random circuit that touches many wires.
fn random_circuit_touching<R: Rng>(num_wires: usize, depth: usize, rng: &mut R) -> CircuitSeq {
    let mut gates = Vec::with_capacity(depth);
    let n = num_wires as u8;

    for _ in 0..depth {
        // Generate random gate with distinct wires
        let t = rng.random_range(0..n);
        let mut c1 = rng.random_range(0..n);
        while c1 == t {
            c1 = rng.random_range(0..n);
        }
        let mut c2 = rng.random_range(0..n);
        while c2 == t || c2 == c1 {
            c2 = rng.random_range(0..n);
        }
        gates.push([t, c1, c2]);
    }

    CircuitSeq { gates }
}

/// Interleave two circuits gate-by-gate (for harder-to-detect gadgets).
pub fn interleave(a: &CircuitSeq, b: &CircuitSeq) -> CircuitSeq {
    let mut gates = Vec::with_capacity(a.len() + b.len());
    let mut ai = a.gates.iter();
    let mut bi = b.gates.iter();

    loop {
        match (ai.next(), bi.next()) {
            (Some(&ga), Some(&gb)) => {
                gates.push(ga);
                gates.push(gb);
            }
            (Some(&ga), None) => gates.push(ga),
            (None, Some(&gb)) => gates.push(gb),
            (None, None) => break,
        }
    }

    CircuitSeq { gates }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_commutator_is_identity() {
        let mut rng = StdRng::seed_from_u64(42);
        let gadget = commutator(8, 5, 5, &mut rng);

        // Verify the gadget evaluates to identity
        for input in 0..256u64 {
            let output = gadget.full.evaluate(input as usize);
            assert_eq!(
                output, input as usize,
                "Commutator not identity at input {}",
                input
            );
        }
    }

    #[test]
    fn test_simple_identity() {
        let mut rng = StdRng::seed_from_u64(123);
        let gadget = simple_identity(6, 10, &mut rng);

        for input in 0..64u64 {
            let output = gadget.evaluate(input as usize);
            assert_eq!(output, input as usize);
        }
    }
}
