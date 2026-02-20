#!/usr/bin/env python3
"""
Plot RNG sweep results with multiple views:
1. Overall pass rate vs gate count (all tests combined)
2. Per-test pass rate vs gate count (one curve per test)
3. P-value scatter: all individual p-values colored by assessment
4. Per-test p-value panels: one subplot per test
5. Pipeline diagram

Usage:
    python scripts/plot_rng_sweep.py [--results PATH] [--out DIR]
"""

import json
import os
import argparse
import sys
from collections import defaultdict

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
from matplotlib.lines import Line2D
import numpy as np


def load_results(path):
    with open(path) as f:
        return json.load(f)


def collect_per_test_data(data):
    """
    Collect per-test pass rates for each (wires, gates) config.
    Returns: {wires: {test_name: {gates: pass_rate}}}
    """
    per_test = defaultdict(lambda: defaultdict(lambda: defaultdict(list)))

    for r in data["results"]:
        wires = r["wires"]
        gates = r["gates"]
        for rep in r["replicates"]:
            # Group tests by name within this replicate
            test_results = defaultdict(list)
            for t in rep["tests"]:
                test_results[t["test_name"]].append(t)

            for test_name, tests in test_results.items():
                # A replicate passes a specific test if none of its entries FAILED
                any_failed = any(t["assessment"] == "FAILED" for t in tests)
                per_test[wires][test_name][gates].append(not any_failed)

    # Convert to pass rates
    result = {}
    for wires in per_test:
        result[wires] = {}
        for test_name in per_test[wires]:
            result[wires][test_name] = {}
            for gates, outcomes in per_test[wires][test_name].items():
                result[wires][test_name][gates] = sum(outcomes) / len(outcomes)

    return result


def plot_overall_pass_rate(data, out_dir):
    """Overall pass rate vs gate count, one line per wire width."""
    fig, ax = plt.subplots(1, 1, figsize=(11, 6))

    by_wires = {}
    for r in data["results"]:
        by_wires.setdefault(r["wires"], []).append(r)

    colors = {16: "#e74c3c", 24: "#e67e22", 32: "#2ecc71", 48: "#3498db", 64: "#9b59b6"}
    markers = {16: "s", 24: "D", 32: "o", 48: "^", 64: "v"}

    for wires in sorted(by_wires.keys()):
        configs = sorted(by_wires[wires], key=lambda c: c["gates"])
        gates = [c["gates"] for c in configs]
        rates = [c["pass_rate"] * 100 for c in configs]

        color = colors.get(wires, "#95a5a6")
        marker = markers.get(wires, "o")
        ax.plot(gates, rates, f"-{marker}", color=color, label=f"n={wires} wires",
                linewidth=2.5, markersize=9)

    ax.set_xlabel("Number of Gates (m)", fontsize=13)
    ax.set_ylabel("Pass Rate (%)", fontsize=13)
    ax.set_title("Dieharder Pass Rate vs Gate Count\n(all tests combined, iterate mode)", fontsize=14)
    ax.set_xscale("log")
    ax.set_ylim(-5, 105)
    ax.set_yticks([0, 25, 50, 75, 100])
    ax.axhline(y=95, color="gray", linestyle="--", alpha=0.5, label="95% threshold")
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    path = os.path.join(out_dir, "pass_rate_vs_gates.png")
    fig.savefig(path, dpi=150)
    print(f"Saved: {path}")
    plt.close()


def plot_per_test_pass_rate(data, out_dir):
    """One curve per test, showing pass rate vs gate count."""
    per_test = collect_per_test_data(data)

    for wires in sorted(per_test.keys()):
        tests = per_test[wires]
        if not tests:
            continue

        test_names = sorted(tests.keys())
        n_tests = len(test_names)

        fig, ax = plt.subplots(1, 1, figsize=(12, 7))

        cmap = plt.cm.tab20
        for i, test_name in enumerate(test_names):
            gate_rates = tests[test_name]
            gates = sorted(gate_rates.keys())
            rates = [gate_rates[g] * 100 for g in gates]

            color = cmap(i / max(n_tests, 1))
            short_name = test_name.replace("diehard_", "").replace("sts_", "STS:").replace("rgb_", "RGB:")
            ax.plot(gates, rates, "-o", color=color, label=short_name,
                    linewidth=1.8, markersize=6, alpha=0.85)

        ax.set_xlabel("Number of Gates (m)", fontsize=13)
        ax.set_ylabel("Pass Rate (%)", fontsize=13)
        ax.set_title(f"Per-Test Pass Rate vs Gate Count (n={wires} wires)\n"
                     f"({n_tests} tests, iterate mode)", fontsize=14)
        ax.set_xscale("log")
        ax.set_ylim(-5, 105)
        ax.set_yticks([0, 25, 50, 75, 100])
        ax.axhline(y=95, color="gray", linestyle="--", alpha=0.4)
        ax.legend(fontsize=8, loc="lower right", ncol=2)
        ax.grid(True, alpha=0.3)

        plt.tight_layout()
        path = os.path.join(out_dir, f"per_test_pass_rate_w{wires}.png")
        fig.savefig(path, dpi=150)
        print(f"Saved: {path}")
        plt.close()


