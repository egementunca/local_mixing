#!/usr/bin/env python3
"""
Cluster Packaging Script for Local Mixing Experiments

Creates a portable job directory for running memory-intensive experiments on the cluster (BU SCC).
Includes:
- config.json
- initial.gate
- submit_job.sh (qsub script)

Usage:
    python3 scripts/pack_for_cluster.py --config <path_to_config> --circuit <path_to_circuit> --output <output_dir_name>
"""
import argparse
import json
import os
import shutil
import stat
from datetime import datetime
from pathlib import Path

def generate_qsub_script(job_name, wires, config_file, circuit_file):
    # Using relative path to binary assuming job works inside local_mixing repo
    # submit.sh is in cluster_jobs/job_name/
    # local_mixing_bin is in target/release/
    # So relative path is ../../target/release/local_mixing_bin
    
    return f"""#!/bin/bash -l
#$ -P bnec                 # Default project (change if needed)
#$ -N {job_name}           # Job name
#$ -l h_rt=12:00:00        # 12 hour runtime limit
#$ -l mem_total=128G       # 128GB RAM for 64-wire experiments
#$ -pe omp 4               # 4 Cores for Rayon parallelism
#$ -j y                    # Merge stdout/stderr
#$ -cwd                    # Run in current directory

# Load required modules (adjust as needed for specific cluster)
module load rust/1.75.0 2>/dev/null || echo "Rust module load skipped"

# Set parallel limit to match requested cores
export RAYON_NUM_THREADS=4

echo "Starting job on $(hostname) at $(date)"
echo "Config: {config_file}"
echo "Circuit: {circuit_file}"

# Relative path to binary from cluster_jobs/job_name/
BINARY="../../target/release/local_mixing_bin"

# Ensure binary exists or build it
if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY"
    echo "Attempting to build from repo root..."
    pushd ../..
    cargo build --release
    popd
fi

# Run the experiment
$BINARY abbutterfly \\
    --path "{circuit_file}" \\
    -n {wires} \\
    --config "{config_file}"

echo "Job completed at $(date)"
"""

def main():
    parser = argparse.ArgumentParser(description="Pack experiment for cluster execution")
    parser.add_argument("--config", required=True, help="Path to experiment config.json")
    parser.add_argument("--circuit", required=True, help="Path to initial.gate")
    parser.add_argument("--output", help="Output directory name (defaults to timestamped name)")
    parser.add_argument("--wires", type=int, default=64, help="Number of wires (default: 64)")
    
    args = parser.parse_args()
    
    # Setup paths
    config_path = Path(args.config)
    circuit_path = Path(args.circuit)
    
    if not args.output:
        timestamp = datetime.now().strftime("%Y-%m-%d_%H%M%S")
        output_dir = Path(f"cluster_jobs/job_{timestamp}")
    else:
        output_dir = Path(args.output)
        
    # Create directory structure
    if output_dir.exists():
        print(f"Warning: Output directory {output_dir} exists, overwriting files...")
    output_dir.mkdir(parents=True, exist_ok=True)
    
    # Copy files
    print(f"Packaging job to: {output_dir}")
    
    dest_config = output_dir / "config.json"
    shutil.copy2(config_path, dest_config)
    print(f"  - Copied config.json")
    
    dest_circuit = output_dir / "initial.gate"
    shutil.copy2(circuit_path, dest_circuit)
    print(f"  - Copied initial.gate")
    
    # Generate qsub script
    qsub_content = generate_qsub_script(
        job_name=f"mix_{args.wires}w",
        wires=args.wires,
        config_file="config.json",
        circuit_file="initial.gate"
    )
    
    qsub_path = output_dir / "submit.sh"
    with open(qsub_path, "w") as f:
        f.write(qsub_content)
    
    # Make executable
    st = os.stat(qsub_path)
    os.chmod(qsub_path, st.st_mode | stat.S_IEXEC)
    print(f"  - Generated submit.sh")
    
    # Create README
    readme_content = f"""
Cluster Execution Instructions:

1. Copy this folder to the cluster, placing it inside your `local_mixing/cluster_jobs/` directory (or similar).
   Example: `scp -r {output_dir.name} user@scc.bu.edu:~/research-group/local_mixing/cluster_jobs/`

   Note: The `submit.sh` script assumes it is 2 levels deep inside the repo (e.g. `cluster_jobs/{output_dir.name}`)
   so it can find the binary at `../../target/release/local_mixing_bin`.

2. Log in to the cluster and navigate to the job directory.

3. Submit the job:
   cd cluster_jobs/{output_dir.name}
   qsub submit.sh
"""
    with open(output_dir / "README.txt", "w") as f:
        f.write(readme_content)
    print(f"  - Created README.txt")
    
    print("\nDone! Use the generated README.txt instructions to run on cluster.")

if __name__ == "__main__":
    main()
