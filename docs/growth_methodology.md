# Identity Growth Methodology and Experimental Setup
This document outlines the methodology used for generating "d-grown" identity circuits and explains the configuration parameters for the "Hard" vs. "Baseline" experiments.
## 1. D-Growth Methodology
The "d-growth" (Depth Growth) algorithm iteratively builds a large identity circuit by layering smaller, local identity structure and diffusing it. The goal is to create circuits that are functionally identity ($C = I$) but structurally complex and resistant to simple compression.
### The Process (per Round)
The algorithm proceeds in 3 stages for $R$ rounds:
#### Stage A: Template Placement
*   **Goal**: Inject standardized units of identity.
*   **Mechanism**:
    *   **Selection**: A "Template" (small identity circuit on 3-6 wires) is sampled.
    *   **Rewiring**: The template is mapped to a subset of the target $W$ wires.
    *   **Guidance (Skeleton Graph)**: The subset of wires is chosen using a **Skeleton Graph** to ensure balanced coverage. It prefers wires that have been used less frequently (min-degree), preventing "hotspots" and ensuring global entanglement.
#### Stage B: Diffusion
*   **Goal**: Spread the local structure globally.
*   **Mechanism**: "Shooting" random gates.
    *   Pairs of cancelling gates ($g \cdot g^{-1}$) are inserted at random positions.
    *   They are then commuted (swapped with neighbors) to separate the $g$ from its inverse $g^{-1}$.
    *   This "smears" the boundaries of the placed templates, making them harder to isolate.
#### Stage C: Bounded Compression
*   **Goal**: Harden the circuit.
*   **Mechanism**:
    *   A compressor (reducer) is run with a limited budget.
    *   **Survival Check**: If the circuit compresses *too much* (e.g., reduces to < 30% of its size), the round is rejected as "too weak."
    *   This evolutionary pressure ensures that only "robust" structure accumulates over time.
---
## 2. Where do Basis Templates come from?
The templates are small identity circuits (e.g., 3-6 wires) that serve as the building blocks. We use a **Mixed Source**:
1.  **Permutation Tables (`PermTables`)**:
    *   Generated on the fly from the permutation database (`local_mixing/db`).
    *   These are canonical identities derived from raw permutation data.
    *   *Advantage*: Mathematically pure, high variety.
2.  **Template Database (`TemplateDB`)**:
    *   Stored in `local_mixing/data/collection.lmdb`.
    *   These are pre-computed, known-good identities (often found from previous searches or optimizations).
    *   *Advantage*: specific structures known to be useful.
The `Mixed` mode alternates between these two sources to provide both variety and structure.
---
## 3. What is the "Hard" Configuration?
The "Hard" configuration (`03_conj4_8_hard80`) adds two critical layers of defense to make the generated identities much more difficult to reverse-engineer or compress.
### A. Template Conjugation ($C \cdot T \cdot C^{-1}$)
*   **Baseline**: Inserts template $T$ directly.
*   **Hard**: Wraps $T$ in a random circuit $C$ (depth 4-8).
    *   Result: $C \cdot T \cdot C^{-1}$.
    *   This is still an identity, but the core structure $T$ is "hidden" behind the scrambling layer $C$. The optimizer must first unravel $C$ to see the simple $T$ inside.
### B. Pre-Placement Hardness Checks
*   **Baseline**: Accepts any valid template.
*   **Hard**: "Audits" the template before using it.
    *   It runs a quick reduction pass on the candidate template.
    *   **Criterion**: If the template simplifies by more than 20% (Ratio < 0.80), it is **rejected**.
    *   Only templates that are *intrinsically difficult to simplify* are added to the circuit.
### Comparisons
| Parameter | Baseline | Hard | Effect |
| :--- | :--- | :--- | :--- |
| **Max Template Gates** | 30 | 40 | Larger blocks are harder to match. |
| **Conjugation** | None | Depth 4-8 | Hides template boundaries ($C \cdot T \cdot C^{-1}$). |
| **Hardness Check** | None | Ratio > 0.80 | Rejects fragile identities. |
| **Passes** | 0 | 2 | Double-checks hardness. |
This results in a circuit that is resistant to standard local optimization (window-based reduction) because every individual piece is designed to be locally optimal (or close to it).
