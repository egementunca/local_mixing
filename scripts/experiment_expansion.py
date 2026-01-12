#!/usr/bin/env python3
"""
Experiment: Circuit Size Expansion Over Rounds
Tracks how circuit size grows with asymmetric butterfly obfuscation.

Expected growth (64 wires, 100 initial gates):
- 5 rounds  -> ~500 gates
- 10 rounds -> ~1100 gates
"""
import subprocess
import os
import shutil
import sys
import json
from datetime import datetime

# Configuration
DATE = "2026-01-06"
RUN_ID = "expansion_study"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
LMDB_PATH = "data/collection.lmdb"
WIRES = 8  # REDUCED to 8 for local memory constraints
INITIAL_GATES = 20  # Smaller for faster runs

# Test different round counts (reduced set)
ROUND_CONFIGS = [1, 2]  # Start small

CARGO_RUN = ["cargo", "run", "--release", "--"]

def run_command(cmd, output_file=None, timeout=7200, capture=True):  # 2 hour timeout
    """Run command with optional output capture"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {' '.join(cmd[:8])}...")
    try:
        if capture:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        else:
            # Stream output in real-time for progress visibility
            result = subprocess.run(cmd, text=True, timeout=timeout)
            result.stdout = ""  # No captured output
        if result.returncode != 0:
            print(f"  Failed (exit code {result.returncode})")
            if capture and result.stderr:
                # Print last few lines of stderr
                lines = result.stderr.strip().split('\n')[-5:]
                for line in lines:
                    print(f"    {line[:100]}")
            return False
        if output_file and capture:
            with open(output_file, "w") as f:
                f.write(result.stdout)
        return True
    except subprocess.TimeoutExpired:
        print(f"  Timeout after {timeout}s")
        return False

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
    
    # Generate initial circuit
    print(f"\n{'='*60}")
    print(f"Generating initial circuit: {WIRES} wires, {INITIAL_GATES} gates")
    print(f"{'='*60}")
    
    initial_gate = os.path.join(BASE_DIR, "initial.gate")
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(INITIAL_GATES)]
    if not run_command(gen_cmd, initial_gate):
        sys.exit(1)
    
    initial_count = count_gates(initial_gate)
    print(f"Initial circuit: {initial_count} gates")
    
    # Results tracking
    results = {
        "initial_wires": WIRES,
        "initial_gates": initial_count,
        "rounds": {}
    }
    
    # Run obfuscation for each round count
    for rounds in ROUND_CONFIGS:
        print(f"\n{'='*60}")
        print(f"TESTING {rounds} ROUNDS")
        print(f"{'='*60}")
        
        round_dir = os.path.join(BASE_DIR, f"rounds_{rounds}")
        os.makedirs(round_dir, exist_ok=True)
        
        # Run obfuscation (using LMDB, no SAT to avoid memory issues)
        obf_cmd = CARGO_RUN + [
            "abbutterfly",
            "--path", initial_gate,
            "-n", str(WIRES),
            "--rounds", str(rounds),
            "--shooting", "1000",  # REDUCED for memory
            "--lmdb-db", LMDB_PATH
        ]
        
        log_file = os.path.join(round_dir, "obfuscation.log")
        start_time = datetime.now()
        success = run_command(obf_cmd, capture=False)  # Stream progress in real-time
        elapsed = (datetime.now() - start_time).total_seconds()
        
        if not success:
            print(f"  Rounds {rounds} failed")
            results["rounds"][rounds] = {"status": "failed", "time_sec": elapsed}
            continue
        
        # Move output
        obf_gate = os.path.join(round_dir, "obfuscated.gate")
        if os.path.exists("recent_circuit.txt"):
            shutil.move("recent_circuit.txt", obf_gate)
            final_count = count_gates(obf_gate)
            expansion = final_count / initial_count if initial_count > 0 else 0
            
            print(f"  Result: {final_count} gates ({expansion:.1f}x expansion)")
            print(f"  Time: {elapsed:.1f}s")
            
            results["rounds"][rounds] = {
                "status": "success",
                "final_gates": final_count,
                "expansion_factor": round(expansion, 2),
                "time_sec": round(elapsed, 1)
            }
            
            # Generate heatmap
            print(f"  Generating heatmap...")
            heatmap_cmd = CARGO_RUN + [
                "heatmap",
                "--c1", initial_gate,
                "--c2", obf_gate,
                "--num_wires", str(WIRES),
                "--inputs", "100"
            ]
            run_command(heatmap_cmd, os.path.join(round_dir, "heatmap.json"))
            
            # Plot heatmap
            plot_cmd = [
                "python3", "scripts/plot_heatmap.py",
                os.path.join(round_dir, "heatmap.json"),
                "-o", os.path.join(round_dir, "heatmap.png"),
                "-t", f"Rounds={rounds}: {initial_count} -> {final_count} gates"
            ]
            run_command(plot_cmd)
        else:
            print(f"  Rounds {rounds}: No output circuit")
            results["rounds"][rounds] = {"status": "no_output", "time_sec": elapsed}
    
    # Save results
    results_file = os.path.join(BASE_DIR, "expansion_results.json")
    with open(results_file, "w") as f:
        json.dump(results, f, indent=2)
    
    # Print summary
    print(f"\n{'='*60}")
    print("EXPANSION STUDY COMPLETE")
    print(f"{'='*60}")
    print(f"\nInitial: {initial_count} gates on {WIRES} wires\n")
    print(f"{'Rounds':<10} {'Final Gates':<15} {'Expansion':<12} {'Time':<10}")
    print("-" * 47)
    for rounds in ROUND_CONFIGS:
        data = results["rounds"].get(rounds, {})
        if data.get("status") == "success":
            print(f"{rounds:<10} {data['final_gates']:<15} {data['expansion_factor']:.1f}x{'':<8} {data['time_sec']:.0f}s")
        else:
            print(f"{rounds:<10} {'FAILED':<15} {'-':<12} {'-':<10}")
    
    print(f"\nResults saved to: {results_file}")

if __name__ == "__main__":
    main()
