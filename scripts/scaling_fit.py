#!/usr/bin/env python3
"""
Fit scaling law m*(n) from RNG sweep data.

Tries several functional forms:
  - Linear: m*(n) = a * n
  - Affine: m*(n) = a * n + b
  - n*log(n): m*(n) = a * n * log(n) + b
  - Power law: m*(n) = a * n^alpha

Outputs fit parameters, R^2 values, and a comparison plot.

Usage:
    python scripts/scaling_fit.py --out rng_sweep/plots_comparison
"""

import argparse
import os
import numpy as np
from scipy.optimize import curve_fit

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt


# Phase 1+2 refined m*(n) estimates (CTR mode, 95% threshold)
CTR_DATA = {
    32: 525,
    48: 850,
    64: 1200,
    96: 2000,
    128: 2500,
}

OFB_DATA = {
    32: 400,
    48: 500,
    64: 800,
    96: 1500,
    128: 2000,
}


def fit_and_report(name, n, m_star):
    """Fit several models and report results."""
    n = np.array(n, dtype=float)
    m = np.array(m_star, dtype=float)

    results = []

    # 1. Linear through origin: m = a*n
    def linear(n, a):
        return a * n
    popt, _ = curve_fit(linear, n, m)
    m_pred = linear(n, *popt)
    ss_res = np.sum((m - m_pred) ** 2)
    ss_tot = np.sum((m - np.mean(m)) ** 2)
    r2 = 1 - ss_res / ss_tot
    a_val = popt[0]
    results.append(("m = a*n", f"a={a_val:.2f}", r2, lambda x, a=a_val: a * x))

    # 2. Affine: m = a*n + b
    popt, _ = curve_fit(lambda n, a, b: a * n + b, n, m)
    m_pred = popt[0] * n + popt[1]
    ss_res = np.sum((m - m_pred) ** 2)
    r2 = 1 - ss_res / ss_tot
    a_val, b_val = popt
    results.append(("m = a*n + b", f"a={a_val:.2f}, b={b_val:.0f}", r2,
                     lambda x, a=a_val, b=b_val: a * x + b))

    # 3. n*log(n): m = a*n*log(n) + b
    popt, _ = curve_fit(lambda n, a, b: a * n * np.log(n) + b, n, m)
    m_pred = popt[0] * n * np.log(n) + popt[1]
    ss_res = np.sum((m - m_pred) ** 2)
    r2 = 1 - ss_res / ss_tot
    a_val, b_val = popt
    results.append(("m = a*n*ln(n) + b", f"a={a_val:.2f}, b={b_val:.0f}", r2,
                     lambda x, a=a_val, b=b_val: a * x * np.log(x) + b))

    # 4. Power law: m = a * n^alpha
    popt, _ = curve_fit(lambda n, a, alpha: a * n ** alpha, n, m, p0=[1, 1.1])
    m_pred = popt[0] * n ** popt[1]
    ss_res = np.sum((m - m_pred) ** 2)
    r2 = 1 - ss_res / ss_tot
    a_val, alpha_val = popt
    results.append((f"m = a*n^α", f"a={a_val:.2f}, α={alpha_val:.3f}", r2,
                     lambda x, a=a_val, al=alpha_val: a * x ** al))

    print(f"\n{'='*60}")
    print(f"Scaling fits for {name}")
    print(f"{'='*60}")
    print(f"{'Model':<20} {'Parameters':<25} {'R²':>8}")
    print("-" * 55)
    for model_name, params, r2, _ in results:
        print(f"{model_name:<20} {params:<25} {r2:>8.5f}")

    return results


def plot_fits(ctr_results, ofb_results, out_dir):
    """Plot data points with fit curves."""
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

    n_plot = np.linspace(20, 160, 200)

    for ax, data, results, title in [
        (ax1, CTR_DATA, ctr_results, "CTR (Counter) Mode"),
        (ax2, OFB_DATA, ofb_results, "OFB (Iterate) Mode"),
    ]:
        ns = sorted(data.keys())
        ms = [data[n] for n in ns]
        ax.scatter(ns, ms, s=120, c="#e74c3c", zorder=5, edgecolors="black", linewidths=1.5)

        colors = ["#3498db", "#2ecc71", "#9b59b6", "#e67e22"]
        for i, (model_name, params, r2, func) in enumerate(results):
            m_pred = func(n_plot)
            ax.plot(n_plot, m_pred, color=colors[i], linewidth=2,
                    label=f"{model_name} (R²={r2:.4f})", alpha=0.8)

        ax.set_xlabel("Width n (wires)", fontsize=13)
        ax.set_ylabel("m*(n)", fontsize=13)
        ax.set_title(title, fontsize=14, fontweight="bold")
        ax.legend(fontsize=9, loc="upper left")
        ax.grid(True, alpha=0.3)

    fig.suptitle("Scaling Law Fits for Pseudorandomness Threshold m*(n)", fontsize=15, y=1.01)
    plt.tight_layout()
    path = os.path.join(out_dir, "scaling_law_fits.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    print(f"\nSaved: {path}")
    plt.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="rng_sweep/plots_comparison")
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)

    ctr_n = sorted(CTR_DATA.keys())
    ctr_m = [CTR_DATA[n] for n in ctr_n]
    ctr_results = fit_and_report("CTR mode", ctr_n, ctr_m)

    ofb_n = sorted(OFB_DATA.keys())
    ofb_m = [OFB_DATA[n] for n in ofb_n]
    ofb_results = fit_and_report("OFB mode", ofb_n, ofb_m)

    plot_fits(ctr_results, ofb_results, args.out)


if __name__ == "__main__":
    main()
