#!/usr/bin/env python3
"""
Generate heatmap + vector field + characteristic scale for two circuits,
plus a random-circuit average baseline.

Defaults:
- inputs: 10000
- random samples: 20
- random mode: pair (randomize both circuits)
- characteristic scale: 1/e decay of 2D autocorrelation (mean-centered similarity)
"""

import argparse
import json
import math
import os
import subprocess
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import numpy as np
import matplotlib.pyplot as plt

LOCAL_MIXING_DIR = Path(__file__).resolve().parents[1]
DEFAULT_OUT_DIR = LOCAL_MIXING_DIR / "experiments" / datetime.now().strftime("%Y-%m-%d") / "heatmap_characteristic"


def run_cmd(cmd: List[str], cwd: Optional[Path] = None, timeout: int = 1800) -> str:
    result = subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\n{result.stderr}")
    return result.stdout


def find_or_build_binary(bin_path: Optional[str], no_build: bool) -> Path:
    if bin_path:
        return Path(bin_path)

    release_bin = LOCAL_MIXING_DIR / "target" / "release" / "local_mixing_bin"
    debug_bin = LOCAL_MIXING_DIR / "target" / "debug" / "local_mixing_bin"

    if release_bin.exists():
        return release_bin
    if debug_bin.exists():
        return debug_bin

    if no_build:
        raise RuntimeError("local_mixing_bin not found and --no-build set")

    # Build release binary
    print("local_mixing_bin not found; building release binary...")
    run_cmd(["cargo", "build", "--release"], cwd=LOCAL_MIXING_DIR)

    if release_bin.exists():
        return release_bin
    raise RuntimeError("Failed to build local_mixing_bin")


def parse_json_from_stdout(stdout: str) -> Any:
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        lines = [line.strip() for line in stdout.splitlines() if line.strip()]
        for line in reversed(lines):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    raise RuntimeError("Could not parse JSON from command output")


def run_heatmap(
    bin_path: Path,
    c1_path: Path,
    c2_path: Path,
    num_wires: int,
    inputs: int,
    raw: bool = False,
) -> Any:
    cmd = [
        str(bin_path),
        "heatmap",
        "--c1",
        str(c1_path),
        "--c2",
        str(c2_path),
        "--num_wires",
        str(num_wires),
        "--inputs",
        str(inputs),
    ]
    if raw:
        cmd.append("--raw")

    stdout = run_cmd(cmd, cwd=LOCAL_MIXING_DIR)
    return parse_json_from_stdout(stdout)


def heatmap_to_grid(data: Any) -> Tuple[np.ndarray, int, int]:
    # data can be:
    # - dict with heatmap_data + x_size/y_size
    # - list of triplets [x, y, value]
    # - matrix (list of lists)
    x_size = None
    y_size = None
    heatmap_data = data

    if isinstance(data, dict):
        heatmap_data = data.get("heatmap_data", data)
        x_size = data.get("x_size")
        y_size = data.get("y_size")

    # Triplet format
    if isinstance(heatmap_data, list) and heatmap_data and isinstance(heatmap_data[0], list) and len(heatmap_data[0]) == 3:
        xs = [int(p[0]) for p in heatmap_data]
        ys = [int(p[1]) for p in heatmap_data]
        max_x = max(xs)
        max_y = max(ys)
        if x_size is None:
            x_size = max_x + 1
        if y_size is None:
            y_size = max_y + 1
        grid = np.zeros((x_size, y_size), dtype=float)
        for x, y, val in heatmap_data:
            grid[int(x), int(y)] = float(val)
        return grid, x_size, y_size

    # Matrix format
    if isinstance(heatmap_data, list) and heatmap_data and isinstance(heatmap_data[0], list):
        grid = np.array(heatmap_data, dtype=float)
        x_size = grid.shape[0]
        y_size = grid.shape[1] if grid.ndim > 1 else 0
        return grid, x_size, y_size

    raise RuntimeError("Unsupported heatmap format")


def gate_count_from_file(path: Path) -> int:
    text = path.read_text().strip()
    if not text:
        return 0
    return len([p for p in text.split(";") if p])


