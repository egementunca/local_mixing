# RNG Sweep: Cluster Deployment Guide (BU SCC)

Statistical testing of random circuits as pseudorandom generators via dieharder and NIST STS.

Two standard block cipher modes (NIST SP 800-38A):
- **CTR (Counter) mode**: `C(0), C(1), ..., C(k)` — single application per output. The harder PRP test. **Primary.**
- **OFB (Output Feedback) mode**: `IV → C(IV) → C(C(IV)) → ...` — cumulative re-application (= iterate mode). Easier test, used in the USE report. **Secondary, for comparison.**

PRP (Pseudorandom Permutation) is the *property* being tested, not a mode.

## Overview

We test whether a random circuit C on n wires, with m gates (gate 57: `x_a ^= x_b OR NOT x_c`),
acts as a pseudorandom permutation by feeding its outputs to statistical test batteries.
The key quantity is **m\*(n)**: the minimum gate count where all tests pass consistently.

Each (n, m, seed) triple is independent — embarrassingly parallel, ideal for SGE array jobs.

---

## Phase 1: Coarse Scan (7 core dieharder tests, counter mode) — COMPLETE

### Results (R=100 per point, counter mode, pipe mode)

| Width | gates | Pass rate | | gates | Pass rate | | gates | Pass rate | | gates | Pass rate |
|-------|-------|-----------|---|-------|-----------|---|-------|-----------|---|-------|-----------|
| **32** | 200 | 0% | | 250 | 0% | | 300 | 1% | | 350 | 9% |
| | 400 | 39% | | 450 | 72% | | 500 | 90% | | 600 | 97% |
| **48** | 300 | 0% | | 400 | 0% | | 500 | 3% | | 600 | 32% |
| | 700 | 67% | | 800 | 91% | | 1000 | 95% | | 1200 | 97% |
| **64** | 400 | 0% | | 600 | 0% | | 800 | 17% | | 1000 | 74% |
| | 1200 | 96% | | 1500 | 98% | | 2000 | 99% | | 2500 | 98% |
| **96** | 600 | 0% | | 1000 | 2% | | 1500 | 71% | | 2000 | 98% |
| | 2500 | 95% | | 3000 | 99% | | 4000 | 100% | | 5000 | 98% |
| **128** | 1000 | 0% | | 1500 | 6% | | 2000 | 73% | | 3000 | 100% |
| | 4000 | 100% | | 5000 | 100% | | 6000 | 100% | | 8000 | 100% |

### m\*(n) Estimates (95% threshold)

| Width (n) | m\*(n) | Gates/wire | Transition region |
|-----------|--------|------------|-------------------|
| 32 | ~600 | 18.8 | 350–600 |
| 48 | ~1000 | 20.8 | 600–1000 |
| 64 | ~1200 | 18.8 | 800–1200 |
| 96 | ~2000 | 20.8 | 1500–2000 |
| 128 | ~3000 | 23.4 | 2000–3000 |

**Observations**: Scaling is roughly m\*(n) ≈ 20n. Pass rates plateau at 95–99% even at high gate
counts (stochastic noise at R=100 with max_weak=1). The bottleneck test is consistently
`sts_monobit` — it's the last to pass in the transition region.

---

## Phase 2: Transition Refinement (denser gate counts) — TODO

Add gate counts within the identified transition regions to pin down m\*(n) precisely.

| Width (n) | New gate counts | Rationale |
|-----------|----------------|-----------|
| 32 | 525, 550, 575 | Between 500 (90%) and 600 (97%) |
| 48 | 650, 750, 850, 900, 950 | Between 600 (32%) and 1000 (95%) |
| 64 | 900, 1100 | Between 800 (17%) and 1200 (96%) |
| 96 | 1750, 2250 | Around 2000 (98%) |
| 128 | 2250, 2500, 2750 | Between 2000 (73%) and 3000 (100%) |

**Total Phase 2 jobs:** ~14 new gate counts × 100 replicates = **1,400 jobs**

```bash
# Example: refine w32 transition
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 32 --gates 525 550 575 \
    --replicates 100 \
    --binary-path "$BINARY" --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w32_p2 \
    --time 01:00:00 --memory 4G --job-name rng_w32_p2
```

---

## Phase 3: Full Dieharder Battery (all good tests, counter mode) — TODO

Phase 1 used only 7 core tests (IDs: 0,2,3,8,15,100,101). The USE report (Chamon et al.)
ran the **complete dieharder battery** (~114 individual tests across 27 test families).
We need to verify that the remaining tests don't shift m\*(n).

### Test Coverage

Run all dieharder tests with "Good" reliability. Exclude:
- ID 5 (diehard_opso) — Suspect
- ID 6 (diehard_oqso) — Suspect
- ID 7 (diehard_dna) — Suspect
- ID 14 (diehard_sums) — Do Not Use

