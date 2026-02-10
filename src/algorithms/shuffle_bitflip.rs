//! Wire shuffle + bit-flip generator (B_{w,s}) for pre-mix obfuscation.
//!
//! This module implements the wire shuffle with bit-flip integration as described
//! in PLAN_wire_shuffle_bitflip.md.
//!
//! Key concepts:
//! - `w`: permutation on wires 0..n-1
//! - `s`: bit-flip mask in {0,1}^n
//! - `β_{w,s}(x_0,..,x_{n-1}) = (x_{w(0)} ⊕ s_0, ..., x_{w(n-1)} ⊕ s_{n-1})`
//! - `B_{w,s}`: circuit implementing `β_{w,s}`
//!
//! Composition rule:
//!     `β_{w,s} ∘ β_{w',s'} = β_{w∘w', w'(s) ⊕ s'}`

use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::{FlipMode, ShuffleBitflipConfig};
use crate::infra::circuit::CircuitSeq;

/// Flip mask for a 2-wire swap gadget: (flip_wire_a, flip_wire_b)
pub type SwapFlipMask = (u8, u8);

/// A swap-with-flip gadget from the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapFlipGadget {
    pub gates: Vec<[u8; 3]>,
    pub flip_mask: [u8; 2],
    #[serde(default)]
    pub flip_name: String,
}

/// Library of swap-with-flip gadgets loaded from JSON
#[derive(Debug, Clone, Default)]
pub struct SwapFlipLibrary {
    /// Gadgets organized by flip mask
    gadgets_by_mask: [Vec<Vec<[u8; 3]>>; 4],
}

impl SwapFlipLibrary {
    /// Load library from JSON file
    pub fn load_json<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let payload: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let mut library = Self::default();

