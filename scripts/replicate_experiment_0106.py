import subprocess
import os
import shutil
import sys
import time

# Configuration
DATE = "2026-01-06"
RUN_ID = "run_1"
BASE_DIR = f"experiments/{DATE}/{RUN_ID}"
INITIAL_CIRCUIT_SRC = f"experiments/{DATE}/initial_32w_36g.txt"
INITIAL_CIRCUIT_DEST = os.path.join(BASE_DIR, "initial.gate")
LMDB_PATH = "../sat_revsynth/data/collection.lmdb"
WIRES = 32
ROUNDS = 2
SHOOTING = 100000

def run_command(cmd, output_file=None):
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Error: {result.stderr}")
        return False
    
    if output_file:
        with open(output_file, "w") as f:
            f.write(result.stdout)
    return True

def main():
    if not os.path.exists(INITIAL_CIRCUIT_SRC):
        print(f"Error: Source circuit {INITIAL_CIRCUIT_SRC} not found")
        sys.exit(1)

    os.makedirs(BASE_DIR, exist_ok=True)
    shutil.copy(INITIAL_CIRCUIT_SRC, INITIAL_CIRCUIT_DEST)

    CARGO_RUN = ["cargo", "run", "--release", "--"]

    # 1. Obfuscation (with LMDB & Parallel SAT)
    print("\n--- 1. Running Obfuscation (LMDB + Parallel SAT) ---")
    obf_cmd = CARGO_RUN + [
        "abbutterfly",
        "--path", INITIAL_CIRCUIT_DEST,
        "--rounds", str(ROUNDS),
        "-n", str(WIRES),
        "--sat",
        "--shooting", str(SHOOTING),
        "--lmdb-db", LMDB_PATH
    ]
    if not run_command(obf_cmd, os.path.join(BASE_DIR, "obfuscation.log")):
        sys.exit(1)

    # Move output
    if os.path.exists("recent_circuit.txt"):
        shutil.move("recent_circuit.txt", os.path.join(BASE_DIR, "obfuscated.gate"))
    else:
        print("Error: Obfuscation failed to produce recent_circuit.txt")
        sys.exit(1)

    obfuscated_gate = os.path.join(BASE_DIR, "obfuscated.gate")
    initial_gate = INITIAL_CIRCUIT_DEST

    # 2. Generate Random Circuit (for comparison)
    print("\n--- 2. Generating Random Control Circuit ---")
    # Read length of initial circuit to match size
    with open(initial_gate, 'r') as f:
        initial_len = len(f.read().strip().split(';'))
    
    gen_cmd = CARGO_RUN + ["gen", "--wires", str(WIRES), "--length", str(initial_len)]
    random_gate = os.path.join(BASE_DIR, "random.gate")
    run_command(gen_cmd, random_gate)

    # 3. Analysis & Plotting
    print("\n--- 3. Running Analysis & Plotting ---")

    tasks = [
        {
            "name": "align_obfuscated_user",
            "type": "align",
            "c1": initial_gate,
            "c2": obfuscated_gate,
            "xlabel": "Obfuscated Gate Index",
            "ylabel": "Initial Gate Index"
        },
        {
            "name": "align_self_metric",
            "type": "align",
            "c1": initial_gate,
            "c2": initial_gate,
            "xlabel": "Initial Gate Index",
            "ylabel": "Initial Gate Index"
        },
        {
            "name": "align_random",
            "type": "align",
            "c1": initial_gate,
            "c2": random_gate,
            "xlabel": "Random Gate Index",
            "ylabel": "Initial Gate Index"
        },
        {
            "name": "heatmap",
            "type": "heatmap",
            "c1": initial_gate,
            "c2": obfuscated_gate,
            "xlabel": "Obfuscated Circuit",
            "ylabel": "Initial Circuit"
        }
    ]

    for task in tasks:
        print(f"Processing {task['name']}...")
        json_path = os.path.join(BASE_DIR, f"{task['name']}.json")
        img_path = os.path.join(BASE_DIR, f"{task['name']}.png")

        # Generate Data
        if task['type'] == 'align':
            cmd = CARGO_RUN + [
                "align", 
                "--c1", task['c1'], 
                "--c2", task['c2'], 
                "-n", str(WIRES), 
                "--inputs", "100"
            ]
        else: # heatmap
            cmd = CARGO_RUN + [
                "heatmap", 
                "--c1", task['c1'], 
                "--c2", task['c2'], 
                "--num_wires", str(WIRES), 
                "--inputs", "100"
            ]
        
        run_command(cmd, json_path)

        # Plot
        if task['type'] == 'align':
            plot_cmd = [
                "python3", "scripts/plot_alignment.py", 
                json_path, 
                "-o", img_path,
                "--xlabel", task['xlabel'],
                "--ylabel", task['ylabel']
            ]
        else:
            plot_cmd = [
                "python3", "scripts/plot_heatmap.py",
                json_path,
                "-o", img_path,
                "-t", "Heatmap: Initial vs Obfuscated"
            ]
        
        run_command(plot_cmd)

    print(f"\nExperiment complete. Results in {BASE_DIR}")

if __name__ == "__main__":
    main()
