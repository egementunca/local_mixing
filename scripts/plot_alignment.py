import argparse
import json
import numpy as np
import matplotlib.pyplot as plt
import sys
import os

def plot_alignment(data, save_path, xlabel="Circuit 2", ylabel="Circuit 1"):
    # Parse ndarray serde format
    # Expecting {"v": 1, "dim": [rows, cols], "data": [...]}
    matrix_data = data["d_matrix"]
    if "dim" in matrix_data and "data" in matrix_data:
        rows, cols = matrix_data["dim"]
        d_matrix = np.array(matrix_data["data"]).reshape(rows, cols)
    else:
        # Fallback if it's just a raw list of lists (unlikely with ndarray serde)
        d_matrix = np.array(matrix_data)
        rows, cols = d_matrix.shape

    path = data["path"] # List of [row, col]
    c_star = data["c_star"]

    # Transpose to match heatmap orientation (X=Circuit 1, Y=Circuit 2)
    d_matrix = d_matrix.T
    
    # The Rust code now computes Distance = 1 - AbsoluteCorrelation.
    # So Similarity = AbsoluteCorrelation = 1 - Distance.
    similarity_matrix = 1.0 - d_matrix

    path = np.array(path)
    # Path is (row, col) in original matrix (C1, C2).
    # Since we transposed, X axis is now C1 (original rows), Y axis is C2 (original cols).
    # So we want to plot x=row, y=col.
    
    plt.figure(figsize=(10, 8))
    
    # Plot similarity matrix
    plt.imshow(similarity_matrix, aspect='auto', origin='lower', cmap='RdYlGn', interpolation='nearest', vmin=0.0, vmax=1.0)
    plt.colorbar(label="Correlation (derived from Hamming Distance)")

    # Plot path
    if path.size > 0:
        # path has [row(C1), col(C2)].
        # We want X=C1, Y=C2. So X=path[:,0], Y=path[:,1].
        plt.plot(path[:, 0], path[:, 1], 'k-', linewidth=2, label="Alignment Path", alpha=0.7)

    plt.title(f"DTW Alignment (Cost: {c_star:.4f})")
    plt.xlabel(ylabel) # Swapped because we transposed
    plt.ylabel(xlabel)
    plt.legend(loc='upper left')
    
    # Add stats box match heatmap style
    stats_text = f"Cost (c*): {c_star:.4f}\nPath Len: {len(path)}"
    plt.text(
        0.98, 0.02,
        stats_text,
        ha="right",
        va="bottom",
        transform=plt.gca().transAxes,
        color="white",
        bbox=dict(facecolor="black", alpha=0.5, boxstyle="round,pad=0.3"),
    )

    if os.path.dirname(save_path):
        os.makedirs(os.path.dirname(save_path), exist_ok=True)
    plt.tight_layout()
    plt.savefig(save_path, dpi=300)
    print(f"Alignment plot saved to {save_path}")
    plt.close()

def main():
    parser = argparse.ArgumentParser(description="Plot DTW alignment from JSON data")
    parser.add_argument("input_file", help="Path to JSON input file")
    parser.add_argument("-o", "--output", required=True, help="Path to save output PNG")
    parser.add_argument("--xlabel", default="Circuit 2 Gate Index", help="Label for X axis")
    parser.add_argument("--ylabel", default="Circuit 1 Gate Index", help="Label for Y axis")

    args = parser.parse_args()

    try:
        with open(args.input_file, 'r') as f:
            data = json.load(f)
    except Exception as e:
        print(f"Error loading JSON data: {e}")
        sys.exit(1)

    plot_alignment(data, args.output, args.xlabel, args.ylabel)

if __name__ == "__main__":
    main()
