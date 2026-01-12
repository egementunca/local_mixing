# Local Mixing — Identity Obfuscation Plan (Rich MD Spec)

This document rewrites and organizes the “right-side” sketch into a clean, implementable plan for a coding agent.

---

## 0) Problem Statement

You want to **generate large identity circuits** (e.g., **64 wires**) and then **obfuscate** them so that:

- the circuit is still an **identity permutation** overall
- obvious cancellations are hidden
- a reducer/attacker that uses local rules + template lookup has a hard time compressing it

In parallel, you want an **attack-style reducer** that tries to simplify circuits so you can measure “how obfuscated” a generated identity is.

---

## 1) Key Objects and Terms

### Identity circuit
A circuit whose overall function is the identity permutation on all wires.

### Template identity
A **known identity subcircuit** stored in a database (template lookup table).

### Window
A contiguous subsequence of gates (length `L`) extracted from a big circuit, used for matching templates.

### Active wire set (size `m`)
The set of wires that appear in gates inside a window (or template).  
`m = |{wires touched by gates in window}|`

This matters because searching templates on smaller `m` is much cheaper.

### Canonicalization
A deterministic mapping of a window (or template) into a canonical form so that matching works even if wire IDs differ.

### Reducer
A simplification engine that tries to shrink the circuit (delete identities) using:
- local rules (swap/commute + cancel)
- window scanning + template matching (delete matched templates)

### Obfuscator
A generator that applies moves to **make reduction difficult**, while preserving identity.

---

## 2) High-Level Strategy

You have **two interacting engines**:

1. **Reducer engine (attacker model):**  
   tries to compress / detect hidden templates inside a large circuit.

2. **Obfuscator engine (defender/generator):**  
   starts from an identity and repeatedly rewrites it to hide cancellations and templates.

You evaluate obfuscation by running the reducer on the obfuscated identity and measuring how much it compresses.

---

## 3) Core Identity Construction

A robust baseline for building nontrivial identities:

### 3.1 Identity from a circuit and its inverse
1. sample a nontrivial circuit `P` (depth > 1)
2. build:
   - `ID = P · inv(P)`

### 3.2 More “random-looking” identity
To avoid immediate adjacency structure:
- apply independent randomizations to the forward and inverse sides:

`ID = P^(1) · inv(P^(2))`

Where `P^(1)` and `P^(2)` are “equivalent in function” but differ syntactically via:
- wire relabeling / placement policies
- internal rewrite moves that preserve function
- insertion of identity templates that later get diffused

This gives you structured identities that can be hard to spot.

---

## 4) Reducer Engine Spec

The reducer is the **baseline attacker** that tries to simplify circuits.

### 4.1 Reducer pipeline
The reducer runs repeated rounds until no progress:

1. **Local pass (cheap):**
   - cancel adjacent inverse pairs
   - apply commutation (swap) rules to expose cancellations
   - do basic gate-normalization (if your basis supports it)

2. **Window scan (more expensive):**
   - slide a window across the circuit
   - for each window:
     - compute canonical key
     - query template DB
     - if exact match: **delete the entire window** (replace with nothing)

3. repeat until fixed point

### 4.2 “m schedule” (your “first m width” idea)
Instead of scanning for all templates at once:
- start with low active-wire count templates first

Example schedule:
- pass 1: only templates with `m ≤ 4`
- pass 2: `m ≤ 6`
- pass 3: `m ≤ 8`
- … (stop at a budget)

This makes the search scalable and models realistic attackers.

### 4.3 Optional witness prefilter (scales window scanning)
To avoid expensive canonicalization for every window:
- store short “witness hashes” in the template DB
- quickly check if a window contains any witness
- only then do full canonical match

This is consistent with your “witness” notes.

---

## 5) Template Database Spec

You already have a template lookup table. This is what it should store and expose.

### 5.1 Stored fields per template
- `template_id`
- `gate_list` (in your gate basis: e.g. ECA57 / Toffoli / etc.)
- `m_active_wires`
- `canonical_hash` (the main lookup key)
- optional:
  - `witness_hashes`: short subsequence hashes
  - `symmetry_variants`: if you support more than canonical relabeling

### 5.2 Query types
#### (A) Exact match
Input: window canonical hash  
Output: list of matching templates (often 0 or 1)

This is the baseline, simplest, most reliable.

#### (B) Approximate match (future)
Input: window features  
Output: closest template candidates by a distance metric

You mentioned “closest by something (Hamming?)” — this is a later stage.

---

## 6) Canonicalization (Critical for Matching)

You need matching to be wire-label invariant.

### 6.1 Canonical window key algorithm
Given a window (list of gates):
1. extract touched wires: `W = sorted(unique(wires in window))`
2. relabel wires to `[0..m-1]` deterministically:
   - e.g. `W[0] -> 0`, `W[1] -> 1`, ...
