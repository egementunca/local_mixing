
import random
import sys

# Simulation Logic
def decode(s): 
    return [int(c, 36) for c in s] if len(s)==3 else None

def simulate_circuit(gates, inputs, num_wires):
    # Track toggles per wire
    wire_toggles = {w: 0 for w in range(num_wires)}
    dead_wires = set(range(num_wires))
    
    for i in range(inputs):
        state = [random.randint(0, 1) for _ in range(num_wires)]
        initial = list(state)
        
        for g in gates:
            if not g: continue
            targets = decode(g)
            if not targets: continue
            
            t, c1, c2 = targets
            
            # Logic: Target flips if ALL controls are 1
            # Check controls (they are pins != target)
            ctrls = [c for c in [c1, c2] if c != t]
            
            if all(state[c] == 1 for c in ctrls):
                state[t] ^= 1
                
        # Update stats
        for w in range(num_wires):
            if state[w] != initial[w]:
                wire_toggles[w] += 1
                if w in dead_wires:
                    dead_wires.remove(w)
                    
    return wire_toggles, dead_wires

def main():
    circuit_path = "experiments/2026-01-06/initial_36g.txt"
    try:
        with open(circuit_path, 'r') as f:
            content = f.read().strip()
            raw_gates = [g.strip() for g in content.split(';') if g.strip()]
    except FileNotFoundError:
        print(f"Error: Could not find {circuit_path}")
        sys.exit(1)
        
    WIRES = 32
    INPUTS = 100
    
    print(f"Analyzing Circuit Activity ({WIRES} wires, {INPUTS} inputs)")
    print("-" * 50)
    
    toggles, dead = simulate_circuit(raw_gates, INPUTS, WIRES)
    
    active_count = WIRES - len(dead)
    print(f"Total Wires: {WIRES}")
    print(f"Active Wires: {active_count} (toggled at least once)")
    print(f"Dead Wires:   {len(dead)}")
    
    print("\nToggle Rates (top 10):")
    sorted_toggles = sorted(toggles.items(), key=lambda x: -x[1])
    for w, count in sorted_toggles[:10]:
        rate = (count / INPUTS) * 100
        print(f"  Wire {w:>2}: {count:>3}/{INPUTS} ({rate:>3.0f}%)")
        
    if dead:
        print(f"\nDead Wires (0 toggles): {sorted(list(dead))}")

if __name__ == "__main__":
    main()
