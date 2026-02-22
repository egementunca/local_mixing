#!/usr/bin/env python3
"""
Phase 3 reanalysis: compute pass rates excluding rgb_minimum_distance.

Also identifies which tests cause failures at high gate counts.

Usage:
    python scripts/analyze_p3.py --results-dir /path/to/rng_sweep
"""

import json
import os
import argparse
from collections import defaultdict
from pathlib import Path


EXCLUDE_TESTS = {"rgb_minimum_distance"}


def load_individual_results(results_dir):
    """Load all individual w*_g*_r*.json files from results/ subdirectory."""
    results_path = Path(results_dir) / "results"
    files = sorted(results_path.glob("w*_g*_r*.json"))
    results = []
    for f in files:
        with open(f) as fh:
            results.append(json.load(fh))
    return results


def analyze_width(width_dir, exclude_tests=EXCLUDE_TESTS):
    """Analyze a single width directory, computing pass rates with/without excluded tests."""
    results = load_individual_results(width_dir)
    if not results:
        return None

    wires = results[0]["wires"]
    stream_mode = results[0]["mode"]

    # Group by gates
    by_gates = defaultdict(list)
    for r in results:
        by_gates[r["gates"]].append(r)

    analysis = []
    for gates in sorted(by_gates.keys()):
        reps = by_gates[gates]
        n_reps = len(reps)

        # Original pass rate
        orig_pass = sum(1 for r in reps if r["overall_pass"])

        # Pass rate excluding specific tests
        excl_pass = 0
        for r in reps:
            tests = [t for t in r["tests"] if t["test_name"] not in exclude_tests]
            n_failed = sum(1 for t in tests if t["assessment"] == "FAILED")
            n_weak = sum(1 for t in tests if t["assessment"] == "WEAK")
            if n_failed == 0 and n_weak <= 1:
                excl_pass += 1

        # Identify failing tests at this gate count
        test_fail_counts = defaultdict(int)
        for r in reps:
            for t in r["tests"]:
                if t["assessment"] == "FAILED":
                    test_fail_counts[t["test_name"]] += 1

        analysis.append({
            "gates": gates,
            "replicates": n_reps,
            "pass_original": orig_pass,
            "pass_rate_original": orig_pass / n_reps,
            "pass_excluded": excl_pass,
            "pass_rate_excluded": excl_pass / n_reps,
            "failing_tests": dict(test_fail_counts),
        })

    return {
        "wires": wires,
        "stream_mode": stream_mode,
        "exclude_tests": list(exclude_tests),
        "configs": analysis,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", default="rng_sweep",
                        help="Base directory containing p3_w* subdirectories")
    args = parser.parse_args()

    base = Path(args.results_dir)
    p3_dirs = sorted(base.glob("p3_w*"))

    if not p3_dirs:
        print(f"No p3_w* directories found in {base}")
        return

    print("=" * 80)
    print("Phase 3 Reanalysis: Full Dieharder Battery")
    print(f"Excluding: {EXCLUDE_TESTS}")
    print("=" * 80)

    all_results = {}
    for d in p3_dirs:
        result = analyze_width(d)
        if result is None:
            continue

        w = result["wires"]
        all_results[w] = result

        print(f"\n### n={w} (stream_mode={result['stream_mode']})")
        print(f"{'Gates':>6} {'R':>4} {'Orig%':>7} {'Excl%':>7} {'Orig':>5} {'Excl':>5}  Failing tests")
        print("-" * 75)

        for c in result["configs"]:
            failing = ", ".join(
                f"{t}({n})" for t, n in sorted(c["failing_tests"].items(), key=lambda x: -x[1])
                if t not in EXCLUDE_TESTS
            )
            print(f"{c['gates']:>6} {c['replicates']:>4} "
                  f"{c['pass_rate_original']*100:>6.0f}% {c['pass_rate_excluded']*100:>6.0f}% "
                  f"{c['pass_original']:>4}/{c['replicates']} {c['pass_excluded']:>4}/{c['replicates']}  "
                  f"{failing or '(none)'}")

    # Summary m*(n) with exclusion
    print("\n" + "=" * 80)
    print("m*(n) Summary (95% threshold)")
    print(f"{'Width':>6} {'m*(orig)':>10} {'m*(excl)':>10}")
    print("-" * 30)

    for w in sorted(all_results.keys()):
        result = all_results[w]
        m_star_orig = None
        m_star_excl = None
        for c in result["configs"]:
            if m_star_orig is None and c["pass_rate_original"] >= 0.95:
                m_star_orig = c["gates"]
            if m_star_excl is None and c["pass_rate_excluded"] >= 0.95:
                m_star_excl = c["gates"]
        print(f"{w:>6} {str(m_star_orig):>10} {str(m_star_excl):>10}")


if __name__ == "__main__":
    main()