The script's `--mode full` already uses `ALL_TESTS` which includes all 27 good test families.

### What to Run

Not every (n, m) point needs the full battery. Run at **3 strategic gate counts per width**:
one below transition, one at transition, one well above.

| Width | Below | At transition | Above |
|-------|-------|---------------|-------|
| 32 | 400 | 550 | 800 |
| 48 | 600 | 850 | 1200 |
| 64 | 800 | 1100 | 1500 |
| 96 | 1500 | 2000 | 3000 |
| 128 | 2000 | 2750 | 4000 |

**R=20 replicates** per point (sufficient to check if any new test shifts m\*).

**Total Phase 3 jobs:** 5 widths × 3 gates × 20 replicates = **300 jobs**

**Estimated wall time per job:** Full battery takes ~30–60 min (vs ~10 min for 7 tests).
Request `--time 02:00:00`.

```bash
python3 scripts/rng_sweep.py submit \
    --mode full --stream-mode counter \
    --wires 32 --gates 400 550 800 \
    --replicates 20 \
    --binary-path "$BINARY" --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_full_w32 \
    --time 02:00:00 --memory 4G --job-name rng_full_w32
```

---

## Phase 4: NIST STS (SP 800-22) — TODO

The USE report also ran the full **NIST Statistical Test Suite** (188 tests across 15 categories),
producing "% of Accepted Sequences" plots comparable to AES evaluation. We should replicate
this for direct comparison.

### NIST STS Tests (15 categories, 188 p-values)

| # | Test | p-values | Notes |
|---|------|----------|-------|
| 1 | Frequency (Monobit) | 1 | Also in dieharder (100) |
| 2 | Frequency Within a Block | 1 | NOT in dieharder |
| 3 | Runs | 1 | Also in dieharder (101) |
| 4 | Longest Run of Ones | 1 | NOT in dieharder |
| 5 | Binary Matrix Rank | 1 | Similar to dieharder (2,3) |
| 6 | Discrete Fourier Transform (Spectral) | 1 | NOT in dieharder |
| 7 | Non-overlapping Template Matching | 148 | NOT in dieharder |
| 8 | Overlapping Template Matching | 1 | NOT in dieharder |
| 9 | Maurer's Universal Statistical | 1 | NOT in dieharder |
| 10 | Linear Complexity | 1 | NOT in dieharder |
| 11 | Serial | 2 | Also in dieharder (102) |
| 12 | Approximate Entropy | 1 | NOT in dieharder |
| 13 | Cumulative Sums | 2 | NOT in dieharder |
| 14 | Random Excursions | 8 | NOT in dieharder |
| 15 | Random Excursions Variant | 18 | NOT in dieharder |

Tests **NOT in dieharder**: 2, 4, 6, 7, 8, 9, 10, 12, 13, 14, 15 — these are why we need NIST STS separately.

### Setup

```bash
# Build NIST STS from source (on cluster login node)
cd ~
wget https://csrc.nist.gov/CSRC/media/Projects/Random-Bit-Generation/documents/sts-2_1_2.zip
unzip sts-2_1_2.zip
cd sts-2.1.2
make
# Binary: ./assess
```

### Workflow per (n, m, seed)

1. Generate binary data: 100 sequences × 1M bits = 100M bits = 12.5 MB
```bash
local_mixing_bin rng-stream --wires 32 --gates 500 --seed 42 \
    --samples 3125000 --mode counter --out data.bin
# 3,125,000 samples × 32 bits = 100M bits = 100 sequences × 1M bits
```

2. Run NIST STS (scripted, non-interactive):
```bash
printf "0\ndata.bin\n1\n100\n1000000\n0\n" | ./assess 1000000
# Reads: file input, filename, all tests, 100 streams, 1M bits each, binary format
```

3. Parse `experiments/AlgorithmTesting/finalAnalysisReport.txt`

### What to Run

Same strategic gate counts as Phase 3 (3 per width, R=20). Only need to run at and above
the transition — NIST STS on a clearly-failing circuit isn't informative.

| Width | At transition | Above |
|-------|---------------|-------|
| 32 | 550 | 800 |
| 48 | 850 | 1200 |
| 64 | 1100 | 1500 |
| 96 | 2000 | 3000 |
| 128 | 2750 | 4000 |

**Total: 10 configurations × 20 replicates = 200 jobs**

### Plots to Generate

Replicate the USE report style:
- **"% of Accepted Sequences" per NIST test** — scatter plot with test number on x-axis
  (like their Figure on page 5). Shows proportion of sequences passing each of 188 tests.
  Add threshold line at 96.92% (NIST minimum).
