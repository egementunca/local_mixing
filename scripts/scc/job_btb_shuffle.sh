#!/bin/bash -l

#$ -N btb_shuffle_n64
#$ -pe omp 8
#$ -l h_rt=12:00:00
#$ -l mem_per_core=8G
#$ -m ea
#$ -M etunca@bu.edu
#$ -o logs/btb_shuffle_$JOB_ID.out
#$ -e logs/btb_shuffle_$JOB_ID.err

# BU SCC job script: BTB with shuffle wrapping on n=64
# Submit with: qsub scripts/scc/job_btb_shuffle.sh

set -euo pipefail

echo "=== BTB + Shuffle (n=64) ==="
echo "Job ID: $JOB_ID"
echo "Host: $(hostname)"
echo "Start: $(date)"
echo "Cores: $NSLOTS"

# Set up paths
WORK_DIR="${PWD}"
BINARY="${WORK_DIR}/target/release/local_mixing_bin"
INPUT="${WORK_DIR}/circuits/c1_n64.txt"
OUTPUT="${WORK_DIR}/results/c1_n64_btb_m50_shuf.txt"

mkdir -p "${WORK_DIR}/results" "${WORK_DIR}/logs"

# Verify everything exists
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found. Run: cargo build --release"
    exit 1
fi
if [ ! -f "$INPUT" ]; then
    echo "ERROR: Input circuit not found at $INPUT"
    exit 1
fi
if [ ! -f "${WORK_DIR}/db/data.mdb" ]; then
    echo "ERROR: LMDB database not found at ${WORK_DIR}/db/data.mdb"
    exit 1
fi

echo "Binary: $BINARY"
echo "Input: $INPUT ($(wc -l < "$INPUT") lines)"
echo "DB: $(du -sh ${WORK_DIR}/db/data.mdb | cut -f1)"

# Set RAYON thread count to match allocated cores
export RAYON_NUM_THREADS=$NSLOTS

# Run BTB with shuffle
time "$BINARY" btb \
    -p "$INPUT" \
    -n 64 \
    -s "$OUTPUT" \
    -i "${WORK_DIR}/results/btb_shuf_intermediate.txt" \
    --flip-mode embedded \
    --shuffle-seed 42

echo ""
echo "Output: $OUTPUT"
if [ -f "$OUTPUT" ]; then
    echo "Output size: $(wc -l < "$OUTPUT") lines, $(du -sh "$OUTPUT" | cut -f1)"
fi
echo "End: $(date)"
