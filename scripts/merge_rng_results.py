#!/usr/bin/env python3
"""
Merge multiple per-width rng_sweep_results.json files into one combined JSON.

Usage:
    python scripts/merge_rng_results.py \
        /path/to/w32/rng_sweep_results.json \
        /path/to/w48/rng_sweep_results.json \
        --output /path/to/combined.json
"""

import json
import argparse
import sys
from collections import defaultdict


def main():
    parser = argparse.ArgumentParser(description="Merge RNG sweep results from multiple widths")
    parser.add_argument("inputs", nargs="+", help="Input rng_sweep_results.json files")
    parser.add_argument("--output", "-o", required=True, help="Output merged JSON path")
    parser.add_argument("--threshold", type=float, default=0.95, help="m*(n) threshold")
    args = parser.parse_args()

    all_results = []
    all_test_ids = set()
    stream_mode = None

    for path in args.inputs:
        with open(path) as f:
            data = json.load(f)

        all_results.extend(data["results"])
        all_test_ids.update(data.get("dieharder_test_ids", []))

        if stream_mode is None:
            stream_mode = data.get("stream_mode", "counter")

    # Sort by (wires, gates) for consistent ordering
    all_results.sort(key=lambda r: (r["wires"], r["gates"]))

    # Compute m*(n) for each width
    by_wires = defaultdict(list)
    for r in all_results:
        by_wires[r["wires"]].append(r)

    m_star = {}
    for wires, configs in sorted(by_wires.items()):
        configs.sort(key=lambda c: c["gates"])
        m_star[str(wires)] = None
        for c in configs:
            if c["pass_rate"] >= args.threshold:
                m_star[str(wires)] = c["gates"]
                break

    merged = {
        "date": "merged",
        "mode": "quick",
        "stream_mode": stream_mode,
        "config": {
            "wires": sorted(by_wires.keys()),
            "gates": sorted(set(r["gates"] for r in all_results)),
            "replicates": max(
                len(r["replicates"]) for r in all_results
            ) if all_results else 0,
        },
        "dieharder_test_ids": sorted(all_test_ids),
        "num_tests": len(all_test_ids),
        "threshold": args.threshold,
        "m_star": m_star,
        "results": all_results,
    }

    with open(args.output, "w") as f:
        json.dump(merged, f, indent=2)

    print(f"Merged {len(args.inputs)} files -> {args.output}")
    print(f"  Widths: {sorted(by_wires.keys())}")
    print(f"  Total configs: {len(all_results)}")
    print(f"  m*(n): {m_star}")


if __name__ == "__main__":
    main()
