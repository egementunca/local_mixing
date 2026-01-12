# Changelog: `feature/annealed-obfuscator` vs `bin`

This document summarizes the major architectural and functional changes introduced in the current branch compared to the original `bin` branch.

**Summary**: The codebase has evolved from a basic "Butterfly" obfuscator into a comprehensive **Obfuscation & Analysis Framework**. It now includes Simulated Annealing, SAT-based minimization, rigorous alignment verification, and a structured database layer.

## 1. New Core Obfuscation Engines

### A. Annealed Obfuscator (`src/obfuscation/anneal.rs`)
**[NEW]** A stochastic obfuscation engine that uses Simulated Annealing.
-   **Mechanism**: Proposes local circuit updates (moves) and accepts/rejects them based on an "Energy Function" (difficulty to reduce).
-   **Moves**: Implemented in `src/local.rs` (Template Insert, Commute, Patch Pair).
-   **Goal**: Create "locally sticky" obfuscations that are harder to reverse than standard random noise.

### B. Local Mixer (`src/local.rs`)
**[NEW]** A dedicated module for local, window-based transformations.
-   Defines the primitive moves used by the Annealer.
-   Implements `is_identity_window` and `commutes` checks for functional preservation.

### C. SAT-Based Compression (`src/optimize/compress_sat.rs`)
**[NEW]** Integration with SAT solvers for optimal local circuit minimization.
-   Can replace the heuristic "Rainbow Table" lookup with a stronger proof-based minimization.
-   Used for "Attack" simulation to measuring obfuscation quality.

## 2. Infrastructure Upgrades

### A. Database Abstraction (`src/store/`)
**[NEW]** Structured access to LMDB and SQLite.
-   `reader.rs`: Strongly typed `TemplateDB` for querying identity templates by canonical hash.
-   `schema.rs`: Binary serialization formats for stored templates.

### B. Verification & Alignment (`src/alignment/`)
**[NEW]** Tools to verify structural obfuscation.
-   **DTW (Dynamic Time Warping)**: Aligns the gate index of the original vs obfuscated circuit to visualize entropy injection.
-   **Trace Alignment**: Ensures that while the structure is different, the wire timeline is logically consistent.

### C. Configuration (`src/config.rs`)
**[NEW]** Centralized configuration structures (`ObfuscationConfig`, `AnnealConfig`) replacing scattered constants.

## 3. CLI Expansions (`src/main.rs`)

The `main.rs` has grown significantly to support new workflows:
-   `anneal`: Run the simulated annealing process.
-   `local-mix`: Run simple randomized local mixing (without annealing schedule).
-   `align`: Run the DTW alignment analysis.
-   `heatmap`: Generate visualization data for experiment results.

## 4. Experimental Scripts (`scripts/`)
**[NEW]** A suite of Python scripts for large-scale benchmarking.
-   `run_experiments.py`: Automates batch testing of obfuscation parameters.
-   `plot_heatmap.py` / `plot_alignment.py`: Visualizes the quality of obfuscation.

## 5. Major Refactors

-   **`src/replace/mixing.rs`**: heavily patched to support the "Asymmetric Butterfly" and integrate the new compression logic and detailed logging.
-   **`src/replace/replace.rs`**: Enhanced with better circuit manipulation helpers.

---

**Diff Stat**: `+23,694 insertions, -1,431 deletions`.
This represents a complete system overhaul, moving from a single-algorithm prototype to a multi-strategy research platform.
