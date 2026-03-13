"""Interface to the local_mixing Rust binary for heatmap metrics."""

import json
import subprocess
import os
from pathlib import Path


def _find_binary():
    """Locate the local_mixing_bin release binary."""
    candidates = [
        Path(__file__).resolve().parents[2] / "target" / "release" / "local_mixing_bin",
        Path("target/release/local_mixing_bin"),
    ]
    for p in candidates:
        if p.exists():
            return str(p)
    raise FileNotFoundError(
        "local_mixing_bin not found. Run: cargo build --release"
    )


def _run(args: list[str], cwd: str | None = None) -> dict:
    """Run the binary and parse JSON stdout."""
    bin_path = _find_binary()
    cmd = [bin_path] + args
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        cwd=cwd or str(Path(__file__).resolve().parents[2]),
    )
    if result.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\n{result.stderr}")
    return json.loads(result.stdout)


class HeatmapRunner:
    """Run heatmap metrics via the Rust binary."""

    def __init__(self, c1_path: str, c2_path: str, num_wires: int, raw: bool = True):
        self.c1 = str(Path(c1_path).resolve())
        self.c2 = str(Path(c2_path).resolve())
        self.n = num_wires
        self.raw_flag = ["--raw"] if raw else []

    def _base_args(self, cmd: str) -> list[str]:
        return [cmd, "--c1", self.c1, "--c2", self.c2, "-n", str(self.n)] + self.raw_flag

    def run_standard(self, inputs: int = 1000) -> dict:
        args = ["heatmap", "--c1", self.c1, "--c2", self.c2,
                "-n", str(self.n), "-i", str(inputs)] + self.raw_flag
        return _run(args)

    def run_gradient(self, inputs: int = 1000) -> dict:
        args = self._base_args("heatmap-gradient") + ["-i", str(inputs)]
        return _run(args)

    def run_hw(self, inputs: int = 1000) -> dict:
        args = self._base_args("heatmap-hw") + ["-i", str(inputs)]
        return _run(args)

    def run_windowed(self, inputs: int = 1000, d: int = 20) -> dict:
        args = self._base_args("heatmap-windowed") + ["-i", str(inputs), "-d", str(d)]
        return _run(args)

    def run_correlated(self, k: int = 8, bases: int = 100) -> dict:
        args = self._base_args("heatmap-correlated") + ["-k", str(k), "--bases", str(bases)]
        return _run(args)
