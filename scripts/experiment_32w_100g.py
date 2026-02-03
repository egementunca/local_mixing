#!/usr/bin/env python3
"""
ABButterfly Experiment Suite: 32 Wires, 100 Gates
Runs non-SAT and SAT experiments with various parameter combinations.

Usage:
    python3 scripts/experiment_32w_100g.py           # Full experiment suite
    python3 scripts/experiment_32w_100g.py --test    # Quick test mode (8w/20g)
    python3 scripts/experiment_32w_100g.py --dry-run # Show commands without running
"""
import subprocess
import os
import shutil
import sys
import json
import argparse
from datetime import datetime
from dataclasses import dataclass, asdict
from typing import Optional, List

# ============================================================================
# CONFIGURATION
# ============================================================================

DATE = datetime.now().strftime("%Y-%m-%d")
RUN_ID = "32w_100g"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
LMDB_PATH = "./db"
TEMPLATE_DB_PATH = "data/collection.lmdb"

# Circuit parameters
WIRES = 32
GATES = 100

# Test mode uses smaller circuits
TEST_WIRES = 8
TEST_GATES = 20

CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]


@dataclass
class ExperimentConfig:
    name: str
    # Structural parameters
    structure_block_size_min: int = 10
    structure_block_size_max: int = 30
    mix_prob_template: float = 0.40
    mix_prob_swaps: float = 0.50
    mix_prob_patch: float = 0.10
    # Intensity
    single_gate_replacements: int = 500
    shooting_count: int = 10000
    shooting_count_inner: int = 0
    rounds: int = 3
    # Modes
    sat_mode: bool = False
    no_ancilla_mode: bool = False
    single_gate_mode: bool = False
    # Compression
    skip_compression: bool = False
    compression_window_size: int = 100
    compression_window_size_sat: int = 10
    compression_sat_limit: int = 1000
    final_stability_threshold: int = 5
    chunk_split_base: int = 1500
    # Reducer
    reducer_active_wire_limit: int = 6
    reducer_window_sizes: List[int] = None
    # System
    lmdb_path: str = "db"
    # CLI-only flags (not in JSON config)
    bookendless: bool = False
    timeout: int = 3600
    
    
    def __post_init__(self):
        if self.reducer_window_sizes is None:
            self.reducer_window_sizes = [4, 6, 8, 10, 12, 16]
    
    def to_config_dict(self) -> dict:
        """Return dict for JSON config file (excludes CLI-only flags)."""
        d = asdict(self)
        del d["name"]
        del d["bookendless"]
        del d["timeout"]
        return d


# ============================================================================
# EXPERIMENT DEFINITIONS
# ============================================================================

EXPERIMENTS: List[ExperimentConfig] = [
    # Non-SAT Experiments
    ExperimentConfig(name="01_baseline", rounds=3, shooting_count=10000),
    ExperimentConfig(name="02_low_rounds", rounds=1, shooting_count=10000),
    ExperimentConfig(name="03_high_rounds", rounds=5, shooting_count=10000),
    ExperimentConfig(name="04_low_shooting", rounds=3, shooting_count=1000),
    ExperimentConfig(name="05_high_shooting", rounds=3, shooting_count=100000),
    ExperimentConfig(name="06_small_blocks", rounds=3, shooting_count=10000,
                     structure_block_size_min=5, structure_block_size_max=15),
    ExperimentConfig(name="07_large_blocks", rounds=3, shooting_count=10000,
                     structure_block_size_min=20, structure_block_size_max=50),
    ExperimentConfig(name="08_no_ancilla", rounds=3, shooting_count=10000, no_ancilla_mode=True),
    ExperimentConfig(name="09_single_gate", rounds=3, shooting_count=10000, single_gate_mode=True),
    ExperimentConfig(name="10_bookendless", rounds=3, shooting_count=10000, bookendless=True),
    
    # SAT Experiments
    ExperimentConfig(name="11_sat_basic", rounds=2, shooting_count=10000, sat_mode=True,
                     compression_window_size_sat=10, compression_sat_limit=20000, timeout=7200),
    ExperimentConfig(name="12_sat_aggressive", rounds=3, shooting_count=10000, sat_mode=True,
                     compression_window_size_sat=20, compression_sat_limit=50000, timeout=18000),
    ExperimentConfig(name="13_sat_bookendless", rounds=2, shooting_count=10000, sat_mode=True,
                     bookendless=True),
]


