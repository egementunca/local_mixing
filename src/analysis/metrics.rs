//! Obfuscation metrics and reporting.

use crate::infra::circuit::CircuitSeq;
use crate::obfuscate::mixer::{coverage_score, reducibility_estimate};
use serde::{Deserialize, Serialize};

/// Report of obfuscation results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObfReport {
    /// Original circuit length.
    pub orig_len: usize,
    /// Obfuscated circuit length.
    pub obf_len: usize,
    /// Length after running reducer (if probed).
    pub reduced_len: Option<usize>,
    /// Survival ratio: reduced_len / obf_len.
    pub survival_ratio: Option<f32>,
    /// Size overhead: obf_len / orig_len.
    pub overhead: f32,
    /// Wire coverage of obfuscated circuit.
    pub wire_coverage: f32,
    /// Reducibility estimate (lower is better).
    pub reducibility: f32,
    /// Number of retry attempts used.
    pub attempts_used: usize,
    /// Whether verification passed.
    pub verified: bool,
}

impl ObfReport {
    /// Create a new report from original and obfuscated circuits.
    pub fn new(
        original: &CircuitSeq,
        obfuscated: &CircuitSeq,
        num_wires: usize,
        verified: bool,
        attempts_used: usize,
    ) -> Self {
        let orig_len = original.len();
        let obf_len = obfuscated.len();

        Self {
            orig_len,
            obf_len,
            reduced_len: None,
            survival_ratio: None,
            overhead: obf_len as f32 / orig_len.max(1) as f32,
            wire_coverage: coverage_score(obfuscated, num_wires),
            reducibility: reducibility_estimate(obfuscated),
            attempts_used,
            verified,
        }
    }

    /// Set reduced length and compute survival ratio.
    pub fn with_reduced_len(mut self, reduced_len: usize) -> Self {
        self.reduced_len = Some(reduced_len);
        self.survival_ratio = Some(reduced_len as f32 / self.obf_len.max(1) as f32);
        self
    }

    /// Convert to JSON string (manual serialization).
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"orig_len": {}, "obf_len": {}, "reduced_len": {}, "survival_ratio": {}, "overhead": {:.4}, "wire_coverage": {:.4}, "reducibility": {:.4}, "attempts_used": {}, "verified": {}}}"#,
            self.orig_len,
            self.obf_len,
            self.reduced_len
                .map_or("null".to_string(), |v| v.to_string()),
            self.survival_ratio
                .map_or("null".to_string(), |v| format!("{:.4}", v)),
            self.overhead,
            self.wire_coverage,
            self.reducibility,
            self.attempts_used,
            self.verified
        )
    }

    /// Print a summary table row.
    pub fn summary_row(&self) -> String {
        format!(
            "{:>6} | {:>6} | {:>6} | {:>6.2} | {:>6.2} | {:>5.2} | {}",
            self.orig_len,
            self.obf_len,
            self.reduced_len.map_or("-".to_string(), |r| r.to_string()),
            self.survival_ratio.unwrap_or(0.0),
            self.overhead,
            self.wire_coverage,
            if self.verified { "✓" } else { "✗" }
        )
    }

    /// Print summary table header.
    pub fn summary_header() -> String {
        " Orig  |  Obf   |  Red   | Surv.  | Over.  | Cov.  | Ver".to_string()
    }
}

/// Aggregate statistics for benchmark runs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BenchmarkStats {
    pub count: usize,
    pub avg_overhead: f32,
    pub avg_survival: f32,
    pub avg_coverage: f32,
    pub min_survival: f32,
    pub max_survival: f32,
    pub verification_failures: usize,
}

impl BenchmarkStats {
    /// Compute aggregate stats from a collection of reports.
    pub fn from_reports(reports: &[ObfReport]) -> Self {
        if reports.is_empty() {
            return Self::default();
        }

        let count = reports.len();
        let avg_overhead = reports.iter().map(|r| r.overhead).sum::<f32>() / count as f32;

        let survivals: Vec<f32> = reports.iter().filter_map(|r| r.survival_ratio).collect();

        let avg_survival = if survivals.is_empty() {
            0.0
        } else {
            survivals.iter().sum::<f32>() / survivals.len() as f32
        };

        let avg_coverage = reports.iter().map(|r| r.wire_coverage).sum::<f32>() / count as f32;

        let min_survival = survivals.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_survival = survivals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        let verification_failures = reports.iter().filter(|r| !r.verified).count();

        Self {
            count,
            avg_overhead,
            avg_survival,
            avg_coverage,
            min_survival: if min_survival.is_finite() {
                min_survival
            } else {
                0.0
            },
            max_survival: if max_survival.is_finite() {
                max_survival
            } else {
                0.0
            },
            verification_failures,
        }
    }

    /// Print summary.
    pub fn summary(&self) -> String {
        format!(
            "Benchmark: {} circuits\n\
             Avg overhead: {:.2}x\n\
             Avg survival: {:.2}\n\
             Survival range: [{:.2}, {:.2}]\n\
             Avg coverage: {:.2}\n\
             Verification failures: {}",
            self.count,
            self.avg_overhead,
            self.avg_survival,
            self.min_survival,
            self.max_survival,
            self.avg_coverage,
            self.verification_failures
        )
    }
}
