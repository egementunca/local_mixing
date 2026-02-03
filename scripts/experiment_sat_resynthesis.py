#!/usr/bin/env python3
"""
Experiment: SAT Resynthesis (No Butterfly Structure)
Config: 32 wires, 100 gates, 10 rounds of [Scramble -> SAT Compress]
"""
import subprocess
import os
import shutil
import sys
from datetime import datetime
import json

# Configuration
DATE = datetime.now().strftime("%Y-%m-%d")
RUN_ID = "sat_resynthesis"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
LMDB_PATH = "data/collection.lmdb"
WIRES = 32
GATES = 100
ROUNDS = 10
REPETITIONS = 1
CONFIG_FILE = f"{BASE_DIR}/config.json"

CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]

def run_command(cmd, output_file=None, timeout=3600, append_stderr=False):
    """Run command with optional output capture"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {' '.join(cmd[:10])}...")
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            print(f"  Failed (exit code {result.returncode})")
            print(f"  stderr: {result.stderr[-500:] if result.stderr else 'none'}")
            # If logging enabled and failed, we might still want to see stderr in file if partial output
            if output_file and append_stderr:
                 with open(output_file, "a") as f: # Append mode if file exists or write
                     f.write("\n\n=== STDERR (FAILED) ===\n")
                     f.write(result.stderr)
            return False
        if output_file:
            with open(output_file, "w") as f:
                f.write(result.stdout)
                if append_stderr and result.stderr:
                    f.write("\n\n=== STDERR ===\n")
                    f.write(result.stderr)
        return True
    except subprocess.TimeoutExpired:
        print(f"  Timeout after {timeout}s")
        return False

def main():
    os.makedirs(BASE_DIR, exist_ok=True)

    # FAST/Low Effort + SAT Config
    config = {
        # DISABLE Butterfly Structure
        "structure_block_size_min": 0,
        "structure_block_size_max": 0,
        
        # Mixing / Scrambling intensities
        "mix_prob_template": 0.05,
        "mix_prob_swaps": 0.90,
        "mix_prob_patch": 0.05,
        
        # Scrambling
        "single_gate_replacements": 50,
        "shooting_count": 0,  # Disabled per user request
        "shooting_count_inner": 50000,
        
        # Loop Control
        "rounds": 10,  # Quick experiment
        
        # Compression Modes
        "sat_mode": True,
        "no_ancilla_mode": False,
        "single_gate_mode": True,
        "skip_compression": False,
        
        # SAT Specifics
        "compression_window_size": 100,
        "compression_window_size_sat": 8,
        "compression_sat_limit": 1000,
        
        "final_stability_threshold": 5,
        "chunk_split_base": 1500,
        "reducer_active_wire_limit": 6,
        "reducer_window_sizes": [4, 6, 8],
        
        # Replacements
        "pair_replacement_mode": False,
        "equal_replacement_mode": True,  # Enable "same size" replacements
        
        "lmdb_path": "data/collection.lmdb"
    }
    
    with open(CONFIG_FILE, "w") as f:
        json.dump(config, f, indent=2)

    # Generate initial circuit
    print(f"\n{'='*60}")
    print(f"Generating initial circuit: {WIRES} wires, {GATES} gates")
    print(f"{'='*60}")
    
    initial_gate = os.path.join(BASE_DIR, "initial.gate")
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(GATES)]
    if not run_command(gen_cmd, initial_gate):
        sys.exit(1)
    
    # Run Repetitions
    for rep in range(1, REPETITIONS + 1):
        print(f"\n{'='*60}")
        print(f"REPETITION {rep}/{REPETITIONS}")
        print(f"{'='*60}")
        
        rep_dir = os.path.join(BASE_DIR, f"rep_{rep}")
        os.makedirs(rep_dir, exist_ok=True)
        
        # Run abbutterfly (which now behaves as loop due to config)
        obf_cmd = CARGO_RUN + [
            "abbutterfly",
            "--path", initial_gate,
            "-n", str(WIRES),
            "--rounds", str(ROUNDS),
            "--sat",
            "--config", CONFIG_FILE,
            "--lmdb-db", LMDB_PATH
        ]
        
        log_file = os.path.join(rep_dir, "obfuscation.log")
        if not run_command(obf_cmd, log_file, append_stderr=True):
            print(f"  Rep {rep} obfuscation failed")
            continue
        
        # Move output
        obf_gate = os.path.join(rep_dir, "obfuscated.gate")
        if os.path.exists("recent_circuit.txt"):
            shutil.move("recent_circuit.txt", obf_gate)
        else:
            print(f"  Rep {rep}: No output circuit")
            continue
        
        # Analysis: Heatmap
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
            "-t", f"SAT Resynthesis: Initial vs Obfuscated"
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
    
    print(f"\n{'='*60}")
    print(f"EXPERIMENT COMPLETE")
    print(f"Results in: {BASE_DIR}")
    print(f"{'='*60}")

if __name__ == "__main__":
    main()
