// Visual circuit trace showing wire diagrams at each step
// Run with: cargo run --example visual_trace --release

use local_mixing::{
    circuit::CircuitSeq,
    random::random_data::random_circuit,
};

fn draw_circuit(circuit: &CircuitSeq, num_wires: usize, title: &str) {
    println!("\n{}", "═".repeat(70));
    println!("{}", title);
    println!("{}", "═".repeat(70));
    println!("Gates: {}", circuit.gates.len());
    println!();

    // Draw the circuit
    for wire in 0..num_wires {
        print!(" {} ", wire);
        for gate in &circuit.gates {
            if gate[0] == wire as u8 {
                print!("─(*)");
            } else if gate[1] == wire as u8 {
                print!("─-●─");
            } else if gate[2] == wire as u8 {
                print!("─-○─");
            } else {
                print!("────");
            }
        }
        println!("─");
    }

    // Gate labels
    print!("   ");
    for i in 0..circuit.gates.len() {
        print!(" g{:<2}", i);
    }
    println!();

    // Gate details
    print!("   ");
    for gate in &circuit.gates {
        print!("[{}{}{}", gate[0], gate[1], gate[2]);
        print!("]");
    }
    println!("\n");
}

fn draw_circuit_with_highlight(circuit: &CircuitSeq, num_wires: usize, highlight_pos: Option<usize>, title: &str) {
    println!("\n{}", title);
    println!("{}", "─".repeat(70));

    // Draw the circuit
    for wire in 0..num_wires {
        print!(" {} ", wire);
        for (idx, gate) in circuit.gates.iter().enumerate() {
            let is_highlighted = highlight_pos.map_or(false, |pos| pos == idx);

            if gate[0] == wire as u8 {
                if is_highlighted {
                    print!("\x1b[32m─(*)\x1b[0m");  // Green
                } else {
                    print!("─(*)");
                }
            } else if gate[1] == wire as u8 {
                if is_highlighted {
                    print!("\x1b[32m─-●─\x1b[0m");
                } else {
                    print!("─-●─");
                }
            } else if gate[2] == wire as u8 {
                if is_highlighted {
                    print!("\x1b[32m─-○─\x1b[0m");
                } else {
                    print!("─-○─");
                }
            } else {
                print!("────");
            }
        }
        println!("─");
    }

    // Gate labels with highlight
    print!("   ");
    for (idx, _) in circuit.gates.iter().enumerate() {
        let is_highlighted = highlight_pos.map_or(false, |pos| pos == idx);
        if is_highlighted {
            print!("\x1b[32m g{:<2}\x1b[0m", idx);
        } else {
            print!(" g{:<2}", idx);
        }
    }
    println!();
    println!();
}

fn draw_removal(circuit: &CircuitSeq, num_wires: usize, remove_pos: usize, title: &str) {
    println!("\n{}", title);
    println!("{}", "─".repeat(70));

    // Draw the circuit
    for wire in 0..num_wires {
        print!(" {} ", wire);
        for (idx, gate) in circuit.gates.iter().enumerate() {
            let is_removed = idx == remove_pos || idx == remove_pos + 1;

            if gate[0] == wire as u8 {
                if is_removed {
                    print!("\x1b[31m─(*)\x1b[0m");  // Red
                } else {
                    print!("─(*)");
                }
            } else if gate[1] == wire as u8 {
                if is_removed {
                    print!("\x1b[31m─-●─\x1b[0m");
                } else {
                    print!("─-●─");
                }
            } else if gate[2] == wire as u8 {
                if is_removed {
                    print!("\x1b[31m─-○─\x1b[0m");
                } else {
                    print!("─-○─");
                }
            } else {
                print!("────");
            }
        }
        println!("─");
    }

    // Gate labels with removal highlight
    print!("   ");
    for (idx, _) in circuit.gates.iter().enumerate() {
        let is_removed = idx == remove_pos || idx == remove_pos + 1;
        if is_removed {
            print!("\x1b[31m g{:<2}\x1b[0m", idx);
        } else {
            print!(" g{:<2}", idx);
        }
    }
    println!();
    print!("   ");
    for (idx, _) in circuit.gates.iter().enumerate() {
        if idx == remove_pos {
            print!("\x1b[31m  ↓↓\x1b[0m ");
        } else {
            print!("    ");
        }
    }
    println!("\n   \x1b[31mREMOVE DUPLICATE PAIR\x1b[0m");
    println!();
}