- **Comparison with AES** — side-by-side plot (if we run AES through the same pipeline)

---

## Phase 5: Iterate Mode (OFB) Comparison — TODO

The professor's group tested their cipher in **OFB mode** (output feedback), which is our
**iterate mode**: `x → C(x) → C(C(x)) → ...`. Running iterate mode at the same configurations
lets us:
1. Show that iterate mode gives a lower (easier) threshold than counter mode
2. Directly compare with the professor's methodology
3. Confirm our counter mode is the harder, more meaningful test

### What to Run

Same gate counts as Phase 1, but with `--stream-mode iterate`. Only need R=20
(the comparison is qualitative, not precision-critical).

```bash
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode iterate \
    --wires 32 --gates 200 300 400 500 600 \
    --replicates 20 \
    --binary-path "$BINARY" --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_iterate_w32 \
    --time 01:00:00 --memory 4G --job-name rng_iter_w32
```

Repeat for all 5 widths. **Total: ~40 gate counts × 20 replicates = 800 jobs**

---

## Environment Setup

### Rust Binary

```bash
cd ~/research-group/local_mixing
module load rust
cargo build --release
# Binary: target/release/local_mixing_bin (supports 1-128 wires)

# Verify:
./target/release/local_mixing_bin rng-stream --wires 32 --gates 100 \
    --samples 10 --seed 42 --mode counter | xxd | head
```

### Dieharder

```bash
cd ~
git clone <dieharder-repo> dieharder
cd dieharder
./configure --prefix=$HOME/dieharder-install
make -j4
# Binary: dieharder/dieharder
./dieharder/dieharder -l   # should list all 27+ tests
```

### NIST STS (Phase 4)

```bash
cd ~
wget https://csrc.nist.gov/CSRC/media/Projects/Random-Bit-Generation/documents/sts-2_1_2.zip
unzip sts-2_1_2.zip
cd sts-2.1.2
make
# Binary: ./assess
# Test: echo -n "test" | ./assess 32
```

---

## Job Submission Reference

### Common Variables

```bash
BINARY="$HOME/research-group/local_mixing/target/release/local_mixing_bin"
DIEHARDER="$HOME/dieharder/dieharder"
SCRIPT="$HOME/research-group/local_mixing/scripts/rng_sweep.py"
```

### Submit Commands by Phase

See each phase section above for specific commands. General pattern:

```bash
python3 scripts/rng_sweep.py submit \
    --mode {quick|full} \
    --stream-mode {counter|iterate} \
    --wires W1 W2 ... \
    --gates G1 G2 ... \
    --replicates R \
    --binary-path "$BINARY" --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/DIRNAME \
    --time HH:MM:SS --memory 4G --job-name NAME
```

Add `--dry-run` to generate scripts without submitting.

### Monitor and Collect

```bash
qstat -u $USER                                    # List all jobs
ls /scratch/$USER/rng_sweep_w32/results/ | wc -l  # Count completed tasks

# Collect results:
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32

# Check for missing tasks:
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32 --require-complete

# Merge widths:
python3 scripts/merge_rng_results.py \
    w32/rng_sweep_results.json w48/rng_sweep_results.json ... \
    --output rng_sweep_all.json

# Plot:
python3 scripts/plot_rng_sweep.py --results rng_sweep_all.json
```

---

## Job Summary

| Phase | What | Jobs | Est. wall time | Status |
|-------|------|------|----------------|--------|
| 1 | Coarse scan (7 tests, counter) | 4,000 | 1–2 hr | **COMPLETE** |
| 2 | Transition refinement (7 tests, counter) | 1,400 | 1 hr | TODO |
| 3 | Full dieharder battery (27 tests, counter) | 300 | 2 hr | TODO |
| 4 | NIST STS (188 tests, counter) | 200 | 1 hr | TODO |
| 5 | Iterate mode comparison (7 tests, iterate) | 800 | 1 hr | TODO |
| **Total** | | **6,700** | | |

---

## Troubleshooting

### "Binary not found"
Ensure absolute path: `$HOME/research-group/local_mixing/target/release/local_mixing_bin`

### "dieharder not found"
Build dieharder from source and point to the built binary.

### Task failures
Check logs: `cat /scratch/$USER/rng_sweep_w32/logs/rng_w32.*.42.log`
Common issues: timeout (increase `--time`), memory (increase `--memory`).

### Resubmit failed tasks
```bash
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32 --require-complete
# Output: "Resubmit: qsub -t 42,85-87 /scratch/$USER/rng_sweep_w32/submit_sweep.sh"
```

### Full dieharder times out
Some tests in the full battery (e.g., rgb_lagged_sum with high ntup) are very data-hungry.
Request `--time 03:00:00` if 2hr isn't enough. The worker has a 1200s per-test timeout.
