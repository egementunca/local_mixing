import random
import sys

def encode_wire(w):
    if 0 <= w <= 9:
        return str(w)
    elif 10 <= w <= 35:
        return chr(w - 10 + ord('a'))
    elif 36 <= w <= 61:
        return chr(w - 36 + ord('A'))
    elif w == 62:
        return '!'
    elif w == 63:
        return '@'
    elif w == 64:
        return '#'
    else:
        raise ValueError(f"Wire {w} not supported in script yet")

def generate_random_circuit_string(n_wires, n_gates, filename):
    with open(filename, 'w') as f:
        for _ in range(n_gates):
            # Target, Ctrl1, Ctrl2
            t = random.randint(0, n_wires - 1)
            c1 = random.randint(0, n_wires - 1)
            while c1 == t:
                c1 = random.randint(0, n_wires - 1)
            c2 = random.randint(0, n_wires - 1)
            while c2 == t or c2 == c1:
                c2 = random.randint(0, n_wires - 1)
            
            # Encode
            s = encode_wire(t) + encode_wire(c1) + encode_wire(c2) + ";"
            f.write(s)

if __name__ == "__main__":
    generate_random_circuit_string(64, 1000, "initial_1000.txt")
