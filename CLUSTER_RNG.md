# RNG Sweep: Cluster Deployment Guide (BU SCC)

Statistical testing of random circuits as pseudorandom generators via dieharder.
All tests use **counter mode** (C(0), C(1), ..., C(k)) — the correct single-application PRP test.

## Overview

We test whether a random circuit C on n wires, with m gates (gate 57: `x_a ^= x_b OR NOT x_c`),
acts as a pseudorandom permutation by feeding its outputs to the dieharder test battery.
The key quantity is **m\*(n)**: the minimum gate count where all dieharder tests pass consistently.

Each (n, m, seed) triple is independent — embarrassingly parallel, ideal for SGE array jobs.

## 1. Environment Setup

```bash
# On BU SCC login node
cd ~

# Clone repositories (if not already present)
git clone <repo-url> research-group
cd research-group/local_mixing

# Build the Rust binary
module load rust   # or ensure cargo is available
cargo build --release
# Binary: target/release/local_mixing_bin

# Verify it works
./target/release/local_mixing_bin rng-stream --wires 32 --gates 100 --samples 10 --seed 42 --mode counter | xxd | head

# Build dieharder from source
cd ~
git clone <dieharder-repo> dieharder
cd dieharder
./configure --prefix=$HOME/dieharder-install
make -j4
# Binary: dieharder/dieharder (or wherever it lands)
# Verify:
./dieharder/dieharder -l   # should list all tests
```

## 2. Test Plan

### Parameters

| Parameter | Value | Notes |
|-----------|-------|-------|
| Stream mode | `counter` | Single-application PRP test (always) |
| Test suite | `quick` (7 core tests) | IDs: 0,2,3,8,15,100,101 |
| Replicates | 100 per (n,m) point | Different seed = different random circuit |
| Burn-in | 1000 | Discarded initial outputs |
| Max weak | 1 | Allow 1 WEAK result per replicate |
| Pipe mode | always | No temp files, unlimited data |

### Dieharder Tests Used (quick mode)

| ID | Test Name | What it checks |
|----|-----------|----------------|
| 0  | diehard_birthdays | Spacing/clustering in random numbers |
| 2  | diehard_rank_32x32 | Linear dependence (matrix rank) |
| 3  | diehard_rank_6x8 | Finer linear structure |
| 8  | diehard_count_1s_str | Bit frequency bias |
| 15 | diehard_runs | Sequential structure (2 p-values) |
| 100 | sts_monobit | Basic frequency (NIST) |
| 101 | sts_runs | Run-length structure (NIST) |

### Gate Count Ranges by Width

Based on local n=32 results: m\*(32) is between 300 and 500 in counter mode.
We estimate m\*(n) scales roughly as O(n * f(n)), so larger widths need more gates.

**Phase 1: Coarse scan** — find the approximate transition for each width.

| Width (n) | Gate counts to test | Est. wall time/replicate | Rationale |
|-----------|-------------------|--------------------------|-----------|
| 32 | 200, 250, 300, 350, 400, 450, 500, 600 | ~3 min | Transition known at 300-500, dense scan |
| 48 | 300, 400, 500, 600, 700, 800, 1000, 1200 | ~5 min | Expected transition ~500-1000 |
| 64 | 400, 600, 800, 1000, 1200, 1500, 2000, 2500 | ~7 min | Expected transition ~800-1500 |
| 96 | 600, 1000, 1500, 2000, 2500, 3000, 4000, 5000 | ~10 min | Expected transition ~1500-3000 |
| 128 | 1000, 1500, 2000, 3000, 4000, 5000, 6000, 8000 | ~15 min | Expected transition ~2000-5000 |

**Total Phase 1 jobs:** 5 widths x 8 gate counts x 100 replicates = **4,000 jobs**

Each job: 1 core, 4GB RAM, ~15 min worst case. Wall time: request `01:00:00` for safety.

**Phase 2 (if needed):** After Phase 1, refine around the transition with denser gate counts.
For example, if n=64 transitions between 1000-1200, add gate counts 1050, 1100, 1150.

### Smart Stopping

Once all 100 replicates pass at some gate count m, the transition is below m.
We still test higher gate counts to confirm, but you can stop a width early if:
- The 3 highest gate counts all show 100% pass rate
- This gives confidence that m\*(n) has been found

## 3. Submitting Jobs

### Generate and Submit (per width)

It's recommended to submit **separate jobs per width** so you can monitor and stop early.

```bash
cd ~/research-group/local_mixing

BINARY="$HOME/research-group/local_mixing/target/release/local_mixing_bin"
DIEHARDER="$HOME/dieharder/dieharder"  # adjust path
SCRIPT="$HOME/research-group/local_mixing/scripts/rng_sweep.py"

# --- Width 32 (dense scan around known transition) ---
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 32 \
    --gates 200 250 300 350 400 450 500 600 \
    --replicates 100 \
    --binary-path "$BINARY" \
    --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w32 \
    --time 01:00:00 --memory 4G \
    --job-name rng_w32

# --- Width 48 ---
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 48 \
    --gates 300 400 500 600 700 800 1000 1200 \
    --replicates 100 \
    --binary-path "$BINARY" \
    --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w48 \
    --time 01:00:00 --memory 4G \
    --job-name rng_w48

# --- Width 64 ---
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 64 \
    --gates 400 600 800 1000 1200 1500 2000 2500 \
    --replicates 100 \
    --binary-path "$BINARY" \
    --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w64 \
    --time 01:00:00 --memory 4G \
    --job-name rng_w64

# --- Width 96 ---
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 96 \
    --gates 600 1000 1500 2000 2500 3000 4000 5000 \
    --replicates 100 \
    --binary-path "$BINARY" \
    --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w96 \
    --time 02:00:00 --memory 4G \
    --job-name rng_w96

# --- Width 128 ---
python3 scripts/rng_sweep.py submit \
    --mode quick --stream-mode counter \
    --wires 128 \
    --gates 1000 1500 2000 3000 4000 5000 6000 8000 \
    --replicates 100 \
    --binary-path "$BINARY" \
    --dieharder-path "$DIEHARDER" \
    --script-path "$SCRIPT" \
    --results-dir /scratch/$USER/rng_sweep_w128 \
    --time 02:00:00 --memory 4G \
    --job-name rng_w128
```

