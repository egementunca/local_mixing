# Research Findings

## 1. Skeleton Based SAT ID Creation
We demonstrated the ability to generate identity circuits with specific "skeleton" constraints (chain collisions) using a SAT solver.

### Methodology
Script: `sat_revsynth/scripts/experiment_skeleton_demo.py`
Constraint: Enforce a chain of non-commuting collisions between adjacent gates.

### Results
| Wires | Constraints | Min Gates Found | Time (s) |
|-------|-------------|-----------------|----------|
| 3     | None        | 8               | 0.04     |
| 4     | None        | 8               | 0.03     |
| 4     | Chain=3     | 8               | 0.11     |
| 5     | None        | 10              | 0.19     |

The solver successfully found identities with the imposed structure, confirming the feasibility of skeleton-based generation.

## 2. d Growth (Identity Growth)
We measured the growth of identity circuits under different hardness parameters using the `grow-identity` tool.

### Methodology
Script: `local_mixing/scripts/experiment_identity_growth.py`
- **Baseline**: Standard parameters (mixed template source).
- **Hard**: Increased conjugation depth (2-4) and hardness ratio (0.70).

### Results (32 Wires, 5 Rounds)
| Experiment | Final Gates | Comp. Ratio (Final) | Time (s) |
|------------|-------------|---------------------|----------|
| Baseline   | 368         | 0.84                | 3.2      |
| Hard       | 1045        | 0.97                | 7.3      |

The "Hard" configuration resulted in significantly larger circuits that were more resistant to compression (ratio closer to 1.0), as expected.

## 3. SAT Based Optimization
Running `experiment_sat_64w.py` to benchmark SAT-based obfuscation on 64-wire, 100-gate circuits (Full Scale).

### Status
- **Initialization**: Validated. Successfully generated initial circuits (64 wires, 100 gates).
- **Execution**: The experiment is currently running to generate heatmaps and alignment plots.
- **Goal**: Demonstrate SAT-based obfuscation effectiveness on larger circuits.

### Control Experiment (Initial vs Random)
To verify the heatmap generation pipeline, we compared the Initial Circuit (Identity) against a Random Circuit.
- **Result**: Successfully generated heatmap.
- **Output**: `experiments/2026-01-20/sat_optimization/control_heatmap.png`
- **Observation**: Random circuit shows high structural divergence from identity, as expected.

## 4. Related Research Context
Exploration of `reversible-synth` and `obfuscated-circuits` yielded additional relevant context:

- **Skeleton Graphs**: `obfuscated-circuits/build-skeleton.py` contains logic for canonicalizing skeleton graphs, supporting the theoretical basis for the "skeleton constraints" used in SAT ID creation.
- **Non-Trivial Identities**: `reversible-synth/RESEARCH_PROGRESS.md` details methods for generating "non-trivial" identities (not just $C \cdot C^{-1}$) using bidirectional BFS. This aligns with the goals of the "d growth" experiments to create complex identity structures.
