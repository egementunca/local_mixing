"""SAT-based Circuit Optimizer for ORNB Gates.

Uses the circuit_improvement library to find smaller equivalent circuits
that use only the ORNB gate (a | !b, truth table "1011").
"""

from typing import List, Optional, Tuple
import sys
import os

# Add circuit_improvement to path
sys.path.insert(0, os.path.dirname(__file__))

try:
    from ornb_gate import ORNB_GATE, FORBIDDEN_OPERATIONS
    from circuit_improvement.circuit_search import CircuitFinder
except ImportError:
    from .ornb_gate import ORNB_GATE, FORBIDDEN_OPERATIONS
    from .circuit_improvement.circuit_search import CircuitFinder


def find_circuit_ornb(
    dimension: int,
    number_of_gates: int,
    output_truth_tables: List[str],
    time_limit: int = 30
) -> Optional[object]:
    """
    Find a circuit using only ORNB gates that computes the given truth tables.
    
    Args:
        dimension: Number of input variables
        number_of_gates: Maximum number of gates to use
        output_truth_tables: List of truth table strings for each output
        time_limit: SAT solver time limit in seconds
    
    Returns:
        Circuit object if found, None if no solution exists, 
        False if time limit exceeded
    """
    try:
        circuit_finder = CircuitFinder(
            dimension=dimension,
            number_of_gates=number_of_gates,
            output_truth_tables=output_truth_tables,
            custom_forbidden_operations=FORBIDDEN_OPERATIONS
        )
        
        circuit = circuit_finder.solve_cnf_formula(
            time_limit=time_limit,
            verbose=0
        )
        
        return circuit
        
    except Exception as e:
        print(f"SAT solver error: {e}", file=sys.stderr)
        return None


def optimize_subcircuit_ornb(
    num_inputs: int,
    output_truth_tables: List[str],
    current_num_gates: int,
    time_limit: int = 30
) -> Optional[List[Tuple[int, int, int, str]]]:
    """
    Find smallest circuit using only ORNB gates that computes given truth tables.
    
    Searches for circuits with fewer gates than current_num_gates.
    Returns the first (smallest) circuit found.
    
    Args:
        num_inputs: Number of input variables
        output_truth_tables: List of truth table strings for each output
        current_num_gates: Current number of gates (search for fewer)
        time_limit: SAT solver time limit in seconds per attempt
    
    Returns:
        List of gates as tuples (output_idx, input1_idx, input2_idx, gate_type)
        if a smaller circuit is found, None otherwise.
        Gate type is always '1011' (ORNB) in the result.
    """
    # Try to find circuits with decreasing number of gates
    for target_size in range(current_num_gates - 1, 0, -1):
        circuit = find_circuit_ornb(
            dimension=num_inputs,
            number_of_gates=target_size,
            output_truth_tables=output_truth_tables,
            time_limit=time_limit
        )
        
        if circuit is None:
            # Time limit or error, stop trying
            return None
            
        if circuit is False:
            # No solution with this many gates, try fewer
            # Actually this means it's optimal at target_size + 1
            continue
            
        if circuit and hasattr(circuit, 'gates') and len(circuit.gates) > 0:
            # Found a smaller circuit, convert to our format
            return convert_circuit_to_gates(circuit, num_inputs)
    
    # No smaller circuit found
    return None


def convert_circuit_to_gates(
    circuit, 
    num_inputs: int
) -> List[Tuple[int, int, int, str]]:
    """
    Convert a circuit_improvement Circuit to a list of gate tuples.
    
    The circuit_improvement format uses string labels for gates.
    We convert to integer indices where:
    - 0 to num_inputs-1 are input wires
    - num_inputs onwards are intermediate gates
    
    Args:
        circuit: Circuit object from circuit_improvement
        num_inputs: Number of input variables
    
    Returns:
        List of (output_idx, input1_idx, input2_idx, gate_type) tuples
    """
    import networkx as nx
    
    # Build label to index mapping
    label_to_idx = {}
    
    # Input labels get indices 0 to num_inputs-1
    for i, label in enumerate(circuit.input_labels):
        label_to_idx[label] = i
    
    # Topological sort to get gate order
    graph = circuit.construct_graph()
    sorted_gates = list(nx.topological_sort(graph))
    
    # Assign indices to gates
    next_idx = num_inputs
    for gate_label in sorted_gates:
        if gate_label not in label_to_idx:
            label_to_idx[gate_label] = next_idx
            next_idx += 1
    
    # Convert gates
    result = []
    for gate_label in sorted_gates:
        if gate_label in circuit.input_labels:
            continue
            
        first, second, gate_type = circuit.gates[gate_label]
        output_idx = label_to_idx[gate_label]
        input1_idx = label_to_idx[first]
        input2_idx = label_to_idx[second]
        
        result.append((output_idx, input1_idx, input2_idx, gate_type))
    
    return result


def gates_to_reversible(
    gates: List[Tuple[int, int, int, str]],
    num_inputs: int,
    output_indices: List[int]
) -> List[Tuple[int, int, int]]:
    """
    Convert standard Boolean circuit gates to reversible ORNB gates.
    
    In standard Boolean circuits: output = f(a, b)
    In reversible circuits: target ^= f(control1, control2)
    
    This is a more complex conversion that requires analysis of the circuit
    structure. For now, we use a simple mapping where the output wire
    becomes the target wire.
    
    Args:
        gates: List of (output_idx, input1_idx, input2_idx, gate_type) tuples
        num_inputs: Number of input variables
        output_indices: Which wire indices are outputs
    
    Returns:
        List of (target, control1, control2) tuples for reversible gates
    """
    # For ORNB-only circuits, we can directly map:
    # The standard gate (out, a, b, '1011') computes out = a | !b
    # The reversible gate (target, c1, c2) computes target ^= (c1 | !c2)
    #
    # Since we're working with small subcircuits and need them to match
    # the reversible semantics, we need to:
    # 1. Map output wire to target wire
    # 2. Map control wires appropriately
    
    result = []
    for output_idx, input1_idx, input2_idx, gate_type in gates:
        assert gate_type == ORNB_GATE, f"Expected ORNB gate, got {gate_type}"
        result.append((output_idx, input1_idx, input2_idx))
    
    return result
