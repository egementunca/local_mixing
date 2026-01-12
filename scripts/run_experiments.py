import subprocess
import os
import shutil
import sys

# Configuration
BASE_DIR = "experiments/2026-01-04/2"
INITIAL_CIRCUIT = os.path.join(BASE_DIR, "initial.gate")
WIRES = 64
ROUNDS = 5

experiments = [
    {
        "name": "01_baseline",
        "cmd": ["abbutterfly", "-r", str(ROUNDS), "-n", str(WIRES), "--path", INITIAL_CIRCUIT]
    },
    {
        "name": "02_no_ancilla",
        "cmd": ["abbutterfly", "-r", str(ROUNDS), "-n", str(WIRES), "--no-ancilla", "--path", INITIAL_CIRCUIT]
    },
    {
        "name": "03_single_gate",
        "cmd": ["abbutterfly", "-r", str(ROUNDS), "-n", str(WIRES), "--single-gate", "--path", INITIAL_CIRCUIT]
    },
    {
        "name": "04_low_shooting",
        "cmd": ["abbutterfly", "-r", str(ROUNDS), "-n", str(WIRES), "--shooting", "1000", "--path", INITIAL_CIRCUIT]
    }
]

def run_command(cmd, log_file=None):
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Error running command: {result.stderr}")
        return False
    if log_file:
        with open(log_file, "w") as f:
            f.write(result.stdout)
            f.write("\nSTDERR:\n")
            f.write(result.stderr)
    return True

def main():
    if not os.path.exists(INITIAL_CIRCUIT):
        print(f"Error: Initial circuit not found at {INITIAL_CIRCUIT}")
        sys.exit(1)

    # Build cargo command base
    CARGO_RUN = ["cargo", "run", "--release", "--"]

    for exp in experiments:
        exp_name = exp["name"]
        exp_dir = os.path.join(BASE_DIR, exp_name)
        os.makedirs(exp_dir, exist_ok=True)
        
        print(f"--- Starting Experiment: {exp_name} ---")
        
        # 1. Run Obfuscation
        cmd = CARGO_RUN + exp["cmd"]
        log_path = os.path.join(exp_dir, "obfuscation.log")
        if not run_command(cmd, log_path):
            continue
            
        # 2. Move Output
        output_circuit = "recent_circuit.txt"
        target_circuit = os.path.join(exp_dir, "final.gate")
        if os.path.exists(output_circuit):
            shutil.move(output_circuit, target_circuit)
        else:
            print(f"Warning: {output_circuit} not found for {exp_name}")
            continue
            
        # 3. Generate Heatmap Data (JSON)
        heatmap_json = os.path.join(exp_dir, "heatmap.json")
        heatmap_cmd = CARGO_RUN + [
            "heatmap", 
            "--c1", INITIAL_CIRCUIT, 
            "--c2", target_circuit, 
            "--num_wires", str(WIRES), 
            "--inputs", "100" # samples
        ]
        
        # Careful: capture stdout for JSON
        print(f"Generating heatmap data for {exp_name}...")
        result = subprocess.run(heatmap_cmd, capture_output=True, text=True)
        if result.returncode != 0:
             print(f"Heatmap generation failed: {result.stderr}")
             continue
        
        # Save JSON
        # The output might contain build logs, so we need to be careful parsing in the plotter, 
        # but here we just dump stdout to file.
        with open(heatmap_json, "w") as f:
            f.write(result.stdout)
            
        # 4. Plot Heatmap
        heatmap_img = os.path.join(exp_dir, "heatmap.png")
        plot_cmd = [
            "python3", "scripts/plot_heatmap.py", 
            heatmap_json, 
            "-o", heatmap_img, 
            "-t", f"{exp_name} (R={ROUNDS})"
        ]
        run_command(plot_cmd)
        
        print(f"Experiment {exp_name} completed.\n")

if __name__ == "__main__":
    main()