# ============================================================================
# UTILITIES
# ============================================================================

def log(msg: str, level: str = "INFO"):
    timestamp = datetime.now().strftime("%H:%M:%S")
    print(f"[{timestamp}] [{level}] {msg}")


def run_command(cmd: List[str], output_file: Optional[str] = None, 
                timeout: int = 3600, dry_run: bool = False) -> bool:
    """Run command with optional output capture."""
    cmd_str = " ".join(cmd[:10]) + ("..." if len(cmd) > 10 else "")
    log(f"Running: {cmd_str}")
    
    if dry_run:
        log("  [DRY RUN] Would execute command", "DEBUG")
        return True
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            log(f"Command failed (exit code {result.returncode})", "ERROR")
            if result.stderr:
                for line in result.stderr.strip().split('\n')[-5:]:
                    log(f"  {line[:100]}", "ERROR")
            return False
        if output_file:
            with open(output_file, "w") as f:
                f.write(result.stdout)
        return True
    except subprocess.TimeoutExpired:
        log(f"Command timed out after {timeout}s", "ERROR")
        return False


def count_gates(circuit_file: str) -> int:
    """Count gates in a circuit file."""
    try:
        with open(circuit_file, 'r') as f:
            content = f.read().strip()
            return content.count(';') if content else 0
    except:
        return 0


def save_config(config: ExperimentConfig, path: str):
    """Save experiment config as JSON."""
    with open(path, 'w') as f:
        json.dump(config.to_config_dict(), f, indent=2)


# ============================================================================
# EXPERIMENT RUNNER
# ============================================================================

def run_experiment(exp: ExperimentConfig, initial_gate: str, wires: int,
                   initial_count: int, exp_dir: str, dry_run: bool = False) -> dict:
    """Run a single experiment and return results."""
    os.makedirs(exp_dir, exist_ok=True)
    
    # Save config
    config_path = os.path.join(exp_dir, "config.json")
    save_config(exp, config_path)
    
    # Create a temporary input file to avoid overwriting the original initial_gate
    temp_input = os.path.join(exp_dir, "input.gate")
    shutil.copyfile(initial_gate, temp_input)

    # Build command
    cmd = CARGO_RUN + [
        "abbutterfly",
        "--path", temp_input,
        "-n", str(wires),
        "--rounds", str(exp.rounds),
        "--shooting", str(exp.shooting_count),
        "--config", config_path,
    ]
    
    if exp.sat_mode:
        cmd.extend(["--sat", "--lmdb-db", TEMPLATE_DB_PATH])
    if exp.no_ancilla_mode:
        cmd.append("--no-ancilla")
    if exp.single_gate_mode:
        cmd.append("--single-gate")
    if exp.bookendless:
        cmd.append("--bookendless")
    
    # Run obfuscation
    log_file = os.path.join(exp_dir, "obfuscation.log")
    start_time = datetime.now()
    success = run_command(cmd, log_file, timeout=exp.timeout, dry_run=dry_run)
    elapsed = (datetime.now() - start_time).total_seconds()
    
    result = {
        "name": exp.name,
        "status": "success" if success else "failed",
        "time_sec": round(elapsed, 1),
    }
    
    if not success:
        return result
    
    # Move output
    obf_gate = os.path.join(exp_dir, "obfuscated.gate")
    if os.path.exists("recent_circuit.txt"):
        shutil.move("recent_circuit.txt", obf_gate)
        final_count = count_gates(obf_gate)
        expansion = final_count / initial_count if initial_count > 0 else 0
        
        result.update({
            "initial_gates": initial_count,
            "final_gates": final_count,
            "expansion_factor": round(expansion, 2),
        })
        
        log(f"  Result: {initial_count} → {final_count} gates ({expansion:.1f}x)")
        
        # Generate visualizations
        if not dry_run:
            # Heatmap (state divergence)
            log(f"  Generating heatmap...")
            heatmap_cmd = CARGO_RUN + [
                "heatmap",
                "--c1", initial_gate,
                "--c2", obf_gate,
                "--num_wires", str(wires),
                "--inputs", "100"
            ]
            run_command(heatmap_cmd, os.path.join(exp_dir, "heatmap.json"))
            
            # Plot heatmap with correct title
            plot_cmd = [
                "python3", "scripts/plot_heatmap.py",
                os.path.join(exp_dir, "heatmap.json"),
                "-o", os.path.join(exp_dir, "heatmap.png"),
                "-t", f"{exp.name}: Initial ({initial_count}) vs Obfuscated ({final_count})"
            ]
            run_command(plot_cmd)
            
            # DTW Alignment
            log(f"  Generating DTW alignment...")
            align_cmd = CARGO_RUN + [
                "align",
                "--c1", initial_gate,
                "--c2", obf_gate,
                "-n", str(wires),
                "--inputs", "100"
            ]
            run_command(align_cmd, os.path.join(exp_dir, "align.json"))
            
            # Plot alignment
            align_plot_cmd = [
                "python3", "scripts/plot_alignment.py",
                os.path.join(exp_dir, "align.json"),
                "-o", os.path.join(exp_dir, "alignment.png"),
                "--xlabel", "Obfuscated Gate Index",
                "--ylabel", "Initial Gate Index"
            ]
            run_command(align_plot_cmd)
    else:
        result["status"] = "no_output"
        log("  No output circuit found", "WARN")
    
    return result


