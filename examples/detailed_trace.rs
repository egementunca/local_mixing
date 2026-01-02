// Detailed step-by-step trace of canonicalization
// Run with: cargo run --example detailed_trace --release

use local_mixing::{
    circuit::CircuitSeq,
    random::random_data::random_circuit,
};

fn print_gate(gate: &[u8; 3]) -> String {
    format!("[{},{},{}]", gate[0], gate[1], gate[2])
}

fn print_circuit(circuit: &CircuitSeq, label: &str) {
    println!("\n{}", label);
    println!("Gates: {}", circuit.gates.len());
    print!("  ");
    for (i, gate) in circuit.gates.iter().enumerate() {
        print!("g{}: {}  ", i, print_gate(gate));
    }
    println!();
}

fn print_circuit_visual(circuit: &CircuitSeq, num_wires: usize) {
    for wire in 0..num_wires {
        print!("{} ", wire);
        for gate in &circuit.gates {
            if gate[0] == wire as u8 {
                print!("(*)");
            } else if gate[1] == wire as u8 {
                print!("-●-");
            } else if gate[2] == wire as u8 {
                print!("-○-");
            } else {
                print!("---");
            }
        }
        println!();
    }
}

fn check_collision(gate1: &[u8; 3], gate2: &[u8; 3]) -> bool {
    gate1[0] == gate2[1] || gate1[0] == gate2[2] ||
    gate1[1] == gate2[0] || gate1[2] == gate2[0]
}

fn canonicalize_verbose(circuit: &mut CircuitSeq) {
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║           CANONICALIZATION PROCESS (DETAILED)             ║");
    println!("╚════════════════════════════════════════════════════════════╝");

    for i in 1..circuit.gates.len() {
        let gi = circuit.gates[i];
        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Processing gate at position {}: {}", i, print_gate(&gi));
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let mut to_swap: Option<usize> = None;
        let mut j = i;

        while j > 0 {
            j -= 1;
            let gj = circuit.gates[j];

            println!("  Comparing with gate at position {}: {}", j, print_gate(&gj));

            // Check collision
            let collides = check_collision(&gi, &gj);
            if collides {
                println!("    ✗ COLLISION detected! Cannot move past this gate.");
                println!("      Reason: Active wire {} conflicts with control wires [{},{}]",
                         gi[0], gj[1], gj[2]);
                break;
            }

            // Check ordering
            let ordered = gi[0] <= gj[0] &&
                         (gi[0] != gj[0] || gi[1] <= gj[1]) &&
                         (gi[0] != gj[0] || gi[1] != gj[1] || gi[2] <= gj[2]);

            if !ordered {
                println!("    → Not in canonical order. Mark for swap.");
                to_swap = Some(j);
            } else {
                println!("    ✓ Already in canonical order relative to this gate.");
            }
        }

        if let Some(pos) = to_swap {
            println!("\n  ⚡ MOVING gate {} from position {} to position {}",
                     print_gate(&gi), i, pos);
            let g = circuit.gates[i];
            circuit.gates.remove(i);
            circuit.gates.insert(pos, g);

            print!("  Result: ");
            for (idx, gate) in circuit.gates.iter().enumerate() {
                if idx == pos {
                    print!("\x1b[32m{}\x1b[0m ", print_gate(gate)); // Green
                } else {
                    print!("{} ", print_gate(gate));
                }
            }
            println!();
        } else {
            println!("\n  ✓ Gate stays at position {}", i);
        }
    }
}

fn remove_duplicates_verbose(circuit: &mut CircuitSeq) -> bool {
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║         DUPLICATE REMOVAL PROCESS (DETAILED)              ║");
    println!("╚════════════════════════════════════════════════════════════╝");

    let before_len = circuit.gates.len();
    let mut removed_any = false;
    let mut i = 0;

    while i < circuit.gates.len().saturating_sub(1) {
        let g1 = circuit.gates[i];
        let g2 = circuit.gates[i + 1];

        if g1 == g2 {
            println!("\n  Found duplicate pair at positions {} and {}:", i, i + 1);
            println!("    Gate: {}", print_gate(&g1));
            println!("    ⚡ REMOVING both gates (g ⊙ g = identity)");

            circuit.gates.drain(i..=i + 1);
            removed_any = true;

            // Backtrack
            if i >= 2 {
                i = i.saturating_sub(2);
                println!("    ← Backtracking to position {} to check for new duplicates", i);
            }

            print!("    Remaining gates: ");
            for gate in &circuit.gates {
                print!("{} ", print_gate(gate));
            }
            println!("({})", circuit.gates.len());
        } else {
            i += 1;
        }
    }

    if !removed_any {
        println!("\n  ✓ No consecutive duplicates found.");
    }

    let after_len = circuit.gates.len();
    after_len != before_len
}

