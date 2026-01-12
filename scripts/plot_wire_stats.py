import numpy as np
import matplotlib.pyplot as plt
import argparse

def char_to_wire(c: str) -> int:
    if '0' <= c <= '9':
        return ord(c) - ord('0')
    elif 'a' <= c <= 'z':
        return ord(c) - ord('a') + 10
    elif 'A' <= c <= 'Z':
        return ord(c) - ord('A') + 36
    elif c == '!':
        return 62
    elif c == '@':
        return 63
    else:
        raise ValueError(f"Invalid wire char: {c}")

def plot_wire_scatter(circuit_str, num_wires, x_label_str, save_path, sort_by_index=False):
    gates = [g.strip() for g in circuit_str.split(";") if g.strip()]
    total_counts = np.zeros(num_wires, dtype=int)
    active_counts = np.zeros(num_wires, dtype=int)

    for gate in gates:
        if len(gate) < 3:
            continue
        try:
            wires = [char_to_wire(c) for c in gate[:3]]
            for i, w in enumerate(wires):
                if w < num_wires:
                    total_counts[w] += 1
                    if i == 0:
                        active_counts[w] += 1
        except ValueError:
            pass

    x = np.arange(num_wires)
    
    if sort_by_index:
        # Plot 0..N-1
        display_indices = x
        y_total = total_counts
        y_active = active_counts
        title = "Gate Counts per Wire (Ordered by Index)"
    else:
        # Plot sorted by busyness
        sorted_indices = np.argsort(-total_counts)
        display_indices = sorted_indices
        y_total = total_counts[sorted_indices]
        y_active = active_counts[sorted_indices]
        title = "Gate Counts per Wire (Sorted by Total Count, Descending)"

    plt.figure(figsize=(16, 6))
    plt.scatter(x, y_total, color="blue", s=30, label="Total gates", alpha=0.7)
    plt.scatter(x, y_active, color="red", s=30, label="Active gates", alpha=0.7)

    plt.xticks(x, display_indices, rotation=90 if num_wires > 30 else 0)
    plt.xlabel(f"Wire Index ({x_label_str})" if x_label_str else "Wire Index")
    plt.ylabel("Gate Count")
    plt.title(title)
    plt.grid(True, linestyle=":", linewidth=0.5, alpha=0.6)
    plt.legend()
    plt.tight_layout()
    plt.savefig(save_path, dpi=300)
    plt.close()
    print(f"Saved wire scatter plot to {save_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Scatter plot for gate counts per wire (0–63 wires)")
    parser.add_argument("--c", type=str, required=True, help="Path to circuit file")
    parser.add_argument("--n", type=int, required=True, help="Number of wires (max 64)")
    parser.add_argument("--x", type=str, default="", help="Label for X-axis (optional)")
    parser.add_argument("--sort-index", action="store_true", help="Sort by wire index instead of count")
    parser.add_argument("--output", "-o", type=str, default="wire_scatter.png", help="Output image path")
    args = parser.parse_args()

    with open(args.c, "r") as f:
        circuit_str = f.read().strip()

    plot_wire_scatter(circuit_str, args.n, args.x, args.output, args.sort_index)
