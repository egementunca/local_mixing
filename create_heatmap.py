#!/usr/bin/env python3
"""
Full heatmap comparing original vs obfuscated circuit.
Raw Hamming distance with intuitive colormap.
"""
import numpy as np
import matplotlib.pyplot as plt
import random
import os
import multiprocessing

# Wire char mapping from Rust code
WIRE_CHARS = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()-_=+[]{}<>?"

def char_to_wire(c):
    return WIRE_CHARS.index(c)

def parse_gate(gate_str):
    wires = []
    chars = list(gate_str)
    i = 0
    while i < len(chars):
        overflow = 0
        while i < len(chars) and chars[i] == '~':
            overflow += 1
            i += 1
        if i < len(chars):
            w = char_to_wire(chars[i]) + overflow * 83
            wires.append(w)
            i += 1
    return wires

def parse_circuit(s):
    gates = []
    for part in s.split(';'):
        if not part.strip():
            continue
        try:
            wires = parse_gate(part)
            if len(wires) == 3:
                gates.append(wires)
        except (ValueError, IndexError):
            continue
    return gates

def apply_gate(state, gate):
    """ECA57: target ^= ctrl1 & ctrl2"""
    target, ctrl1, ctrl2 = gate
    c1_bit = (state >> ctrl1) & 1
    c2_bit = (state >> ctrl2) & 1
    if c1_bit or (not c2_bit):
        state ^= (1 << target)
    return state

def evaluate_evolution(gates, state):
    """Return list of states after each gate."""
    evolution = [state]
    for gate in gates:
        state = apply_gate(state, gate)
        evolution.append(state)
    return evolution

def compute_sample(args):
    """Compute one sample's Hamming distance matrix."""
    c1_gates, c2_gates, n_wires, seed = args
    random.seed(seed)
    input_bits = random.randint(0, (1 << min(n_wires, 63)) - 1)
    
    evo1 = evaluate_evolution(c1_gates, input_bits)
    evo2 = evaluate_evolution(c2_gates, input_bits)
    
    height = len(evo2)
    width = len(evo1)
    local_hmap = np.zeros((height, width), dtype=np.float32)
    
    for i1 in range(len(evo1)):
        for i2 in range(len(evo2)):
            diff = evo1[i1] ^ evo2[i2]
            hd = bin(diff).count('1') / n_wires
            local_hmap[i2, i1] = hd
    
    return local_hmap