### Dry Run First

Add `--dry-run` to any submit command to generate the scripts without submitting:
```bash
python3 scripts/rng_sweep.py submit ... --dry-run
# Then inspect the generated files:
ls /scratch/$USER/rng_sweep_w32/
# submit_sweep.sh  task_manifest.tsv  sweep_config.json  logs/  results/
```

### Monitor Jobs

```bash
qstat -u $USER                              # List all your jobs
qstat -u $USER | grep rng_w32               # Filter by width
tail -f /scratch/$USER/rng_sweep_w32/logs/*  # Follow logs
ls /scratch/$USER/rng_sweep_w32/results/ | wc -l  # Count completed tasks
```

## 4. Collecting Results

After jobs complete (or while they run for partial results):

```bash
# Collect per-width results
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w48
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w64
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w96
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w128

# Each produces: rng_sweep_results.json + rng_sweep_summary.txt

# Check for missing tasks (and get resubmit command):
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32 --require-complete
# If tasks are missing, it prints: "Resubmit: qsub -t 42,85-87 submit_sweep.sh"
```

### Merge All Widths (for combined plotting)

To create a single results JSON with all widths, use the merge script:

```bash
python3 scripts/merge_rng_results.py \
    /scratch/$USER/rng_sweep_w32/rng_sweep_results.json \
    /scratch/$USER/rng_sweep_w48/rng_sweep_results.json \
    /scratch/$USER/rng_sweep_w64/rng_sweep_results.json \
    /scratch/$USER/rng_sweep_w96/rng_sweep_results.json \
    /scratch/$USER/rng_sweep_w128/rng_sweep_results.json \
    --output /scratch/$USER/rng_sweep_all.json
```

## 5. Plotting

```bash
# Plot per-width results
python3 scripts/plot_rng_sweep.py --results /scratch/$USER/rng_sweep_w32/rng_sweep_results.json

# Plot combined (all widths on one graph)
python3 scripts/plot_rng_sweep.py --results /scratch/$USER/rng_sweep_all.json

# Output: pass_rate_vs_gates.png, per_test_pass_rate_w*.png, pvalues_scatter.png, etc.
```

## 6. Results Tracking

### Expected Output Structure

```
/scratch/$USER/
  rng_sweep_w32/
    sweep_config.json       # Sweep parameters (auto-generated)
    task_manifest.tsv       # Task ID -> (wires, gates, seed, rep_idx)
    submit_sweep.sh         # SGE job script (auto-generated)
    logs/                   # Per-task SGE logs
    results/                # Per-replicate JSON files
      w32_g200_r0.json      # One file per (wires, gates, replicate)
      w32_g200_r1.json
      ...
    rng_sweep_results.json  # Aggregated results (after collect)
    rng_sweep_summary.txt   # Text summary table
  rng_sweep_w48/
    ...
  rng_sweep_all.json        # Combined all-width results (after merge)
```

### Result File Schema (per replicate)

```json
{
  "wires": 32,
  "gates": 500,
  "seed": 42,
  "replicate_index": 0,
  "mode": "counter",
  "overall_pass": true,
  "num_passed": 8,
  "num_weak": 0,
  "num_failed": 0,
  "num_tests": 8,
  "duration_sec": 199.1,
  "tests": [
    {"test_name": "diehard_birthdays", "test_id": 0, "p_value": 0.143, "assessment": "PASSED"},
    ...
  ]
}
```

### What Success Looks Like

For each width n, the collected results should show:
- **0% pass rate** at low gate counts (circuit too shallow to mix)
- **Sharp transition** over a narrow range of gate counts
- **100% pass rate** at high gate counts (circuit is pseudorandom)

The transition point m\*(n) is where pass rate first hits 95%+.

## 7. Troubleshooting

### "Binary not found"
Ensure the binary path in submit is absolute: `$HOME/research-group/local_mixing/target/release/local_mixing_bin`

### "dieharder not found"
Build dieharder from source and point to the built binary (not an installed one).

### Task failures
Check logs: `cat /scratch/$USER/rng_sweep_w32/logs/rng_w32.*.42.log`
Common issues: timeout (increase `--time`), memory (increase `--memory`).

### Resubmit failed tasks
```bash
# collect will report missing tasks with the exact resubmit command:
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32 --require-complete
# Output: "Resubmit: qsub -t 42,85-87 /scratch/$USER/rng_sweep_w32/submit_sweep.sh"
```

### Partial results
You can collect and plot partial results while jobs are still running:
```bash
python3 scripts/rng_sweep.py collect --results-dir /scratch/$USER/rng_sweep_w32
# Will warn about missing tasks but proceed with available data
```
