#!/usr/bin/env python3
"""
RNG Sweep: Statistical Testing of Random Circuits

Implements the RNG_TEST_PLAN.md:
- Sweep over (n, m) = (wires, gates) configurations
- For each config, run R replicates with different seeds
- Generate bitstream via local_mixing_bin rng-stream
- Run dieharder tests INDIVIDUALLY (one invocation per test)
- Aggregate pass rates and estimate m*(n) thresholds

Subcommands:
    python scripts/rng_sweep.py [local]  --mode quick     # local sequential sweep
    python scripts/rng_sweep.py worker   --wires 32 ...   # single replicate (SGE task)
    python scripts/rng_sweep.py submit   --mode quick ...  # generate SGE array job
    python scripts/rng_sweep.py collect  --results-dir ... # aggregate worker results
"""

import os
import sys
import json
import glob as glob_module
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
# NOTE: Tests requiring -n ntuple (200-204) need special handling and are
# excluded from core. Use -a mode or handle ntuple explicitly.
CORE_TESTS = [
    0,   # birthdays -- spacing/clustering
    2,   # rank 32x32 -- linear dependence
    3,   # rank 6x8 -- finer linear structure
    8,   # count-the-1s (stream) -- bit frequency
    15,  # runs -- sequential structure
    100, # STS monobit -- basic frequency
    101, # STS runs -- run-length structure (NIST variant)
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
        "wires": [32, 48, 64, 96, 128],
        "gates": [200, 300, 500, 750, 1000, 1500, 2000, 3000, 5000],
        "replicates": 100,
        "samples": 50_000_000,    # ~191MB for 32 wires
        "burn_in": 0,             # only relevant for iterate mode
        "dieharder_tests": CORE_TESTS,
        "max_weak": 1,
    },
    "medium": {
        "wires": [32, 48, 64, 96, 128],
        "gates": [200, 300, 500, 750, 1000, 1500, 2000, 3000, 5000],
        "replicates": 100,
        "samples": 50_000_000,
        "burn_in": 0,
        "dieharder_tests": EXTENDED_TESTS,
        "max_weak": 1,
    },
    "full": {
        "wires": [32, 48, 64, 96, 128],
        "gates": [200, 300, 500, 750, 1000, 1500, 2000, 3000, 5000, 8000],
        "replicates": 100,
        "samples": 100_000_000,   # ~381MB for 32 wires
        "burn_in": 0,
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
    WARNING: File mode can cause data reuse (rewinding) if the file is
    too small for a test. Use run_dieharder_pipe for unlimited data.
    """
    cmd = [dieharder_path, "-g", "201", "-f", file_path, "-d", str(test_id)]

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=600,
    )

    return parse_dieharder_output(result.stdout, test_id)


def run_dieharder_pipe(test_id, wires, gates, seed, mode, burn_in,
                       dieharder_path, cwd, gen_cmd_prefix=None):
    """
    Run a SINGLE dieharder test by piping generator output directly.
    No file, no rewind, no size limit -- dieharder draws as many numbers
    as it needs. The generator must produce data faster than dieharder
    consumes it.

    This is the recommended approach since individual tests have very
    different data requirements (5M to 640M+ random numbers).

    gen_cmd_prefix: override for CARGO_RUN (e.g. path to pre-compiled binary).
    """
    prefix = gen_cmd_prefix or CARGO_RUN
    gen_cmd = prefix + [
        "rng-stream",
        "--wires", str(wires),
        "--gates", str(gates),
        "--samples", str(1_000_000_000),  # 1B samples = effectively unlimited
        "--seed", str(seed),
        "--mode", mode,
        "--burn-in", str(burn_in),
    ]

    dh_cmd = [dieharder_path, "-g", "200", "-d", str(test_id)]

    # Pipe: generator -> dieharder
    gen_proc = subprocess.Popen(
        gen_cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=cwd,
    )
    dh_proc = subprocess.Popen(
        dh_cmd,
        stdin=gen_proc.stdout,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    gen_proc.stdout.close()  # Allow gen_proc to receive SIGPIPE

    try:
        dh_stdout, dh_stderr = dh_proc.communicate(timeout=1200)
    except subprocess.TimeoutExpired:
        dh_proc.kill()
        gen_proc.kill()
        raise
    finally:
        gen_proc.terminate()
        gen_proc.wait()

    return parse_dieharder_output(dh_stdout.decode(), test_id)


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
    use_pipe: bool = False,
    gen_cmd_prefix: List[str] = None,
    cwd: str = None,
) -> ReplicateResult:
    """
    Run a single replicate. Two modes:

    File mode (use_pipe=False, legacy):
      1. Generate stream to a temp file
      2. Run each dieharder test individually against that file
      3. Delete the temp file
      WARNING: If the file is too small, dieharder rewinds and results
      are invalid. Need ~3GB for 7 core tests, ~20GB for full battery.

    Pipe mode (use_pipe=True, recommended):
      For each test, pipe the generator directly to dieharder.
      No file, no rewind, no size limit. Each test draws exactly
      the data it needs. The generator re-runs per test (same seed).

    gen_cmd_prefix: override for CARGO_RUN (e.g. pre-compiled binary path).
    cwd: working directory for the generator subprocess.
    """
    start = time.time()
    if cwd is None:
        cwd = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    if use_pipe:
        # Pipe mode: each test gets its own fresh pipe from the generator
        all_tests = []
        for test_id in dieharder_tests:
            try:
                results = run_dieharder_pipe(
                    test_id, wires, gates, seed, mode, burn_in,
                    dieharder_path, cwd, gen_cmd_prefix=gen_cmd_prefix,
                )
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
            wires=wires, gates=gates, seed=seed, mode=mode,
            tests=all_tests, num_passed=num_passed,
            num_weak=num_weak, num_failed=num_failed,
            overall_pass=overall_pass, duration_sec=duration,
            stream_bytes=0,  # no file in pipe mode
        )

    # File mode (legacy)
    with tempfile.NamedTemporaryFile(suffix=".bin", delete=False) as tmp:
        tmp_path = tmp.name

    try:
        generate_stream(wires, gates, samples, seed, mode, burn_in, tmp_path, cwd)
        stream_bytes = os.path.getsize(tmp_path)

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
            wires=wires, gates=gates, seed=seed, mode=mode,
            tests=all_tests, num_passed=num_passed,
            num_weak=num_weak, num_failed=num_failed,
            overall_pass=overall_pass, duration_sec=duration,
            stream_bytes=stream_bytes,
        )

    except Exception as e:
        duration = time.time() - start
        return ReplicateResult(
            wires=wires, gates=gates, seed=seed, mode=mode,
            tests=[], num_passed=0, num_weak=0, num_failed=0,
            overall_pass=False, duration_sec=duration, error=str(e),
        )

    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)


def run_config(
    wires: int,
    gates: int,
    config: dict,
    base_seed: int = 42,
    use_pipe: bool = False,
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
            use_pipe=use_pipe,
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
# CLI HELPERS
# ============================================================================

def _add_sweep_args(parser):
    """Add common sweep arguments shared by local and submit."""
    parser.add_argument(
        "--mode", choices=["quick", "medium", "full"], default="quick",
        help="Sweep mode (quick=7 core tests, medium=15 tests, full=all 30)")
    parser.add_argument("--wires", type=int, nargs="+", help="Override wire widths")
    parser.add_argument("--gates", type=int, nargs="+", help="Override gate counts")
    parser.add_argument("--replicates", type=int, help="Override replicate count")
    parser.add_argument("--threshold", type=float, default=0.95,
                        help="Pass rate threshold for m*(n)")
    parser.add_argument(
        "--stream-mode", choices=["iterate", "random-input", "counter"],
        default=None, help="Override bitstream mode")


def _build_config(args):
    """Build sweep config from args + SWEEP_CONFIGS."""
    config = SWEEP_CONFIGS[args.mode].copy()
    if args.wires:
        config["wires"] = args.wires
    if args.gates:
        config["gates"] = args.gates
    if args.replicates:
        config["replicates"] = args.replicates
    if args.stream_mode:
        config["mode"] = args.stream_mode
    return config


# ============================================================================
# CMD: LOCAL (original sequential sweep)
# ============================================================================

def cmd_local(args):
    """Run the full sweep locally (original behavior)."""
    config = _build_config(args)

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
    if args.pipe:
        log("Data mode: PIPE (generator -> dieharder, no file, no rewind)")
    else:
        log(f"Data mode: FILE (~{config['samples'] * max(config['wires']) // 8 // 1_000_000}MB temp, auto-deleted)")
        log("WARNING: File mode may cause data reuse for data-hungry tests. Use --pipe for reliable results.")
    log("")

    all_results = []
    total_configs = len(config["wires"]) * len(config["gates"])
    config_idx = 0

    for wires in config["wires"]:
        for gates in config["gates"]:
            config_idx += 1
            log(f"[{config_idx}/{total_configs}] Config: wires={wires}, gates={gates}")

            result = run_config(wires, gates, config, use_pipe=args.pipe)
            all_results.append(result)

            log(f"  Pass rate: {result.pass_rate:.0%} ({result.total_passed}/{result.total_passed + result.total_failed})")
            log("")

    m_star = estimate_m_star(all_results, threshold=args.threshold)

    summary_text = format_summary_table(all_results, m_star)
    print(summary_text)

    results_data = _build_results_json(args.mode, stream_mode, config, all_results,
                                       m_star, args.threshold)

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


def _build_results_json(mode_name, stream_mode, config, all_results, m_star, threshold):
    """Build the JSON dict matching plot_rng_sweep.py's expected schema."""
    return {
        "date": DATE,
        "mode": mode_name,
        "stream_mode": stream_mode,
        "config": {k: v for k, v in config.items() if k != "dieharder_tests"},
        "dieharder_test_ids": config["dieharder_tests"],
        "num_tests": len(config["dieharder_tests"]),
        "threshold": threshold,
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


# ============================================================================
# CMD: WORKER (single replicate for SGE array tasks)
# ============================================================================

def cmd_worker(args):
    """Execute a single replicate and write result JSON."""
    config_mode = SWEEP_CONFIGS[args.mode]
    test_ids = (
        [int(x) for x in args.test_ids.split(",")]
        if args.test_ids
        else config_mode["dieharder_tests"]
    )

    # Use the pre-compiled binary directly (no cargo run)
    binary = os.path.abspath(args.binary_path)
    gen_prefix = [binary]

    ensure_dir(args.output_dir)

    log(f"Worker: wires={args.wires}, gates={args.gates}, seed={args.seed}, "
        f"replicate_index={args.replicate_index}")

    result = run_single_replicate(
        wires=args.wires,
        gates=args.gates,
        seed=args.seed,
        samples=config_mode["samples"],
        burn_in=args.burn_in,
        mode=args.stream_mode,
        dieharder_tests=test_ids,
        max_weak=args.max_weak,
        dieharder_path=os.path.abspath(args.dieharder_path),
        use_pipe=True,  # always pipe on cluster
        gen_cmd_prefix=gen_prefix,
        cwd=os.getcwd(),  # cwd doesn't matter for pre-compiled binary
    )

    # Output: w{wires}_g{gates}_r{replicate_index}.json
    out_filename = f"w{args.wires}_g{args.gates}_r{args.replicate_index}.json"
    out_path = os.path.join(args.output_dir, out_filename)

    result_data = {
        "wires": args.wires,
        "gates": args.gates,
        "seed": args.seed,
        "replicate_index": args.replicate_index,
        "mode": args.stream_mode,
        "overall_pass": result.overall_pass,
        "num_passed": result.num_passed,
        "num_weak": result.num_weak,
        "num_failed": result.num_failed,
        "num_tests": len(result.tests),
        "duration_sec": result.duration_sec,
        "stream_bytes": result.stream_bytes,
        "error": result.error,
        "tests": [asdict(t) for t in result.tests],
    }

    with open(out_path, "w") as f:
        json.dump(result_data, f, indent=2)

    status = "PASS" if result.overall_pass else "FAIL"
    log(f"Worker complete: {status} -> {out_path} [{result.duration_sec:.1f}s]")

    if result.error:
        sys.exit(1)


# ============================================================================
# CMD: SUBMIT (generate SGE array job for BU SCC)
# ============================================================================

def cmd_submit(args):
    """Generate and optionally submit an SGE array job for the full sweep."""
    config = _build_config(args)

    ensure_dir(args.results_dir)
    ensure_dir(os.path.join(args.results_dir, "logs"))
    ensure_dir(os.path.join(args.results_dir, "results"))

    stream_mode = config.get("mode", "iterate")
    test_ids_str = ",".join(str(t) for t in config["dieharder_tests"])

    # Build task manifest: one line per (wires, gates, replicate_index)
    # SGE_TASK_ID is 1-indexed, so task_idx starts at 1
    tasks = []
    task_idx = 1
    for wires in config["wires"]:
        for gates in config["gates"]:
            for i in range(config["replicates"]):
                seed = args.base_seed + i * 1000 + wires * 100 + gates
                tasks.append((task_idx, wires, gates, seed, i))
                task_idx += 1

    manifest_path = os.path.join(args.results_dir, "task_manifest.tsv")
    with open(manifest_path, "w") as f:
        f.write("# task_index\twires\tgates\tseed\treplicate_index\n")
        for t in tasks:
            f.write(f"{t[0]}\t{t[1]}\t{t[2]}\t{t[3]}\t{t[4]}\n")

    # Save sweep config for the collect step
    sweep_config_path = os.path.join(args.results_dir, "sweep_config.json")
    with open(sweep_config_path, "w") as f:
        json.dump({
            "mode": args.mode,
            "stream_mode": stream_mode,
            "config": {k: v for k, v in config.items() if k != "dieharder_tests"},
            "dieharder_test_ids": config["dieharder_tests"],
            "threshold": args.threshold,
            "base_seed": args.base_seed,
        }, f, indent=2)

    script_path = args.script_path or os.path.abspath(__file__)
    max_idx = len(tasks)  # SGE is 1-indexed, so range is 1-max_idx

    sge_script = f"""#!/bin/bash -l
#$ -P {args.project}
#$ -N {args.job_name}
#$ -t 1-{max_idx}
#$ -l h_rt={args.time}
#$ -l mem_per_core={args.memory}
#$ -j y
#$ -o {args.results_dir}/logs/$JOB_NAME.$JOB_ID.$TASK_ID.log
#$ -cwd

# ---- RNG Sweep Worker (auto-generated) ----
MANIFEST="{manifest_path}"
TASK_ID=$SGE_TASK_ID

# Read task parameters from manifest (skip comment lines)
LINE=$(awk -v idx="$TASK_ID" '!/^#/ && $1 == idx {{ print; exit }}' "$MANIFEST")
if [ -z "$LINE" ]; then
    echo "ERROR: No manifest entry for TASK_ID=$TASK_ID"
    exit 1
fi

WIRES=$(echo "$LINE" | awk '{{print $2}}')
GATES=$(echo "$LINE" | awk '{{print $3}}')
SEED=$(echo "$LINE" | awk '{{print $4}}')
REP_IDX=$(echo "$LINE" | awk '{{print $5}}')

echo "Task $TASK_ID: wires=$WIRES gates=$GATES seed=$SEED replicate=$REP_IDX"
echo "Started: $(date)"
echo "Node: $(hostname)"

# Check binaries
if [ ! -x "{args.binary_path}" ]; then
    echo "ERROR: Binary not found: {args.binary_path}"
    exit 1
fi
if [ ! -x "{args.dieharder_path}" ]; then
    echo "ERROR: dieharder not found: {args.dieharder_path}"
    exit 1
fi

python3 {script_path} worker \\
    --wires $WIRES \\
    --gates $GATES \\
    --seed $SEED \\
    --replicate-index $REP_IDX \\
    --mode {args.mode} \\
    --stream-mode {stream_mode} \\
    --burn-in {config['burn_in']} \\
    --max-weak {config['max_weak']} \\
    --test-ids "{test_ids_str}" \\
    --binary-path "{args.binary_path}" \\
    --dieharder-path "{args.dieharder_path}" \\
    --output-dir "{args.results_dir}/results"

EXIT_CODE=$?
echo "Finished: $(date), exit code: $EXIT_CODE"
exit $EXIT_CODE
"""

    sge_path = os.path.join(args.results_dir, "submit_sweep.sh")
    with open(sge_path, "w") as f:
        f.write(sge_script)
    os.chmod(sge_path, 0o755)

    log(f"Generated {len(tasks)} tasks -> {manifest_path}")
    log(f"SGE script: {sge_path}")
    log(f"Sweep config: {sweep_config_path}")

    if not args.dry_run:
        result = subprocess.run(["qsub", sge_path], capture_output=True, text=True)
        if result.returncode == 0:
            log(f"Submitted: {result.stdout.strip()}")
        else:
            log(f"qsub failed: {result.stderr}")
            sys.exit(1)
    else:
        log("Dry run -- script generated but not submitted")
        log(f"Submit manually: qsub {sge_path}")


# ============================================================================
# CMD: COLLECT (aggregate worker results)
# ============================================================================

def _compress_indices(indices):
    """Compress [0,1,2,5,6,10] to '0-2,5-6,10' for SGE -t resubmission."""
    if not indices:
        return ""
    indices = sorted(set(indices))
    ranges = []
    start = end = indices[0]
    for i in indices[1:]:
        if i == end + 1:
            end = i
        else:
            ranges.append(f"{start}-{end}" if start != end else str(start))
            start = end = i
    ranges.append(f"{start}-{end}" if start != end else str(start))
    return ",".join(ranges)


def _report_missing(manifest_path, results_dir):
    """Identify missing tasks and print resubmit command."""
    existing = set()
    for fpath in glob_module.glob(os.path.join(results_dir, "w*_g*_r*.json")):
        existing.add(os.path.basename(fpath))

    missing = []
    with open(manifest_path) as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            parts = line.strip().split("\t")
            task_idx, wires, gates, _seed, rep_idx = parts
            expected = f"w{wires}_g{gates}_r{rep_idx}.json"
            if expected not in existing:
                missing.append(int(task_idx))

    if missing:
        log(f"Missing task indices: {missing[:20]}{'...' if len(missing) > 20 else ''}")
        ranges = _compress_indices(missing)
        log(f"Resubmit: qsub -t {ranges} submit_sweep.sh")

    return missing


def cmd_collect(args):
    """Aggregate per-replicate JSON files into final results JSON."""
    results_dir = os.path.join(args.results_dir, "results")
    config_path = os.path.join(args.results_dir, "sweep_config.json")

    if not os.path.exists(config_path):
        print(f"ERROR: sweep_config.json not found in {args.results_dir}")
        print("This directory was not created by 'submit'. Provide the correct --results-dir.")
        sys.exit(1)

    with open(config_path) as f:
        sweep_config = json.load(f)

    # Load all per-replicate JSONs
    rep_files = sorted(glob_module.glob(os.path.join(results_dir, "w*_g*_r*.json")))
    log(f"Found {len(rep_files)} replicate result files")

    # Check completeness
    manifest_path = os.path.join(args.results_dir, "task_manifest.tsv")
    expected_count = 0
    if os.path.exists(manifest_path):
        with open(manifest_path) as f:
            expected_count = sum(1 for line in f
                                 if not line.startswith("#") and line.strip())

    if expected_count > 0 and len(rep_files) < expected_count:
        missing_count = expected_count - len(rep_files)
        log(f"WARNING: {missing_count}/{expected_count} tasks missing")
        _report_missing(manifest_path, results_dir)
        if args.require_complete:
            sys.exit(1)

    # Group by (wires, gates)
    by_config = {}
    for fpath in rep_files:
        with open(fpath) as f:
            rep = json.load(f)
        key = (rep["wires"], rep["gates"])
        by_config.setdefault(key, []).append(rep)

    # Build ConfigResult-compatible structures
    all_results = []
    config_results = []  # for m* estimation
    for (wires, gates) in sorted(by_config.keys()):
        reps = sorted(by_config[(wires, gates)],
                       key=lambda r: r.get("replicate_index", 0))

        replicates_data = []
        for rep in reps:
            replicates_data.append({
                "seed": rep["seed"],
                "mode": rep["mode"],
                "overall_pass": rep["overall_pass"],
                "num_passed": rep["num_passed"],
                "num_weak": rep["num_weak"],
                "num_failed": rep["num_failed"],
                "num_tests": rep["num_tests"],
                "duration_sec": rep["duration_sec"],
                "stream_bytes": rep.get("stream_bytes", 0),
                "error": rep.get("error"),
                "tests": rep["tests"],
            })

        total_passed = sum(1 for r in reps if r["overall_pass"])
        total_failed = len(reps) - total_passed
        pass_rate = total_passed / len(reps) if reps else 0.0

        all_results.append({
            "wires": wires,
            "gates": gates,
            "pass_rate": pass_rate,
            "total_passed": total_passed,
            "total_failed": total_failed,
            "replicates": replicates_data,
        })

        config_results.append(ConfigResult(
            wires=wires, gates=gates, replicates=[],
            pass_rate=pass_rate, total_passed=total_passed,
            total_failed=total_failed,
        ))

    threshold = args.threshold or sweep_config.get("threshold", 0.95)
    m_star = estimate_m_star(config_results, threshold=threshold)

    # Build final JSON matching plot_rng_sweep.py schema
    sc = sweep_config
    final = {
        "date": DATE,
        "mode": sc["mode"],
        "stream_mode": sc["stream_mode"],
        "config": sc["config"],
        "dieharder_test_ids": sc["dieharder_test_ids"],
        "num_tests": len(sc["dieharder_test_ids"]),
        "threshold": threshold,
        "m_star": {str(k): v for k, v in m_star.items()},
        "results": all_results,
    }

    output_path = args.output or os.path.join(args.results_dir, "rng_sweep_results.json")
    with open(output_path, "w") as f:
        json.dump(final, f, indent=2)
    log(f"Aggregated results -> {output_path}")

    summary_text = format_summary_table(config_results, m_star)
    print(summary_text)
    summary_path = os.path.join(os.path.dirname(output_path), "rng_sweep_summary.txt")
    with open(summary_path, "w") as f:
        f.write(summary_text)
    log(f"Summary -> {summary_path}")

    log("")
    log("m*(n) estimates:")
    for wires, m in sorted(m_star.items()):
        log(f"  n={wires}: m*={m if m else 'not reached'}")


# ============================================================================
# MAIN
# ============================================================================

def main():
    parser = argparse.ArgumentParser(
        description="RNG Sweep for Random Circuits",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
subcommands:
  (default)  Run sweep locally (same as 'local')
  local      Run sweep locally (sequential)
  worker     Run a single replicate (for SGE array tasks)
  submit     Generate and submit SGE array job
  collect    Aggregate worker results into final JSON
""")
    subparsers = parser.add_subparsers(dest="command")

    # --- local ---
    p_local = subparsers.add_parser("local", help="Run sweep locally (sequential)")
    _add_sweep_args(p_local)
    p_local.add_argument("--output-dir", type=str, help="Override output directory")
    p_local.add_argument("--pipe", action="store_true", default=False,
                         help="Use pipe mode (recommended)")

    # --- worker ---
    p_worker = subparsers.add_parser("worker", help="Run a single replicate (SGE task)")
    p_worker.add_argument("--wires", type=int, required=True)
    p_worker.add_argument("--gates", type=int, required=True)
    p_worker.add_argument("--seed", type=int, required=True)
    p_worker.add_argument("--replicate-index", type=int, required=True)
    p_worker.add_argument("--mode", choices=["quick", "medium", "full"], default="quick")
    p_worker.add_argument("--stream-mode",
                          choices=["iterate", "random-input", "counter"],
                          default="counter")
    p_worker.add_argument("--burn-in", type=int, default=0,
                          help="Burn-in samples (only used by iterate mode, ignored in counter)")
    p_worker.add_argument("--max-weak", type=int, default=1)
    p_worker.add_argument("--test-ids", type=str, default=None,
                          help="Comma-separated dieharder test IDs")
    p_worker.add_argument("--binary-path", type=str, required=True,
                          help="Path to local_mixing_bin")
    p_worker.add_argument("--dieharder-path", type=str, required=True,
                          help="Path to dieharder binary")
    p_worker.add_argument("--output-dir", type=str, required=True)

    # --- submit ---
    p_submit = subparsers.add_parser("submit", help="Generate SGE array job")
    _add_sweep_args(p_submit)
    p_submit.add_argument("--binary-path", type=str, required=True,
                          help="Path to pre-compiled local_mixing_bin on cluster")
    p_submit.add_argument("--dieharder-path", type=str, required=True,
                          help="Path to dieharder binary on cluster")
    p_submit.add_argument("--script-path", type=str, default=None,
                          help="Path to this script on cluster (default: auto)")
    p_submit.add_argument("--results-dir", type=str, required=True,
                          help="Output directory for all sweep artifacts")
    p_submit.add_argument("--project", type=str, default="bnec",
                          help="SGE project name (#$ -P)")
    p_submit.add_argument("--time", type=str, default="01:00:00",
                          help="Walltime per task (h_rt)")
    p_submit.add_argument("--memory", type=str, default="4G",
                          help="Memory per core (mem_per_core)")
    p_submit.add_argument("--base-seed", type=int, default=42)
    p_submit.add_argument("--dry-run", action="store_true",
                          help="Generate script but do not submit")
    p_submit.add_argument("--job-name", type=str, default="rng_sweep")

    # --- collect ---
    p_collect = subparsers.add_parser("collect", help="Aggregate worker results")
    p_collect.add_argument("--results-dir", type=str, required=True)
    p_collect.add_argument("--output", type=str, default=None,
                           help="Output JSON path (default: results_dir/rng_sweep_results.json)")
    p_collect.add_argument("--threshold", type=float, default=None)
    p_collect.add_argument("--require-complete", action="store_true",
                           help="Exit with error if any results are missing")

    # Backward compatibility: if first arg is not a known subcommand,
    # prepend "local" so old-style invocations still work
    known_cmds = {"local", "worker", "submit", "collect"}
    argv = sys.argv[1:]
    if not argv or argv[0] not in known_cmds:
        argv = ["local"] + argv

    args = parser.parse_args(argv)

    if args.command == "local":
        cmd_local(args)
    elif args.command == "worker":
        cmd_worker(args)
    elif args.command == "submit":
        cmd_submit(args)
    elif args.command == "collect":
        cmd_collect(args)


if __name__ == "__main__":
    main()
