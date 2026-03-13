#!/bin/bash
# Run this ON the SCC after SSH-ing in.
# Usage: bash setup_scc.sh [workspace_path]
#
# Example:
#   bash setup_scc.sh /projectnb/yourproject/etunca/local_mixing
#   bash setup_scc.sh /scratch/etunca/local_mixing

set -euo pipefail

WORK="${1:-/scratch/etunca/local_mixing}"

echo "=== SCC Setup for Local Mixing ==="
echo "Workspace: $WORK"

# 1. Create workspace
mkdir -p "$WORK"
cd "$WORK"

# 2. Check/load modules
echo ""
echo "--- Loading modules ---"
module load git 2>/dev/null || echo "git: using system default"

# Check for Rust
if command -v rustc &>/dev/null; then
    echo "Rust found: $(rustc --version)"
elif [ -f "$HOME/.cargo/bin/rustc" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "Rust found in cargo: $(rustc --version)"
else
    echo "Rust not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo "Rust installed: $(rustc --version)"
fi

# 3. Clone or update repo
echo ""
echo "--- Setting up repository ---"
if [ -d ".git" ]; then
    echo "Repo exists, pulling latest..."
    git pull
else
    echo "Cloning repository..."
    git clone https://github.com/egementunca/local_mixing.git .
fi

# 4. Build
echo ""
echo "--- Building (release mode) ---"
cargo build --release
echo "Binary: $(ls -la target/release/local_mixing_bin)"

# 5. Check DB
echo ""
echo "--- Database check ---"
mkdir -p db circuits results logs
if [ -f "db/data.mdb" ]; then
    echo "LMDB: $(du -sh db/data.mdb)"
else
    echo "LMDB: NOT FOUND — transfer with:"
    echo "  rsync -avP db/data.mdb etunca@scc1.bu.edu:$WORK/db/"
fi
if [ -f "db/circuits.db" ]; then
    echo "SQLite: $(du -sh db/circuits.db)"
else
    echo "SQLite: NOT FOUND (optional) — transfer with:"
    echo "  rsync -avP db/circuits.db etunca@scc1.bu.edu:$WORK/db/"
fi

# 6. Check input circuit
if [ -f "circuits/c1_n64.txt" ]; then
    echo "Input circuit: $(du -sh circuits/c1_n64.txt)"
else
    echo "Input circuit: NOT FOUND — transfer with:"
    echo "  rsync -avP results/n64/circuits/c1_n64.txt etunca@scc1.bu.edu:$WORK/circuits/"
fi

# 7. Disk usage
echo ""
echo "--- Storage ---"
du -sh "$WORK" 2>/dev/null || true
quota -s 2>/dev/null || echo "(quota command not available)"
df -h "$WORK" 2>/dev/null | tail -1

echo ""
echo "=== Setup complete ==="
echo "Next steps:"
echo "  1. Transfer DB files (if not done): rsync from your Mac"
echo "  2. Transfer input circuit: rsync from your Mac"
echo "  3. Submit jobs: qsub scripts/scc/job_rac_shuffle.sh"
echo "                  qsub scripts/scc/job_btb_shuffle.sh"
