"""Generate full heatmap analysis report with plots and statistics."""

import json
import time
from pathlib import Path

from .runner import HeatmapRunner
from .plotting import plot_single, plot_comparison, plot_dashboard


def generate_report(
    c1_path: str,
    c2_path: str,
    num_wires: int,
    label: str,
    output_dir: str,
    inputs: int = 10000,
    window_d: int = 50,
):
    """Run all metrics for one scheme and generate plots + statistics.

    Returns dict of statistics for each metric.
    """
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)

    runner = HeatmapRunner(c1_path, c2_path, num_wires)
    stats = {}

    # --- Standard HD ---
    print(f"[{label}] Running standard heatmap ({inputs} inputs)...")
    t0 = time.time()
    data_std = runner.run_standard(inputs=inputs)
    print(f"  done in {time.time() - t0:.1f}s")
    (out / "standard.json").write_text(json.dumps(data_std))
    stats["standard"] = plot_single(
        data_std, str(out / "standard.png"),
        title=f"{label}: Standard HD (n={num_wires})",
        num_wires=num_wires, scale=num_wires,
    )

    # --- Gradient ---
    print(f"[{label}] Running gradient heatmap ({inputs} inputs)...")
    t0 = time.time()
    data_grad = runner.run_gradient(inputs=inputs)
    print(f"  done in {time.time() - t0:.1f}s")
    (out / "gradient.json").write_text(json.dumps(data_grad))
    stats["gradient"] = plot_single(
        data_grad, str(out / "gradient.png"),
        title=f"{label}: Gradient Distance (n={num_wires})",
        num_wires=num_wires, diverging=True,
    )

    # --- Hamming Weight ---
    print(f"[{label}] Running HW heatmap ({inputs} inputs)...")
    t0 = time.time()
    data_hw = runner.run_hw(inputs=inputs)
    print(f"  done in {time.time() - t0:.1f}s")
    (out / "hamming_weight.json").write_text(json.dumps(data_hw))
    stats["hamming_weight"] = plot_single(
        data_hw, str(out / "hamming_weight.png"),
        title=f"{label}: HW Difference (n={num_wires})",
        num_wires=num_wires,
    )

    # --- Windowed Min ---
    print(f"[{label}] Running windowed-min heatmap (d={window_d}, {inputs} inputs)...")
    t0 = time.time()
    data_win = runner.run_windowed(inputs=inputs, d=window_d)
    print(f"  done in {time.time() - t0:.1f}s")
    (out / "windowed_min.json").write_text(json.dumps(data_win))
    stats["windowed_min"] = plot_single(
        data_win, str(out / "windowed_min.png"),
        title=f"{label}: Windowed Min HD (d={window_d}, n={num_wires})",
        num_wires=num_wires, scale=num_wires,
    )

    # --- Dashboard ---
    print(f"[{label}] Generating dashboard...")
    plot_dashboard(
        {"standard": data_std, "gradient": data_grad,
         "hamming_weight": data_hw, "windowed_min": data_win},
        str(out / "dashboard.png"),
        label=label, num_wires=num_wires,
    )

    # --- Save stats ---
    (out / "statistics.json").write_text(json.dumps(stats, indent=2))
    print(f"[{label}] All outputs in {out}/")
    return stats


def generate_comparison(
    dir_a: str,
    dir_b: str,
    label_a: str,
    label_b: str,
    output_dir: str,
    num_wires: int = 64,
):
    """Generate side-by-side comparisons from pre-computed JSON files."""
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    da = Path(dir_a)
    db = Path(dir_b)

    metrics = [
        ("standard", "Standard HD", False, num_wires),
        ("gradient", "Gradient Distance", True, 1.0),
        ("hamming_weight", "HW Difference", False, 1.0),
        ("windowed_min", "Windowed Min HD", False, num_wires),
    ]

    stats_a = json.loads((da / "statistics.json").read_text())
    stats_b = json.loads((db / "statistics.json").read_text())

    for key, name, diverging, scale in metrics:
        json_a = da / f"{key}.json"
        json_b = db / f"{key}.json"
        if not json_a.exists() or not json_b.exists():
            print(f"  Skipping {key} (missing JSON)")
            continue

        data_a = json.loads(json_a.read_text())
        data_b = json.loads(json_b.read_text())

        plot_comparison(
            data_a, data_b,
            str(out / f"comparison_{key}.png"),
            label_a=label_a, label_b=label_b,
            metric_name=name,
            num_wires=num_wires,
            diverging=diverging,
            scale=scale,
        )
        print(f"  Saved comparison_{key}.png")

    # --- Summary table JSON ---
    summary = {}
    for key, name, _, _ in metrics:
        summary[key] = {
            "name": name,
            label_a: stats_a.get(key, {}),
            label_b: stats_b.get(key, {}),
        }
    (out / "summary_table.json").write_text(json.dumps(summary, indent=2))

    # --- Markdown report ---
    md = _build_report_md(label_a, label_b, num_wires, summary)
    (out / "HEATMAPS_V2.md").write_text(md)
    print(f"  Report: {out / 'HEATMAPS_V2.md'}")
    return summary


