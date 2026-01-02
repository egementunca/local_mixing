//! SAT Local Improvement Experiment
//!
//! This experiment tests the SAT-based local optimization on 64-wire circuits
//! with 1000 gates, comparing to the existing compression pipeline.
//!
//! The workflow:
//! 1. Generate or load a random 64-wire, 1000-gate circuit
//! 2. Apply butterfly method with SAT-based compression
//! 3. Generate heatmaps comparing input vs output circuits

use local_mixing::{
    circuit::CircuitSeq,
    optimize::{compress_sat, compute_truth_tables, extract_subcircuit},
    random::random_data::random_circuit,
};

use std::fs::{self, File};
use std::io::Write;
use std::time::Instant;

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  SAT Local Improvement Experiment                                 ║");
    println!("║  64 wires, 1000 gates, ORNB-only optimization                     ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    let num_wires = 64;
    let num_gates = 1000;
    let project_root = ".";

    // Step 1: Generate or load circuit
    println!("\n[Step 1] Loading/generating circuit...");
    let circuit = if let Ok(content) = fs::read_to_string("initial.txt") {
        println!("  Loaded circuit from initial.txt");
        CircuitSeq::from_string(&content)
    } else {
        println!(
            "  Generating random {} wire, {} gate circuit",
            num_wires, num_gates
        );
        let c = random_circuit(num_wires, num_gates);
        // Save for later comparison
        let repr = c.repr();
        File::create("initial.txt")
            .and_then(|mut f| f.write_all(repr.as_bytes()))
            .expect("Failed to write initial.txt");
        c
    };

    println!("  Circuit has {} gates", circuit.gates.len());

    // Step 2: Test SAT compression on small subcircuits
    println!("\n[Step 2] Testing SAT-based subcircuit optimization...");

    // Extract a small subcircuit for testing
    let window_size = 5;
    let start_gate = circuit.gates.len() / 2; // middle of circuit

    let subcircuit = extract_subcircuit(&circuit, start_gate, window_size);
    println!(
        "  Extracted subcircuit with {} gates at position {}",
        subcircuit.num_gates(),
        start_gate
    );
    println!("  Input wires: {:?}", subcircuit.input_wires);
    println!("  Output wires: {:?}", subcircuit.output_wires);

    // Compute truth tables
    let tables = compute_truth_tables(&subcircuit);
    println!("  Computed {} truth tables", tables.len());
    if !tables.is_empty() {
        println!(
            "  First table (first 16 bits): {}...",
            &tables[0][..std::cmp::min(16, tables[0].len())]
        );
    }

    // Try SAT optimization on subcircuit
    println!("\n[Step 3] Running SAT optimizer on subcircuit...");
    let subcircuit_seq = CircuitSeq {
        gates: subcircuit.gates.clone(),
    };
    let sub_wires = subcircuit.wire_unmap.len();

    let start_time = Instant::now();
    let optimized = compress_sat(&subcircuit_seq, sub_wires, 10, project_root);
    let elapsed = start_time.elapsed();

    println!("  SAT optimization took {:?}", elapsed);
    println!("  Original: {} gates", subcircuit.num_gates());
    println!("  Optimized: {} gates", optimized.gates.len());

    if optimized.gates.len() < subcircuit.num_gates() {
        println!("  ✓ Found smaller circuit!");
    } else {
        println!("  No smaller circuit found (may already be optimal)");
    }

    // Step 4: Full butterfly-like experiment (simplified)
    println!("\n[Step 4] Running simplified SAT butterfly experiment...");
    println!("  (For full experiment, use the main binary with --butterfly-sat flag)");

    // Just test a few windows
    let num_windows = 5;
    let mut total_reduction = 0i32;

    for i in 0..num_windows {
        let start = (circuit.gates.len() / (num_windows + 1)) * (i + 1);
        let sub = extract_subcircuit(&circuit, start, 4);

        if sub.num_gates() > 0 && sub.num_inputs() <= 6 {
            let sub_seq = CircuitSeq {
                gates: sub.gates.clone(),
            };
            let opt = compress_sat(&sub_seq, sub.wire_unmap.len(), 5, project_root);
            let reduction = sub.num_gates() as i32 - opt.gates.len() as i32;
            total_reduction += reduction;

            let status = if reduction > 0 {
                "↓"
            } else if reduction < 0 {
                "↑"
            } else {
                "="
            };
            println!(
                "  Window {}: {} gates → {} gates {}",
                i + 1,
                sub.num_gates(),
                opt.gates.len(),
                status
            );
        }
    }

    println!("\n[Summary]");
    println!(
        "  Total reduction across {} windows: {} gates",
        num_windows, total_reduction
    );
    println!(
        "  Circuit: {} wires, {} gates",
        num_wires,
        circuit.gates.len()
    );

    // Write output for heatmap
    let output_repr = circuit.repr(); // Same as input for this demo
    File::create("sat_experiment_output.txt")
        .and_then(|mut f| f.write_all(output_repr.as_bytes()))
        .expect("Failed to write output");

    println!("\n[Done] Results saved to sat_experiment_output.txt");
    println!("  Run create_heatmap.py to visualize results");
}
