//! Canonical Hashing - Content hashing for circuit memoization
//!
//! Provides fast hashing for caching reducer results and avoiding
//! redundant computation during annealing.

use crate::infra::circuit::CircuitSeq;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Compute a 64-bit hash of a circuit for memoization
pub fn circuit_hash_64(c: &CircuitSeq) -> u64 {
    let mut hasher = DefaultHasher::new();
    c.gates.hash(&mut hasher);
    hasher.finish()
}

/// Compute a canonical key for a window of gates
/// Uses first-occurrence wire relabeling for position-independence
pub fn canonical_window_key(gates: &[[u8; 3]]) -> u64 {
    if gates.is_empty() {
        return 0;
    }

    // First-occurrence wire relabeling
    let mut wire_map: [u8; 256] = [255; 256];
    let mut next_id = 0u8;
    let mut canonical_gates = Vec::with_capacity(gates.len());

    for gate in gates {
        let mut new_gate = [0u8; 3];
        for (i, &wire) in gate.iter().enumerate() {
            if wire_map[wire as usize] == 255 {
                wire_map[wire as usize] = next_id;
                next_id += 1;
            }
            new_gate[i] = wire_map[wire as usize];
        }
        canonical_gates.push(new_gate);
    }

    // Hash the canonical representation
    let mut hasher = DefaultHasher::new();
    canonical_gates.hash(&mut hasher);
    hasher.finish()
}

/// Simple LRU-style cache for energy values
pub struct EnergyCache {
    cache: std::collections::HashMap<u64, f64>,
    max_size: usize,
}

impl EnergyCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            max_size,
        }
    }

    pub fn get(&self, hash: u64) -> Option<f64> {
        self.cache.get(&hash).copied()
    }

    pub fn insert(&mut self, hash: u64, energy: f64) {
        // Simple eviction: clear if too large
        if self.cache.len() >= self.max_size {
            self.cache.clear();
        }
        self.cache.insert(hash, energy);
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_hash_deterministic() {
        let c = CircuitSeq {
            gates: vec![[0, 1, 2], [3, 4, 5]],
        };

        let h1 = circuit_hash_64(&c);
        let h2 = circuit_hash_64(&c);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_canonical_window_key() {
        // Same gates on different wires should produce same canonical key
        let gates1 = vec![[0, 1, 2], [0, 2, 1]];
        let gates2 = vec![[5, 6, 7], [5, 7, 6]]; // Same pattern, different wires

        let key1 = canonical_window_key(&gates1);
        let key2 = canonical_window_key(&gates2);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_energy_cache() {
        let mut cache = EnergyCache::new(10);
        cache.insert(123, 0.5);
        assert_eq!(cache.get(123), Some(0.5));
        assert_eq!(cache.get(456), None);
    }
}
