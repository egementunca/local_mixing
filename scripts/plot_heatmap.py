import argparse
import json
import sys
import numpy as np
import matplotlib.pyplot as plt

def plot_heatmap(data, output_path, title="Heatmap", xlabel="Circuit 2", ylabel="Circuit 1"):
    # Data is expected to be a list of [x, y, value]
    # x -> i1 (rows? or cols?), y -> i2
    # Rust code: average[index][0] = i1 as f64; average[index][1] = i2 as f64;
    
    # Find dimensions
    xs = [int(p[0]) for p in data]
    ys = [int(p[1]) for p in data]
    max_x = max(xs)
    max_y = max(ys)
    
    grid = np.zeros((max_x + 1, max_y + 1))
    
    for p in data:
        x, y, val = int(p[0]), int(p[1]), p[2]
        grid[x, y] = val
        
    plt.figure(figsize=(10, 8))
    # imshow origin='lower' usually places (0,0) at bottom-left. 
    # If x is row index (i1) and y is col index (i2), we might want standard matrix view 'upper'.
    # Rust loop: i1 is outer loop (rows), i2 is inner (cols).
    # So grid[i1, i2] = val.
    
    # We use 'nearest' interpolation to see pixels clearly
    plt.imshow(grid, cmap='RdYlGn_r', aspect='auto', interpolation='nearest', origin='lower', vmin=0.0, vmax=1.0)
    plt.colorbar(label="Average Hamming Distance")
    plt.title(title)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)
    
    # Add mean value annotation
    mean_val = np.mean(grid)
    plt.text(
        0.98, 0.02,
        f"Mean = {mean_val:.3f}",
        ha="right",
        va="bottom",
        transform=plt.gca().transAxes,
        color="black",
        bbox=dict(facecolor="lightgray", alpha=0.7, boxstyle="round,pad=0.3"),
    )
    
    plt.tight_layout()
    plt.savefig(output_path, dpi=300)
    print(f"Saved heatmap to {output_path}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Plot heatmap from JSON data")
    parser.add_argument("input_file", help="Path to JSON file (or '-' for stdin)")
    parser.add_argument("-o", "--output", default="heatmap.png", help="Output image path")
    parser.add_argument("-t", "--title", default="Heatmap", help="Plot title")
    args = parser.parse_args()
    
    if args.input_file == "-":
        # Read from stdin
        content = sys.stdin.read()
    else:
        with open(args.input_file, 'r') as f:
            content = f.read()
            
    # Parse JSON (handle potential non-JSON lines if piped from cargo run)
    try:
        data = json.loads(content)
    except json.JSONDecodeError:
        # Try to find the JSON line (usually the last long line)
        lines = content.strip().split('\n')
        json_line = lines[-1]
        try:
            data = json.loads(json_line)
        except:
            print("Error: Could not parse JSON input")
            sys.exit(1)
            
    if isinstance(data, dict):
        if "heatmap_data" in data:
            data = data["heatmap_data"]
        else:
            print("Error: JSON input seems to be a dict but missing 'heatmap_data' key")
            sys.exit(1)

    plot_heatmap(data, args.output, args.title)
