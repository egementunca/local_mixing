"""Visualization functions for heatmap metrics."""

import numpy as np
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
from pathlib import Path


def _grid_from_data(data: dict, scale: float = 1.0) -> tuple[np.ndarray, int, int]:
    """Convert heatmap JSON to 2D numpy array (downsampled for large circuits)."""
    x_size = data["x_size"]
    y_size = data["y_size"]
    y_bins = min(y_size, 500)
    x_bins = x_size

    matrix = np.full((x_bins, y_bins), np.nan)
    counts = np.zeros((x_bins, y_bins), dtype=int)

    for entry in data["heatmap_data"]:
        x_idx = int(entry[0])
        y_raw = entry[1]
        val = entry[2] * scale

        y_idx = int(y_raw * y_bins / y_size)
        y_idx = min(y_idx, y_bins - 1)
        x_idx = min(x_idx, x_bins - 1)

        if np.isnan(matrix[x_idx, y_idx]):
            matrix[x_idx, y_idx] = val
            counts[x_idx, y_idx] = 1
        else:
            matrix[x_idx, y_idx] += val
            counts[x_idx, y_idx] += 1

    mask = counts > 0
    matrix[mask] /= counts[mask]
    return matrix, x_size, y_size


def plot_single(
    data: dict,
    output_path: str,
    title: str,
    num_wires: int = 64,
    cmap: str = "inferno",
    vmin: float | None = None,
    vmax: float | None = None,
    diverging: bool = False,
    scale: float = 1.0,
):
    """Plot a single heatmap metric."""
    matrix, x_size, y_size = _grid_from_data(data, scale=scale)
    valid = matrix[~np.isnan(matrix)]

    if diverging:
        cmap = "RdBu_r"
        abs_max = max(abs(np.nanmin(matrix)), abs(np.nanmax(matrix)))
        if vmin is None:
            vmin = -abs_max
        if vmax is None:
            vmax = abs_max
    else:
        if vmin is None:
            vmin = 0
        if vmax is None:
            vmax = num_wires / 2 if scale != 1.0 else np.nanmax(matrix)

    fig, ax = plt.subplots(figsize=(12, 8))
    im = ax.imshow(
        matrix.T, aspect="auto", origin="lower",
        cmap=cmap, vmin=vmin, vmax=vmax,
        extent=[0, x_size, 0, y_size],
    )
    ax.set_xlabel("Original Circuit (gate index)", fontsize=13)
    ax.set_ylabel("Obfuscated Circuit (gate index)", fontsize=13)

    mean_val = np.nanmean(matrix)
    std_val = np.nanstd(matrix)
    ax.set_title(f"{title}\nmean={mean_val:.3f}, std={std_val:.3f}", fontsize=14)
    plt.colorbar(im, ax=ax)
    plt.tight_layout()
    plt.savefig(output_path, dpi=200, bbox_inches="tight")
    plt.close()
    return {"mean": float(mean_val), "std": float(std_val),
            "min": float(np.nanmin(matrix)), "max": float(np.nanmax(matrix))}


def plot_comparison(
    data_a: dict, data_b: dict,
    output_path: str,
    label_a: str = "RAC", label_b: str = "BTB",
    metric_name: str = "Standard HD",
    num_wires: int = 64,
    cmap: str = "inferno",
    diverging: bool = False,
    scale: float = 1.0,
):
    """Side-by-side comparison of two schemes for the same metric."""
    mat_a, xa, ya = _grid_from_data(data_a, scale=scale)
    mat_b, xb, yb = _grid_from_data(data_b, scale=scale)

    if diverging:
        cmap = "RdBu_r"
        abs_max = max(
            abs(np.nanmin(mat_a)), abs(np.nanmax(mat_a)),
            abs(np.nanmin(mat_b)), abs(np.nanmax(mat_b)),
        )
        vmin, vmax = -abs_max, abs_max
    else:
        vmin = 0
        vmax = num_wires / 2 if scale != 1.0 else max(np.nanmax(mat_a), np.nanmax(mat_b))

    fig = plt.figure(figsize=(22, 9))
    gs = gridspec.GridSpec(1, 3, width_ratios=[1, 1, 0.05], wspace=0.25)

    mean_a = np.nanmean(mat_a)
    mean_b = np.nanmean(mat_b)

    ax1 = fig.add_subplot(gs[0])
    im1 = ax1.imshow(mat_a.T, aspect="auto", origin="lower", cmap=cmap,
                      vmin=vmin, vmax=vmax, extent=[0, xa, 0, ya])
    ax1.set_xlabel("Original (gate index)", fontsize=13)
    ax1.set_ylabel(f"{label_a} Obfuscated (gate index)", fontsize=13)
    ax1.set_title(f"{label_a}: {metric_name}\nmean={mean_a:.3f}", fontsize=13)

    ax2 = fig.add_subplot(gs[1])
    im2 = ax2.imshow(mat_b.T, aspect="auto", origin="lower", cmap=cmap,
                      vmin=vmin, vmax=vmax, extent=[0, xb, 0, yb])
    ax2.set_xlabel("Original (gate index)", fontsize=13)
    ax2.set_ylabel(f"{label_b} Obfuscated (gate index)", fontsize=13)
    ax2.set_title(f"{label_b}: {metric_name}\nmean={mean_b:.3f}", fontsize=13)

    cax = fig.add_subplot(gs[2])
    fig.colorbar(im2, cax=cax)

    fig.suptitle(f"{metric_name}: {label_a} vs {label_b} (n={num_wires})",
                 fontsize=16, fontweight="bold", y=1.02)
    plt.tight_layout()
    plt.savefig(output_path, dpi=200, bbox_inches="tight")
    plt.close()


def plot_dashboard(
    metrics: dict[str, dict],
    output_path: str,
    label: str = "RAC",
    num_wires: int = 64,
):
    """2x2 dashboard of all metrics for one scheme."""
    fig, axes = plt.subplots(2, 2, figsize=(20, 16))
    configs = [
        ("standard", "Standard HD", "inferno", False, num_wires),
        ("gradient", "Gradient Distance", "RdBu_r", True, 1.0),
        ("hamming_weight", "HW Difference", "inferno", False, 1.0),
        ("windowed_min", "Windowed Min (d=50)", "inferno", False, num_wires),
    ]

    for ax, (key, title, cmap, diverging, scale) in zip(axes.flat, configs):
        if key not in metrics:
            ax.set_visible(False)
            continue

        data = metrics[key]
        matrix, x_size, y_size = _grid_from_data(data, scale=scale)

        if diverging:
            abs_max = max(abs(np.nanmin(matrix)), abs(np.nanmax(matrix)))
            vmin, vmax = -abs_max, abs_max
        else:
            vmin, vmax = 0, np.nanmax(matrix)

        im = ax.imshow(matrix.T, aspect="auto", origin="lower",
                       cmap=cmap, vmin=vmin, vmax=vmax,
                       extent=[0, x_size, 0, y_size])
        mean_val = np.nanmean(matrix)
        ax.set_title(f"{title}\nmean={mean_val:.4f}", fontsize=12)
        ax.set_xlabel("Original gate", fontsize=10)
        ax.set_ylabel("Obfuscated gate", fontsize=10)
        plt.colorbar(im, ax=ax, fraction=0.046, pad=0.04)

    fig.suptitle(f"Heatmap Dashboard: {label} (n={num_wires})",
                 fontsize=16, fontweight="bold")
    plt.tight_layout()
    plt.savefig(output_path, dpi=200, bbox_inches="tight")
    plt.close()
