//! Reducer-resistant circuit obfuscation pipeline.
//!
//! This module transforms circuits into functionally equivalent but structurally
//! obfuscated versions that resist simplification by local rewrite rules.

pub mod config;
pub mod gadgets;
pub mod mixer;
pub mod passes;

pub use crate::analysis::metrics::ObfReport;
pub use config::ObfConfig;
pub use passes::obfuscate;