def generate_random_circuit_file(bin_path: Path, num_wires: int, length: int) -> Path:
    stdout = run_cmd(
        [str(bin_path), "gen", "--wires", str(num_wires), "--length", str(length)],
        cwd=LOCAL_MIXING_DIR,
    )
    fd, path = tempfile.mkstemp(suffix=".gate", prefix="random_circuit_")
    with os.fdopen(fd, "w") as f:
        f.write(stdout.strip())
    return Path(path)


def downsample_grid(grid: np.ndarray, max_dim: Optional[int]) -> Tuple[np.ndarray, int]:
    if not max_dim:
        return grid, 1
    max_current = max(grid.shape)
    if max_current <= max_dim:
        return grid, 1
    step = int(math.ceil(max_current / max_dim))
    return grid[::step, ::step], step


def autocorr_2d(field: np.ndarray) -> np.ndarray:
    field = field - field.mean()
    fft = np.fft.fft2(field)
    corr = np.fft.ifft2(np.abs(fft) ** 2).real
    corr = np.fft.fftshift(corr)
    if corr.max() > 0:
        corr = corr / corr.max()
    return corr


def radial_profile(corr: np.ndarray) -> Tuple[np.ndarray, np.ndarray]:
    y, x = np.indices(corr.shape)
    cy = (corr.shape[0] - 1) / 2.0
    cx = (corr.shape[1] - 1) / 2.0
    r = np.sqrt((x - cx) ** 2 + (y - cy) ** 2)
    r_int = r.astype(int)

    max_r = r_int.max()
    radial_sum = np.zeros(max_r + 1, dtype=float)
    radial_count = np.zeros(max_r + 1, dtype=float)

    for i in range(max_r + 1):
        mask = r_int == i
        radial_sum[i] = corr[mask].sum()
        radial_count[i] = mask.sum()

    radial_mean = np.divide(radial_sum, radial_count, out=np.zeros_like(radial_sum), where=radial_count > 0)
    return np.arange(len(radial_mean)), radial_mean


def find_crossing(r: np.ndarray, v: np.ndarray, threshold: float) -> Optional[float]:
    for i in range(1, len(v)):
        if v[i] <= threshold:
            v0, v1 = v[i - 1], v[i]
            r0, r1 = r[i - 1], r[i]
            if v1 == v0:
                return float(r1)
            t = (threshold - v0) / (v1 - v0)
            return float(r0 + t * (r1 - r0))
    return None


def characteristic_scale(similarity: np.ndarray, autocorr_max: Optional[int]) -> Dict[str, Any]:
    sim_ds, step = downsample_grid(similarity, autocorr_max)
    corr = autocorr_2d(sim_ds)
    r, radial = radial_profile(corr)

    scale_1e = find_crossing(r, radial, 1.0 / math.e)
    scale_half = find_crossing(r, radial, 0.5)

    # Axis-specific (central row/col of corr)
    cy = corr.shape[0] // 2
    cx = corr.shape[1] // 2
    row = corr[cy, :]
    col = corr[:, cx]

    x_dist = np.arange(len(row)) - cx
    y_dist = np.arange(len(col)) - cy

    # Use positive direction only for axis scales
    x_pos = row[cx:]
    y_pos = col[cy:]
    x_r = np.arange(len(x_pos))
    y_r = np.arange(len(y_pos))

    x_1e = find_crossing(x_r, x_pos, 1.0 / math.e)
    y_1e = find_crossing(y_r, y_pos, 1.0 / math.e)

    return {
        "autocorr_downsample_step": step,
        "radial_profile": {
            "r": r.tolist(),
            "corr": radial.tolist(),
        },
        "scale_1e": scale_1e,
        "scale_half": scale_half,
        "axis_scale_1e": {
            "x": x_1e,
            "y": y_1e,
        },
    }


def vector_field(similarity: np.ndarray) -> Tuple[np.ndarray, np.ndarray]:
    # Gradient: axis 0 = x (circuit 1 index), axis 1 = y (circuit 2 index)
    gy, gx = np.gradient(similarity)
    return gx, gy


