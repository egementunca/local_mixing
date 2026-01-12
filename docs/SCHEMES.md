# Mixing Schemes Documentation

This document details the two primary obfuscation schemes in the `local_mixing` framework: the **Original Scheme (Asymmetric Butterfly)** and the **New Scheme (Annealed Obfuscator)**.

---

## 1. Original Scheme: Asymmetric Butterfly
**Implementation**: `algorithms/butterfly/mixing.rs` (originally `replace/mixing.rs`)

The **Butterfly** scheme is a hierarchical, block-based approach designed to obfuscate large circuits by recursively wrapping them in random "noise" layers and compressing the result.

### Core Architecture
1.  **Splitting**: The input circuit $C$ is divided into $k$ parallel blocks $B_1, \dots, B_k$.
2.  **Wrapping (The "Butterfly" Step)**:
    Each block $B_i$ is wrapped with a random reversible circuit $R_i$ and its inverse $R_i^{-1}$ (or $R_{i+1}$ for chaining).
    $$ B'_i \leftarrow R_i^{-1} \cdot B_i \cdot R_{i+1} $$
    The boundaries are handled such that the $R$'s telescope (cancel out) when blocks are concatenated, preserving the global function $F(C)$.

3.  **Local Compression**:
    Each wrapped block $B'_i$ acts as a "window". The system computes its canonical signature and queries the **Rainbow Table**.
    *   If a smaller equivalent $S_i \equiv B'_i$ is found, $B'_i$ is replaced by $S_i$.
    *   Crucially, since $R_i$ is random, $B'_i$ likely has no trivial identity pattern, but *inside* it lurks the original logic $B_i$. The compressor tries to "crunch" the combined entropy.

4.  **Merging**:
    Compressed blocks are merged pairwise. The process repeats hierarchically until a single circuit remains.

### Use Case
*   **Status**: Legacy / Baseline.
*   **Pros**: Highly parallelizable; good for bulk "entropy injection".
*   **Cons**: Can be brittle; adjacent $R \cdot R^{-1}$ pairs might be easily detected if simple compression is run without context.

---

## 2. New Scheme: Annealed Obfuscator
**Implementation**: `algorithms/annealing/anneal.rs` (originally `obfuscation/anneal.rs`)

The **Annealed Obfuscator** treats obfuscation as an optimization problem: *Find the circuit $C'$ such that $C' \equiv C$ and $Cost(Attacker(C'))$ is maximized.*

### Core Architecture
It uses **Simulated Annealing** (Metropolis-Hastings algorithm) to navigate the space of equivalent circuits.

#### A. The Energy Function (Adversarial Score)
We define the "Energy" $E(C)$ of a circuit as its resistance to reduction.
$$ E(C) \approx \text{Steps\_To\_Reduce}(C) - \text{Size\_Reduction}(C) $$
*   A "High Energy" circuit is one that the `Reducer` (attacker) struggles to simplify.
*   The `Reducer` (Attacker Model) runs fast, local reduction passes (Cancellation, Template Matching).

#### B. The Moves (`algorithms/annealing/local.rs`)
The annealer proposes stochastic updates to the circuit:
1.  **Move A (Template Insertion)**:
    *   Pick a random position $i$.
    *   Insert an Identity Template $T = P \cdot P^{-1}$.
    *   *Adversarial Goal*: Choose $P$ such that it "tangles" with neighbors (anti-commutes), preventing trivial removal.
2.  **Move B (Commuting Swaps)**:
    *   Identify adjacent gates $g_i, g_{i+1}$ that commute ($g_i \cdot g_{i+1} = g_{i+1} \cdot g_i$).
    *   Swap them.
    *   *Goal*: Diffuse gates physically to hide their logical relationships.
3.  **Move D (Patch Pairs)**:
    *   Insert $R \dots R^{-1}$ where the two halves are separated by distance $d$.
    *   *Goal*: Create non-local identities that local window reducers cannot see.

### Process Flow
1.  Start with circuit $C_{current}$.
2.  Propose $C_{proposal} = \text{Move}(C_{current})$.
3.  Calculate $\Delta E = E(C_{proposal}) - E(C_{current})$.
4.  **Acceptance**:
    *   If $\Delta E > 0$ (Obfuscation improved): Accept.
    *   If $\Delta E \le 0$: Accept with probability $e^{\Delta E / T}$, where $T$ is the "Temperature".
5.  Cool down $T$ over time.

### Use Case
*   **Status**: Active Research.
*   **Pros**: Produces "Sticky" noise; targets specific weaknesses of the reduction tool.
*   **Cons**: Slower (requires running the reducer in the loop).

---

## 3. Comparison

| Feature | Butterfly (Original) | Annealing (New) |
| :--- | :--- | :--- |
| **Approach** | Constructive (Wrap & Squash) | Search-based (Optimization) |
| **Randomness** | Global structure injection | derived from Local Moves |
| **Guidance** | Unguided (Random R) | Guided (Energy Function) |
| **Parallelism** | High (Block-based) | Low (Sequential MCMC) |
| **Output** | "Crunchy" (Compressed but messy) | "Tangled" (Structurally resistant) |
