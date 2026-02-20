#!/usr/bin/env python3
"""
RNG Sweep: Statistical Testing of Random Circuits

Implements the RNG_TEST_PLAN.md:
- Sweep over (n, m) = (wires, gates) configurations
- For each config, run R replicates with different seeds
- Generate bitstream via local_mixing_bin rng-stream (temp file, auto-deleted)
- Run dieharder tests INDIVIDUALLY against that file (one invocation per test)
- Aggregate pass rates and estimate m*(n) thresholds

Usage:
    python scripts/rng_sweep.py --mode quick    # 7 core tests, R=5, ~30min
    python scripts/rng_sweep.py --mode medium   # 15 tests, R=10, ~3h
    python scripts/rng_sweep.py --mode full     # all 30 tests, R=10, ~12h+
"""

import os
import sys
import json
import subprocess
import tempfile
import re
import argparse
import time
from datetime import datetime
from dataclasses import dataclass, asdict
from typing import List, Dict, Optional

# ============================================================================
# CONFIGURATION
# ============================================================================

DATE = datetime.now().strftime("%Y-%m-%d")
SUITE_NAME = "rng_sweep"
BASE_DIR = os.path.join("experiments", DATE, SUITE_NAME)

# Local dieharder path (built from source)
DIEHARDER_PATH = "../dieharder-3.31.1/dieharder/dieharder"

CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]

# All usable dieharder test IDs (excludes 14 = "Do Not Use")
ALL_TESTS = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 17,
    100, 101, 102,
    200, 201, 202, 203, 204, 205, 206, 207, 208, 209,
]

# Core tests: good reliability, reasonable data requirements, diverse coverage
CORE_TESTS = [
    0,   # birthdays -- spacing/clustering
    2,   # rank 32x32 -- linear dependence
    3,   # rank 6x8 -- finer linear structure
    8,   # count-the-1s (stream) -- bit frequency
    15,  # runs -- sequential structure
    100, # STS monobit -- basic frequency
    200, # RGB bit distribution -- positional bias
]

# Extended tests: core + more diehard + STS serial + RGB extras
EXTENDED_TESTS = CORE_TESTS + [
    1,   # OPERM5 -- ordering bias
    4,   # bitstream -- sparse bit patterns
    10,  # parking lot -- multi-dimensional uniformity
    11,  # min distance 2D -- spatial clustering
    13,  # squeeze -- floating-point uniformity
    101, # STS runs
    102, # STS serial -- pattern frequency
    205, # byte distribution -- byte-level bias
]

# Sampling configurations per mode
SWEEP_CONFIGS = {
    "quick": {
        "wires": [32, 48],
        "gates": [100, 200, 500, 1000, 2000],
        "replicates": 5,
        "samples": 50_000_000,    # ~191MB for 32 wires
        "burn_in": 1000,
        "dieharder_tests": CORE_TESTS,
        "max_weak": 1,
    },
    "medium": {
        "wires": [32, 48, 64],
        "gates": [100, 200, 500, 1000, 2000, 5000],
        "replicates": 10,
        "samples": 50_000_000,
        "burn_in": 1000,
        "dieharder_tests": EXTENDED_TESTS,
        "max_weak": 1,
    },
    "full": {
        "wires": [32, 48, 64],
        "gates": [100, 200, 500, 1000, 2000, 5000],
        "replicates": 10,
        "samples": 100_000_000,   # ~381MB for 32 wires
        "burn_in": 10000,
        "dieharder_tests": ALL_TESTS,
        "max_weak": 0,
    },
}


@dataclass
class TestResult:
    """Result from a single dieharder test."""
    test_name: str
    test_id: int
    ntup: int
    tsamples: int
    psamples: int
    p_value: float
    assessment: str  # PASSED, WEAK, FAILED


@dataclass
class ReplicateResult:
    """Result from a single (wires, gates, seed) run."""
    wires: int
    gates: int
    seed: int
    mode: str
    tests: List[TestResult]
    num_passed: int
    num_weak: int
    num_failed: int
    overall_pass: bool
    duration_sec: float
    stream_bytes: int = 0
    error: Optional[str] = None


