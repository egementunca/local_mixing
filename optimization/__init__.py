"""SAT-based Circuit Optimization for ORNB Gates.

This module provides SAT-based local circuit optimization using only the ORNB gate.
"""

from .ornb_gate import ORNB_GATE, ORNB_TRUTH_TABLE
from .sat_optimizer import optimize_subcircuit_ornb
from .rust_interface import run_optimizer_cli

__all__ = [
    'ORNB_GATE',
    'ORNB_TRUTH_TABLE', 
    'optimize_subcircuit_ornb',
    'run_optimizer_cli',
]
