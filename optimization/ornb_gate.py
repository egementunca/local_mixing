"""ORNB Gate Definition and Utilities.

The ORNB gate computes: a | !b (OR with NOT on second input)
Truth table: 1011 (binary representation of the function)

For inputs (a, b):
  (0, 0) -> 0 | !0 = 0 | 1 = 1
  (0, 1) -> 0 | !1 = 0 | 0 = 0  
  (1, 0) -> 1 | !0 = 1 | 1 = 1
  (1, 1) -> 1 | !1 = 1 | 0 = 1

Reading from (0,0) to (1,1): 1, 0, 1, 1 = "1011"
"""

# The ORNB gate truth table as a 4-bit string
# Index i gives the output when inputs are (i>>1, i&1)
ORNB_GATE = '1011'
ORNB_TRUTH_TABLE = ORNB_GATE  # Alias for clarity

# All 16 possible binary operations (as 4-bit truth tables)
ALL_BINARY_OPS = [format(i, '04b') for i in range(16)]

# Operations to forbid (all except ORNB)
FORBIDDEN_OPERATIONS = [op for op in ALL_BINARY_OPS if op != ORNB_GATE]


def evaluate_ornb(a: int, b: int) -> int:
    """Evaluate the ORNB gate: a | !b"""
    return a | (1 - b)


def evaluate_ornb_gate(state: int, target: int, control1: int, control2: int) -> int:
    """
    Evaluate a reversible ORNB gate on a state.
    
    The gate computes: target ^= (control1 | !control2)
    
    Args:
        state: Current state as a bitmask
        target: Target wire index
        control1: First control wire index
        control2: Second control wire index
    
    Returns:
        New state after applying the gate
    """
    c1 = (state >> control1) & 1
    c2 = (state >> control2) & 1
    ornb_result = c1 | (1 - c2)
    return state ^ (ornb_result << target)


def truth_table_to_string(tt: list) -> str:
    """Convert a truth table list to a string."""
    return ''.join(str(b) for b in tt)


def string_to_truth_table(s: str) -> list:
    """Convert a truth table string to a list."""
    return [int(c) for c in s]
