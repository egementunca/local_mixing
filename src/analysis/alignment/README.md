# Dynamic Time Warping (DTW) for Circuit Alignment

This module implements **Dynamic Time Warping (DTW)** to quantify the structural similarity between two Boolean circuits. Unlike simple gate-by-gate comparison (which fails if a single gate is inserted or deleted), DTW finds the optimal *non-linear* alignment between the two gate sequences.

## Core Concept in Context

In circuit obfuscation, we often insert "junk" gates or unrelated sub-circuits. This shifts the indices of the original gates.
*   **Circuit 1 (Original)**: `[G1, G2, G3]`
*   **Circuit 2 (Obfuscated)**: `[Junk, G1, Junk, G2, G3]`

A direct index-based comparison (Heatmap diagonal) would fail to see that `G1` in Circuit 1 matches the 2nd gate in Circuit 2. DTW resolves this by allowing the time axis to "warp": it matches `G1` (index 0) to `G1` (index 1), effectively skipping the junk.

## Algorithm Breakdown

### 1. State Tracing (`trace_states`)
Instead of comparing gate operators directly, we compare the **functional state** of the circuit after each gate.
*   We generate a set of random inputs (e.g., 64 random u64 integers for 64 inputs).
*   We simulate the circuit gate-by-gate.
*   **Trace**: A vector where `trace[i]` is the state of all wires after the $i$-th gate.

### 2. Distance Matrix (`distance_matrix`)
We compute a matrix $D$ of size $(M+1) \times (N+1)$, where $M, N$ are the gate counts of the two circuits.
*   $D[i, j]$ represents the **dissimilarity** (Distance) between the state after gate $i$ of Circuit 1 and the state after gate $j$ of Circuit 2.
*   **Metric**: Absolute Correlation Distance.
    *   First, we calculate normalized Hamming Distance $d \in [0, 1]$.
    *   Then we calculate Absolute Correlation $s = |1 - 2d| \in [0, 1]$.
    *   Finally, Distance $D[i,j] = 1 - s$.
    *   Distance is 0.0 when identical or inverse (Perfect Correlation).
    *   Distance is 1.0 when uncorrelated (Random).

### 3. Alignment via Dynamic Programming (`dtw_alignment`)
We find a path from $(0,0)$ to $(M,N)$ through matrix $D$ that minimizes the cumulative cost.
*   **DP State**: `dp_cost[i, j]` = Min cost to reach state $(i,j)$.
*   **Transitions**:
    *   **Diagonal**: Match $i$ with $j$. (Cost: `prev + D[i,j] + penalty_diag`)
    *   **Vertical**: Gap in Circuit 2 (Circuit 1 advanced, Circuit 2 stayed). (Cost: `prev + D[i,j] + penalty_hv`)
    *   **Horizontal**: Gap in Circuit 1. (Cost: `prev + D[i,j] + penalty_hv`)

```rust
// Simplified recurrence
dp[i][j] = D[i][j] + min(
    dp[i-1][j-1],    // Diagonal (Match)
    dp[i-1][j] + P,  // Vertical (Insert/Skip)
    dp[i][j-1] + P   // Horizontal (Insert/Skip)
)
```

## Relationship to Heatmap

You may notice that **Step 1 (Tracing)** and **Step 2 (Distance Matrix)** are identical to what the **Heatmap** tool does.
*   **Heatmap**: Visualizes the Distance Matrix $D$ directly. It gives you a "landscape" view of similarity.
*   **Alignment**: Takes that same matrix $D$ and finds the **optimal path** through the landscape. It quantifies the structural preservation with a single score ($c^*$) and identifies exactly *which* gate in C1 corresponds to *which* gate in C2.

## Example Walkthrough

Let's align two simple 1-wire circuits.
*   **C1**: `NOT(0)`
*   **C2**: `ID(0); NOT(0)` (Identity then NOT)

**Input**: `0`.

**Traces**:
*   **C1 Trace**:
    *   $t=0$ (Start): `0`
    *   $t=1$ (After NOT): `1`
*   **C2 Trace**:
    *   $t=0$ (Start): `0`
    *   $t=1$ (After ID): `0`
    *   $t=2$ (After NOT): `1`

**Distance Matrix ($D$)**:
| C1 \ C2 | t=0 (0) | t=1 (0) | t=2 (1) |
| :--- | :---: | :---: | :---: |
| **t=0 (0)** | **0.0** | 0.0 | 1.0 |
| **t=1 (1)** | 1.0 | 1.0 | **0.0** |

**Optimal Path**:
1.  **(0, 0)**: Both start at `0`. Cost: 0.
2.  **(0, 1)**: C1 stays at $t=0$, C2 advances to $t=1$ (ID gate). State is still `0` vs `0`. Cost += 0.
3.  **(1, 2)**: Both advance. C1 does `NOT` -> `1`. C2 does `NOT` -> `1`. Match! Cost += 0.

**Result**: The alignment correctly identifies that C2's second gate corresponds to C1's first gate, identifying the inserted Identity gate as a "stutter" or insertion.

## Usage in Code

```rust
use local_mixing::alignment::{trace_states, distance_matrix, dtw_alignment};

// 1. Generate traces
let trace1 = trace_states(&c1, &inputs);
let trace2 = trace_states(&c2, &inputs);

// 2. Compute similarity matrix
let d = distance_matrix(&trace1, &trace2, num_wires);

// 3. Compute structural alignment
// Penalties: diag=0.1 (prefer matching), hv=0.5 (discourage gaps)
let result = dtw_alignment(&d, 0.5, 0.1);

println!("Alignment Cost: {}", result.c_star);
```
