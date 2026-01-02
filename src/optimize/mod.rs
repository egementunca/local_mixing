//! SAT-based local circuit optimization module.
//!
//! This module provides functionality for:
//! - Extracting subcircuits from larger circuits
//! - Computing truth tables for subcircuits
//! - Calling Python SAT solver for optimization
//! - Replacing subcircuits with optimized versions
//!
//! All optimized circuits use only the ORNB gate (a | !b).

pub mod compress_sat;
pub mod python_bridge;
pub mod replacement;
pub mod subcircuit;
pub mod truth_table;

pub use compress_sat::compress_sat;
pub use python_bridge::{OptimizeRequest, OptimizeResponse, call_python_optimizer};
pub use replacement::{replace_subcircuit, verify_equivalence};
pub use subcircuit::{SubCircuit, extract_by_wire_window, extract_subcircuit};
pub use truth_table::compute_truth_tables;