@dataclass
class ConfigResult:
    """Aggregated result for a (wires, gates) configuration."""
    wires: int
    gates: int
    replicates: List[ReplicateResult]
    pass_rate: float
    total_passed: int
    total_failed: int


# ============================================================================
# DIEHARDER OUTPUT PARSING
# ============================================================================

def parse_dieharder_output(output: str, test_id: int) -> List[TestResult]:
    """
    Parse dieharder stdout to extract test results.
    Some tests produce multiple result lines (e.g., different ntup values).
    """
    results = []

    pattern = re.compile(
        r'^\s*(\S+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([\d.eE+-]+)\s*\|\s*(PASSED|WEAK|FAILED)\s*$',
        re.MULTILINE
    )

    for match in pattern.finditer(output):
        test_name = match.group(1)
        ntup = int(match.group(2))
        tsamples = int(match.group(3))
        psamples = int(match.group(4))
        p_value = float(match.group(5))
        assessment = match.group(6)

        results.append(TestResult(
            test_name=test_name,
            test_id=test_id,
            ntup=ntup,
            tsamples=tsamples,
            psamples=psamples,
            p_value=p_value,
            assessment=assessment,
        ))

    return results


# ============================================================================
# CORE FUNCTIONS
# ============================================================================

def log(msg: str):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {msg}", flush=True)


def ensure_dir(path: str):
    os.makedirs(path, exist_ok=True)


def generate_stream(wires, gates, samples, seed, mode, burn_in, out_path, cwd):
    """Generate a bitstream file using rng-stream."""
    cmd = CARGO_RUN + [
        "rng-stream",
        "--wires", str(wires),
        "--gates", str(gates),
        "--samples", str(samples),
        "--seed", str(seed),
        "--mode", mode,
        "--burn-in", str(burn_in),
        "--out", out_path,
    ]

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        cwd=cwd,
        timeout=600,
    )

    if result.returncode != 0:
        raise RuntimeError(f"rng-stream failed: {result.stderr}")


def run_dieharder_test(test_id, file_path, dieharder_path):
    """
    Run a SINGLE dieharder test against a file.
    Each test gets its own invocation so it can read the full file.
    """
    cmd = [dieharder_path, "-g", "201", "-f", file_path, "-d", str(test_id)]

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=600,
    )

    return parse_dieharder_output(result.stdout, test_id)


def run_single_replicate(
    wires: int,
    gates: int,
    seed: int,
    samples: int,
    burn_in: int,
    mode: str,
    dieharder_tests: List[int],
    max_weak: int,
    dieharder_path: str,
) -> ReplicateResult:
    """
    Run a single replicate:
    1. Generate stream to a temp file
    2. Run each dieharder test individually against that file
    3. Collect all results
    4. Delete the temp file
    """
    start = time.time()
    cwd = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    with tempfile.NamedTemporaryFile(suffix=".bin", delete=False) as tmp:
        tmp_path = tmp.name

    try:
        # Step 1: generate the bitstream file
        generate_stream(wires, gates, samples, seed, mode, burn_in, tmp_path, cwd)
        stream_bytes = os.path.getsize(tmp_path)

        # Step 2: run each test individually
        all_tests = []
        for test_id in dieharder_tests:
            try:
                results = run_dieharder_test(test_id, tmp_path, dieharder_path)
                all_tests.extend(results)
                for r in results:
                    log(f"      test {test_id:>3d} ({r.test_name}): p={r.p_value:.6f} {r.assessment}")
            except subprocess.TimeoutExpired:
                log(f"      test {test_id:>3d}: TIMEOUT (skipped)")
            except Exception as e:
                log(f"      test {test_id:>3d}: ERROR ({e})")

        duration = time.time() - start

        num_passed = sum(1 for t in all_tests if t.assessment == "PASSED")
        num_weak = sum(1 for t in all_tests if t.assessment == "WEAK")
        num_failed = sum(1 for t in all_tests if t.assessment == "FAILED")

        overall_pass = (num_failed == 0) and (num_weak <= max_weak)

        return ReplicateResult(
            wires=wires,
            gates=gates,
            seed=seed,
            mode=mode,
            tests=all_tests,
            num_passed=num_passed,
            num_weak=num_weak,
            num_failed=num_failed,
            overall_pass=overall_pass,
            duration_sec=duration,
            stream_bytes=stream_bytes,
        )

    except Exception as e:
        duration = time.time() - start
        return ReplicateResult(
            wires=wires,
            gates=gates,
            seed=seed,
            mode=mode,
            tests=[],
            num_passed=0,
            num_weak=0,
            num_failed=0,
            overall_pass=False,
            duration_sec=duration,
            error=str(e),
        )

    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)


