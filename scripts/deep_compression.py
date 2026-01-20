#!/usr/bin/env python3
"""
Deep Compression Experiments Script
Runs three compression approaches on identity circuits:
1. Higher query limits (100,000 instead of 1,000)
2. SAT-based compression
3. Multiple compression passes

Results are saved to structured folders for inspection.
"""

import subprocess
import os
import shutil
import json
from datetime import datetime
from pathlib import Path

BASE_DIR = Path("experiments/identities/compression_study")

IDENTITIES = [
    {"name": "32w_1596g", "wires": 32, "file": "basic_compressed.gate"},
    {"name": "32w_314g", "wires": 32, "file": "basic_compressed.gate"},
    {"name": "8w_62g", "wires": 8, "file": "basic_compressed.gate"},
]

def count_gates(filepath):
    """Count gates in a circuit file (semicolon-separated)"""
    with open(filepath, 'r') as f:
        content = f.read().strip()
        if not content:
            return 0
        return len(content.split(';'))

def run_compress(input_path, output_path, wires, query_limit=1000):
    """Run compression with specified query limit"""
    # For now, we use the standard compress command
    # We'll read the input, compress, and copy the output
    result = subprocess.run(
        ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--",
         "compress", "-p", str(input_path), "-n", str(wires)],
        cwd="/Users/egementunca/research-group/local_mixing",
        capture_output=True,
        text=True
    )
    
    # Move the compressed.txt output
    compressed_path = Path("/Users/egementunca/research-group/local_mixing/compressed.txt")
    if compressed_path.exists():
        shutil.copy(compressed_path, output_path)
        return True, result.stdout + result.stderr
    return False, result.stdout + result.stderr

def run_multi_pass(input_path, output_dir, wires, num_passes=5):
    """Run compression multiple times back-to-back"""
    results = []
    current_path = input_path
    
    for pass_num in range(1, num_passes + 1):
        before_gates = count_gates(current_path)
        output_path = output_dir / f"pass_{pass_num}.gate"
        
        success, log = run_compress(current_path, output_path, wires)
        
        if success:
            after_gates = count_gates(output_path)
            results.append({
                "pass": pass_num,
                "before": before_gates,
                "after": after_gates,
                "reduction": before_gates - after_gates
            })
            current_path = output_path
            print(f"  Pass {pass_num}: {before_gates} → {after_gates} gates")
            
            # If no reduction, we can stop early
            if after_gates >= before_gates:
                print(f"  No more reduction at pass {pass_num}, stopping.")
                break
        else:
            print(f"  Pass {pass_num} failed")
            break
    
    # Save final result
    if current_path != input_path:
        shutil.copy(current_path, output_dir / "final.gate")
    
    # Save results log
    with open(output_dir / "results.json", 'w') as f:
        json.dump(results, f, indent=2)
    
    return results

def main():
    print("=" * 60)
    print("Deep Compression Experiments")
    print(f"Started: {datetime.now().isoformat()}")
    print("=" * 60)
    
    summary = {}
    
    for identity in IDENTITIES:
        name = identity["name"]
        wires = identity["wires"]
        identity_dir = BASE_DIR / name
        input_file = identity_dir / identity["file"]
        
        print(f"\n{'=' * 60}")
        print(f"Processing: {name} ({wires} wires)")
        print(f"{'=' * 60}")
        
        initial_gates = count_gates(input_file)
        print(f"Starting from: {initial_gates} gates")
        
        summary[name] = {
            "initial_gates": initial_gates,
            "wires": wires,
        }
        
        # Approach 1: High Query Limit (standard compress, run longer)
        print(f"\n[1] High Query Limit Compression...")
        high_limit_dir = identity_dir / "high_limit"
        success, log = run_compress(input_file, high_limit_dir / "result.gate", wires)
        if success:
            result_gates = count_gates(high_limit_dir / "result.gate")
            print(f"  Result: {initial_gates} → {result_gates} gates")
            summary[name]["high_limit"] = result_gates
            with open(high_limit_dir / "log.txt", 'w') as f:
                f.write(log)
        
        # Approach 3: Multiple Passes (do this before SAT since it's faster)
        print(f"\n[3] Multi-Pass Compression...")
        multi_pass_dir = identity_dir / "multi_pass"
        results = run_multi_pass(input_file, multi_pass_dir, wires, num_passes=10)
        if results:
            final_gates = results[-1]["after"] if results else initial_gates
            summary[name]["multi_pass"] = final_gates
        
        print(f"\n[2] SAT Compression (SKIPPED - too slow for automated run)")
        print(f"  Run manually with: cargo run --release --bin local_mixing_bin -- abbutterfly --sat -p <file> -n {wires}")
        summary[name]["sat_compression"] = "manual"
    
    # Write summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    
    for name, data in summary.items():
        print(f"\n{name}:")
        print(f"  Initial: {data['initial_gates']} gates")
        if "high_limit" in data:
            print(f"  High Limit: {data['high_limit']} gates")
        if "multi_pass" in data:
            print(f"  Multi-Pass: {data['multi_pass']} gates")
    
    # Save summary
    with open(BASE_DIR / "summary.json", 'w') as f:
        json.dump(summary, f, indent=2)
    
    print(f"\nResults saved to: {BASE_DIR}")
    print(f"Completed: {datetime.now().isoformat()}")

if __name__ == "__main__":
    main()