def plot_heatmap(grid: np.ndarray, output_path: Path, title: str, xlabel: str, ylabel: str) -> None:
    plt.figure(figsize=(10, 8))
    plt.imshow(
        grid,
        cmap="RdYlGn_r",
        aspect="auto",
        interpolation="nearest",
        origin="lower",
        vmin=0.0,
        vmax=1.0,
    )
    plt.colorbar(label="Normalized Hamming Distance")
    plt.title(title)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)
    mean_val = float(np.mean(grid))
    plt.text(
        0.98,
        0.02,
        f"Mean = {mean_val:.3f}",
        ha="right",
        va="bottom",
        transform=plt.gca().transAxes,
        color="black",
        bbox=dict(facecolor="lightgray", alpha=0.7, boxstyle="round,pad=0.3"),
    )
    plt.tight_layout()
    plt.savefig(output_path, dpi=300)
    plt.close()


def plot_vector_field(
    similarity: np.ndarray,
    vx: np.ndarray,
    vy: np.ndarray,
    output_path: Path,
    title: str,
    xlabel: str,
    ylabel: str,
    quiver_max: int,
) -> Dict[str, Any]:
    # Downsample for quiver
    max_dim = max(similarity.shape)
    step = max(1, int(math.ceil(max_dim / quiver_max)))

    sim_ds = similarity[::step, ::step]
    vx_ds = vx[::step, ::step]
    vy_ds = vy[::step, ::step]

    x = np.arange(sim_ds.shape[0]) * step
    y = np.arange(sim_ds.shape[1]) * step
    X, Y = np.meshgrid(y, x)  # note: X=cols (y-axis), Y=rows (x-axis)

    plt.figure(figsize=(10, 8))
    plt.imshow(
        similarity,
        cmap="RdYlGn",
        aspect="auto",
        interpolation="nearest",
        origin="lower",
        vmin=0.0,
        vmax=1.0,
    )
    plt.colorbar(label="Similarity (1 - distance)")

    plt.quiver(
        X,
        Y,
        vx_ds,
        vy_ds,
        color="black",
        alpha=0.6,
        scale=50,
        width=0.002,
    )

    plt.title(title)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)
    plt.tight_layout()
    plt.savefig(output_path, dpi=300)
    plt.close()

    return {
        "step": step,
        "x_coords": x.tolist(),
        "y_coords": y.tolist(),
        "vx": vx_ds.tolist(),
        "vy": vy_ds.tolist(),
        "source": "similarity",
    }


def save_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as f:
        json.dump(data, f, indent=2)


def compute_and_save_bundle(
    tag: str,
    grid: np.ndarray,
    xlabel: str,
    ylabel: str,
    out_dir: Path,
    quiver_max: int,
    autocorr_max: Optional[int],
) -> Dict[str, Any]:
    stats = {
        "mean": float(np.mean(grid)),
        "std": float(np.std(grid)),
        "min": float(np.min(grid)),
        "max": float(np.max(grid)),
    }

    similarity = 1.0 - grid
    stats["characteristic_scale"] = characteristic_scale(similarity, autocorr_max)

    # Heatmap plot (distance)
    plot_heatmap(
        grid,
        out_dir / f"{tag}_heatmap.png",
        title=f"Heatmap ({tag})",
        xlabel=xlabel,
        ylabel=ylabel,
    )

    # Vector field (similarity gradient)
    vx, vy = vector_field(similarity)
    vf = plot_vector_field(
        similarity,
        vx,
        vy,
        out_dir / f"{tag}_vector_field.png",
        title=f"Vector Field ({tag})",
        xlabel=xlabel,
        ylabel=ylabel,
        quiver_max=quiver_max,
    )

    save_json(out_dir / f"{tag}_vector_field.json", vf)
    save_json(out_dir / f"{tag}_stats.json", stats)

    return stats