        if let Some(circuits) = payload.get("circuits").and_then(|c| c.as_array()) {
            for item in circuits {
                if let (Some(gates_arr), Some(flip_arr)) =
                    (item.get("gates").and_then(|g| g.as_array()),
                     item.get("flip_mask").and_then(|f| f.as_array()))
                {
                    let gates: Vec<[u8; 3]> = gates_arr
                        .iter()
                        .filter_map(|g| {
                            if let Some(arr) = g.as_array() {
                                if arr.len() >= 3 {
                                    Some([
                                        arr[0].as_u64()? as u8,
                                        arr[1].as_u64()? as u8,
                                        arr[2].as_u64()? as u8,
                                    ])
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect();

                    if flip_arr.len() >= 2 {
                        let s0 = flip_arr[0].as_u64().unwrap_or(0) as u8;
                        let s1 = flip_arr[1].as_u64().unwrap_or(0) as u8;
                        let index = Self::mask_to_index(s0, s1);
                        library.gadgets_by_mask[index].push(gates);
                    }
                }
            }
        }

        Ok(library)
    }

    /// Convert flip mask (s0, s1) to array index 0-3
    fn mask_to_index(s0: u8, s1: u8) -> usize {
        ((s0 & 1) | ((s1 & 1) << 1)) as usize
    }

    /// Check if library has gadgets for the given flip mask
    pub fn has_mask(&self, s0: u8, s1: u8) -> bool {
        !self.gadgets_by_mask[Self::mask_to_index(s0, s1)].is_empty()
    }

    /// Get a random gadget for the given flip mask
    pub fn random_gadget<R: Rng>(&self, s0: u8, s1: u8, rng: &mut R) -> Option<&Vec<[u8; 3]>> {
        let candidates = &self.gadgets_by_mask[Self::mask_to_index(s0, s1)];
        if candidates.is_empty() {
            None
        } else {
            Some(&candidates[rng.random_range(0..candidates.len())])
        }
    }

    /// Total number of gadgets in library
    pub fn total_count(&self) -> usize {
        self.gadgets_by_mask.iter().map(|v| v.len()).sum()
    }
}

/// State for cumulative shuffle + bit-flip composition
#[derive(Debug, Clone)]
pub struct ShuffleBitflipState {
    /// Current cumulative permutation
    pub permutation: Vec<usize>,
    /// Current cumulative flip mask
    pub flip_mask: Vec<u8>,
}

impl ShuffleBitflipState {
    /// Create identity state
    pub fn identity(width: usize) -> Self {
        Self {
            permutation: (0..width).collect(),
            flip_mask: vec![0; width],
        }
    }

    /// Compose with another (w', s') pair
    /// Uses: β_{w,s} ∘ β_{w',s'} = β_{w∘w', w'(s) ⊕ s'}
    pub fn compose(&self, w_prime: &[usize], s_prime: &[u8]) -> Self {
        let n = self.permutation.len();

        // w_new = w ∘ w' (apply w' then w)
        let w_new: Vec<usize> = (0..n)
            .map(|i| self.permutation[w_prime[i]])
            .collect();

        // s_new = w'(s) ⊕ s' where w'(s)[i] = s[w'[i]]
        let s_new: Vec<u8> = (0..n)
            .map(|i| self.flip_mask[w_prime[i]] ^ s_prime[i])
            .collect();

        Self {
            permutation: w_new,
            flip_mask: s_new,
        }
    }

    /// Compute inverse state
    /// Inverse of β_{w,s} is β_{w^{-1}, w^{-1}(s)}
    pub fn inverse(&self) -> Self {
        let n = self.permutation.len();

        // w_inv[w[i]] = i
        let mut w_inv = vec![0; n];
        for i in 0..n {
            w_inv[self.permutation[i]] = i;
        }

        // s_inv = w^{-1}(s)
        let s_inv: Vec<u8> = (0..n)
            .map(|i| self.flip_mask[w_inv[i]])
            .collect();

        Self {
            permutation: w_inv,
            flip_mask: s_inv,
        }
    }
}

/// Generator for B_{w,s} circuits
pub struct ShuffleBitflipGenerator {
    width: usize,
    config: ShuffleBitflipConfig,
    library: Option<SwapFlipLibrary>,
}

impl ShuffleBitflipGenerator {
    /// Create a new generator
    pub fn new(width: usize, config: ShuffleBitflipConfig) -> Self {
        let library = config.gadget_library_path.as_ref().and_then(|path| {
            SwapFlipLibrary::load_json(path).ok()
        });

        Self {
            width,
            config,
            library,
        }
    }

    /// Generate B_{w,s} circuit
    pub fn generate<R: Rng>(
        &self,
        permutation: &[usize],
        flip_mask: &[u8],
        rng: &mut R,
    ) -> CircuitSeq {
        assert_eq!(permutation.len(), self.width);
        assert_eq!(flip_mask.len(), self.width);

        match self.config.flip_mode {
            FlipMode::None => self.generate_shuffle_only(permutation, rng),
            FlipMode::Separate => self.generate_style_a(permutation, flip_mask, rng),
            FlipMode::Embedded => self.generate_style_b(permutation, flip_mask, rng),
        }
    }

    /// Generate random B_{w,s} with random permutation and flip mask
    pub fn generate_random<R: Rng>(&self, rng: &mut R) -> (CircuitSeq, Vec<usize>, Vec<u8>) {
        // Random permutation
        let mut permutation: Vec<usize> = (0..self.width).collect();
        permutation.shuffle(rng);

        // Random flip mask
        let flip_mask: Vec<u8> = (0..self.width)
            .map(|_| if rng.random::<f64>() < self.config.flip_probability { 1 } else { 0 })
            .collect();

        let circuit = self.generate(&permutation, &flip_mask, rng);
        (circuit, permutation, flip_mask)
    }

    /// Generate inverse B_{w,s}^{-1}
    pub fn generate_inverse<R: Rng>(
        &self,
        permutation: &[usize],
        flip_mask: &[u8],
        rng: &mut R,
    ) -> CircuitSeq {
        let state = ShuffleBitflipState {
            permutation: permutation.to_vec(),
            flip_mask: flip_mask.to_vec(),
        };
        let inv = state.inverse();
        self.generate(&inv.permutation, &inv.flip_mask, rng)
    }

    /// Generate shuffle-only circuit (no bit-flips)
    fn generate_shuffle_only<R: Rng>(&self, permutation: &[usize], rng: &mut R) -> CircuitSeq {
        // Use aux wire = width (one extra wire)
        let aux = self.width;
        let circuit_width = self.width + 1;

        let swaps = self.decompose_permutation(permutation);
        let mut gates: Vec<[u8; 3]> = Vec::new();

        for (wire_a, wire_b) in swaps {
            let swap_gates = self.generate_swap(wire_a, wire_b, aux, circuit_width, rng);
            gates.extend(swap_gates);
        }

        CircuitSeq { gates }
    }

    /// Generate Style A: shuffle followed by explicit X layer
    fn generate_style_a<R: Rng>(
        &self,
        permutation: &[usize],
        flip_mask: &[u8],
        rng: &mut R,
    ) -> CircuitSeq {
        let mut circuit = self.generate_shuffle_only(permutation, rng);

        // Add X gates for flips
        // ECA57 X gate: target ^= (ctrl OR NOT ctrl) = NOT target
        let aux = self.width;
        for wire in 0..self.width {
            if flip_mask[wire] != 0 {
                // Use aux as both controls for unconditional flip
                let ctrl = if aux != wire { aux } else { (wire + 1) % (self.width + 1) };
                circuit.gates.push([wire as u8, ctrl as u8, ctrl as u8]);
            }
        }

        circuit
    }

    /// Generate Style B: swap-with-flip gadgets
    fn generate_style_b<R: Rng>(
        &self,
        permutation: &[usize],
        flip_mask: &[u8],
        rng: &mut R,
    ) -> CircuitSeq {
        let aux = self.width;
        let circuit_width = self.width + 1;

        let swaps = self.decompose_permutation(permutation);
        let mut gates: Vec<[u8; 3]> = Vec::new();
        let mut remaining_flips = flip_mask.to_vec();

        for (wire_a, wire_b) in swaps {
            // Try to consume flips in this swap
            let s0 = remaining_flips[wire_a];
            let s1 = remaining_flips[wire_b];

            let swap_gates = if self.library.as_ref().map_or(false, |l| l.has_mask(s0, s1)) {
                // Use gadget with embedded flips
                remaining_flips[wire_a] = 0;
                remaining_flips[wire_b] = 0;
                self.generate_swap_flip(wire_a, wire_b, aux, circuit_width, s0, s1, rng)
            } else {
                // Fallback to plain swap
                self.generate_swap(wire_a, wire_b, aux, circuit_width, rng)
            };

            gates.extend(swap_gates);
        }

        // Add explicit flips for any remaining
        for wire in 0..self.width {
            if remaining_flips[wire] != 0 {
                let ctrl = if aux != wire { aux } else { (wire + 1) % circuit_width };
                gates.push([wire as u8, ctrl as u8, ctrl as u8]);
            }
        }

        CircuitSeq { gates }
    }

    /// Decompose permutation into swap sequence (selection-sort style)
    fn decompose_permutation(&self, permutation: &[usize]) -> Vec<(usize, usize)> {
        let mut current: Vec<usize> = (0..self.width).collect();
        let mut swaps = Vec::new();

        for output_pos in 0..self.width {
            let needed_input = permutation[output_pos];
            let current_pos = current.iter().position(|&x| x == needed_input).unwrap();

            if current_pos != output_pos {
                swaps.push((output_pos, current_pos));
                current.swap(output_pos, current_pos);
            }
        }

        swaps
    }

    /// Generate canonical 6-gate swap for two wires
    fn generate_swap<R: Rng>(
        &self,
        wire_a: usize,
        wire_b: usize,
        aux: usize,
        _width: usize,
        _rng: &mut R,
    ) -> Vec<[u8; 3]> {
        // Canonical swap sequence: (target, ctrl1, ctrl2) with wire_map applied
        // Original: [(0,2,1), (0,1,2), (1,0,2), (1,2,0), (0,1,2), (0,2,1)]
        let canonical: [[u8; 3]; 6] = [
            [0, 2, 1],
            [0, 1, 2],
            [1, 0, 2],
            [1, 2, 0],
            [0, 1, 2],
            [0, 2, 1],
        ];

        let wire_map = [wire_a as u8, wire_b as u8, aux as u8];

        canonical
            .iter()
            .map(|[t, c1, c2]| {
                [wire_map[*t as usize], wire_map[*c1 as usize], wire_map[*c2 as usize]]
            })
            .collect()
    }

    /// Generate swap-with-flip gadget
    fn generate_swap_flip<R: Rng>(
        &self,
        wire_a: usize,
        wire_b: usize,
        aux: usize,
        width: usize,
        s0: u8,
        s1: u8,
        rng: &mut R,
    ) -> Vec<[u8; 3]> {
        // Try to use library gadget
        if let Some(ref library) = self.library {
            if let Some(gadget) = library.random_gadget(s0, s1, rng) {
                let wire_map = [wire_a as u8, wire_b as u8, aux as u8];
                return gadget
                    .iter()
                    .map(|[t, c1, c2]| {
                        [wire_map[*t as usize], wire_map[*c1 as usize], wire_map[*c2 as usize]]
                    })
                    .collect();
            }
        }

        // Fallback: plain swap + explicit flips
        let mut gates = self.generate_swap(wire_a, wire_b, aux, width, rng);

        if s0 != 0 {
            let ctrl = if aux != wire_a { aux } else { wire_b };
            gates.push([wire_a as u8, ctrl as u8, ctrl as u8]);
        }
        if s1 != 0 {
            let ctrl = if aux != wire_b { aux } else { wire_a };
            gates.push([wire_b as u8, ctrl as u8, ctrl as u8]);
        }

        gates
    }
}

/// Apply B_{w,s} pre-mix stage to a circuit
pub fn apply_premix<R: Rng>(
    circuit: &CircuitSeq,
    width: usize,
    config: &ShuffleBitflipConfig,
    rng: &mut R,
) -> (CircuitSeq, ShuffleBitflipState) {
    if !config.enabled {
        return (circuit.clone(), ShuffleBitflipState::identity(width));
    }

    let generator = ShuffleBitflipGenerator::new(width, config.clone());
    let (premix, permutation, flip_mask) = generator.generate_random(rng);

    let state = ShuffleBitflipState {
        permutation,
        flip_mask,
    };

    // Conjugate circuit: B_{w,s} · C · B_{w,s}^{-1}
    // For now, we just prepend the shuffle (caller handles inverse)
    let mut combined = premix;
    combined.gates.extend(circuit.gates.iter().cloned());

    (combined, state)
}

/// Verify a B_{w,s} circuit is correct (for testing)
pub fn verify_shuffle_bitflip(
    circuit: &CircuitSeq,
    permutation: &[usize],
    flip_mask: &[u8],
    data_width: usize,
    circuit_width: usize,
) -> bool {
    for input in 0u64..(1u64 << circuit_width) {
        let mut state: Vec<u8> = (0..circuit_width)
            .map(|b| ((input >> b) & 1) as u8)
            .collect();

        // Apply circuit
        for gate in &circuit.gates {
            let t = gate[0] as usize;
            let c1 = gate[1] as usize;
            let c2 = gate[2] as usize;
            // ECA57: target ^= (ctrl1 OR NOT ctrl2)
            let flip = state[c1] | (1 - state[c2]);
            state[t] ^= flip;
        }

        // Check data wires implement β_{w,s}
        for out_idx in 0..data_width {
            let in_wire = permutation[out_idx];
            let expected = (((input >> in_wire) & 1) as u8) ^ flip_mask[out_idx];
            if state[out_idx] != expected {
                return false;
            }
        }

        // Check non-data wires are preserved
        for w in data_width..circuit_width {
            if state[w] != ((input >> w) & 1) as u8 {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_state() {
        let state = ShuffleBitflipState::identity(4);
        assert_eq!(state.permutation, vec![0, 1, 2, 3]);
        assert_eq!(state.flip_mask, vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_inverse_composition() {
        let state = ShuffleBitflipState {
            permutation: vec![2, 0, 1, 3],
            flip_mask: vec![1, 0, 1, 0],
        };
        let inv = state.inverse();
        let composed = state.compose(&inv.permutation, &inv.flip_mask);

        assert_eq!(composed.permutation, vec![0, 1, 2, 3]);
        assert_eq!(composed.flip_mask, vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_generate_shuffle_only() {
        let config = ShuffleBitflipConfig {
            enabled: true,
            flip_mode: FlipMode::None,
            ..Default::default()
        };
        let generator = ShuffleBitflipGenerator::new(3, config);
        let mut rng = rand::thread_rng();

        let circuit = generator.generate(&[1, 0, 2], &[0, 0, 0], &mut rng);
        assert!(verify_shuffle_bitflip(&circuit, &[1, 0, 2], &[0, 0, 0], 3, 4));
    }

    #[test]
    fn test_generate_style_a() {
        let config = ShuffleBitflipConfig {
            enabled: true,
            flip_mode: FlipMode::Separate,
            ..Default::default()
        };
        let generator = ShuffleBitflipGenerator::new(3, config);
        let mut rng = rand::thread_rng();

        let circuit = generator.generate(&[1, 0, 2], &[1, 0, 1], &mut rng);
        assert!(verify_shuffle_bitflip(&circuit, &[1, 0, 2], &[1, 0, 1], 3, 4));
    }

    #[test]
    fn test_all_permutations_3_wire() {
        use itertools::Itertools;

        let config = ShuffleBitflipConfig {
            enabled: true,
            flip_mode: FlipMode::Separate,
            ..Default::default()
        };
        let generator = ShuffleBitflipGenerator::new(3, config);
        let mut rng = rand::thread_rng();

        for perm in (0..3).permutations(3) {
            let circuit = generator.generate(&perm, &[0, 0, 0], &mut rng);
            assert!(
                verify_shuffle_bitflip(&circuit, &perm, &[0, 0, 0], 3, 4),
                "Failed for permutation {:?}",
                perm
            );
        }
    }
}
