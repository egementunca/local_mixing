
import json
import matplotlib.pyplot as plt
import numpy as np
import argparse
import sys

def plot_histogram(json_path, output_path):
    try:
        with open(json_path, 'r') as f:
            data = json.load(f)
            
        # Helper to recursively find the matrix data
        matrix = data.get('data')
        
        # Flatten
        values = np.array(matrix).flatten()
        
        plt.figure(figsize=(10, 6))
        # Histogram of distances
        # Distances are 0.0 to 1.0 (normalized Hamming) or raw counts?
        # The rust code produces normalized distances.
        
        plt.hist(values, bins=50, color='skyblue', edgecolor='black', alpha=0.7)
        
        plt.title('Hamming Distance Distribution (Heatmap Data)')
        plt.xlabel('Normalized Hamming Distance')
        plt.ylabel('Frequency (Cell Count)')
        plt.grid(axis='y', alpha=0.3)
        
        # Stats
        mean_val = np.mean(values)
        median_val = np.median(values)
        plt.axvline(mean_val, color='red', linestyle='dashed', linewidth=1, label=f'Mean: {mean_val:.2f}')
        plt.axvline(median_val, color='green', linestyle='dashed', linewidth=1, label=f'Median: {median_val:.2f}')
        plt.legend()
        
        plt.tight_layout()
        if output_path:
            plt.savefig(output_path, dpi=150)
            print(f"Histogram saved to {output_path}")
        else:
            plt.show()
            
    except Exception as e:
        print(f"Error plotting histogram: {e}")
        sys.exit(1)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Plot histogram of heatmap values")
    parser.add_argument("json_file", help="Path to heatmap.json")
    parser.add_argument("-o", "--output", help="Output PNG file")
    args = parser.parse_args()
    
    plot_histogram(args.json_file, args.output)
