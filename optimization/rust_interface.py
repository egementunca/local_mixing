"""JSON-based interface for Rust subprocess communication.

This module provides a CLI interface that reads optimization requests
from stdin as JSON and writes results to stdout as JSON.

Request format:
{
    "num_inputs": 3,
    "output_truth_tables": ["11010110", "10110100"],
    "current_num_gates": 5,
    "time_limit": 30
}

Response format (success):
{
    "success": true,
    "gates": [[3, 0, 1], [4, 2, 3], ...],
    "num_gates": 4
}

Response format (no improvement found):
{
    "success": false,
    "gates": null,
    "error": null
}

Response format (error):
{
    "success": false,
    "gates": null,
    "error": "Error message"
}
"""

import json
import sys
import os
from typing import Optional, List, Tuple

# Add optimization directory to path for standalone execution
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

try:
    from sat_optimizer import optimize_subcircuit_ornb, gates_to_reversible
except ImportError:
    from .sat_optimizer import optimize_subcircuit_ornb, gates_to_reversible


def process_request(request: dict) -> dict:
    """
    Process an optimization request and return the result.
    
    Args:
        request: Dictionary with optimization parameters
        
    Returns:
        Dictionary with optimization result
    """
    try:
        num_inputs = request['num_inputs']
        output_truth_tables = request['output_truth_tables']
        current_num_gates = request['current_num_gates']
        time_limit = request.get('time_limit', 30)
        
        # Run optimization
        result = optimize_subcircuit_ornb(
            num_inputs=num_inputs,
            output_truth_tables=output_truth_tables,
            current_num_gates=current_num_gates,
            time_limit=time_limit
        )
        
        if result is None:
            return {
                'success': False,
                'gates': None,
                'num_gates': None,
                'error': None
            }
        
        # Convert to reversible gate format
        output_indices = list(range(num_inputs, num_inputs + len(output_truth_tables)))
        reversible_gates = gates_to_reversible(result, num_inputs, output_indices)
        
        return {
            'success': True,
            'gates': [list(g) for g in reversible_gates],
            'num_gates': len(reversible_gates),
            'error': None
        }
        
    except Exception as e:
        return {
            'success': False,
            'gates': None,
            'num_gates': None,
            'error': str(e)
        }


def run_optimizer_cli():
    """
    Run the optimizer as a CLI that reads from stdin and writes to stdout.
    
    This is the entry point when called from Rust via subprocess.
    """
    # Read request from stdin
    try:
        input_data = sys.stdin.read()
        request = json.loads(input_data)
    except json.JSONDecodeError as e:
        result = {
            'success': False,
            'gates': None,
            'num_gates': None,
            'error': f'Invalid JSON input: {e}'
        }
        print(json.dumps(result))
        sys.exit(1)
    
    # Process request
    result = process_request(request)
    
    # Write result to stdout
    print(json.dumps(result))


if __name__ == '__main__':
    run_optimizer_cli()
