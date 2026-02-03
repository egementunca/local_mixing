#!/usr/bin/env python3
"""
Identity Growth & Compression Suite (2026-01-20)

Runs identity growth for varying widths and hardness levels,
then compresses them to see if they hold up.
"""
import os
import json
import subprocess
import shutil
import time
from datetime import datetime

# ============================================================================
# CONFIGURATION
# ============================================================================

DATE = datetime.now().strftime("%Y-%m-%d")
SUITE_NAME = "identity_suite"
BASE_DIR = os.path.join("experiments", DATE, SUITE_NAME)

WIRES_LIST = [8, 16, 32]
ROUNDS = 5
PLACEMENTS = 12
DIFFUSION = 100000

# We define two configurations: Baseline and Hard
CONFIGS = {
    "baseline": {
        "template_min_gates": 6,
        "template_max_gates": 30,
        "conjugation_min": 0,
        "conjugation_max": 0,
        "hardness_passes": 0,
        "hardness_ratio": 0.0,
        "compression_budget": 50,
        "min_survival_ratio": 0.3
    },
    "hard": {
        "template_min_gates": 6,
        "template_max_gates": 40,
        "conjugation_min": 4,
        "conjugation_max": 8,
        "hardness_passes": 2,
        "hardness_ratio": 0.80,
        "compression_budget": 50,
        "min_survival_ratio": 0.3
    }
}

CARGO_RUN = ["cargo", "run", "--release", "--bin", "local_mixing_bin", "--"]
LMDB_PATH = "data/collection.lmdb"  # As seen in experiment_identity_growth.py

# ============================================================================
# HELPER FUNCTIONS
# ============================================================================

def log(msg):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {msg}")

def ensure_dir(path):
    os.makedirs(path, exist_ok=True)

def run_command(cmd, log_file=None):
    cmd_str = " ".join(cmd)
    log(f"Running: {cmd_str}")
    
    start = time.time()
    result = subprocess.run(cmd, capture_output=True, text=True)
    duration = time.time() - start
    
    if log_file:
        with open(log_file, "w") as f:
            f.write(f"Command: {cmd_str}\n")
            f.write(f"Duration: {duration:.2f}s\n")
            f.write(f"Ref Code: {result.returncode}\n")
            f.write("-" * 40 + "\n")
            f.write(result.stdout)
            if result.stderr:
                f.write("\nstderr:\n" + result.stderr)
    
    if result.returncode != 0:
        log(f"  FAILED (code {result.returncode})")
        if result.stderr:
            print(result.stderr[-500:])
        return False
    
    return True

# ============================================================================
# MAIN
# ============================================================================

def main():
    ensure_dir(BASE_DIR)
    log(f"Starting Suite: {SUITE_NAME}")
    log(f"Output Directory: {BASE_DIR}")
    
    summary = {
        "date": DATE,
        "wires": WIRES_LIST,
        "results": []
    }

    for wires in WIRES_LIST:
        for config_name, params in CONFIGS.items():
            log("-" * 60)
            log(f"Config: {config_name} | Wires: {wires}")
            log("-" * 60)
            
            # Setup directories
            exp_id = f"w{wires}_{config_name}"
            exp_dir = os.path.join(BASE_DIR, exp_id)
            ensure_dir(exp_dir)
            
            # 1. Create Config File
            config_data = {
                "target_width": wires,
                "template_width_min": 3,
                "template_width_max": 6,
                "template_gate_count_min": params["template_min_gates"],
                "template_gate_count_max": params["template_max_gates"],
                "template_attempts": 30,
                "template_source": "Mixed", # perm tables + DB
                "template_conjugation_depth_min": params["conjugation_min"],
                "template_conjugation_depth_max": params["conjugation_max"],
                "template_hardness_passes": params["hardness_passes"],
                "template_min_reducer_ratio": params["hardness_ratio"],
                "placements_per_round": PLACEMENTS,
                "rounds": ROUNDS,
                "diffusion_passes": DIFFUSION,
                "compression_budget": params["compression_budget"],
                "min_survival_ratio": params["min_survival_ratio"],
                "skeleton_mode": "Balanced"
            }
            
            config_path = os.path.join(exp_dir, "growth_config.json")
            with open(config_path, "w") as f:
                json.dump(config_data, f, indent=2)
            
            # 2. Run Growth
            identity_gate = os.path.join(exp_dir, "identity.gate")
            growth_log = os.path.join(exp_dir, "growth.log")
            
            cmd_grow = CARGO_RUN + [
                "grow-identity",
                "--config", config_path,
                "--wires", str(wires),
                "--db", "db", # Permutation DB path
                "--template-source", "mixed",
                "--template-db", LMDB_PATH,
                "--output", identity_gate
            ]
            
            if not run_command(cmd_grow, growth_log):
                continue
                
            # 3. Validating Growth Output
            if not os.path.exists(identity_gate):
                log("  Error: Identity file not created.")
                continue
                
            with open(identity_gate, 'r') as f:
                gate_count = f.read().strip().count(';')
            log(f"  Generated Gates: {gate_count}")
            
            # 4. Run Compressor (Multiple Attempts)
            log("  Compressing (Multiple Variants)...")
            
            compression_results = []
            
            # Try different pass counts (Intensity)
            pass_variants = [100, 1000]
            
            for p in pass_variants:
                variant_name = f"p{p}"
                compressed_gate = os.path.join(exp_dir, f"compressed_{variant_name}.gate")
                compress_log = os.path.join(exp_dir, f"compress_{variant_name}.log")
                
                # Copy identity to target
                shutil.copy(identity_gate, compressed_gate)
                
                cmd_compress = CARGO_RUN + [
                    "compress",
                    "--path", compressed_gate,
                    "--wires", str(wires),
                    "--db", "db",
                    "--passes", str(p)
                ]
                
                if run_command(cmd_compress, compress_log):
                    with open(compressed_gate, 'r') as f:
                        compressed_count = f.read().strip().count(';')
                    ratio = compressed_count / gate_count if gate_count > 0 else 1.0
                    log(f"    [Passes {p}] Gates: {compressed_count} (Ratio: {ratio:.2f})")
                    compression_results.append({
                        "passes": p,
                        "gates": compressed_count,
                        "ratio": ratio
                    })
                else:
                    log(f"    [Passes {p}] Failed")

            # Record best result for summary
            if compression_results:
                best = min(compression_results, key=lambda x: x["gates"])
                summary["results"].append({
                    "id": exp_id,
                    "wires": wires,
                    "config": config_name,
                    "growth_gates": gate_count,
                    "best_compressed": best["gates"],
                    "best_ratio": best["ratio"],
                    "all_attempts": compression_results
                })

    # Save Summary
    with open(os.path.join(BASE_DIR, "suite_summary.json"), "w") as f:
        json.dump(summary, f, indent=2)
        
    log("=" * 60)
    log("Suite Complete")
    log(f"Summary saved to {os.path.join(BASE_DIR, 'suite_summary.json')}")

if __name__ == "__main__":
    main()