fn main() {
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║     DETAILED CANONICALIZATION TRACE                      ║");
    println!("║     Small Circuit: 5 wires, 5 gates                      ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    let num_wires = 5;
    let num_gates = 5;

    // Create random circuit with fixed seed for reproducibility
    let random_circ = random_circuit(num_wires, num_gates);

    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  STEP 1: CREATE RANDOM CIRCUIT R                         ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    print_circuit(&random_circ, "Random Circuit R:");
    println!("\nVisual representation:");
    print_circuit_visual(&random_circ, num_wires as usize);

    // Create inverse
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  STEP 2: CREATE INVERSE R⁻¹                              ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    let mut inverse_circ = random_circ.clone();
    inverse_circ.gates.reverse();
    print_circuit(&inverse_circ, "Inverse Circuit R⁻¹:");
    println!("\nVisual representation:");
    print_circuit_visual(&inverse_circ, num_wires as usize);

    // Concatenate
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  STEP 3: CONCATENATE R ⊙ R⁻¹                             ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    let mut combined = random_circ.concat(&inverse_circ);
    print_circuit(&combined, "Combined Circuit (before simplification):");

    println!("\nCompact representation:");
    println!("  {}", combined.repr());

    println!("\nNotice the symmetry:");
    print!("  First half:  ");
    for i in 0..num_gates {
        print!("{} ", print_gate(&combined.gates[i]));
    }
    println!();
    print!("  Second half: ");
    for i in num_gates..combined.gates.len() {
        print!("{} ", print_gate(&combined.gates[i]));
    }
    println!();

    // Verify identity
    let perm = combined.permutation(num_wires as usize);
    let is_identity = perm.data.iter().enumerate().all(|(i, &val)| i == val);
    println!("\n✓ Permutation verification: {}",
             if is_identity { "IS IDENTITY ✓" } else { "NOT IDENTITY ✗" });

    // Now simplify with detailed traces
    let mut round = 0;

    loop {
        round += 1;
        println!("\n\n");
        println!("╔═══════════════════════════════════════════════════════════╗");
        println!("║  ROUND {:<2}                                               ║", round);
        println!("╚═══════════════════════════════════════════════════════════╝");

        let before_gates = combined.gates.len();

        if before_gates == 0 {
            println!("\n✅ Circuit is empty (identity)! No more simplification needed.");
            break;
        }

        print_circuit(&combined, &format!("Input to round {} ({} gates):", round, before_gates));

        // Canonicalize
        canonicalize_verbose(&mut combined);

        let after_canon = combined.gates.len();
        print_circuit(&combined, &format!("After canonicalization ({} gates):", after_canon));

        // Remove duplicates
        let changed = remove_duplicates_verbose(&mut combined);

        let after_gates = combined.gates.len();

        println!("\n┌──────────────────────────────────────────────────────────┐");
        println!("│  ROUND {} SUMMARY                                         │", round);
        println!("├──────────────────────────────────────────────────────────┤");
        println!("│  Before:  {} gates                                       │", before_gates);
        println!("│  After:   {} gates                                       │", after_gates);
        println!("│  Removed: {} gates                                       │", before_gates - after_gates);
        if changed {
            println!("│  Status:  CHANGED ⚡                                      │");
        } else {
            println!("│  Status:  STABLE ✓                                       │");
        }
        println!("└──────────────────────────────────────────────────────────┘");

        if !changed && after_gates == before_gates {
            println!("\n✅ No changes in this round. Circuit is stable.");
            break;
        }

        if after_gates == 0 {
            println!("\n✅ SUCCESS! Circuit fully simplified to identity (0 gates)!");
            break;
        }
    }

    println!("\n\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  FINAL RESULTS                                            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!("Total rounds:        {}", round);
    println!("Original gates:      {}", num_gates * 2);
    println!("Final gates:         {}", combined.gates.len());
    println!("Gates removed:       {}", (num_gates * 2) - combined.gates.len());
    println!("Simplification:      {}%",
             100 * ((num_gates * 2) - combined.gates.len()) / (num_gates * 2));

    if combined.gates.is_empty() {
        println!("\n✅ SUCCESS! Circuit is now the identity (0 gates)");
    }
}
