#!/usr/bin/env python3
"""
Generate comparison and analysis plots for RNG sweep phases.

1. CTR vs OFB (iterate) comparison
2. Phase 3 per-test failure analysis (which tests in the full battery fail?)
3. All-phases summary comparison

Usage:
    python scripts/plot_comparison.py \
        --p1p2 rng_sweep/combined_p1p2_counter.json \
        --p3 rng_sweep/combined_p3_full.json \
        --p5 rng_sweep/combined_p5_iterate.json \
        --p6 rng_sweep/combined_p6_relkey.json \
        --out rng_sweep/plots_comparison
"""

import json
import os
import argparse
from collections import defaultdict

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


def load(path):
    with open(path) as f:
        return json.load(f)


def get_pass_rates(data):
    """Returns {wires: [(gates, pass_rate), ...]} sorted by gates."""
    by_wires = defaultdict(list)
    for r in data["results"]:
        by_wires[r["wires"]].append((r["gates"], r["pass_rate"]))
    return {w: sorted(pts) for w, pts in by_wires.items()}


def plot_ctr_vs_ofb(ctr_data, ofb_data, out_dir):
    """Side-by-side CTR vs OFB pass rate curves."""
    ctr = get_pass_rates(ctr_data)
    ofb = get_pass_rates(ofb_data)

    widths = sorted(set(list(ctr.keys()) + list(ofb.keys())))
    n_widths = len(widths)

    fig, axes = plt.subplots(1, n_widths, figsize=(4.5 * n_widths, 5), sharey=True)
    if n_widths == 1:
        axes = [axes]

    for idx, w in enumerate(widths):
        ax = axes[idx]

        if w in ctr:
            gates_c, rates_c = zip(*ctr[w])
            ax.plot(gates_c, [r * 100 for r in rates_c], "o-",
                    color="#e74c3c", label="CTR (counter)", linewidth=2.5, markersize=7)

        if w in ofb:
            gates_o, rates_o = zip(*ofb[w])
            ax.plot(gates_o, [r * 100 for r in rates_o], "s--",
                    color="#3498db", label="OFB (iterate)", linewidth=2.5, markersize=7)

        ax.set_xscale("log")
        ax.set_ylim(-5, 105)
        ax.set_yticks([0, 25, 50, 75, 95, 100])
        ax.axhline(y=95, color="gray", linestyle=":", alpha=0.5)
        ax.set_xlabel("Gates (m)", fontsize=12)
        ax.set_title(f"n = {w}", fontsize=13, fontweight="bold")
        ax.grid(True, alpha=0.3)

        if idx == 0:
            ax.set_ylabel("Pass Rate (%)", fontsize=12)
        if idx == n_widths - 1:
            ax.legend(fontsize=10, loc="lower right")

    fig.suptitle("CTR Mode vs OFB Mode: Pass Rate Comparison\n"
                 "(7 core dieharder tests, R=100 CTR / R=20 OFB)",
                 fontsize=14, y=1.02)
    plt.tight_layout()
    path = os.path.join(out_dir, "ctr_vs_ofb_comparison.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    print(f"Saved: {path}")
    plt.close()


def plot_mstar_comparison(ctr_data, ofb_data, out_dir):
    """m*(n) comparison: CTR vs OFB as a function of width."""
    ctr_mstar = ctr_data.get("m_star", {})
    ofb_mstar = ofb_data.get("m_star", {})

    widths = sorted(set(list(ctr_mstar.keys()) + list(ofb_mstar.keys())), key=int)

    fig, ax = plt.subplots(1, 1, figsize=(8, 6))

    w_vals = [int(w) for w in widths]
    ctr_vals = [ctr_mstar.get(w) for w in widths]
    ofb_vals = [ofb_mstar.get(w) for w in widths]

    # Filter out None
    ctr_w = [w for w, v in zip(w_vals, ctr_vals) if v is not None]
    ctr_m = [v for v in ctr_vals if v is not None]
    ofb_w = [w for w, v in zip(w_vals, ofb_vals) if v is not None]
    ofb_m = [v for v in ofb_vals if v is not None]

    ax.plot(ctr_w, ctr_m, "o-", color="#e74c3c", label="CTR (counter)", linewidth=2.5, markersize=10)
    ax.plot(ofb_w, ofb_m, "s--", color="#3498db", label="OFB (iterate)", linewidth=2.5, markersize=10)

    # Reference line: m = 20n
    ref_w = np.linspace(min(w_vals), max(w_vals), 100)
    ax.plot(ref_w, 20 * ref_w, ":", color="gray", alpha=0.5, label="m = 20n")

    ax.set_xlabel("Width n (wires)", fontsize=13)
    ax.set_ylabel("m*(n) (minimum gates for 95% pass rate)", fontsize=13)
    ax.set_title("Pseudorandomness Threshold: CTR vs OFB Mode", fontsize=14)
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    path = os.path.join(out_dir, "mstar_ctr_vs_ofb.png")
    fig.savefig(path, dpi=150)
    print(f"Saved: {path}")
    plt.close()


def plot_p3_test_failure_analysis(p3_data, out_dir):
    """Analyze which tests in the full battery consistently fail."""
    # Collect per-test failure rates across all configs
    test_failures = defaultdict(lambda: {"total": 0, "failed": 0, "weak": 0})

    for r in p3_data["results"]:
        for rep in r["replicates"]:
            for t in rep["tests"]:
                key = t["test_name"]
                test_failures[key]["total"] += 1
                if t["assessment"] == "FAILED":
                    test_failures[key]["failed"] += 1
                elif t["assessment"] == "WEAK":
                    test_failures[key]["weak"] += 1

    # Also collect per-test failure rates at HIGH gate counts only
    test_failures_high = defaultdict(lambda: {"total": 0, "failed": 0, "weak": 0})
    for r in p3_data["results"]:
        # Only look at the highest gate count per width
        pass  # we'll do this differently

    # Group by (wires, gates) to find high-gate configs
    by_wg = defaultdict(list)
    for r in p3_data["results"]:
        by_wg[r["wires"]].append(r)

    high_gate_results = []
    for wires, configs in by_wg.items():
        configs.sort(key=lambda c: c["gates"])
        if configs:
            high_gate_results.append(configs[-1])  # highest gate count per width

    for r in high_gate_results:
        for rep in r["replicates"]:
            for t in rep["tests"]:
                key = t["test_name"]
                test_failures_high[key]["total"] += 1
                if t["assessment"] == "FAILED":
                    test_failures_high[key]["failed"] += 1
                elif t["assessment"] == "WEAK":
                    test_failures_high[key]["weak"] += 1

    # Sort by failure rate (highest first)
    tests_sorted = sorted(test_failures.keys(),
                          key=lambda k: test_failures[k]["failed"] / max(test_failures[k]["total"], 1),
                          reverse=True)

    # Plot: horizontal bar chart of failure rates
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, max(8, len(tests_sorted) * 0.35)))

    # Left: all configs
    names = []
    fail_rates = []
    weak_rates = []
    for t in tests_sorted:
        d = test_failures[t]
        short = t.replace("diehard_", "").replace("sts_", "STS:").replace("rgb_", "RGB:").replace("dab_", "DAB:")
        names.append(short)
        fail_rates.append(d["failed"] / max(d["total"], 1) * 100)
        weak_rates.append(d["weak"] / max(d["total"], 1) * 100)

    y = range(len(names))
    ax1.barh(y, fail_rates, color="#e74c3c", alpha=0.8, label="FAILED")
    ax1.barh(y, weak_rates, left=fail_rates, color="#f39c12", alpha=0.8, label="WEAK")
    ax1.set_yticks(y)
    ax1.set_yticklabels(names, fontsize=8)
    ax1.set_xlabel("Rate (%)", fontsize=11)
    ax1.set_title("All Configs (full battery)", fontsize=12)
    ax1.legend(fontsize=9)
    ax1.invert_yaxis()
    ax1.grid(True, alpha=0.3, axis="x")

    # Right: high gate counts only
    names_h = []
    fail_rates_h = []
    weak_rates_h = []
    for t in tests_sorted:
        d = test_failures_high.get(t, {"total": 0, "failed": 0, "weak": 0})
        short = t.replace("diehard_", "").replace("sts_", "STS:").replace("rgb_", "RGB:").replace("dab_", "DAB:")
        names_h.append(short)
        if d["total"] > 0:
            fail_rates_h.append(d["failed"] / d["total"] * 100)
            weak_rates_h.append(d["weak"] / d["total"] * 100)
        else:
            fail_rates_h.append(0)
            weak_rates_h.append(0)

    ax2.barh(y, fail_rates_h, color="#e74c3c", alpha=0.8, label="FAILED")
    ax2.barh(y, weak_rates_h, left=fail_rates_h, color="#f39c12", alpha=0.8, label="WEAK")
    ax2.set_yticks(y)
    ax2.set_yticklabels(names_h, fontsize=8)
    ax2.set_xlabel("Rate (%)", fontsize=11)
    ax2.set_title("Highest Gate Count Only (above m*)", fontsize=12)
    ax2.legend(fontsize=9)
    ax2.invert_yaxis()
    ax2.grid(True, alpha=0.3, axis="x")

    fig.suptitle("Phase 3: Full Dieharder Battery — Per-Test Failure Analysis\n"
                 "(counter mode, 27 test families)",
                 fontsize=14, y=1.01)
    plt.tight_layout()
    path = os.path.join(out_dir, "p3_test_failure_analysis.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    print(f"Saved: {path}")
    plt.close()

    # Print summary of always-failing tests
    print("\n=== Tests that ALWAYS fail (>90% failure rate across all configs) ===")
    for t in tests_sorted:
        d = test_failures[t]
        rate = d["failed"] / max(d["total"], 1) * 100
        if rate > 90:
            print(f"  {t}: {rate:.0f}% failed ({d['failed']}/{d['total']})")

    print("\n=== Tests that fail at HIGH gate counts (>50% failure rate) ===")
    for t in tests_sorted:
        d = test_failures_high.get(t, {"total": 0, "failed": 0, "weak": 0})
        if d["total"] > 0:
            rate = d["failed"] / d["total"] * 100
            if rate > 50:
                print(f"  {t}: {rate:.0f}% failed ({d['failed']}/{d['total']})")


def plot_all_modes_summary(ctr_data, ofb_data, relkey_data, out_dir):
    """Summary comparison of all 3 stream modes at the same widths."""
    fig, ax = plt.subplots(1, 1, figsize=(10, 6))

    for data, label, color, marker, ls in [
        (ctr_data, "CTR (counter)", "#e74c3c", "o", "-"),
        (ofb_data, "OFB (iterate)", "#3498db", "s", "--"),
        (relkey_data, "Related-key", "#2ecc71", "D", "-."),
    ]:
        if data is None:
            continue
        mstar = data.get("m_star", {})
        widths = sorted([int(w) for w in mstar.keys() if mstar[w] is not None])
        vals = [mstar[str(w)] for w in widths]
        if widths:
            ax.plot(widths, vals, f"{marker}{ls}", color=color, label=label,
                    linewidth=2.5, markersize=10)

    ref_w = np.linspace(30, 130, 100)
    ax.plot(ref_w, 20 * ref_w, ":", color="gray", alpha=0.5, label="m = 20n")

    ax.set_xlabel("Width n (wires)", fontsize=13)
    ax.set_ylabel("m*(n) (95% threshold)", fontsize=13)
    ax.set_title("Pseudorandomness Threshold by Mode\n(7 core dieharder tests)", fontsize=14)
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    path = os.path.join(out_dir, "mstar_all_modes.png")
    fig.savefig(path, dpi=150)
    print(f"Saved: {path}")
    plt.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--p1p2", required=True, help="Phase 1+2 combined (counter)")
    parser.add_argument("--p3", default=None, help="Phase 3 (full battery)")
    parser.add_argument("--p5", default=None, help="Phase 5 (iterate)")
    parser.add_argument("--p6", default=None, help="Phase 6 (related-key)")
    parser.add_argument("--out", default="rng_sweep/plots_comparison")
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)

    ctr = load(args.p1p2)
    ofb = load(args.p5) if args.p5 else None
    p3 = load(args.p3) if args.p3 else None
    relkey = load(args.p6) if args.p6 else None

    if ofb:
        plot_ctr_vs_ofb(ctr, ofb, args.out)
        plot_mstar_comparison(ctr, ofb, args.out)

    if p3:
        plot_p3_test_failure_analysis(p3, args.out)

    plot_all_modes_summary(ctr, ofb, relkey, args.out)

    print(f"\nAll comparison plots saved to: {args.out}")


if __name__ == "__main__":
    main()
