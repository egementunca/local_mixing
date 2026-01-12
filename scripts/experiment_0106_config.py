#!/usr/bin/env python3
"""
Experiment Suite: 2026-01-06 Configuration System Test

Tests the new ObfuscationConfig system with:
1. SAT mode obfuscation
2. LMDB mode obfuscation
3. Comparison heatmaps and alignment plots
"""
import subprocess
import os
import shutil
import sys
import json
from datetime import datetime

# Configuration
DATE = "2026-01-06"
RUN_ID = "run_config_test"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
INITIAL_CIRCUIT = f"experiments/{DATE}/initial_36g.txt"
LMDB_PATH = "../sat_revsynth/data/collection.lmdb"
WIRES = 32

CONFIGS = {
    "sat": {
        "file": f"{BASE_DIR}/config_sat.json",
        "desc": "SAT-based compression"
    },
    "lmdb": {
        "file": f"{BASE_DIR}/config_lmdb.json",
        "desc": "LMDB-based compression"
    }
}

def run_command(cmd, output_file=None, timeout=1800):
    """Run command with optional output capture and timeout (30 min default)"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] Running: {' '.join(cmd)}")
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            print(f"Error: {result.stderr}")
            return False
        
        if output_file:
            with open(output_file, "w") as f:
                f.write(result.stdout)
        return True
    except subprocess.TimeoutExpired:
        print(f"Command timed out after {timeout}s")
        return False

def main():
    # Validate inputs
    if not os.path.exists(INITIAL_CIRCUIT):
        print(f"Error: Initial circuit {INITIAL_CIRCUIT} not found")
        print("Please create an initial circuit file first.")
        sys.exit(1)

    os.makedirs(BASE_DIR, exist_ok=True)
    
    # Copy initial circuit
    initial_dest = os.path.join(BASE_DIR, "initial.gate")
    shutil.copy(INITIAL_CIRCUIT, initial_dest)
    
    CARGO_RUN = ["cargo", "run", "--release", "--"]
    
    # Run obfuscation with each config
    for name, cfg in CONFIGS.items():
        print(f"\n{'='*60}")
        print(f"Running {cfg['desc']} ({name})")
        print(f"{'='*60}")
        
        obf_cmd = CARGO_RUN + [
            "abbutterfly",
            "--path", initial_dest,
            "-n", str(WIRES),
            "--config", cfg["file"],
            "--lmdb-db", LMDB_PATH
        ]
        
        log_file = os.path.join(BASE_DIR, f"obfuscation_{name}.log")
        if not run_command(obf_cmd, log_file):
            print(f"Obfuscation {name} failed, skipping...")
            continue
        
        # Move output
        if os.path.exists("recent_circuit.txt"):
            shutil.move(
                "recent_circuit.txt", 
                os.path.join(BASE_DIR, f"obfuscated_{name}.gate")
            )
        else:
            print(f"Warning: No output for {name}")
    
    # Generate random circuit for comparison
    print("\n--- Generating Random Control Circuit ---")
    with open(initial_dest, 'r') as f:
        initial_len = len(f.read().strip().split(';'))
    
    random_gate = os.path.join(BASE_DIR, "random.gate")
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(initial_len)]
    run_command(gen_cmd, random_gate)
    
    # Analysis for each obfuscated circuit
    for name in CONFIGS.keys():
        obf_gate = os.path.join(BASE_DIR, f"obfuscated_{name}.gate")
        if not os.path.exists(obf_gate):
            continue
            
        print(f"\n--- Analyzing {name} ---")
        
        # Heatmap
        heatmap_cmd = CARGO_RUN + [
            "heatmap",
            "--c1", initial_dest,
            "--c2", obf_gate,
            "--num_wires", str(WIRES),
            "--inputs", "100"
        ]
        run_command(heatmap_cmd, os.path.join(BASE_DIR, f"heatmap_{name}.json"))
        
        # Alignment
        align_cmd = CARGO_RUN + [
            "align",
            "--c1", initial_dest,
            "--c2", obf_gate,
            "-n", str(WIRES),
            "--inputs", "100"
        ]
        run_command(align_cmd, os.path.join(BASE_DIR, f"align_{name}.json"))
    
    # Summary
    print(f"\n{'='*60}")
    print(f"Experiment complete! Results in: {BASE_DIR}")
    print(f"{'='*60}")
    
    for f in sorted(os.listdir(BASE_DIR)):
        size = os.path.getsize(os.path.join(BASE_DIR, f))
        print(f"  {f}: {size:,} bytes")

if __name__ == "__main__":
    main()
