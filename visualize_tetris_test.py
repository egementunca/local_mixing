#!/usr/bin/env python3
"""
Visualization script for comparing canonicalization methods.

Generates:
1. Circuit diagrams for Original, Tetris-canonicalized, and Insertion-sort-canonicalized circuits
2. Skeleton graphs showing gate dependencies
3. Saves ASCII and matplotlib visualizations to files

Usage:
    python visualize_tetris_test.py
"""

import sys
import os

# Add sat_revsynth src to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../sat_revsynth/src"))

import random
from typing import List, Tuple
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import networkx as nx

# Import from sat_revsynth
try:
    from gates.eca57 import ECA57Gate, ECA57Circuit
    from utils.eca57_viz import (
        draw_circuit_ascii, 
        build_skeleton_graph, 
        get_topological_levels,
        visualize_circuit_and_skeleton
    )
except ImportError:
    print("Error: Could not import from sat_revsynth. Make sure you're in the correct directory.")
    print("Try: cd /Users/egementunca/research-group/local_mixing && python visualize_tetris_test.py")
    sys.exit(1)


def random_circuit(width: int, gate_count: int) -> ECA57Circuit:
    """Generate a random ECA57 circuit."""
    circuit = ECA57Circuit(width)
    for _ in range(gate_count):
        wires = random.sample(range(width), 3)
        circuit.add_gate(target=wires[0], ctrl1=wires[1], ctrl2=wires[2])
    return circuit


def gates_collide(g1: ECA57Gate, g2: ECA57Gate) -> bool:
    """Check if two gates collide (cannot commute)."""
    return (g1.target in (g2.ctrl1, g2.ctrl2) or 
            g2.target in (g1.ctrl1, g1.ctrl2))


def canonicalize_tetris(circuit: ECA57Circuit) -> ECA57Circuit:
    """
    Push-left canonicalization: move each gate as far left as possible
    until it collides with a previous gate.
    
    This breaks up consecutive collision chains by moving non-colliding 
    gates past collision groups.
    """
    gates = list(circuit.gates())
    width = circuit.width()
    
    if len(gates) <= 1:
        return circuit.copy()
    
    # For each gate, try to move it as far left as possible
    for i in range(1, len(gates)):
        current_gate = gates[i]
        
        # Find the leftmost position where this gate can go
        # (first position j where gate[j-1] collides with current_gate)
        leftmost = i
        for j in range(i - 1, -1, -1):
            if gates_collide(gates[j], current_gate):
                # Can't go past gate[j], stop at j+1
                leftmost = j + 1
                break
            else:
                # Can go past gate[j]
                leftmost = j
        
        # Move the gate if we found a better position
        if leftmost < i:
            gate = gates.pop(i)
            gates.insert(leftmost, gate)
    
    # Build new circuit
    new_circuit = ECA57Circuit(width)
    for g in gates:
        new_circuit.add_gate(g.target, g.ctrl1, g.ctrl2)
    return new_circuit


def canonicalize_insertion_sort(circuit: ECA57Circuit) -> ECA57Circuit:
    """
    Insertion-sort canonicalization: bubble each gate left until it collides,
    swapping if out of order.
    """
    gates = list(circuit.gates())
    width = circuit.width()
    
    for i in range(1, len(gates)):
        gi = gates[i]
        to_swap = None
        
        j = i
        while j > 0:
            j -= 1
            gj = gates[j]
            
            if gates_collide(gi, gj):
                break
            elif not gate_ordered(gj, gi):
                to_swap = j
        
        if to_swap is not None:
            g = gates.pop(i)
            gates.insert(to_swap, g)
    
    # Build new circuit
    new_circuit = ECA57Circuit(width)
    for g in gates:
        new_circuit.add_gate(g.target, g.ctrl1, g.ctrl2)
    return new_circuit


def gate_ordered(g1: ECA57Gate, g2: ECA57Gate) -> bool:
    """Returns True if g1 < g2 in lexicographic order."""
    return (g1.target, g1.ctrl1, g1.ctrl2) < (g2.target, g2.ctrl1, g2.ctrl2)


def save_ascii_circuit(circuit: ECA57Circuit, filename: str, title: str):
    """Save ASCII circuit representation to file."""
    ascii_art = draw_circuit_ascii(circuit)
    with open(filename, 'w') as f:
        f.write(f"{'='*70}\n")
        f.write(f"{title}\n")
        f.write(f"{'='*70}\n")
        f.write(f"Width: {circuit.width()}, Gates: {len(circuit.gates())}\n\n")
        f.write(ascii_art)
        f.write("\n\n")
        
        # Add layer info
        G = build_skeleton_graph(circuit)
        levels = get_topological_levels(G)
        f.write(f"\nSkeleton Depth: {len(levels)} layers\n")
        f.write("Layer Distribution:\n")
        for i, level in enumerate(levels):
            f.write(f"  Layer {i}: {len(level)} gates {level}\n")


