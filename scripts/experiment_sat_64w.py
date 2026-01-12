#!/usr/bin/env python3
"""
Experiment Suite: 2026-01-06 SAT-based Obfuscation (64 wires, 100 gates)
Runs 5 repetitions with heatmap and alignment analysis.
"""
import subprocess
import os
import shutil
import sys
from datetime import datetime

# Configuration
DATE = "2026-01-06"
RUN_ID = "sat_64w_100g"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
LMDB_PATH = "data/collection.lmdb"
WIRES = 64
GATES = 100
REPETITIONS = 5
CONFIG_FILE = f"{BASE_DIR}/config.json"

CARGO_RUN = ["cargo", "run", "--release", "--"]

def run_command(cmd, output_file=None, timeout=3600):
    """Run command with optional output capture"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {' '.join(cmd[:6])}...")
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        # Only check return code, not stderr (which may contain warnings)
        if result.returncode != 0:
            print(f"  Failed (exit code {result.returncode})")
            print(f"  stderr: {result.stderr[-500:] if result.stderr else 'none'}")
            return False
        if output_file:
            with open(output_file, "w") as f:
                # Only write stdout - stderr contains compiler warnings that corrupt circuit files
                f.write(result.stdout)
        return True
    except subprocess.TimeoutExpired:
        print(f"  Timeout after {timeout}s")
        return False

def main():
    os.makedirs(BASE_DIR, exist_ok=True)
    
    # Generate initial circuit
    print(f"\n{'='*60}")
    print(f"Generating initial circuit: {WIRES} wires, {GATES} gates")
    print(f"{'='*60}")
    
    initial_gate = os.path.join(BASE_DIR, "initial.gate")
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(GATES)]
    if not run_command(gen_cmd, initial_gate):
        sys.exit(1)
    
    # Generate random control
    random_gate = os.path.join(BASE_DIR, "random.gate")
    run_command(gen_cmd, random_gate)
    
    # Run 5 repetitions
    for rep in range(1, REPETITIONS + 1):
        print(f"\n{'='*60}")
        print(f"REPETITION {rep}/{REPETITIONS}")
        print(f"{'='*60}")
        
        rep_dir = os.path.join(BASE_DIR, f"rep_{rep}")
        os.makedirs(rep_dir, exist_ok=True)
        
        # Run SAT obfuscation
        obf_cmd = CARGO_RUN + [
            "abbutterfly",
            "--path", initial_gate,
            "-n", str(WIRES),
            "--rounds", "2",
            "--sat",
            "--config", CONFIG_FILE,
            "--lmdb-db", LMDB_PATH
        ]
        
        log_file = os.path.join(rep_dir, "obfuscation.log")
        if not run_command(obf_cmd, log_file):
            print(f"  Rep {rep} obfuscation failed")
            continue
        
        # Move output
        obf_gate = os.path.join(rep_dir, "obfuscated.gate")
        if os.path.exists("recent_circuit.txt"):
            shutil.move("recent_circuit.txt", obf_gate)
        else:
            print(f"  Rep {rep}: No output circuit")
            continue
        
        # Heatmap
        print(f"  Generating heatmap...")
        heatmap_cmd = CARGO_RUN + [
            "heatmap",
            "--c1", initial_gate,
            "--c2", obf_gate,
            "--num_wires", str(WIRES),
            "--inputs", "100"
        ]
        run_command(heatmap_cmd, os.path.join(rep_dir, "heatmap.json"))
        
        # Plot heatmap
        plot_cmd = [
            "python3", "scripts/plot_heatmap.py",
            os.path.join(rep_dir, "heatmap.json"),
            "-o", os.path.join(rep_dir, "heatmap.png"),
            "-t", f"Rep {rep}: Initial vs Obfuscated"
        ]
        run_command(plot_cmd)
        
        # Alignment
        print(f"  Generating alignment...")
        align_cmd = CARGO_RUN + [
            "align",
            "--c1", initial_gate,
            "--c2", obf_gate,
            "-n", str(WIRES),
            "--inputs", "100"
        ]
        run_command(align_cmd, os.path.join(rep_dir, "align.json"))
        
        # Plot alignment
        align_plot_cmd = [
            "python3", "scripts/plot_alignment.py",
            os.path.join(rep_dir, "align.json"),
            "-o", os.path.join(rep_dir, "alignment.png"),
            "--xlabel", "Obfuscated Gate Index",
            "--ylabel", "Initial Gate Index"
        ]
        run_command(align_plot_cmd)
    
    # Summary
    print(f"\n{'='*60}")
    print(f"EXPERIMENT COMPLETE")
    print(f"Results in: {BASE_DIR}")
    print(f"{'='*60}")
    
    for rep in range(1, REPETITIONS + 1):
        rep_dir = os.path.join(BASE_DIR, f"rep_{rep}")
        if os.path.exists(rep_dir):
            files = os.listdir(rep_dir)
            print(f"  rep_{rep}/: {len(files)} files")

if __name__ == "__main__":
    main()
