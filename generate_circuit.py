#!/usr/bin/env python3
"""Generate a random 64-wire, 1000-gate circuit in local_mixing format."""
import random

# Character mapping from circuit.rs
WIRE_CHARS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()-_=+[]{}<>?"
BASE = 83

def encode_wire(w):
    """Encode wire number using the Rust codebase's character mapping."""
    if w < BASE:
        return WIRE_CHARS[w]
    else:
        # Use tilde prefix for overflow (~ adds 83)
        overflow = w // BASE
        remainder = w % BASE
        return '~' * overflow + WIRE_CHARS[remainder]

def generate_gate(n_wires):
    """Generate a random 3-wire gate."""
    wires = random.sample(range(n_wires), 3)
    return wires

def circuit_repr(gates):
    """Convert gates to circuit representation."""
    parts = []
    for g in gates:
        part = encode_wire(g[0]) + encode_wire(g[1]) + encode_wire(g[2])
        parts.append(part)
    return ';'.join(parts) + ';'

# Generate random circuit
random.seed(42)
n_wires = 64
n_gates = 1000

gates = [generate_gate(n_wires) for _ in range(n_gates)]
circuit_str = circuit_repr(gates)

# Write to initial.txt 
with open('initial.txt', 'w') as f:
    f.write(circuit_str)

print(f"Generated {n_gates} gates on {n_wires} wires")
print(f"Saved to initial.txt ({len(circuit_str)} chars)")
print(f"First 5 gates: {gates[:5]}")
print(f"Circuit string start: {circuit_str[:100]}")