def find_collision_groups(circuit: ECA57Circuit) -> List[Tuple[int, int]]:
    """
    Find groups of consecutive gates that cannot be separated by commutation.
    
    A gate is part of the current group if it collides with the immediately
    previous gate. If gate[i] doesn't collide with gate[i-1], the group ends.
    
    Returns list of (start_idx, end_idx) tuples for groups with > 1 gate.
    """
    gates = circuit.gates()
    if len(gates) <= 1:
        return []
    
    groups = []
    group_start = 0
    
    for i in range(1, len(gates)):
        # Check if gate[i] collides with gate[i-1] (consecutive collision)
        if not gates_collide(gates[i], gates[i - 1]):
            # Current group ends, record if > 1 gate
            if i - group_start > 1:
                groups.append((group_start, i - 1))
            group_start = i
    
    # Don't forget the last group
    if len(gates) - group_start > 1:
        groups.append((group_start, len(gates) - 1))
    
    return groups


def create_comparison_figure(
    original: ECA57Circuit,
    tetris: ECA57Circuit, 
    insertion: ECA57Circuit,
    save_path: str
):
    """Create a 3x2 figure comparing all three circuits with full display."""
    
    n_gates = len(original.gates())
    width = original.width()
    
    # Dynamic figure size: each gate takes ~0.4 inches, minimum 20 inches
    fig_width = max(40, n_gates * 0.5 + 4)
    fig_height = width * 0.8 * 3 + 6  # 3 rows of circuits
    
    fig, axes = plt.subplots(3, 2, figsize=(fig_width, fig_height), 
                             gridspec_kw={'width_ratios': [4, 1]})
    
    circuits = [
        (original, "Original", axes[0]),
        (tetris, "Tetris Canonicalized", axes[1]),
        (insertion, "Insertion-Sort Canonicalized", axes[2])
    ]
    
    for circuit, title, (ax_circuit, ax_skeleton) in circuits:
        gates = circuit.gates()
        circ_width = circuit.width()
        circ_n_gates = len(gates)
        
        # Find collision groups
        collision_groups = find_collision_groups(circuit)
        
        # Build skeleton
        G = build_skeleton_graph(circuit)
        levels = get_topological_levels(G)
        
        # Color by level
        cmap = plt.cm.Set3
        level_colors = {}
        for level_idx, level in enumerate(levels):
            color = cmap(level_idx / max(len(levels), 1))
            for node in level:
                level_colors[node] = color
        
        # === Circuit Diagram (FULL) ===
        ax_circuit.set_title(f"{title} - {circ_n_gates} gates, {len(levels)} layers, {len(collision_groups)} collision groups", 
                            fontweight='bold', fontsize=10)
        ax_circuit.set_xlim(-1, circ_n_gates + 0.5)
        ax_circuit.set_ylim(-0.5, circ_width + 0.5)
        ax_circuit.invert_yaxis()
        
        # Draw wires
        for wire in range(circ_width):
            ax_circuit.plot([-0.5, circ_n_gates + 0.3], [wire, wire], 'k-', linewidth=0.3, zorder=1, alpha=0.5)
            ax_circuit.text(-0.8, wire, f"{wire}", ha='right', va='center', fontsize=6)
        
        # Draw ALL gates
        for g_idx, g in enumerate(gates):
            color = level_colors.get(g_idx, 'gray')
            
            min_wire = min(g.target, g.ctrl1, g.ctrl2)
            max_wire = max(g.target, g.ctrl1, g.ctrl2)
            ax_circuit.plot([g_idx, g_idx], [min_wire, max_wire], 'k-', linewidth=1, zorder=2)
            
            # Target (XOR)
            circle = plt.Circle((g_idx, g.target), 0.08, color=color, ec='black', linewidth=0.8, zorder=3)
            ax_circuit.add_patch(circle)
            ax_circuit.plot([g_idx - 0.05, g_idx + 0.05], [g.target, g.target], 'k-', linewidth=0.6, zorder=4)
            ax_circuit.plot([g_idx, g_idx], [g.target - 0.05, g.target + 0.05], 'k-', linewidth=0.6, zorder=4)
            
            # Controls
            ax_circuit.plot(g_idx, g.ctrl1, 'ko', markersize=3, zorder=3)
            circle2 = plt.Circle((g_idx, g.ctrl2), 0.04, color='white', ec='black', linewidth=0.8, zorder=3)
            ax_circuit.add_patch(circle2)
        
        # Draw collision group rectangles (RED DASHED)
        for (start, end) in collision_groups:
            # Find min/max wires in the group
            group_gates = gates[start:end+1]
            all_wires = []
            for g in group_gates:
                all_wires.extend([g.target, g.ctrl1, g.ctrl2])
            min_w = min(all_wires)
            max_w = max(all_wires)
            
            # Draw rectangle
            rect = plt.Rectangle(
                (start - 0.4, min_w - 0.3),
                (end - start) + 0.8,
                (max_w - min_w) + 0.6,
                fill=False,
                edgecolor='red',
                linestyle='--',
                linewidth=1.5,
                zorder=5
            )
            ax_circuit.add_patch(rect)
            
            # Label the group size
            ax_circuit.text(start - 0.3, min_w - 0.5, f"{end - start + 1}", 
                          fontsize=5, color='red', fontweight='bold', va='bottom')
        
        ax_circuit.set_xticks(range(0, circ_n_gates, max(1, circ_n_gates // 20)))
        ax_circuit.tick_params(axis='x', labelsize=5)
        ax_circuit.set_yticks([])
        ax_circuit.set_aspect('equal')
        
        # === Skeleton Graph (FULL) ===
        ax_skeleton.set_title(f"Skeleton ({len(levels)} levels)", fontweight='bold', fontsize=9)
        
        # Position nodes by level
        pos = {}
        for level_idx, level in enumerate(levels):
            for i, node in enumerate(sorted(level)):
                pos[node] = (level_idx, i - len(level) / 2)
        
        # Draw FULL skeleton
        nx.draw_networkx_edges(G, pos, ax=ax_skeleton, edge_color='gray', 
                               arrows=True, arrowsize=5, alpha=0.3, width=0.5)
        nx.draw_networkx_nodes(G, pos, ax=ax_skeleton, 
                               node_color=[level_colors.get(n, 'gray') for n in G.nodes()],
                               node_size=50, edgecolors='black', linewidths=0.5)
        
        # Add layer separators
        for level_idx in range(len(levels)):
            ax_skeleton.axvline(x=level_idx - 0.5, color='lightgray', linestyle=':', linewidth=0.5, alpha=0.5)
        
        ax_skeleton.axis('off')
        ax_skeleton.set_aspect('equal')
    
    plt.tight_layout()
    plt.savefig(save_path, dpi=100, bbox_inches='tight')
    print(f"Saved comparison figure to: {save_path}")
    plt.close()


def main():
    # Parameters
    width = 20
    gate_count = 100
    seed = 42
    
    random.seed(seed)
    
    print(f"╔{'═'*68}╗")
    print(f"║  TETRIS CANONICALIZATION VISUALIZATION                             ║")
    print(f"║  Width: {width}, Gates: {gate_count}                                          ║")
    print(f"╚{'═'*68}╝")
    
    # Generate circuits
    print("\n1. Generating random circuit...")
    original = random_circuit(width, gate_count)
    
    print("2. Applying Tetris canonicalization...")
    tetris = canonicalize_tetris(original)
    
    print("3. Applying Insertion-sort canonicalization...")
    insertion = canonicalize_insertion_sort(original)
    
    # Create output directory
    output_dir = "tetris_viz_output"
    os.makedirs(output_dir, exist_ok=True)
    
    # Save ASCII versions
    print("\n4. Saving ASCII diagrams...")
    save_ascii_circuit(original, f"{output_dir}/1_original.txt", "ORIGINAL CIRCUIT")
    save_ascii_circuit(tetris, f"{output_dir}/2_tetris.txt", "TETRIS CANONICALIZED")
    save_ascii_circuit(insertion, f"{output_dir}/3_insertion.txt", "INSERTION-SORT CANONICALIZED")
    
    # Create comparison figure
    print("\n5. Creating comparison figure...")
    create_comparison_figure(original, tetris, insertion, f"{output_dir}/comparison.png")
    
    # Print skeleton depths
    print("\n" + "="*70)
    print("SKELETON DEPTH COMPARISON:")
    print("="*70)
    
    for circuit, name in [(original, "Original"), (tetris, "Tetris"), (insertion, "Insertion")]:
        G = build_skeleton_graph(circuit)
        levels = get_topological_levels(G)
        print(f"  {name:20s}: {len(levels)} layers")
    
    # Verify equivalence
    print("\n" + "="*70)
    print("EQUIVALENCE CHECK:")
    print("="*70)
    
    def circuits_equal(c1: ECA57Circuit, c2: ECA57Circuit, samples: int = 10000) -> bool:
        """Monte Carlo equivalence check for large circuits."""
        width = c1.width()
        for _ in range(samples):
            # Random input state
            state = [random.randint(0, 1) for _ in range(width)]
            if c1.apply(state) != c2.apply(state):
                return False
        return True
    
    if circuits_equal(original, tetris):
        print("  ✓ Tetris circuit is equivalent to original")
    else:
        print("  ✗ Tetris circuit differs from original!")
    
    if circuits_equal(original, insertion):
        print("  ✓ Insertion-sort circuit is equivalent to original")
    else:
        print("  ✗ Insertion-sort circuit differs from original!")
    
    print(f"\n✓ Output saved to: {output_dir}/")
    print(f"  - 1_original.txt")
    print(f"  - 2_tetris.txt")
    print(f"  - 3_insertion.txt")
    print(f"  - comparison.png")


if __name__ == "__main__":
    main()