def main():
    parser = argparse.ArgumentParser(description="ABButterfly Experiment Suite")
    parser.add_argument("--test", action="store_true", help="Test mode (8w/20g)")
    parser.add_argument("--dry-run", action="store_true", help="Show commands without running")
    parser.add_argument("--experiments", type=str, help="Comma-separated experiment names to run")
    args = parser.parse_args()
    
    # Set parameters
    wires = TEST_WIRES if args.test else WIRES
    gates = TEST_GATES if args.test else GATES
    run_id = "test_8w_20g" if args.test else RUN_ID
    base_dir = f"experiments/{DATE}/{run_id}"
    
    os.makedirs(base_dir, exist_ok=True)
    
    log("=" * 60)
    log(f"ABButterfly Experiment Suite")
    log(f"  Wires: {wires}, Gates: {gates}")
    log(f"  Output: {base_dir}")
    log(f"  Mode: {'TEST' if args.test else 'FULL'}{' (DRY RUN)' if args.dry_run else ''}")
    log("=" * 60)
    
    # Generate initial circuit
    initial_gate = os.path.join(base_dir, "initial.gate")
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(wires), "--length", str(gates)]
    if not run_command(gen_cmd, initial_gate, dry_run=args.dry_run):
        log("Failed to generate initial circuit", "ERROR")
        sys.exit(1)
    
    initial_count = count_gates(initial_gate) if not args.dry_run else gates
    log(f"Initial circuit: {initial_count} gates")
    
    # Filter experiments if specified
    experiments = EXPERIMENTS
    if args.experiments:
        names = set(args.experiments.split(","))
        experiments = [e for e in EXPERIMENTS if e.name in names]
        log(f"Running {len(experiments)} selected experiments")
    
    # Run experiments
    results = {
        "wires": wires,
        "initial_gates": initial_count,
        "experiments": {}
    }
    
    for i, exp in enumerate(experiments, 1):
        log("")
        log("=" * 60)
        log(f"[{i}/{len(experiments)}] {exp.name}")
        log("=" * 60)
        
        exp_dir = os.path.join(base_dir, exp.name)
        result = run_experiment(exp, initial_gate, wires, initial_count, exp_dir, args.dry_run)
        results["experiments"][exp.name] = result
    
    # Save summary
    summary_file = os.path.join(base_dir, "summary.json")
    with open(summary_file, "w") as f:
        json.dump(results, f, indent=2)
    
    # Print summary
    log("")
    log("=" * 60)
    log("EXPERIMENT SUITE COMPLETE")
    log("=" * 60)
    log(f"\nInitial: {initial_count} gates on {wires} wires\n")
    log(f"{'Experiment':<20} {'Status':<10} {'Gates':<10} {'Expansion':<12} {'Time':<10}")
    log("-" * 62)
    
    for name, data in results["experiments"].items():
        status = data.get("status", "?")
        gates_str = str(data.get("final_gates", "-"))
        exp_str = f"{data.get('expansion_factor', 0):.1f}x" if data.get("expansion_factor") else "-"
        time_str = f"{data.get('time_sec', 0):.0f}s" if data.get("time_sec") else "-"
        log(f"{name:<20} {status:<10} {gates_str:<10} {exp_str:<12} {time_str:<10}")
    
    log(f"\nResults saved to: {summary_file}")


if __name__ == "__main__":
    main()
