# Rainbow Table / Canonicalization Engine

This directory contains the core logic for **Circuit Canonicalization**, which is the fundamental mechanism enabling "Rainbow Table" lookups in this project.

## What is a Rainbow Table in this context?

A Rainbow Table is a precomputed database used to reverse a function. In this project, the function is:
> **Circuit -> Canonical ID**

Any two circuits that perform the exact same boolean logic (permutation) are accepted as inputs. The goal is to map them both to a single, unique **Canonical ID**.

Once we have this ID, we can look it up in a database (stored in LMDB, outside this directory) to find the "best" (smallest) implementation of that logic. This allows us to:
1.  Take a messy, obfuscated subcircuit.
2.  Compute its Canonical ID.
3.  Query the DB: "What is the shortest circuit for this ID?"
4.  Replace the messy version with the short one (Compression).

## Implementation: `canonical.rs`

This file implements the hard computational work of finding that unique ID.

### The Problem
A circuit is defined by its gates. However, you can reorder independent gates (that don't share wires) without changing the logic. You can also re-label wires ($w_0 \leftrightarrow w_1$) if you are checking for *structural* equivalence under permutation.

Finding the "standard" form of a circuit under these symmetries is essentially the **Graph Isomorphism** problem.

### The Solution: Candidate Sets & Backtracking
The code uses a `CandSet` (Candidate Set) structure to prune the search space of possible wire permutations.

1.  **`Permutation::canonical()`**: The entry point.
2.  **`fast_canon()`**: Heuristically tries to find the lexicographically smallest truth table (permutation vector) by refining the candidate set of wire mappings.
3.  **`brute_canonical()`**: A fallback that tries every valid wire shuffle if the fast method fails or is ambiguous.
4.  **`CandSet`**: Represents an $N \times N$ boolean matrix where `candidate[i][j]` is true if wire $i$ *could* map to wire $j$. It uses bitwise operations to propagate constraints (e.g., "if wire 0 maps to wire 2, then wire 1 cannot map to wire 2").

### Key Functions
- **`fast_canon`**: The workhorse. It iterates through "strings of weight" (subsets of inputs) to narrow down which input bits can map to which output bits while preserving the circuit's truth table structure.
- **`brute_canonical`**: Slow but guaranteed. Iterates all valid wire permutations to find the one that produces the lexicographically smallest output vector.
- **`CircuitSeq::canonicalize`**: Locally reorders gates within a circuit (bubble sort) to a standard order, respecting non-commutativity (dependencies).

## Summary
`src/rainbow` does not *store* the table. It provides the **Hash Function** (Canonicalization) that allows the rest of the application ("compress", "mix") to use the table effectively.