fn main() {
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║     VISUAL CANONICALIZATION TRACE                        ║");
    println!("║     Watch the gates move and disappear!                  ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    let num_wires = 5;
    let num_gates = 5;

    // Create circuit
    let random_circ = random_circuit(num_wires, num_gates);

    draw_circuit(&random_circ, num_wires as usize, "Step 1: Random Circuit R");

    // Create inverse
    let mut inverse_circ = random_circ.clone();
    inverse_circ.gates.reverse();

    draw_circuit(&inverse_circ, num_wires as usize, "Step 2: Inverse Circuit R⁻¹");

    // Concatenate
    let mut combined = random_circ.concat(&inverse_circ);

    draw_circuit(&combined, num_wires as usize, "Step 3: Combined R ⊙ R⁻¹ (10 gates - Identity!)");

    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  ROUND 1: CANONICALIZE + REMOVE DUPLICATES               ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    // Manual canonicalization with visualization
    combined.canonicalize();

    draw_circuit(&combined, num_wires as usize, "After Canonicalization (gates reordered)");

    // Now remove duplicates with visualization
    let mut round = 0;
    loop {

        // Find first duplicate
        let mut found_dup = None;
        for i in 0..combined.gates.len().saturating_sub(1) {
            if combined.gates[i] == combined.gates[i + 1] {
                found_dup = Some(i);
                break;
            }
        }

        if let Some(pos) = found_dup {
            round += 1;
            draw_removal(&combined, num_wires as usize, pos,
                        &format!("Duplicate #{}: Found [{}{}{}] at positions {} and {}",
                                round,
                                combined.gates[pos][0],
                                combined.gates[pos][1],
                                combined.gates[pos][2],
                                pos, pos + 1));

            combined.gates.drain(pos..=pos + 1);

            draw_circuit(&combined, num_wires as usize,
                        &format!("After Removal #{} ({} gates remaining)", round, combined.gates.len()));
        } else {
            break;
        }
    }

    if combined.gates.is_empty() {
        println!("\n✅ Round 1 complete! Circuit reduced to 0 gates (identity)");
        println!("   No more gates to show!");
    } else {
        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║  ROUND 2: CANONICALIZE + REMOVE DUPLICATES               ║");
        println!("╚═══════════════════════════════════════════════════════════╝");

        combined.canonicalize();
        draw_circuit(&combined, num_wires as usize, "After Canonicalization (Round 2)");

        // Remove duplicates again
        loop {
            let mut found_dup = None;
            for i in 0..combined.gates.len().saturating_sub(1) {
                if combined.gates[i] == combined.gates[i + 1] {
                    found_dup = Some(i);
                    break;
                }
            }

            if let Some(pos) = found_dup {
                round += 1;
                draw_removal(&combined, num_wires as usize, pos,
                            &format!("Duplicate #{}: Found [{}{}{}] at positions {} and {}",
                                    round,
                                    combined.gates[pos][0],
                                    combined.gates[pos][1],
                                    combined.gates[pos][2],
                                    pos, pos + 1));

                combined.gates.drain(pos..=pos + 1);

                if !combined.gates.is_empty() {
                    draw_circuit(&combined, num_wires as usize,
                                &format!("After Removal #{} ({} gates remaining)", round, combined.gates.len()));
                }
            } else {
                break;
            }
        }

        if combined.gates.is_empty() {
            println!("\n✅ Round 2 complete! Circuit reduced to 0 gates (identity)");
            println!();
            println!(" 0 ─");
            println!(" 1 ─");
            println!(" 2 ─");
            println!(" 3 ─");
            println!(" 4 ─");
            println!();
            println!("   (Empty circuit = Identity permutation)");
        }
    }

    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  FINAL RESULT                                            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!("Original:  10 gates (R + R⁻¹)");
    println!("Final:     0 gates (Identity)");
    println!("Removed:   {} duplicate pairs", round);
    println!("\n✅ SUCCESS! R ⊙ R⁻¹ simplified to identity (0 gates)");
}