def run_config(
    wires: int,
    gates: int,
    config: dict,
    base_seed: int = 42,
) -> ConfigResult:
    """Run all replicates for a (wires, gates) configuration."""
    replicates = []

    for i in range(config["replicates"]):
        seed = base_seed + i * 1000 + wires * 100 + gates

        log(f"  Replicate {i+1}/{config['replicates']} (seed={seed})")

        result = run_single_replicate(
            wires=wires,
            gates=gates,
            seed=seed,
            samples=config["samples"],
            burn_in=config["burn_in"],
            mode=config.get("mode", "iterate"),
            dieharder_tests=config["dieharder_tests"],
            max_weak=config["max_weak"],
            dieharder_path=DIEHARDER_PATH,
        )

        status = "PASS" if result.overall_pass else "FAIL"
        log(f"    {status} ({result.num_passed}P/{result.num_weak}W/{result.num_failed}F, "
            f"{len(result.tests)} tests) [{result.duration_sec:.1f}s]")

        replicates.append(result)

    total_passed = sum(1 for r in replicates if r.overall_pass)
    total_failed = len(replicates) - total_passed
    pass_rate = total_passed / len(replicates) if replicates else 0.0

    return ConfigResult(
        wires=wires,
        gates=gates,
        replicates=replicates,
        pass_rate=pass_rate,
        total_passed=total_passed,
        total_failed=total_failed,
    )


def estimate_m_star(results: List[ConfigResult], threshold: float = 0.95) -> Dict[int, Optional[int]]:
    """Estimate m*(n) = min{ m : pass_rate(n, m) >= threshold } for each width n."""
    by_wires: Dict[int, List[ConfigResult]] = {}
    for r in results:
        by_wires.setdefault(r.wires, []).append(r)

    m_star = {}
    for wires, configs in sorted(by_wires.items()):
        configs.sort(key=lambda c: c.gates)
        m_star[wires] = None
        for c in configs:
            if c.pass_rate >= threshold:
                m_star[wires] = c.gates
                break

    return m_star


def format_summary_table(results: List[ConfigResult], m_star: Dict[int, Optional[int]]) -> str:
    """Format results as a text table."""
    by_wires: Dict[int, List[ConfigResult]] = {}
    for r in results:
        by_wires.setdefault(r.wires, []).append(r)

    lines = []
    lines.append("=" * 70)
    lines.append("RNG SWEEP RESULTS SUMMARY")
    lines.append("=" * 70)
    lines.append("")

    all_gates = sorted(set(r.gates for r in results))
    header = f"{'wires':<8}" + "".join(f"{g:>8}" for g in all_gates) + f"  {'m*(n)':>8}"
    lines.append(header)
    lines.append("-" * len(header))

    for wires in sorted(by_wires.keys()):
        configs = {c.gates: c for c in by_wires[wires]}
        row = f"{wires:<8}"
        for g in all_gates:
            if g in configs:
                rate = configs[g].pass_rate
                row += f"{rate:>7.0%} "
            else:
                row += f"{'---':>8}"
        m = m_star.get(wires)
        row += f"  {m if m else '---':>8}"
        lines.append(row)

    lines.append("")
    lines.append("Legend: pass_rate = (circuits passing all tests) / (total replicates)")
    lines.append("m*(n) = minimum gates where pass_rate >= 95%")
    lines.append("")

    return "\n".join(lines)


# ============================================================================
# MAIN
# ============================================================================