def main():
    original_path = 'initial.txt'
    obfuscated_path = 'recent_circuit.txt'
    #obfuscated_path = 'initial.txt'
    with open(original_path) as f:
        c1_str = f.read()
    with open(obfuscated_path) as f:
        c2_str = f.read()
    
    c1_gates = parse_circuit(c1_str)
    c2_gates = parse_circuit(c2_str)
    
    n_wires = 64
    n_samples = 200
    
    print(f"Original: {len(c1_gates)} gates")
    print(f"Obfuscated: {len(c2_gates)} gates")
    print(f"Full grid: {len(c1_gates)+1} x {len(c2_gates)+1} = {(len(c1_gates)+1)*(len(c2_gates)+1):,} points")
    print(f"Computing with {n_samples} samples...")
    
    height = len(c2_gates) + 1
    width = len(c1_gates) + 1
    heatmap = np.zeros((height, width), dtype=np.float32)
    
    args_list = [(c1_gates, c2_gates, n_wires, i) for i in range(n_samples)]
    
    with multiprocessing.Pool(multiprocessing.cpu_count()) as pool:
        for i, local_hmap in enumerate(pool.imap(compute_sample, args_list)):
            heatmap += local_hmap / n_samples
            if (i+1) % 50 == 0:
                print(f"  Progress: {i+1}/{n_samples}")
    
    print("Computation complete!")
    
    # Check corner values
    print(f"\nCorner values (expected: 0 at corners if equivalent):")
    print(f"  (0,0) start-start: {heatmap[0,0]:.4f}")
    print(f"  (end,end) final-final: {heatmap[-1,-1]:.4f}")
    print(f"  (0,end) start-final: {heatmap[0,-1]:.4f}")
    print(f"  (end,0) final-start: {heatmap[-1,0]:.4f}")
    
    # Create figure with INTUITIVE colormap:
    # Blue = 0 (match/similar)
    # White = 0.5 (random/uncorrelated)  
    # Red = 1 (opposite)
    fig, axes = plt.subplots(1, 2, figsize=(18, 8))
    
    # Left: Full heatmap with coolwarm (blue=low, red=high)
    ax1 = axes[0]
    im1 = ax1.imshow(heatmap, cmap='coolwarm', aspect='auto', origin='lower',
                     vmin=0, vmax=0.6)
    ax1.set_xlabel('Original Circuit Gate Index', fontsize=12)
    ax1.set_ylabel('Obfuscated Circuit Gate Index', fontsize=12)
    ax1.set_title('Full Distinguisher Heatmap', fontsize=14)
    cbar1 = plt.colorbar(im1, ax=ax1)
    cbar1.set_label('Hamming Distance\n(0=match, 0.5=random)', fontsize=10)
    
    # Mark corners
    ax1.plot(0, 0, 'go', markersize=10, label='Start (should be 0)')
    ax1.plot(width-1, height-1, 'g^', markersize=10, label='End (should be 0)')
    ax1.legend(loc='upper left')
    
    # Right: Zoomed diagonals - corners only
    ax2 = axes[1]
    corner_size = 50
    
    # Create a combined view of 4 corners
    corner_view = np.zeros((corner_size*2, corner_size*2))
    # Top-left corner (start-start)
    corner_view[:corner_size, :corner_size] = heatmap[:corner_size, :corner_size]
    # Top-right corner (start-end) - flip
    corner_view[:corner_size, corner_size:] = heatmap[:corner_size, -corner_size:]
    # Bottom-left (end-start)
    corner_view[corner_size:, :corner_size] = heatmap[-corner_size:, :corner_size]
    # Bottom-right (end-end)
    corner_view[corner_size:, corner_size:] = heatmap[-corner_size:, -corner_size:]
    
    im2 = ax2.imshow(corner_view, cmap='coolwarm', aspect='auto', origin='lower',
                     vmin=0, vmax=0.6)
    ax2.axhline(corner_size, color='black', linewidth=2)
    ax2.axvline(corner_size, color='black', linewidth=2)
    ax2.set_xlabel('Original (Start | End)', fontsize=12)
    ax2.set_ylabel('Obfuscated (Start | End)', fontsize=12)
    ax2.set_title('Four Corners Zoomed', fontsize=14)
    
    # Label quadrants
    ax2.text(corner_size//2, corner_size//2, 'Start-Start', ha='center', va='center', 
             fontsize=12, color='white', fontweight='bold')
    ax2.text(corner_size + corner_size//2, corner_size//2, 'Start-End', ha='center', va='center',
             fontsize=12, color='white', fontweight='bold')
    ax2.text(corner_size//2, corner_size + corner_size//2, 'End-Start', ha='center', va='center',
             fontsize=12, color='white', fontweight='bold')
    ax2.text(corner_size + corner_size//2, corner_size + corner_size//2, 'End-End', ha='center', va='center',
             fontsize=12, color='white', fontweight='bold')
    
    cbar2 = plt.colorbar(im2, ax=ax2)
    cbar2.set_label('Hamming Distance', fontsize=10)
    
    # Stats
    mean_hd = np.mean(heatmap)
    fig.suptitle(f'Original ({len(c1_gates)} gates) vs Obfuscated ({len(c2_gates)} gates)\n'
                 f'64 wires | Mean HD: {mean_hd:.4f} | Blue=Match, White=Random, Red=Different', 
                 fontsize=13, y=1.02)
    
    plt.tight_layout()
    plt.savefig('distinguisher_heatmap_full_2.png', dpi=150, bbox_inches='tight')
    print(f"\nSaved: distinguisher_heatmap_full_2.png")

if __name__ == '__main__':
    main()