def _build_report_md(label_a, label_b, num_wires, summary):
    target = num_wires / 2
    lines = [
        f"# Heatmaps 2.0: Extended Obfuscation Quality Metrics",
        f"",
        f"**n={num_wires}, 100 ECA57 gates — {label_a} vs {label_b}**",
        f"",
        f"Metrics proposed by Ran Canetti (Issue #44).",
        f"",
        f"---",
        f"",
        f"## Summary Table",
        f"",
        f"| Metric | {label_a} mean | {label_a} std | {label_b} mean | {label_b} std | Target |",
        f"|--------|{'-'*len(label_a)+'-'*6}|{'-'*len(label_a)+'-'*6}|{'-'*len(label_b)+'-'*6}|{'-'*len(label_b)+'-'*6}|--------|",
    ]
    for key, info in summary.items():
        sa = info.get(label_a, {})
        sb = info.get(label_b, {})
        tgt = f"{target:.1f}" if "Standard" in info["name"] or "Windowed" in info["name"] else "0.0"
        lines.append(
            f"| {info['name']} "
            f"| {sa.get('mean', 'N/A'):.4f} " if isinstance(sa.get('mean'), (int, float)) else f"| {'N/A':>8} "
        )
        # Simpler approach
    # Rebuild cleanly
    lines = [
        f"# Heatmaps 2.0: Extended Obfuscation Quality Metrics",
        f"",
        f"**n={num_wires}, 100 ECA57 gates — {label_a} vs {label_b}**",
        f"",
        f"Metrics proposed by Ran Canetti (Issue #44).",
        f"",
        f"---",
        f"",
        f"## Summary Table",
        f"",
    ]

    # Build table
    header = f"| Metric | {label_a} mean | {label_a} std | {label_b} mean | {label_b} std | Target |"
    sep = "|--------|---------|---------|---------|---------|--------|"
    lines.extend([header, sep])

    for key, info in summary.items():
        sa = info.get(label_a, {})
        sb = info.get(label_b, {})
        is_hd = "Standard" in info["name"] or "Windowed" in info["name"]
        tgt = f"{target:.1f}" if is_hd else "0.0"
        ma = f"{sa['mean']:.4f}" if isinstance(sa.get('mean'), (int, float)) else "N/A"
        sa_std = f"{sa['std']:.4f}" if isinstance(sa.get('std'), (int, float)) else "N/A"
        mb = f"{sb['mean']:.4f}" if isinstance(sb.get('mean'), (int, float)) else "N/A"
        sb_std = f"{sb['std']:.4f}" if isinstance(sb.get('std'), (int, float)) else "N/A"
        lines.append(f"| {info['name']} | {ma} | {sa_std} | {mb} | {sb_std} | {tgt} |")

    lines.extend([
        "",
        "---",
        "",
        "## Standard Hamming Distance (baseline)",
        "",
        "HM[i,j] = E_x[HD(C_i(x), C'_j(x))] — average Hamming distance between",
        "intermediate states at gate i of the original and gate j of the obfuscated circuit.",
        f"Target for ideal obfuscation: {target:.0f} (n/2).",
        "",
        f"### {label_a}",
        f"![{label_a} Standard HD](plots/standard_{label_a.lower()}.png)",
        "",
        f"### {label_b}",
        f"![{label_b} Standard HD](plots/standard_{label_b.lower()}.png)",
        "",
        f"### Comparison",
        f"![Comparison Standard HD](plots/comparison_standard.png)",
        "",
        "---",
        "",
        "## Gradient Distance (local mixing rate)",
        "",
        "Grad[i,j] = HD[i,j] - HD[i-1,j-1], normalized to [-1, 1].",
        "Positive values mean states are diverging; negative means converging.",
        "Uniform ~0 indicates steady-state mixing.",
        "",
        f"### {label_a}",
        f"![{label_a} Gradient](plots/gradient_{label_a.lower()}.png)",
        "",
        f"### {label_b}",
        f"![{label_b} Gradient](plots/gradient_{label_b.lower()}.png)",
        "",
        f"### Comparison",
        f"![Comparison Gradient](plots/comparison_gradient.png)",
        "",
        "---",
        "",
        "## Hamming Weight Difference (shuffle detection)",
        "",
        "|HW(state_a) - HW(state_b)| / n. Wire permutations (shuffles) preserve",
        "Hamming weight, so regions where this is ~0 indicate shuffle-only transformations.",
        "Non-zero values indicate bit flips or non-linear gates.",
        "",
        f"### {label_a}",
        f"![{label_a} HW Diff](plots/hamming_weight_{label_a.lower()}.png)",
        "",
        f"### {label_b}",
        f"![{label_b} HW Diff](plots/hamming_weight_{label_b.lower()}.png)",
        "",
        f"### Comparison",
        f"![Comparison HW Diff](plots/comparison_hamming_weight.png)",
        "",
        "---",
        "",
        "## Windowed Minimum HD (d=50)",
        "",
        "min_{i <= i' <= i+d} HD(C'_{i'}(x), C_j(x)) / n. Smooths out mixing",
        "that only has local effect but cancels itself out. Lower values in",
        "this metric indicate that within a window of d gates, there exists a",
        "close alignment — a potential structural weakness.",
        "",
        f"### {label_a}",
        f"![{label_a} Windowed Min](plots/windowed_min_{label_a.lower()}.png)",
        "",
        f"### {label_b}",
        f"![{label_b} Windowed Min](plots/windowed_min_{label_b.lower()}.png)",
        "",
        f"### Comparison",
        f"![Comparison Windowed Min](plots/comparison_windowed_min.png)",
        "",
        "---",
        "",
        "## Dashboards",
        "",
        f"### {label_a}",
        f"![{label_a} Dashboard](plots/dashboard_{label_a.lower()}.png)",
        "",
        f"### {label_b}",
        f"![{label_b} Dashboard](plots/dashboard_{label_b.lower()}.png)",
        "",
        "---",
        "",
        "## Next Steps",
        "",
        "- **Correlated inputs** (k=8): Fix n-k wires, vary k — test sub-circuit leakage",
        "- **Chunked heatmap**: Zoom into specific gate ranges for local analysis",
        "- **3D heatmap / Linearity probe**: Future work",
        "",
    ])
    return "\n".join(lines)
