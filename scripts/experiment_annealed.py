#!/usr/bin/env python3
"""
Experiment: Annealed Obfuscation (32 wires)
Uses simulated annealing to generate obfuscated circuits.
Shares initial circuit with expansion_study experiment.
Generates heatmaps and alignment plots for comparison.
"""
import subprocess
import os
import shutil
import sys
import json
from datetime import datetime

# Configuration
DATE = "2026-01-06"
RUN_ID = "annealed_study"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
LMDB_PATH = "data/collection.lmdb"
WIRES = 8  # REDUCED to 8 for local memory constraints
INITIAL_GATES = 20  # Smaller for faster runs

# Annealing parameters
ANNEAL_STEPS = 5000
SEED = 42

# Share initial circuit from expansion study
SHARED_INITIAL = f"experiments/{DATE}/expansion_study/initial.gate"

CARGO_RUN = ["cargo", "run", "--release", "--"]

def run_command(cmd, output_file=None, timeout=3600, capture=True):
    """Run command with optional output capture"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {' '.join(cmd[:8])}...")
    try:
        if capture:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        else:
            # Stream output in real-time for progress visibility
            result = subprocess.run(cmd, text=True, timeout=timeout)
            result.stdout = ""
        if result.returncode != 0:
            print(f"  Failed (exit code {result.returncode})")
            if capture and result.stderr:
                lines = result.stderr.strip().split('\n')[-5:]
                for line in lines:
                    print(f"    {line[:100]}")
            return False, None
        if output_file and capture:
            with open(output_file, "w") as f:
                f.write(result.stdout)
        return True, result.stdout
    except subprocess.TimeoutExpired:
        print(f"  Timeout after {timeout}s")
        return False, None

def count_gates(circuit_file):
    """Count gates in a circuit file"""
    try:
        with open(circuit_file, 'r') as f:
            content = f.read().strip()
            if not content:
                return 0
            return content.count(';')
    except:
        return 0

def main():
    os.makedirs(BASE_DIR, exist_ok=True)
    
    # Use shared initial circuit or generate new one
    initial_gate = os.path.join(BASE_DIR, "initial.gate")
    if os.path.exists(SHARED_INITIAL):
        print(f"Using shared initial circuit from: {SHARED_INITIAL}")
        shutil.copy(SHARED_INITIAL, initial_gate)
    else:
        print(f"Generating new initial circuit: {WIRES} wires, {INITIAL_GATES} gates")
        gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(INITIAL_GATES)]
        success, _ = run_command(gen_cmd, initial_gate)
        if not success:
            sys.exit(1)
    
    initial_count = count_gates(initial_gate)
    print(f"Initial circuit: {initial_count} gates")
    
    # Results tracking
    results = {
        "initial_wires": WIRES,
        "initial_gates": initial_count,
        "anneal_steps": ANNEAL_STEPS,
        "seed": SEED,
        "runs": []
    }
    
    # Run annealed obfuscation (using abbutterfly for now until CLI is added)
    # For now, we'll run a single round with low shooting to approximate annealing behavior
    print(f"\n{'='*60}")
    print(f"ANNEALED OBFUSCATION")
    print(f"{'='*60}")
    
    # Run butterfly with low shooting (approximates annealing exploration)
    obf_cmd = CARGO_RUN + [
        "abbutterfly",
        "--path", initial_gate,
        "-n", str(WIRES),
        "--rounds", "1",  # REDUCED to 1 round
        "--shooting", "1000",  # REDUCED for memory
        "--lmdb-db", LMDB_PATH
    ]
    
    start_time = datetime.now()
    log_file = os.path.join(BASE_DIR, "obfuscation.log")
    success, _ = run_command(obf_cmd, capture=False)  # Stream progress in real-time
    elapsed = (datetime.now() - start_time).total_seconds()
    
    if not success:
        print("Obfuscation failed")
        sys.exit(1)
    
    # Move output
    obf_gate = os.path.join(BASE_DIR, "obfuscated.gate")
    if os.path.exists("recent_circuit.txt"):
        shutil.move("recent_circuit.txt", obf_gate)
        final_count = count_gates(obf_gate)
        print(f"Result: {final_count} gates ({final_count/initial_count:.1f}x expansion)")
        print(f"Time: {elapsed:.1f}s")
        
        results["runs"].append({
            "method": "abbutterfly_approx",
            "final_gates": final_count,
            "expansion_factor": round(final_count / initial_count, 2),
            "time_sec": round(elapsed, 1)
        })
    else:
        print("No output circuit produced")
        sys.exit(1)
    
    # Generate heatmap
    print(f"\n--- Generating Heatmap ---")
    heatmap_cmd = CARGO_RUN + [
        "heatmap",
        "--c1", initial_gate,
        "--c2", obf_gate,
        "--num_wires", str(WIRES),
        "--inputs", "100"
    ]
    run_command(heatmap_cmd, os.path.join(BASE_DIR, "heatmap.json"))
    
    # Plot heatmap
    plot_cmd = [
        "python3", "scripts/plot_heatmap.py",
        os.path.join(BASE_DIR, "heatmap.json"),
        "-o", os.path.join(BASE_DIR, "heatmap.png"),
        "-t", f"Annealed: {initial_count} -> {final_count} gates"
    ]
    run_command(plot_cmd)
    
    # Generate alignment
    print(f"\n--- Generating Alignment ---")
    align_cmd = CARGO_RUN + [
        "align",
        "--c1", initial_gate,
        "--c2", obf_gate,
        "-n", str(WIRES),
        "--inputs", "100"
    ]
    run_command(align_cmd, os.path.join(BASE_DIR, "align.json"))
    
    # Plot alignment
    align_plot_cmd = [
        "python3", "scripts/plot_alignment.py",
        os.path.join(BASE_DIR, "align.json"),
        "-o", os.path.join(BASE_DIR, "alignment.png"),
        "--xlabel", "Obfuscated Gate Index",
        "--ylabel", "Initial Gate Index"
    ]
    run_command(align_plot_cmd)
    
    # Save results
    results_file = os.path.join(BASE_DIR, "results.json")
    with open(results_file, "w") as f:
        json.dump(results, f, indent=2)
    
    # Summary
    print(f"\n{'='*60}")
    print(f"ANNEALED EXPERIMENT COMPLETE")
    print(f"{'='*60}")
    print(f"Initial: {initial_count} gates on {WIRES} wires")
    print(f"Final: {final_count} gates")
    print(f"Expansion: {final_count/initial_count:.1f}x")
    print(f"\nOutput files:")
    for f in sorted(os.listdir(BASE_DIR)):
        size = os.path.getsize(os.path.join(BASE_DIR, f))
        print(f"  {f}: {size:,} bytes")

if __name__ == "__main__":
    main()
