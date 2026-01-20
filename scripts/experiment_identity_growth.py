#!/usr/bin/env python3
"""
Identity Growth sweep runner.

Runs multiple grow-identity settings, saves configs/commands/logs/outputs,
and writes a summary.json for later comparison in butterfly experiments.
"""
import argparse
import json
import os
import re
import subprocess
from dataclasses import dataclass, asdict
from datetime import datetime
from typing import Dict, List, Optional

# ============================================================================
# CONFIGURATION
# ============================================================================

DATE = datetime.now().strftime("%Y-%m-%d")
RUN_ID = "identity_growth_32w"
BASE_DIR = os.path.join("experiments", DATE, RUN_ID)

# Identity growth base parameters
WIRES = 32
ROUNDS = 5
PLACEMENTS = 12
DIFFUSION = 100000
TEMPLATE_WIDTH_MIN = 3
TEMPLATE_WIDTH_MAX = 6
TEMPLATE_ATTEMPTS = 30
TEMPLATE_SOURCE = "mixed"  # perm, template-db, mixed

# DB paths (relative to local_mixing/)
LMDB_PATH = "./db"
TEMPLATE_DB_PATH = "data/collection.lmdb"

CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]


@dataclass
class GrowthExperiment:
    name: str
    template_min_gates: int = 6
    template_max_gates: int = 30
    conjugation_min: int = 0
    conjugation_max: int = 0
    hardness_passes: int = 0
    hardness_ratio: float = 0.0
    compression_budget: int = 50
    min_survival_ratio: float = 0.3

    def to_config_dict(self, wires: int) -> dict:
        return {
            "target_width": wires,
            "template_width_min": TEMPLATE_WIDTH_MIN,
            "template_width_max": TEMPLATE_WIDTH_MAX,
            "template_gate_count_min": self.template_min_gates,
            "template_gate_count_max": self.template_max_gates,
            "template_attempts": TEMPLATE_ATTEMPTS,
            "template_conjugation_depth_min": self.conjugation_min,
            "template_conjugation_depth_max": self.conjugation_max,
            "template_hardness_passes": self.hardness_passes,
            "template_min_reducer_ratio": self.hardness_ratio,
            "placements_per_round": PLACEMENTS,
            "rounds": ROUNDS,
            "diffusion_passes": DIFFUSION,
            "compression_budget": self.compression_budget,
            "min_survival_ratio": self.min_survival_ratio,
        }


EXPERIMENTS: List[GrowthExperiment] = [
    GrowthExperiment(name="01_baseline_30"),
    GrowthExperiment(name="02_conj2_4_hard70", conjugation_min=2, conjugation_max=4,
                     hardness_passes=1, hardness_ratio=0.70),
    GrowthExperiment(name="03_conj4_8_hard80", conjugation_min=4, conjugation_max=8,
                     hardness_passes=2, hardness_ratio=0.80),
    GrowthExperiment(name="04_max40_conj4_8_hard80", template_max_gates=40,
                     conjugation_min=4, conjugation_max=8,
                     hardness_passes=2, hardness_ratio=0.80),
    GrowthExperiment(name="05_max40_conj6_10_hard85", template_max_gates=40,
                     conjugation_min=6, conjugation_max=10,
                     hardness_passes=3, hardness_ratio=0.85),
    GrowthExperiment(name="06_comp_aggressive", conjugation_min=2, conjugation_max=4,
                     hardness_passes=1, hardness_ratio=0.70,
                     compression_budget=150, min_survival_ratio=0.20),
]


def log(msg: str, level: str = "INFO") -> None:
    timestamp = datetime.now().strftime("%H:%M:%S")
    print(f"[{timestamp}] [{level}] {msg}")


def count_gates(circuit_file: str) -> int:
    try:
        with open(circuit_file, "r") as f:
            content = f.read().strip()
            return content.count(";") if content else 0
    except Exception:
        return 0


def parse_metrics(log_text: str) -> Dict[str, Optional[object]]:
    metrics: Dict[str, Optional[object]] = {
        "final_gates": None,
        "templates_placed": None,
        "templates_skipped": None,
        "wire_coverage": None,
        "compression_ratios": [],
    }
    for line in log_text.splitlines():
        if "Stage C: Compressed" in line:
            m = re.search(r"ratio:\s*([0-9.]+)", line)
            if m:
                metrics["compression_ratios"].append(float(m.group(1)))
        elif line.strip().startswith("Final gates:"):
            m = re.search(r"Final gates:\s*(\d+)", line)
            if m:
                metrics["final_gates"] = int(m.group(1))
        elif line.strip().startswith("Templates placed:"):
            m = re.search(r"Templates placed:\s*(\d+)", line)
            if m:
                metrics["templates_placed"] = int(m.group(1))
        elif line.strip().startswith("Templates skipped:"):
            m = re.search(r"Templates skipped:\s*(\d+)", line)
            if m:
                metrics["templates_skipped"] = int(m.group(1))
        elif line.strip().startswith("Wire coverage:"):
            m = re.search(r"Wire coverage:\s*([0-9.]+)%", line)
            if m:
                metrics["wire_coverage"] = float(m.group(1))
    return metrics


