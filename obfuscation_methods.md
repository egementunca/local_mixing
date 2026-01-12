# Obfuscation Methods & Options

This document outlines the available obfuscation strategies in `local_mixing`, their current status (hardcoded vs configurable), and where they are defined in the codebase.

## 1. Shooting (`shoot_random_gate`)
Injects `P * P^-1` identity pairs (random gates followed by their inverse) into the circuit to increase entropy and separate existing components.
*   **Status**: **Hardcoded**.
*   **Location**: `src/replace/mixing.rs` inside `abutterfly_big`.
    *   **Global Shooting**: 500,000 attempts at the start of the round.
    *   **Block Shooting**: 1,000 attempts per gate-block.
*   **CLI Option**: None (currently).

## 2. Ancilla Expansion / Big Blocks (`expand_big` / `compress_big`)
Expands the circuit state to use more wires (ancillas) to find simplifications or obfuscations that wouldn't be possible in the original width, then compresses it back down.
*   **Status**: **Enabled by default** in `abbutterfly` and `bbutterfly`.
*   **Location**: `src/replace/mixing.rs` inside `abutterfly_big`.
*   **CLI Flags**:
    *   `--sat` (`-S`): Uses SAT solver for compression (`compress_big_sat`) instead of the standard heuristic (`compress_big`).

## 3. Single Gate Replacements (`random_gate_replacements`)
Replaces a single gate with a longer equivalent identity sequence (e.g., replacing a Toffoli with a sequence of gates that equals Toffoli).
*   **Status**: **Disabled** (Commented out).
*   **Location**: `src/replace/replace.rs` inside `replace_pairs`.
    *   Line 1852: `// random_gate_replacements(...)` is commented out.
*   **CLI Option**: None.

## 4. Pair Replacement (`replace_pairs`)
Finds pairs of gates (like `A ... A^-1`) and replaces the inner section with a different identity (random ID "friends" from the rainbow table).
*   **Status**: **Enabled** in `abbutterfly` and `bbutterfly`.
*   **Location**: `src/replace/mixing.rs`.
*   **Method**: `replace_pairs` iterates through the circuit, finds inverses, and swaps the identity sequence between them with a new random one of the same length provided by `random_id`.

## 5. Reverse Obfuscation (`reverse` command)
The issue mentions testing if patterns are due to circuit structure by mixing "in reverse".
*   **Status**: **Available** as `reverse` subcommand.
*   **Location**: `src/main.rs`.
*   **Function**: Reads a circuit, reverses the gate order (and inverts gates if needed), and saves it. Useful for checking if "red beam" artifacts appear at the "start" (new end) of the circuit.

## 5. Butterfly Variants (The Subcommands)

### A. `butterfly` (Standard Butterfly)
The original implementation.
*   **Method**: `main_butterfly`
*   **Process**:
    1.  Repeatedly replaces `A ... A^-1` pairs with `B ... B^-1` (where B is a different random id).
    2.  Compresses the result.
*   **Limitation**: Operates strictly within the original wire count (no ancillas).

### B. `bbutterfly` (Big Butterfly)
The "Big" version that uses ancillas (extra wires) to find more simplifications.
*   **Method**: `butterfly_big` (called via `main_butterfly_big` with `asymmetric=false`).
*   **Process**:
    1.  **Shoots** random gates.
    2.  **Replaces Pairs** (`replace_pairs`).
    3.  **Expands** blocks of gates to use more wires (`expand_big`).
    4.  **Compresses** these expanded blocks (`compress_big`).
    5.  Repeats for N rounds.

### C. `abbutterfly` (Asymmetric Big Butterfly)
The strongest/current recommended version. "Asymmetric" refers to how it handles the mixing to avoid symmetry artifacts (like the "red beam").
*   **Method**: `abutterfly_big` (called via `main_butterfly_big` with `asymmetric=true`).
*   **Key Feature**: Includes the full "Big" pipeline but orchestrated differently to break symmetries.
    *   **Bookendless Mode (`-b`)**: Calls `main_butterfly_big_bookendsless`, which uses `abutterfly_big_delay_bookends`. Skips initial/final randomizations.
    *   **SAT Mode (`-S`)**: Uses a SAT solver instead of heuristics for the compression step.
