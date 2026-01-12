#!/usr/bin/env python3
import subprocess
import datetime
import sys

BIN = "./target/release/local_mixing_bin"

def run(cmd):
    full_cmd = [BIN] + cmd
    print(f"[{datetime.datetime.now()}] Running: {' '.join(full_cmd)}")
    result = subprocess.run(full_cmd)
    if result.returncode != 0:
        print(f"Error executing command: {full_cmd}")
        sys.exit(result.returncode)

def main():
    print("Starting Rainbow Table Generation...")
    
    # N=7
    run(["init", "-n", "7"])
    run(["load", "-n", "7", "-m", "2"])
    run(["load", "-n", "7", "-m", "3"])
    run(["load", "-n", "7", "-m", "4"])
    
    # N=6
    run(["init", "-n", "6"])
    run(["load", "-n", "6", "-m", "2"])
    run(["load", "-n", "6", "-m", "3"])
    run(["load", "-n", "6", "-m", "4"])
    run(["load", "-n", "6", "-m", "5"])
    
    print("All Rainbow Tables Generated Successfully!")

if __name__ == "__main__":
    main()