def plot_pvalues_scatter(data, out_dir):
    """All p-values in one scatter plot, colored by assessment."""
    by_wires = {}
    for r in data["results"]:
        by_wires.setdefault(r["wires"], []).append(r)

    n_panels = len(by_wires)
    fig, axes = plt.subplots(1, max(n_panels, 1), figsize=(7 * max(n_panels, 1), 6), sharey=True)
    if n_panels == 1:
        axes = [axes]

    for idx, wires in enumerate(sorted(by_wires.keys())):
        ax = axes[idx]
        configs = sorted(by_wires[wires], key=lambda c: c["gates"])

        for c in configs:
            gates = c["gates"]
            for rep in c["replicates"]:
                for t in rep["tests"]:
                    p = t["p_value"]
                    if p == 0:
                        p = 1e-10
                    color = {"PASSED": "#2ecc71", "WEAK": "#f39c12", "FAILED": "#e74c3c"}[t["assessment"]]
                    ax.scatter(gates, p, c=color, s=30, alpha=0.7,
                              edgecolors="black", linewidths=0.3)

        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_xlabel("Number of Gates (m)", fontsize=12)
        ax.set_title(f"n = {wires} wires", fontsize=13)
        ax.axhline(y=0.005, color="orange", linestyle="--", alpha=0.5, linewidth=1,
                   label="WEAK threshold")
        ax.axhline(y=1e-6, color="red", linestyle="--", alpha=0.5, linewidth=1,
                   label="FAIL threshold")
        ax.set_ylim(1e-11, 2)
        ax.grid(True, alpha=0.3, which="both")

    axes[0].set_ylabel("p-value", fontsize=12)

    legend_elements = [
        Line2D([0], [0], marker='o', color='w', markerfacecolor='#2ecc71', markersize=10, label='PASSED'),
        Line2D([0], [0], marker='o', color='w', markerfacecolor='#f39c12', markersize=10, label='WEAK'),
        Line2D([0], [0], marker='o', color='w', markerfacecolor='#e74c3c', markersize=10, label='FAILED'),
    ]
    axes[-1].legend(handles=legend_elements, fontsize=10, loc="lower right")

    fig.suptitle("Individual p-values across all tests\n(each dot = one test on one circuit replicate)",
                 fontsize=14)
    plt.tight_layout()
    path = os.path.join(out_dir, "pvalues_scatter.png")
    fig.savefig(path, dpi=150)
    print(f"Saved: {path}")
    plt.close()


