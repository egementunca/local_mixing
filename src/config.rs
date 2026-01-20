use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for the `abbutterfly` and `butterfly` obfuscation pipeline
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ObfuscationConfig {
    // --- Structural Parameters (Mixing) ---
    /// Minimum size of the random identity structure ($R \cdot R^{-1}$)
    pub structure_block_size_min: usize,
    /// Maximum size of the random identity structure
    pub structure_block_size_max: usize,

    /// Probability of inserting a template (Move A)
    pub mix_prob_template: f64,
    /// Probability of performing adjacent commuting swaps (Move B)
    pub mix_prob_swaps: f64,
    /// Probability of performing patch-and-cancel (Move D)
    /// Note: Usually calculated as remainder, but explicit for config
    pub mix_prob_patch: f64,

    // --- Obfuscation Intensity ---
    /// Number of single gate replacements to perform before main loop
    pub single_gate_replacements: usize,
    /// Main "shooting" intensity (random gate insertions) at start
    pub shooting_count: usize,
    /// Inner "shooting" intensity applied inside each block/round
    pub shooting_count_inner: usize,
    /// Number of butterfly rounds
    pub rounds: usize,

    // --- Modes ---
    /// Enable SAT-based compression
    pub sat_mode: bool,
    /// Disable ancilla expansion (work in place)
    pub no_ancilla_mode: bool,
    /// Enable single gate replacement pass
    pub single_gate_mode: bool,

    // --- Compression/Optimization ---
    /// Skip compression entirely (Inflation only)
    pub skip_compression: bool,
    /// Window size for peephole optimization (Normal mode)
    pub compression_window_size: usize,
    /// Window size for peephole optimization (SAT mode)
    pub compression_window_size_sat: usize,
    /// Timeout/conflict limit for SAT solver
    pub compression_sat_limit: usize,
    /// Number of stable passes required to stop final compression
    pub final_stability_threshold: usize,
    /// Denominator for splitting circuit into chunks (Gates / Base = K chunks)
    pub chunk_split_base: usize,

    // --- Reducer Config (if used) ---
    /// Maximum number of active wires to check in a window
    pub reducer_active_wire_limit: usize,
    /// Window sizes to scan for identity removal
    pub reducer_window_sizes: Vec<usize>,

    // --- Replacements ---
    /// Enable pair gate replacements
    pub pair_replacement_mode: bool,
    /// Enable equal-length replacements (blurring)
    pub equal_replacement_mode: bool,

    // --- System ---
    /// Path to LMDB database directory
    pub lmdb_path: PathBuf,
}

impl Default for ObfuscationConfig {
    fn default() -> Self {
        Self {
            structure_block_size_min: 10,
            structure_block_size_max: 30,
            mix_prob_template: 0.40,
            mix_prob_swaps: 0.50,
            mix_prob_patch: 0.10,
            single_gate_replacements: 500,
            shooting_count: 500_000,
            shooting_count_inner: 0,
            rounds: 3,
            sat_mode: true,
            no_ancilla_mode: false,
            single_gate_mode: false,
            skip_compression: false,
            compression_window_size: 100,
            compression_window_size_sat: 10,
            compression_sat_limit: 1000,
            final_stability_threshold: 12,
            chunk_split_base: 1500,
            reducer_active_wire_limit: 6,
            reducer_window_sizes: vec![4, 6, 8, 10, 12, 16],
            pair_replacement_mode: true,
            equal_replacement_mode: true,
            lmdb_path: PathBuf::from("db"),
        }
    }
}

/// Configuration for the `obfuscate` command (legacy/separate pipeline)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ObfConfig {
    pub segments: usize,
    pub gadget_size: usize,
    pub target_overhead: f64,
    pub max_attempts: usize,
}

impl Default for ObfConfig {
    fn default() -> Self {
        Self {
            segments: 10,
            gadget_size: 6,
            target_overhead: 5.0,
            max_attempts: 100,
        }
    }
}