def main():
    parser = argparse.ArgumentParser(description="RNG Sweep for Random Circuits")
    parser.add_argument(
        "--mode",
        choices=["quick", "medium", "full"],
        default="quick",
        help="Sweep mode (quick=7 core tests, medium=15 tests, full=all 30)"
    )
    parser.add_argument(
        "--wires",
        type=int,
        nargs="+",
        help="Override wire widths to test"
    )
    parser.add_argument(
        "--gates",
        type=int,
        nargs="+",
        help="Override gate counts to test"
    )
    parser.add_argument(
        "--replicates",
        type=int,
        help="Override number of replicates per config"
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        help="Override output directory"
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.95,
        help="Pass rate threshold for m*(n) estimation"
    )
    parser.add_argument(
        "--stream-mode",
        choices=["iterate", "random-input", "counter"],
        default=None,
        help="Override bitstream generation mode (iterate, random-input, or counter)"
    )

    args = parser.parse_args()

    config = SWEEP_CONFIGS[args.mode].copy()

    if args.wires:
        config["wires"] = args.wires
    if args.gates:
        config["gates"] = args.gates
    if args.replicates:
        config["replicates"] = args.replicates
    if args.stream_mode:
        config["mode"] = args.stream_mode

    output_dir = args.output_dir or BASE_DIR
    ensure_dir(output_dir)

    dh_full_path = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        DIEHARDER_PATH
    )
    if not os.path.exists(dh_full_path):
        print(f"ERROR: dieharder not found at {dh_full_path}")
        print("Please build dieharder first: cd dieharder-3.31.1 && ./configure && make")
        sys.exit(1)

    num_tests = len(config["dieharder_tests"])
    stream_mode = config.get("mode", "iterate")
    log(f"Starting RNG Sweep ({args.mode} mode)")
    log(f"Output: {output_dir}")
    log(f"Wires: {config['wires']}, Gates: {config['gates']}")
    log(f"Replicates: {config['replicates']}, Tests per replicate: {num_tests}")
    log(f"Samples: {config['samples']:,}, Stream mode: {stream_mode}")
    log(f"Max file size: ~{config['samples'] * max(config['wires']) // 8 // 1_000_000}MB (temp, auto-deleted)")
    log("")

    all_results = []
    total_configs = len(config["wires"]) * len(config["gates"])
    config_idx = 0

    for wires in config["wires"]:
        for gates in config["gates"]:
            config_idx += 1
            log(f"[{config_idx}/{total_configs}] Config: wires={wires}, gates={gates}")

            result = run_config(wires, gates, config)
            all_results.append(result)

            log(f"  Pass rate: {result.pass_rate:.0%} ({result.total_passed}/{result.total_passed + result.total_failed})")
            log("")

    m_star = estimate_m_star(all_results, threshold=args.threshold)

    summary_text = format_summary_table(all_results, m_star)
    print(summary_text)

    results_data = {
        "date": DATE,
        "mode": args.mode,
        "stream_mode": stream_mode,
        "config": {k: v for k, v in config.items() if k != "dieharder_tests"},
        "dieharder_test_ids": config["dieharder_tests"],
        "num_tests": num_tests,
        "threshold": args.threshold,
        "m_star": {str(k): v for k, v in m_star.items()},
        "results": [
            {
                "wires": r.wires,
                "gates": r.gates,
                "pass_rate": r.pass_rate,
                "total_passed": r.total_passed,
                "total_failed": r.total_failed,
                "replicates": [
                    {
                        "seed": rep.seed,
                        "mode": rep.mode,
                        "overall_pass": rep.overall_pass,
                        "num_passed": rep.num_passed,
                        "num_weak": rep.num_weak,
                        "num_failed": rep.num_failed,
                        "num_tests": len(rep.tests),
                        "duration_sec": rep.duration_sec,
                        "stream_bytes": rep.stream_bytes,
                        "error": rep.error,
                        "tests": [asdict(t) for t in rep.tests],
                    }
                    for rep in r.replicates
                ],
            }
            for r in all_results
        ],
    }

    results_path = os.path.join(output_dir, "rng_sweep_results.json")
    with open(results_path, "w") as f:
        json.dump(results_data, f, indent=2)

    summary_path = os.path.join(output_dir, "rng_sweep_summary.txt")
    with open(summary_path, "w") as f:
        f.write(summary_text)

    log(f"Results saved to {results_path}")
    log(f"Summary saved to {summary_path}")

    log("")
    log("m*(n) estimates (minimum gates for 95% pass rate):")
    for wires, m in sorted(m_star.items()):
        log(f"  n={wires}: m*={m if m else 'not reached'}")


if __name__ == "__main__":
    main()
