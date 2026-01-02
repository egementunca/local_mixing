# SAT-based Circuit Optimization for ORNB Gates

This module provides SAT-based local circuit optimization for the local_mixing project.
All optimized circuits use only the ORNB gate (a | !b, truth table "1011").

## Usage

```python
from optimization import optimize_subcircuit_ornb

result = optimize_subcircuit_ornb(
    num_inputs=3,
    output_truth_tables=["11010110", "10110100"],
    current_num_gates=5,
    time_limit=30
)
```

## Requirements

Install dependencies:
```bash
pip install -r requirements.txt
```
