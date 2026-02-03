#!/usr/bin/env python3
"""
Blurring Experiment Runner

Easily run obfuscation experiments with togglable components (Pair Replacements, Single Gate, etc.)
to analyze their effect on circuit blurring.

Usage:
    python3 scripts/experiment_blurring.py --test --no-pairs --no-equal
    python3 scripts/experiment_blurring.py --no-single
"""
import subprocess
import os
import shutil
import sys
import argparse
import json
from datetime import datetime

# Configuration
DATE = datetime.now().strftime("%Y-%m-%d")
RUN_ID = f"blurring_exp_{datetime.now().strftime('%H%M%S')}"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]
WIRES = 32
GATES = 100

def log(msg):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {msg}")

def run_command(cmd, log_file=None):
    cmd_str = " ".join(cmd)
    log(f"Running: {cmd_str}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        log(f"Error: {result.stderr}")
        return False
    if log_file:
        with open(log_file, "w") as f:
            f.write(result.stdout)
    return True

def main():
    parser = argparse.ArgumentParser(description="Blurring Experiment Runner")
    parser.add_argument("--test", action="store_true", help="Run with small circuit (8w, 20g)")
    parser.add_argument("--no-pairs", action="store_true", help="Disable Pair Replacements")
    parser.add_argument("--no-single", action="store_true", help="Disable Single Gate Replacements")
    parser.add_argument("--no-equal", action="store_true", help="Disable Equal Replacements (Blurring)")
    parser.add_argument("--no-ancilla", action="store_true", help="Disable Ancilla Expansion")
    parser.add_argument("--rounds", type=int, default=3, help="Number of rounds")
    args = parser.parse_args()

    # Setup
    wires = 8 if args.test else WIRES
    gates = 20 if args.test else GATES
    os.makedirs(BASE_DIR, exist_ok=True)
    
    log(f"Starting Experiment: {RUN_ID}")
    log(f"Config: Wires={wires}, Gates={gates}, Rounds={args.rounds}")
    log(f"Modes: Pairs={not args.no_pairs}, Single={not args.no_single}, Equal={not args.no_equal}")

    # 1. Generate Initial Circuit
    initial_gate = os.path.join(BASE_DIR, "initial.gate")
    if not run_command(CARGO_RUN + ["gen", "--wires", str(wires), "--length", str(gates)], initial_gate):
        sys.exit(1)

    # 2. Obfuscate
    obf_cmd = CARGO_RUN + ["abbutterfly", "--path", initial_gate, "--rounds", str(args.rounds), "-n", str(wires)]
    
    if args.no_pairs: obf_cmd.append("--no-pairs")
    if args.no_single: obf_cmd.append("--single-gate") # Wait, flag enables it? 
    # Current main.rs: --single-gate ENABLES it. Default is OFF?
    # Let's check main.rs defaults.
    # config.rs default single_gate_mode = false.
    # main.rs: arg "single-gate" sets it to true.
    # So if I want to DISABLE it, I just don't pass the flag.
    # But wait, experiment_32w_100g enables it explicitly?
    # If user wants "Excluding step 4 (Single Gate Replacement)", and default is OFF, then it is already excluded.
    # I should check if I should ENABLE it by default in this script?
    # Usually we want everything ON for a baseline.
    
    # Revised logic: Default everything ON. Flags disable them.
    # Check default config.rs: single_gate_mode = false.
    # So strictly speaking, single gate replacement is OFF by default.
    # If I want to verify "Excluding step...", I assume the baseline has it ON.
    # So I will ENABLE it unless --no-single is passed.
    
    if not args.no_single: obf_cmd.append("--single-gate")
    
    if args.no_ancilla: obf_cmd.append("--no-ancilla")
    if args.no_equal: obf_cmd.append("--no-equal")
    
    # Enable SAT (LMDB disabled due to schema mismatch)
    obf_cmd.extend(["--sat"])
    
    log_file = os.path.join(BASE_DIR, "obfuscation.log")
    if not run_command(obf_cmd, log_file):
        log("Obfuscation failed!")
        sys.exit(1)
        
    # Move output
    if os.path.exists("recent_circuit.txt"):
        shutil.move("recent_circuit.txt", os.path.join(BASE_DIR, "obfuscated.gate"))
    else:
        log("Output file not found!")
        sys.exit(1)

    # 3. Analyze
    obf_gate = os.path.join(BASE_DIR, "obfuscated.gate")
    
    # Heatmap
    heatmap_json = os.path.join(BASE_DIR, "heatmap.json")
    run_command(CARGO_RUN + ["heatmap", "--c1", initial_gate, "--c2", obf_gate, "--num_wires", str(wires), "--inputs", "100"], heatmap_json)
    
    # Plot Heatmap
    run_command(["python3", "scripts/plot_heatmap.py", heatmap_json, "-o", os.path.join(BASE_DIR, "heatmap.png")])
    
    log(f"Done! Results in {BASE_DIR}")

if __name__ == "__main__":
    main()
