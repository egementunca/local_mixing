#!/usr/bin/env python3
"""CLI entry point for heatmap analysis suite (Issue #44).

Usage:
    # Single scheme analysis
    python -m scripts.heatmaps.run_analysis \
        --c1 results/n64/circuits/c1_n64.txt \
        --c2 results/n64/circuits/c1_n64_rac_r1.txt \
        -n 64 -i 10000 --label RAC --out results/n64/heatmaps_v2/rac

    # Comparison from pre-computed directories
    python -m scripts.heatmaps.run_analysis --compare \
        --dir-a results/n64/heatmaps_v2/rac \
        --dir-b results/n64/heatmaps_v2/btb \
        --label-a RAC --label-b BTB \
        --out results/n64/heatmaps_v2/comparison
"""

import argparse
import sys
from pathlib import Path

# Allow running from repo root
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from scripts.heatmaps.report import generate_report, generate_comparison


def main():
    parser = argparse.ArgumentParser(
        description="Heatmap analysis suite (Issue #44)"
    )
    sub = parser.add_subparsers(dest="mode")

    # --- Single scheme ---
    sp = sub.add_parser("single", help="Analyze one obfuscation scheme")
    sp.add_argument("--c1", required=True, help="Original circuit path")
    sp.add_argument("--c2", required=True, help="Obfuscated circuit path")
    sp.add_argument("-n", type=int, required=True, help="Number of wires")
    sp.add_argument("-i", "--inputs", type=int, default=10000, help="Number of random inputs")
    sp.add_argument("--label", required=True, help="Scheme label (e.g. RAC, BTB)")
    sp.add_argument("--out", required=True, help="Output directory")
    sp.add_argument("-d", "--window-d", type=int, default=50, help="Window size for windowed-min")

    # --- Comparison ---
    cp = sub.add_parser("compare", help="Compare two pre-computed analyses")
    cp.add_argument("--dir-a", required=True, help="Directory for scheme A")
    cp.add_argument("--dir-b", required=True, help="Directory for scheme B")
    cp.add_argument("--label-a", default="RAC", help="Label for scheme A")
    cp.add_argument("--label-b", default="BTB", help="Label for scheme B")
    cp.add_argument("-n", type=int, default=64, help="Number of wires")
    cp.add_argument("--out", required=True, help="Output directory")

    # --- Full pipeline ---
    fp = sub.add_parser("full", help="Run both schemes + comparison")
    fp.add_argument("--c1", required=True, help="Original circuit path")
    fp.add_argument("--c2-a", required=True, help="Obfuscated circuit A")
    fp.add_argument("--c2-b", required=True, help="Obfuscated circuit B")
    fp.add_argument("-n", type=int, required=True, help="Number of wires")
    fp.add_argument("-i", "--inputs", type=int, default=10000)
    fp.add_argument("--label-a", default="RAC")
    fp.add_argument("--label-b", default="BTB")
    fp.add_argument("--out", required=True, help="Base output directory")
    fp.add_argument("-d", "--window-d", type=int, default=50)

    args = parser.parse_args()

    if args.mode == "single":
        generate_report(
            args.c1, args.c2, args.n, args.label, args.out,
            inputs=args.inputs, window_d=args.window_d,
        )

    elif args.mode == "compare":
        generate_comparison(
            args.dir_a, args.dir_b, args.label_a, args.label_b,
            args.out, num_wires=args.n,
        )

    elif args.mode == "full":
        base = Path(args.out)
        dir_a = str(base / args.label_a.lower())
        dir_b = str(base / args.label_b.lower())

        generate_report(
            args.c1, args.c2_a, args.n, args.label_a, dir_a,
            inputs=args.inputs, window_d=args.window_d,
        )
        generate_report(
            args.c1, args.c2_b, args.n, args.label_b, dir_b,
            inputs=args.inputs, window_d=args.window_d,
        )
        generate_comparison(
            dir_a, dir_b, args.label_a, args.label_b,
            str(base / "comparison"), num_wires=args.n,
        )
        print(f"\nFull analysis complete: {base}/")

    else:
        parser.print_help()


if __name__ == "__main__":
    main()
