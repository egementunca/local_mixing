#!/usr/bin/env python3
"""
Experiment: Inflation-Only Obfuscation (No Compression)
- Disables compression/reduction
- Uses pure random identity insertion (Move A)
- Generates Heatmap, Alignment, and Histogram
"""
import subprocess
import os
import shutil
import sys
import json
from datetime import datetime

# Configuration
DATE = "2026-01-07"
RUN_ID = "inflation_bookendless"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
CONFIG_PATH = f"{BASE_DIR}/config.json"
LMDB_PATH = "data/collection.lmdb"
WIRES = 8 # Safe for local
INITIAL_GATES = 20

CARGO_RUN = ["cargo", "run", "--release", "--"]

def run_command(cmd, output_file=None, capture=True):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {' '.join(cmd[:8])}...")
    try:
        if capture:
            result = subprocess.run(cmd, capture_output=True, text=True)
        else:
            result = subprocess.run(cmd, text=True)
            result.stdout = ""
            
        if result.returncode != 0:
            print(f"  Failed (exit code {result.returncode})")
            if capture and result.stderr:
                print(result.stderr[-500:])
            return False
        if output_file and capture:
            with open(output_file, "w") as f:
                f.write(result.stdout)
        return True
    except Exception as e:
        print(f"Error: {e}")
        return False

def count_gates(circuit_file):
    try:
        with open(circuit_file) as f:
            return f.read().count(';')
    except:
        return 0

def main():
    os.makedirs(BASE_DIR, exist_ok=True)
    
    # 1. Use Original 36g Circuit
    original_circuit = "experiments/2026-01-06/initial_36g.txt"
    initial_gate = f"{BASE_DIR}/initial.gate"
    
    if os.path.exists(original_circuit):
        print(f"Using original circuit from: {original_circuit}")
        shutil.copy(original_circuit, initial_gate)
        # Update wires to match original circuit
        global WIRES
        WIRES = 32  # Original circuit is 32 wires
    else:
        print(f"Original circuit not found at {original_circuit}")
        print("Generating fallback...")
        run_command(CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(INITIAL_GATES)], initial_gate)
    
    # 2. Run Inflation-Only Obfuscation
    print(f"\nRunning Inflation-Only Obfuscation...")
    obf_gate = f"{BASE_DIR}/obfuscated.gate"
    
    # Ensure config exists (I created it in previous step)
    if not os.path.exists(CONFIG_PATH):
        print(f"Config not found at {CONFIG_PATH}")
        sys.exit(1)
        
    obf_cmd = CARGO_RUN + [
        "abbutterfly",
        "--path", initial_gate,
        "-n", str(WIRES),
        "--rounds", "1",
        "--bookendless",
        "--lmdb-db", LMDB_PATH,
        "--config", CONFIG_PATH
    ]
    
    if run_command(obf_cmd, capture=False):
        if os.path.exists("recent_circuit.txt"):
            shutil.move("recent_circuit.txt", obf_gate)
        else:
            print("Output not found!")
            sys.exit(1)
    else:
        sys.exit(1)
        
    idx_count = count_gates(initial_gate)
    obf_count = count_gates(obf_gate)
    ratio = obf_count / idx_count if idx_count else 0
    print(f"\nResult: {idx_count} -> {obf_count} gates ({ratio:.1f}x expansion)")

    # 3. Generate Heatmap Data
    print(f"\nGenerating Heatmap...")
    heatmap_json = f"{BASE_DIR}/heatmap.json"
    run_command(CARGO_RUN + [
        "heatmap", "--c1", initial_gate, "--c2", obf_gate,
        "--num_wires", str(WIRES), "--inputs", "100"
    ], heatmap_json)
    
    # 4. Plot Heatmap
    print(f"Plotting Heatmap...")
    run_command(["python3", "scripts/plot_heatmap.py", heatmap_json, 
                 "-o", f"{BASE_DIR}/heatmap.png", 
                 "-t", f"Inflation Only: {idx_count}->{obf_count}"])

    # 5. Generate Alignment
    print(f"\nGenerating Alignment...")
    align_json = f"{BASE_DIR}/alignment.json"
    run_command(CARGO_RUN + [
        "align", "--c1", initial_gate, "--c2", obf_gate,
        "-n", str(WIRES), "--inputs", "100"
    ], align_json)
    
    # 6. Plot Alignment
    print(f"Plotting Alignment...")
    run_command(["python3", "scripts/plot_alignment.py", align_json, 
                 "-o", f"{BASE_DIR}/alignment.png",
                 "--xlabel", "Obfuscated (Inflation Only)", 
                 "--ylabel", "Initial"])
                 
    # 7. Plot Histogram (New)
    print(f"Plotting Histogram...")
    run_command(["python3", "scripts/plot_histogram.py", heatmap_json,
                 "-o", f"{BASE_DIR}/histogram.png"])

if __name__ == "__main__":
    main()