def run_command(cmd: List[str], log_path: str, dry_run: bool = False) -> bool:
    cmd_str = " ".join(cmd)
    log(f"Running: {cmd_str}")
    if dry_run:
        log("  [DRY RUN] Skipping execution", "DEBUG")
        return True
    result = subprocess.run(cmd, capture_output=True, text=True)
    with open(log_path, "w") as f:
        f.write(result.stdout)
        if result.stderr:
            f.write("\n[stderr]\n")
            f.write(result.stderr)
    if result.returncode != 0:
        log(f"Command failed (exit code {result.returncode})", "ERROR")
        return False
    return True


def run_experiment(exp: GrowthExperiment, base_dir: str, dry_run: bool = False) -> dict:
    exp_dir = os.path.join(base_dir, exp.name)
    os.makedirs(exp_dir, exist_ok=True)

    config_path = os.path.join(exp_dir, "config.json")
    with open(config_path, "w") as f:
        json.dump(exp.to_config_dict(WIRES), f, indent=2)

    output_path = os.path.join(exp_dir, "identity.gate")
    log_path = os.path.join(exp_dir, "run.log")
    cmd_path = os.path.join(exp_dir, "command.txt")

    cmd = CARGO_RUN + [
        "grow-identity",
        "--config", config_path,
        "--wires", str(WIRES),
        "--db", LMDB_PATH,
        "--template-source", TEMPLATE_SOURCE,
        "--template-db", TEMPLATE_DB_PATH,
        "--output", output_path,
    ]

    with open(cmd_path, "w") as f:
        f.write(" ".join(cmd) + "\n")

    start = datetime.now()
    success = run_command(cmd, log_path, dry_run=dry_run)
    elapsed = (datetime.now() - start).total_seconds()

    result = {
        "name": exp.name,
        "status": "success" if success else "failed",
        "time_sec": round(elapsed, 1),
        "config_path": config_path,
        "output_path": output_path,
        "command": " ".join(cmd),
    }

    if not success:
        return result

    if os.path.exists(output_path):
        gate_count = count_gates(output_path)
        result["gate_count_file"] = gate_count
    else:
        result["gate_count_file"] = None

    with open(log_path, "r") as f:
        log_text = f.read()
    metrics = parse_metrics(log_text)
    result.update(metrics)

    return result


def main() -> None:
    parser = argparse.ArgumentParser(description="Identity Growth experiment sweep")
    parser.add_argument("--dry-run", action="store_true", help="Show commands without running")
    parser.add_argument("--experiments", type=str, help="Comma-separated experiment names to run")
    args = parser.parse_args()

    os.makedirs(BASE_DIR, exist_ok=True)

    log("=" * 60)
    log("Identity Growth Sweep")
    log(f"  Wires: {WIRES}")
    log(f"  Output: {BASE_DIR}")
    log(f"  Mode: {'DRY RUN' if args.dry_run else 'RUN'}")
    log("=" * 60)

    experiments = EXPERIMENTS
    if args.experiments:
        names = set(x.strip() for x in args.experiments.split(","))
        experiments = [e for e in EXPERIMENTS if e.name in names]
        log(f"Running {len(experiments)} selected experiments")

    results = {
        "wires": WIRES,
        "base_config": {
            "rounds": ROUNDS,
            "placements_per_round": PLACEMENTS,
            "diffusion_passes": DIFFUSION,
            "template_width_min": TEMPLATE_WIDTH_MIN,
            "template_width_max": TEMPLATE_WIDTH_MAX,
            "template_attempts": TEMPLATE_ATTEMPTS,
            "template_source": TEMPLATE_SOURCE,
            "lmdb_path": LMDB_PATH,
            "template_db_path": TEMPLATE_DB_PATH,
        },
        "experiments": {},
    }

    for i, exp in enumerate(experiments, 1):
        log("")
        log("=" * 60)
        log(f"[{i}/{len(experiments)}] {exp.name}")
        log("=" * 60)
        results["experiments"][exp.name] = run_experiment(exp, BASE_DIR, dry_run=args.dry_run)

    summary_path = os.path.join(BASE_DIR, "summary.json")
    with open(summary_path, "w") as f:
        json.dump(results, f, indent=2)

    log("")
    log("=" * 60)
    log("SWEEP COMPLETE")
    log("=" * 60)
    log(f"Summary saved to: {summary_path}")


if __name__ == "__main__":
    main()
