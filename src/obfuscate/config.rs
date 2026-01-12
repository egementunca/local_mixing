//! Obfuscation configuration and level presets.

use serde::{Deserialize, Serialize};

/// Configuration for the obfuscation pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObfConfig {
    /// Number of segments to split the circuit into.
    pub segments: usize,
    /// Size of identity gadgets to inject at boundaries.
    pub gadget_size: usize,
    /// Depth of the random mixer circuit for conjugation.
    pub mixer_depth: usize,
    /// Target size overhead multiplier (e.g., 2.0 = double size).
    pub target_overhead: f32,
    /// Minimum survival ratio after reducer probe.
    pub min_survival_ratio: f32,
    /// Whether to probe with reducer and retry on failure.
    pub reducer_probe: bool,
    /// Maximum retry attempts when survival ratio is too low.
    pub max_attempts: usize,
    /// Budget for reducer iterations during probing.
    pub reducer_budget: usize,
    /// Whether to verify functional equivalence.
    pub verify: bool,
    /// Random seed (0 = use system random).
    pub seed: u64,
    /// Number of wires in the circuit.
    pub num_wires: usize,
}

impl Default for ObfConfig {
    fn default() -> Self {
        Self::level(3)
    }
}

impl ObfConfig {
    /// Create config for a given obfuscation level (1-5).
    pub fn level(level: u8) -> Self {
        match level {
            1 => Self {
                segments: 4,
                gadget_size: 10,
                mixer_depth: 20,
                target_overhead: 2.0,
                min_survival_ratio: 0.7,
                reducer_probe: true,
                max_attempts: 10,
                reducer_budget: 100,
                verify: true,
                seed: 0,
                num_wires: 64,
            },
            2 => Self {
                segments: 6,
                gadget_size: 20,
                mixer_depth: 40,
                target_overhead: 3.5,
                min_survival_ratio: 0.7,
                reducer_probe: true,
                max_attempts: 10,
                reducer_budget: 200,
                verify: true,
                seed: 0,
                num_wires: 64,
            },
            3 => Self {
                segments: 8,
                gadget_size: 30,
                mixer_depth: 60,
                target_overhead: 5.0,
                min_survival_ratio: 0.7,
                reducer_probe: true,
                max_attempts: 10,
                reducer_budget: 300,
                verify: true,
                seed: 0,
                num_wires: 64,
            },
            4 => Self {
                segments: 12,
                gadget_size: 50,
                mixer_depth: 100,
                target_overhead: 8.0,
                min_survival_ratio: 0.7,
                reducer_probe: true,
                max_attempts: 10,
                reducer_budget: 500,
                verify: true,
                seed: 0,
                num_wires: 64,
            },
            5 | _ => Self {
                segments: 16,
                gadget_size: 80,
                mixer_depth: 150,
                target_overhead: 12.0,
                min_survival_ratio: 0.7,
                reducer_probe: true,
                max_attempts: 10,
                reducer_budget: 1000,
                verify: true,
                seed: 0,
                num_wires: 64,
            },
        }
    }

    /// Set the number of wires.
    pub fn with_wires(mut self, n: usize) -> Self {
        self.num_wires = n;
        self
    }

    /// Set the random seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}
