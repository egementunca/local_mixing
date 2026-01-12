import random
import argparse
import sys

def char_to_wire_map():
    # Constructing the map exactly as in Rust code
    chars = []
    # 0-9
    chars.extend(chr(ord('0') + i) for i in range(10))
    # a-z
    chars.extend(chr(ord('a') + i) for i in range(26))
    # A-Z
    chars.extend(chr(ord('A') + i) for i in range(26))
    # ! @
    chars.append('!')
    chars.append('@')
    return chars

def generate_random_circuit(num_gates, num_wires, filename):
    mapping = char_to_wire_map()
    if num_wires > len(mapping):
        print(f"Error: Only {len(mapping)} wires supported")
        sys.exit(1)
    
    gates = []
    for _ in range(num_gates):
        # target, control1, control2
        g = random.sample(range(num_wires), 3)
        # Map to chars
        s = "".join(mapping[w] for w in g)
        gates.append(s)
    
    # Format: gate;gate;...;
    circuit_str = ";".join(gates) + ";"
    
    with open(filename, "w") as f:
        f.write(circuit_str)
    print(f"Generated {filename} with {num_gates} gates and {num_wires} wires.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("-n", "--num_wires", type=int, default=64)
    parser.add_argument("-g", "--gates", type=int, default=100)
    parser.add_argument("-o", "--output", default="initial.txt")
    args = parser.parse_args()
    
    generate_random_circuit(args.gates, args.num_wires, args.output)
