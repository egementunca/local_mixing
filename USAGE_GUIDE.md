# Local Mixing - User Guide

A Rust CLI tool for circuit obfuscation via local mixing transforms.

## Installation

```bash
cd local_mixing
cargo build --release
```

The binary will be at `target/release/local_mixing`.

## Commands

### 1. Build Random Circuit Database
Generate and store random circuits:

```bash
# Generate random circuits (n=5 wires, m=6 gates, count=1000)
./local_mixing random -n 5 -m 6 -c 1000

# Sliding window mode (auto-adjusts)
./local_mixing random -n 5 -m 6 -C
```

### 2. Load/Explore Database
```bash
# Load from SQLite into internal structures
./local_mixing load -n 5 -m 6

# Explore existing database
./local_mixing explore -n 5 -m 6

# Convert SQLite to LMDB
./local_mixing lmdb -n 5 -m 6
```

### 3. Mix/Obfuscate Circuits
Put your initial circuit in `initial.txt`, then:

```bash
# Mix using standard method (20 rounds)
./local_mixing mix -r 20

# Butterfly obfuscation (better for large circuits)
./local_mixing butterfly -r 20

# Big butterfly (for larger circuits)
./local_mixing bbutterfly -r 10 -n 32 -p ./output

# Asymmetric butterfly
./local_mixing abbutterfly -r 10 -n 32 -p ./output
```

### 4. Analyze & Visualize

```bash
# Generate distinguisher heatmap
./local_mixing heatmap -n 32 -i 1000

# Wire usage dot plot
./local_mixing wiredot -n 32 -p circuit.txt
```

### 5. Utility Commands

```bash
# Reverse circuit order
./local_mixing reverse -s input.txt -d reversed.txt

# Compress circuit
./local_mixing compress -p circuit.txt -n 32
```

## Circuit File Format

Gates are listed one per line:
```
[target, ctrl1, ctrl2]
[target, ctrl1, ctrl2]
...
```

Example (Toffoli gates on 4 wires):
```
[0, 1, 2]
[1, 0, 3]
[2, 1, 0]
```

## Databases

### SQLite (`circuits.db`)
Main storage for enumerated circuits, accessed via `load` command.

### LMDB (`./db/`)
High-performance key-value store for large-scale operations. Created via `lmdb` command.

## Python Bindings

Build with Python support:
```bash
cargo build --release --features python
```

Then use in Python:
```python
import local_mixing

# Generate heatmap data
data = local_mixing.heatmap(
    num_wires=32,
    num_inputs=1000,
    flag=True,
    c1="circuit1.txt",
    c2="circuit2.txt",
    canon=True
)
```

## Example Workflow

```bash
# 1. Build circuit database
./local_mixing load -n 5 -m 6

# 2. Create initial circuit (put in initial.txt)
echo "[0, 1, 2]" > initial.txt
echo "[1, 2, 3]" >> initial.txt

# 3. Obfuscate
./local_mixing butterfly -r 50

# 4. Compress result
./local_mixing compress -p butterfly_recent.txt -n 5
```