3. rewrite every gate’s operands under this relabeling
4. serialize gate sequence into a stable representation
5. hash the serialization -> `canonical_hash`

**Result:** windows match templates even if they appear on different physical wires.

### 6.2 Important constraint
This canonicalization does *not* search over all permutations — it just normalizes the wires *as they appear*.
That avoids factorial explosion (which your notes flagged).

---

## 7) Obfuscator Engine Spec

The obfuscator generates identities that are hard for the reducer to compress.

### 7.1 Obfuscator pipeline
1. Start from a structured identity:
   - `ID = P · inv(P)` (baseline)
   - or `P^(1) · inv(P^(2))` (stronger)

2. Repeat for N steps:
   - apply “hiding moves” that preserve identity
   - aim to:
     - spread cancellations apart
     - diffuse template structure
     - touch many wires (64-wire coverage goal)

3. Output an identity circuit `ID_obf`

### 7.2 Allowed move categories (basis-independent wording)
- **Commutation moves:** swap two gates when they are independent under your gate semantics.
- **Cancellation moves:** remove adjacent inverse pairs.
- **Template insertion:** insert a known identity template into the circuit.
- **Diffusion:** after insertion, apply commutations to spread the inserted structure so it doesn’t remain as a clean block.

**Note:** The exact commutation conditions depend on your gate basis (ECA57 vs Toffoli etc.).

---

## 8) Metrics (How you Measure Obfuscation)

You implicitly want a “quality of obfuscation” metric.
A clean baseline is:

### 8.1 Compression ratio under attacker
Let:
- `len_before = gate_count(ID_obf)`
- `len_after = gate_count(Reducer(ID_obf))`

Define:
- `compression_ratio = len_after / len_before`

Interpretation:
- closer to **1.0** = harder to reduce = better obfuscation (under this attacker)
- smaller value = easier to reduce = weaker obfuscation

### 8.2 Attack trace metrics
Collect:
- number of template deletions found
- smallest `m` level that still yields deletions
- number of reducer rounds before convergence
- distribution of where deletions occur (clustered vs spread)

### 8.3 Interaction graph metrics (your “skeleton/edges” note)
Optional but aligned with your sketch:

Build a graph:
- nodes = wires
- each gate adds hyperedge / pairwise edges between touched wires

Track:
- edge density
- connected component sizes
- “distance” distribution if wires are ordered (range of interactions)
- how these correlate with reducer success

---

## 9) SAT Repair Extension (Future, matches your later messages)

You suggested:
> if I don’t have the exact permutation for a subcircuit I picked to replace, select closest then synthesize rest incrementally by SAT.

This is a second-stage pipeline:

### 9.1 Approximate replacement + repair
1. choose a target window `W` to replace
2. pick a template `T` that is “closest” (by some distance)
3. replace `W` with `T` (circuit may no longer be identity)
4. synthesize a compensator `R` so that the global circuit returns to identity:
   - solve `C · R = ID` on boundary wires / limited scope
   - SAT constraints enforce allowed depth/width budget

This is powerful but more complex; implement after the exact-match pipeline is stable.

---

## 10) Concrete Implementation TODOs for an Agent

### Phase 1 — baseline system
1. Implement `CanonicalWindowKey(window) -> hash`
2. Implement `TemplateDB` indexed by `canonical_hash`, grouped by `m`
3. Implement `Reducer`:
   - `local_pass_swap_cancel()`
   - `window_scan_exact_match_delete(m_schedule)`
4. Implement `Obfuscator` baseline:
   - build `P · inv(P)`
   - insert templates + diffusion passes
5. Add metrics:
   - compression ratio
   - deletion counts
   - reducer round counts

### Phase 2 — scaling improvements
6. Add witness prefilter
7. Add interaction-graph stats
8. Add approximate matching + SAT repair (optional)

---

## 11) What the Agent Needs From You (Minimal Clarifications)

To implement correctly, the agent must know:

1. **Gate basis definitions**
   - how to compute inverse of a gate
   - what “independent gates commute” means in your semantics

2. **Template format**
   - how templates are stored now (file? sqlite? serialized rust structs?)

3. **Window parameters**
   - typical window lengths you want to scan
   - m-schedule (max active wires to try)

You can keep everything else as engineering choices.

---

## 12) Output Deliverables

After implementation, you should have:

- `obfuscate_identity_64w(...) -> Circuit`
- `reduce_circuit(...) -> Circuit + trace`
- metrics report per generated identity:
  - compression ratio
  - deletion counts
  - runtime
  - graph stats (optional)

This supports systematic experiments on “how hard to simplify” under your attacker model.

---