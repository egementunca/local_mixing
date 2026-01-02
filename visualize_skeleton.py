#!/usr/bin/env python3
"""
Visualize the skeleton graph from our 5-gate example.
Based on the gate sequence: [1,0,3], [1,4,2], [0,4,1], [4,2,0], [4,3,0]

Skeleton Graph Structure:
- Nodes = Gates
- Edges = Collisions (directed from earlier to later gate)
- Levels = Non-colliding gates can be on the same level
"""

import networkx as nx
import matplotlib.pyplot as plt

# Our example circuit from the detailed trace
gates = [
    [1, 0, 3],  # g0
    [1, 4, 2],  # g1
    [0, 4, 1],  # g2
    [4, 2, 0],  # g3
    [4, 3, 0],  # g4
]

def collides(g1, g2):
    """Check if two gates collide (share wires in a conflicting way)."""
    # Active pin of g1 collides with control pins of g2, or vice versa
    return (g1[0] == g2[1] or g1[0] == g2[2] or
            g1[1] == g2[0] or g1[2] == g2[0])

# Build skeleton graph
G = nx.DiGraph()

# Add nodes (gates)
for i, gate in enumerate(gates):
    G.add_node(i, gate=gate)

# Add edges (collisions)
for i in range(len(gates)):
    for j in range(i + 1, len(gates)):
        if collides(gates[i], gates[j]):
            G.add_edge(i, j)  # Directed edge from earlier to later gate

# Compute topological levels
levels = list(nx.topological_generations(G))

print("╔═══════════════════════════════════════════════════════════╗")
print("║  SKELETON GRAPH ANALYSIS                                 ║")
print("╚═══════════════════════════════════════════════════════════╝")
print(f"\nCircuit: 5 gates on 5 wires")
print(f"Gates:")
for i, gate in enumerate(gates):
    print(f"  g{i}: [{gate[0]},{gate[1]},{gate[2]}] (active={gate[0]}, controls=[{gate[1]},{gate[2]}])")

print(f"\nTopological Levels: {len(levels)}")
for level_idx, level_gates in enumerate(levels):
    print(f"  Level {level_idx}: gates {list(level_gates)}")
    for g_idx in level_gates:
        print(f"    g{g_idx}: {gates[g_idx]}")

print(f"\nEdges (Collisions):")
for src, dst in G.edges():
    g1, g2 = gates[src], gates[dst]
    print(f"  g{src} → g{dst}  (gate {g1} collides with gate {g2})")

# Visualize
plt.figure(figsize=(12, 8))

# Assign layers for multipartite layout
for layer, nodes in enumerate(levels):
    for node in nodes:
        G.nodes[node]["layer"] = layer

# Use multipartite layout
pos = nx.multipartite_layout(G, subset_key="layer", align='horizontal')

# Draw graph
nx.draw(
    G,
    pos=pos,
    with_labels=True,
    node_color='lightblue',
    node_size=1500,
    font_size=10,
    font_weight='bold',
    arrows=True,
    arrowsize=20,
    arrowstyle='->',
    edge_color='gray',
    width=2
)

# Add gate labels
labels = {i: f"g{i}\n[{g[0]},{g[1]},{g[2]}]" for i, g in enumerate(gates)}
nx.draw_networkx_labels(G, pos, labels, font_size=8)

plt.title("Skeleton Graph: 5 Gates on 5 Wires\nNodes = Gates, Edges = Collisions", fontsize=14, fontweight='bold')
plt.axis('off')
plt.tight_layout()
plt.savefig('/Users/egementunca/research-group/local_mixing/skeleton_graph.png', dpi=150, bbox_inches='tight')
print(f"\n✓ Skeleton graph saved to skeleton_graph.png")
plt.show()

# ASCII visualization
print("\n╔═══════════════════════════════════════════════════════════╗")
print("║  SKELETON GRAPH (ASCII)                                  ║")
print("╚═══════════════════════════════════════════════════════════╝")
print()
print("  Level 0:    g0[1,0,3]    g1[1,4,2]")
print("               │            │")
print("               │            │")
print("               ↓            ↓")
print("  Level 1:         g2[0,4,1]")
print("                      │")
print("                      │")
print("                      ↓")
print("  Level 2:    g3[4,2,0]    g4[4,3,0]")
print()

# Detailed collision analysis
print("\n╔═══════════════════════════════════════════════════════════╗")
print("║  COLLISION ANALYSIS                                      ║")
print("╚═══════════════════════════════════════════════════════════╝")
print()
for i in range(len(gates)):
    for j in range(i + 1, len(gates)):
        g1, g2 = gates[i], gates[j]
        if collides(g1, g2):
            # Find which wires collide
            conflicts = []
            if g1[0] == g2[1]:
                conflicts.append(f"g{i} active wire {g1[0]} = g{j} control wire {g2[1]}")
            if g1[0] == g2[2]:
                conflicts.append(f"g{i} active wire {g1[0]} = g{j} control wire {g2[2]}")
            if g1[1] == g2[0]:
                conflicts.append(f"g{i} control wire {g1[1]} = g{j} active wire {g2[0]}")
            if g1[2] == g2[0]:
                conflicts.append(f"g{i} control wire {g1[2]} = g{j} active wire {g2[0]}")

            print(f"  g{i} → g{j}:")
            for conflict in conflicts:
                print(f"    • {conflict}")

print("\n╔═══════════════════════════════════════════════════════════╗")
print("║  CONVEX SUBCIRCUIT SELECTION                             ║")
print("╚═══════════════════════════════════════════════════════════╝")
print()
print("Convex subcircuits are subgraphs where all paths between nodes")
print("in the subgraph remain within the subgraph.")
print()
print("Example convex subcircuits:")
print("  • {g0, g2} - g0 → g2, no external paths")
print("  • {g1, g2} - g1 → g2, no external paths")
print("  • {g2, g3, g4} - All paths stay within this set")
print("  • {g0, g1, g2, g3, g4} - The entire graph")
print()
print("Non-convex example:")
print("  • {g0, g3} - Path g0 → g2 → g3 goes through g2 (not in set)")
