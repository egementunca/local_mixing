# Obfuscation Quality Metrics (Proposed)

This document proposes metrics to quantify obfuscation quality in `local_mixing`.
It complements existing measures like reducer compression ratio, heatmaps, and DTW alignment.
These metrics are meant to be practical security proxies, not claims of full iO-style security.

## 0. Goals and scope

- Quantify how hard it is for a realistic attacker model to simplify or recognize circuits.
- Capture "trace scrambling" that does not show up in final outputs.
- Support metrics-driven tuning and design space exploration.
- Keep metrics reproducible: fixed inputs, fixed budgets, and recorded seeds.

## 1. Distinguisher advantage (indistinguishability-inspired)

Idea: treat obfuscation quality as a practical indistinguishability experiment.

Setup:
- Choose source circuits `C_a`, `C_b`.
- Generate multiple obfuscations `C_tilde = O(C; seed)` for each source.
- Extract features with a chosen distinguisher `D` (DTW cost, template-hit counts, graph stats, TVD features, reducer stats).
- Train and evaluate a classifier to label "same source vs different source."

Metric:
- `advantage = accuracy - 0.5`
- Report mean and variance across cross-validation splits and across source pairs.

Notes:
- Keep datasets balanced and use held-out seeds.
- Start with a simple linear model; if it already performs well, that is a strong signal.

## 2. Trace distribution TVD (intermediate states)

Final outputs match for reversible circuits, so TVD on final outputs is always 0.
Instead, compare intermediate trace distributions.

Setup:
- Fix a probe set of inputs `X`.
- For each depth `i`, collect `S_C(i; x)` over `x in X`.
- Build an empirical distribution per depth (or per aligned depth).

Metric:
- `TVD(P, Q) = 0.5 * sum_s |P(s) - Q(s)|`

Practical variants:
- Per-depth TVD using aligned steps (DTW path).
- Global TVD by pooling all intermediate states.
- Feature-based TVD for large widths: bit marginals, Hamming-weight histograms, pairwise correlations.

Reporting:
- Mean TVD across depths.
- Max TVD across depths.
- Area under the TVD-vs-depth curve.

## 3. Degree of Functional Corruption (DFC) under an attacker

Define DFC as how wrong the circuit looks after a naive deobfuscation attempt.

Setup:
- Fix an attacker reducer `R` with a budget.
- Compute `C_guess = R(C_tilde)`.
- Compare `C_guess` to the true function on test inputs.

Metric:
- `DFC = (# mismatching test inputs) / (# test inputs)`
- For identity targets: fraction of inputs where output != input.

Notes:
- Report DFC as a function of reducer budget (a curve is more informative than one point).
- For small widths, compute exact DFC; otherwise use randomized testing.

## 4. Normal-form distance metrics (canonicalizer fragility)

Inspired by normal-form notions in group obfuscation: obfuscation fails if outputs collapse quickly to a canonical form.

Metrics:
- Rewrite steps to canonical form (how many local rewrites are needed).
- Shrink ratio under canonicalization: `|NF(C_tilde)| / |C_tilde|`.
- Canonical key stability across seeds (fraction of seeds that map to the same key).

Implementation note:
- The canonicalizer can reuse the same primitives as the reducer or template+witness DB.

## 5. Metrics-driven design space exploration (Mirage-style)

Treat obfuscation as multi-objective optimization with a metric vector.

Suggested metric vector:
- Overhead: gate count inflation, runtime, wire coverage, ancilla usage.
- Security proxies: reducer survival ratio, template-hit rate, DTW cost, trace TVD, classifier advantage, DFC.

Process:
- Run annealing or evolutionary search with these metrics as objectives.
- Maintain a Pareto front rather than collapsing to a single scalar.

## 6. Minimal metric suite (start here)

Recommended starting suite for the repo:

1. Reducer survival ratios:
   - `|R(C_tilde)| / |C_tilde|`
   - `|R(C_tilde)| / |R(C)|`
2. DTW cost (`c*`) plus warp fraction (non-diagonal steps).
3. Trace TVD on intermediate states (aligned or pooled).
4. Classifier advantage on "same source vs different source" using lightweight features.

These four give a compact baseline that captures attacker hardness, trace scrambling, and distinguishability.

## 7. Local integration hooks (existing code)

Potential places to extend:
- DTW + trace: `local_mixing/src/analysis/alignment/mod.rs`.
- Report struct: `local_mixing/src/analysis/metrics.rs` (`ObfReport`).
- Reducers: `local_mixing/src/reducer/budget.rs`, `local_mixing/src/algorithms/annealing/local.rs`.
- Circuit evaluation: `local_mixing/src/infra/circuit/circuit.rs`.
