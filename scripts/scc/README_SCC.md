# Running Local Mixing on BU SCC

## Overview

Run obfuscation (RAC/BTB with shuffle wrapping) on SCC, then copy circuit files back locally for heatmap analysis.

**What goes to SCC:**
- Source code: clone from GitHub (~1MB)
- Databases: `db/data.mdb` (52GB) + `db/circuits.db` (10GB) = **62GB**
- Input circuit: `c1_n64.txt` (400 bytes)

**What comes back:**
- Obfuscated circuit files (~few MB each)

## Step-by-Step

### 1. SSH into SCC
```bash
ssh etunca@scc1.bu.edu
# Enter password + DUO 2FA
```

### 2. Check storage quota
```bash
quota -s
df -h /projectnb/
# If /projectnb has space, use it. Otherwise use /scratch.
```

### 3. Set up workspace
```bash
# Option A: project space (persistent)
export WORK=/projectnb/<your-project>/etunca/local_mixing
# Option B: scratch (temporary, auto-purged after 30 days)
export WORK=/scratch/etunca/local_mixing

mkdir -p $WORK
cd $WORK
```

### 4. Clone repo and build
```bash
module load git
module load rust  # or: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/egementunca/local_mixing.git .
cargo build --release
```

### 5. Transfer databases (from your Mac, in a new terminal)
```bash
# This will take a while (~62GB). Use rsync for resumability.
rsync -avP --progress local_mixing/db/data.mdb etunca@scc1.bu.edu:$WORK/db/
rsync -avP --progress local_mixing/db/circuits.db etunca@scc1.bu.edu:$WORK/db/
```

### 6. Transfer input circuit
```bash
rsync -avP results/n64/circuits/c1_n64.txt etunca@scc1.bu.edu:$WORK/circuits/
```

### 7. Submit jobs (see job scripts below)
```bash
cd $WORK
qsub scripts/scc/job_rac_shuffle.sh
qsub scripts/scc/job_btb_shuffle.sh
```

### 8. Monitor
```bash
qstat -u etunca
# Check output files in $WORK/results/
```

### 9. Copy results back (from your Mac)
```bash
rsync -avP etunca@scc1.bu.edu:$WORK/results/ results/n64/circuits_shuffled/
```

### 10. Run heatmaps locally
```bash
python -m scripts.heatmaps.run_analysis full \
    --c1 results/n64/circuits/c1_n64.txt \
    --c2-a results/n64/circuits_shuffled/c1_n64_rac_r1_shuf.txt \
    --c2-b results/n64/circuits_shuffled/c1_n64_btb_m50_shuf.txt \
    -n 64 -i 1000 --out results/n64/heatmaps_v2_shuffled
```
