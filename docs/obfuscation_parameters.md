# Hidden Obfuscation Parameters in `local_mixing`

This document catalogs the "hidden" or hardcoded parameters identified in the codebase that affect obfuscation and compression.

## `src/replace/mixing.rs`

### `abbutterfly_big` / `butterfly`
| Parameter | Default Value | Location | Description & Impact |
|-----------|---------------|----------|----------------------|
| **Structure Block Size** | `10..=30` (abbutterfly)<br>`3..=25` (butterfly) | `random_id(n, rng.random_range(...))` | **Description:** Size of the random identity circuit ($R \cdot R^{-1}$) wrapping each gate/block.<br>**Impact:** Larger values increase "depth" of obfuscation, hiding original structure better, but result in significantly larger final circuits. |
| **Single Gate Replacements** | `500` | `random_gate_replacements(..., 500, ...)` | **Description:** Number of single gates replaced with equivalent subcircuits *before* the main pass.<br>**Impact:** Increases initial entropy. High verify count prevents trivial pattern matching on the original circuit structure before heavy mixing begins. |
| **Shooting 2** | `1_000` | `shoot_random_gate(..., 1_000)` | **Description:** Random gate insertions applied *inside* each block during processing.<br>**Impact:** Adds noise within the protected $R \cdot R^{-1}$ shell. This noise is "local" and easier to remove if not properly entangled by subsequent merges. |
| **Compression Window** | `100` (normal)<br>`10` (SAT mode) | `expand_big(..., 100, ...)` | **Description:** Window size for peephole optimization.<br>**Impact:** Larger windows find more reductions (smaller output) but are computationally expensive ($O(L \cdot 2^k)$ or similar). Small windows leave more redundancy. |
| **SAT Limit** | `1000` | `compress_big_sat(..., 1000)` | **Description:** Timeout/conflict limit for SAT solver.<br>**Impact:** Higher limits allow the solver to prove more complex equivalences, potentially finding better compressions for hard blocks, at the cost of runtime. |
| **Final Stability** | `12` (was 6) | `while stable_count < 12` | **Description:** Consecutive passes with no change required to stop.<br>**Impact:** Higher threshold ensures we don't stop prematurely in a local minimum, squeezing out every last bit of redundancy. Critical for final circuit size. |
| **Chunk Split Logic** | `1500` (base) | `(before + 1499) / 1500` | **Description:** Denominator for splitting circuit into chunks.<br>**Impact:** Smaller base = more chunks = more parallelism but less global optimization context. |

## `src/local.rs` (Mixing & Reduction)

| Parameter | Default Value | Location | Description & Impact |
|-----------|---------------|----------|----------------------|
| **Mix Move Probabilities** | 40% Tmpl, 50% Swap, 10% Patch | `mix_step` | **Description:** Probabilities for mix moves.<br>**Impact:** High Swap% = High diffusion (scrambled wires). High Template% = Hides structure with identities. High Patch% = Inserts long-range dependencies. |
| **Swap Burst Count** | `10..100` | `mix_step` (Move B) | **Description:** Number of swaps per burst.<br>**Impact:** Higher values aggressively "bubble" gates through the circuit, rapidly destroying local structure. |
| **Patch-Cancel Size** | `k` in `3..=8` | `mix_step` (Move D) | **Description:** Number of wires in $R \dots R^{-1}$.<br>**Impact:** Wider patches tangle more wires together, making it harder to isolate components. |
| **Reducer Windows** | `[4, .. 16]` | `pass_template_delete` | **Description:** Window sizes scanned for identity.<br>**Impact:** Attack parameter. Checking larger windows is exponentially more expensive ($2^k$ states) but finds larger replaceable redundancies. |

## `src/replace/replace.rs` (Compression)

| Parameter | Default Value | Location | Description |
|-----------|---------------|----------|-------------|
| **Max Window (n=4)** | `6` | `compress_lmdb` | Dynamic max window size based on wire count `n`. |
| **Max Window (n=5,6)** | `5` | `compress_lmdb` | Dynamic max window size based on wire count `n`. |
| **Max Window (n=7)** | `4` | `compress_lmdb` | Dynamic max window size based on wire count `n`. |
| **Max Window (n>7)** | `10` | `compress_lmdb` | Default max window size for larger wire counts. |
| **Subcircuit Shift** | `0..4` (1,2,4,8) | `random_subcircuit` | Log-scale random length selection for subcircuits. |

## `src/obfuscate/passes.rs`

| Parameter | Default Value | Location | Description |
|-----------|---------------|----------|-------------|
| **Gadget Size** | `cfg.gadget_size / 2` | `gadget_size.max(3)` | Size of commutator gadgets injected at boundaries. |
| **Mixer Depth** | `5` | `generate_quality_mixer(..., 5, ...)` | Depth of the mixing circuit used for conjugation (currently disabled). |
| **Noise Gadget Size** | `cfg.gadget_size / 3` | `gadget_size.max(2)` | Size of noise gadgets injected to reach target overhead. |

## `src/main.rs` (CLI Defaults)

| Parameter | Default Value | Description |
|-----------|---------------|-------------|
| **Shooting** | `500000` | Main "shooting" intensity (random gate insertions) applied at the start. |
| **Level** | `3` | Obfuscation level (1-5), often unused in `abbutterfly` but used in `obfuscate`. |