def plot_pvalues_per_test(data, out_dir):
    """Grid of subplots: one per test, showing p-values vs gate count."""
    # Collect all test names
    all_test_names = set()
    for r in data["results"]:
        for rep in r["replicates"]:
            for t in rep["tests"]:
                all_test_names.add(t["test_name"])

    test_names = sorted(all_test_names)
    n_tests = len(test_names)
    if n_tests == 0:
        return

    ncols = min(4, n_tests)
    nrows = (n_tests + ncols - 1) // ncols
    fig, axes = plt.subplots(nrows, ncols, figsize=(4 * ncols, 3.5 * nrows), sharey=True)
    if nrows == 1 and ncols == 1:
        axes = np.array([[axes]])
    elif nrows == 1:
        axes = axes.reshape(1, -1)
    elif ncols == 1:
        axes = axes.reshape(-1, 1)

    for i, test_name in enumerate(test_names):
        row, col = divmod(i, ncols)
        ax = axes[row][col]

        for r in data["results"]:
            gates = r["gates"]
            for rep in r["replicates"]:
                for t in rep["tests"]:
                    if t["test_name"] != test_name:
                        continue
                    p = t["p_value"]
                    if p == 0:
                        p = 1e-10
                    color = {"PASSED": "#2ecc71", "WEAK": "#f39c12", "FAILED": "#e74c3c"}[t["assessment"]]
                    ax.scatter(gates, p, c=color, s=25, alpha=0.7,
                              edgecolors="black", linewidths=0.3)

        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_ylim(1e-11, 2)
        ax.axhline(y=0.005, color="orange", linestyle="--", alpha=0.4, linewidth=0.8)
        ax.axhline(y=1e-6, color="red", linestyle="--", alpha=0.4, linewidth=0.8)
        # Shade the "PASSED" region
        ax.axhspan(0.005, 0.995, color="#2ecc71", alpha=0.05)
        short_name = test_name.replace("diehard_", "").replace("sts_", "STS:").replace("rgb_", "RGB:")
        ax.set_title(short_name, fontsize=10, fontweight="bold")
        ax.grid(True, alpha=0.2, which="both")

        if col == 0:
            ax.set_ylabel("p-value", fontsize=9)
        if row == nrows - 1:
            ax.set_xlabel("gates (m)", fontsize=9)

    # Hide unused subplots
    for i in range(n_tests, nrows * ncols):
        row, col = divmod(i, ncols)
        axes[row][col].set_visible(False)

    fig.suptitle("P-values by test (each dot = one circuit replicate)", fontsize=14, y=1.01)
    plt.tight_layout()
    path = os.path.join(out_dir, "pvalues_per_test.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    print(f"Saved: {path}")
    plt.close()


def plot_pipeline_diagram(out_dir):
    """Visual pipeline diagram."""
    fig, ax = plt.subplots(1, 1, figsize=(14, 4))
    ax.set_xlim(0, 14)
    ax.set_ylim(0, 4)
    ax.axis("off")

    boxes = [
        (1, 1.5, "Random\nCircuit C\n(n wires, m gates)", "#3498db"),
        (4, 1.5, "Iterate:\nx_{t+1} = C(x_t)\nor Counter:\ny_i = C(i)", "#2ecc71"),
        (7, 1.5, "Raw Binary\nBitstream\n(n*k bits)", "#e67e22"),
        (10, 1.5, "Dieharder\nTest Battery\n(~30 tests)", "#9b59b6"),
        (13, 1.5, "p-values\n+ PASS/FAIL\nassessment", "#e74c3c"),
    ]

    for x, y, text, color in boxes:
        rect = mpatches.FancyBboxPatch((x - 1.2, y - 0.8), 2.4, 1.6,
                                        boxstyle="round,pad=0.15",
                                        facecolor=color, alpha=0.2,
                                        edgecolor=color, linewidth=2)
        ax.add_patch(rect)
        ax.text(x, y, text, ha="center", va="center", fontsize=9, fontweight="bold")

    for x in [2.2, 5.2, 8.2, 11.2]:
        ax.annotate("", xy=(x + 0.6, 1.5), xytext=(x, 1.5),
                    arrowprops=dict(arrowstyle="->", lw=2, color="#555"))

    ax.set_title("Pipeline: Random Circuit  ->  Bitstream  ->  Statistical Testing", fontsize=14, pad=20)
    plt.tight_layout()
    path = os.path.join(out_dir, "pipeline_diagram.png")
    fig.savefig(path, dpi=150)
    print(f"Saved: {path}")
    plt.close()


def main():
    parser = argparse.ArgumentParser(description="Plot RNG sweep results")
    parser.add_argument("--results", default=None, help="Path to rng_sweep_results.json")
    parser.add_argument("--out", default=None, help="Output directory for plots")
    args = parser.parse_args()

    if args.results:
        results_path = args.results
    else:
        base = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "experiments")
        if not os.path.isdir(base):
            base = os.path.join("experiments")
        candidates = []
        for root, dirs, files in os.walk(base):
            for f in files:
                if f == "rng_sweep_results.json":
                    candidates.append(os.path.join(root, f))
        if not candidates:
            print("No rng_sweep_results.json found. Run rng_sweep.py first.")
            sys.exit(1)
        results_path = sorted(candidates)[-1]

    print(f"Loading: {results_path}")
    data = load_results(results_path)

    out_dir = args.out or os.path.dirname(results_path)
    os.makedirs(out_dir, exist_ok=True)

    plot_overall_pass_rate(data, out_dir)
    plot_per_test_pass_rate(data, out_dir)
    plot_pvalues_scatter(data, out_dir)
    plot_pvalues_per_test(data, out_dir)
    plot_pipeline_diagram(out_dir)

    print(f"\nAll plots saved to: {out_dir}")


if __name__ == "__main__":
    main()