def main() -> None:
    parser = argparse.ArgumentParser(description="Heatmap + characteristic scale + vector field")
    parser.add_argument("--c1", required=True, help="Path to circuit 1")
    parser.add_argument("--c2", required=True, help="Path to circuit 2")
    parser.add_argument("-n", "--num-wires", type=int, required=True, help="Number of wires")
    parser.add_argument("-i", "--inputs", type=int, default=10000, help="Random inputs per heatmap")
    parser.add_argument("--random-samples", type=int, default=20, help="Number of random circuit samples")
    parser.add_argument(
        "--random-mode",
        choices=["pair", "c1", "c2"],
        default="pair",
        help="pair=randomize both circuits; c1=randomize c1 only; c2=randomize c2 only",
    )
    parser.add_argument("--raw", action="store_true", help="Disable canonicalization")
    parser.add_argument("--bin", help="Path to local_mixing_bin")
    parser.add_argument("--no-build", action="store_true", help="Do not build binary if missing")
    parser.add_argument("--out-dir", default=str(DEFAULT_OUT_DIR), help="Output directory")
    parser.add_argument("--quiver-max", type=int, default=64, help="Max quiver grid size")
    parser.add_argument(
        "--autocorr-max",
        type=int,
        default=512,
        help="Max dimension for autocorrelation (downsample if larger)",
    )

    args = parser.parse_args()

    bin_path = find_or_build_binary(args.bin, args.no_build)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    c1_path = Path(args.c1)
    c2_path = Path(args.c2)

    # Base heatmap
    base_json = run_heatmap(bin_path, c1_path, c2_path, args.num_wires, args.inputs, raw=args.raw)
    save_json(out_dir / "base_heatmap.json", base_json)
    base_grid, _, _ = heatmap_to_grid(base_json)

    base_stats = compute_and_save_bundle(
        "base",
        base_grid,
        xlabel="Circuit 2 Gate Index",
        ylabel="Circuit 1 Gate Index",
        out_dir=out_dir,
        quiver_max=args.quiver_max,
        autocorr_max=args.autocorr_max,
    )

    # Random average
    len1 = gate_count_from_file(c1_path)
    len2 = gate_count_from_file(c2_path)

    avg_grid = None
    for i in range(args.random_samples):
        if args.random_mode == "pair":
            r1 = generate_random_circuit_file(bin_path, args.num_wires, len1)
            r2 = generate_random_circuit_file(bin_path, args.num_wires, len2)
        elif args.random_mode == "c1":
            r1 = generate_random_circuit_file(bin_path, args.num_wires, len1)
            r2 = c2_path
        else:  # c2
            r1 = c1_path
            r2 = generate_random_circuit_file(bin_path, args.num_wires, len2)

        try:
            random_json = run_heatmap(bin_path, r1, r2, args.num_wires, args.inputs, raw=args.raw)
            random_grid, _, _ = heatmap_to_grid(random_json)
            if avg_grid is None:
                avg_grid = random_grid
            else:
                avg_grid += random_grid
        finally:
            # Clean temp files if generated
            if args.random_mode == "pair":
                r1.unlink(missing_ok=True)
                r2.unlink(missing_ok=True)
            elif args.random_mode == "c1":
                r1.unlink(missing_ok=True)
            elif args.random_mode == "c2":
                r2.unlink(missing_ok=True)

        print(f"Random sample {i + 1}/{args.random_samples} complete")

    if avg_grid is None:
        raise RuntimeError("No random samples generated")

    avg_grid = avg_grid / float(args.random_samples)
    save_json(out_dir / "random_avg_heatmap.json", avg_grid.tolist())

    random_stats = compute_and_save_bundle(
        "random_avg",
        avg_grid,
        xlabel="Circuit 2 Gate Index",
        ylabel="Circuit 1 Gate Index",
        out_dir=out_dir,
        quiver_max=args.quiver_max,
        autocorr_max=args.autocorr_max,
    )

    summary = {
        "inputs": args.inputs,
        "random_samples": args.random_samples,
        "random_mode": args.random_mode,
        "base": base_stats,
        "random_avg": random_stats,
    }
    save_json(out_dir / "summary.json", summary)

    print(f"Done. Outputs in {out_dir}")


if __name__ == "__main__":
    main()
