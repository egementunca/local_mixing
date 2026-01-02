// Standalone demonstration of circuit simplification
// Run with: cargo run --example simplification_demo

use local_mixing::{
    circuit::CircuitSeq,
    random::random_data::random_circuit,
};

fn visualize_circuit(circuit: &CircuitSeq, num_wires: usize, label: &str) {
    println!("\n{}", "=".repeat(60));
    println!("{}", label);
    println!("{}", "=".repeat(60));
    println!("Gates: {}", circuit.gates.len());
    println!("Circuit representation:");

    // Print wire diagram
    for wire in 0..num_wires {
        print!("{:<2} --", wire);
        for gate in &circuit.gates {
            if gate[0] == wire as u8 {
                print!("( )");
            } else if gate[1] == wire as u8 {
                print!("-●-");
            } else if gate[2] == wire as u8 {
                print!("-○-");
            } else {
                print!("-|-");
            }
            print!("---");
        }
        println!();
    }

    // Print compact representation
    println!("\nCompact: {}", circuit.repr());
    println!();
}

fn canonicalize_and_remove_duplicates(circuit: &mut CircuitSeq) -> bool {
    let before_len = circuit.gates.len();

    // Step 1: Canonicalize
    circuit.canonicalize();

    // Step 2: Remove consecutive duplicates
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    let after_len = circuit.gates.len();
    after_len != before_len
}

fn main() {
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  BUTTERFLY METHOD SIMPLIFICATION TEST                    ║");
    println!("║  Testing: Random Circuit + Inverse → Identity            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    let num_wires = 5;
    let num_gates = 5;

    // Step 1: Create random circuit
    println!("\n[STEP 1] Creating random circuit with {} wires and {} gates...", num_wires, num_gates);
    let random_circ = random_circuit(num_wires, num_gates);
    visualize_circuit(&random_circ, num_wires as usize, "Random Circuit R");

    // Step 2: Create inverse (reverse)
    println!("\n[STEP 2] Creating inverse circuit R⁻¹...");
    let mut inverse_circ = random_circ.clone();
    inverse_circ.gates.reverse();
    visualize_circuit(&inverse_circ, num_wires as usize, "Inverse Circuit R⁻¹");

    // Step 3: Concatenate R ⊙ R⁻¹ (should be identity)
    println!("\n[STEP 3] Concatenating: R ⊙ R⁻¹ = Identity");
    let mut combined = random_circ.concat(&inverse_circ);
    visualize_circuit(&combined, num_wires as usize, "Combined Circuit R ⊙ R⁻¹ (before simplification)");

    // Verify it's actually an identity by checking permutation
    let perm = combined.permutation(num_wires as usize);
    let is_identity = perm.data.iter().enumerate().all(|(i, &val)| i == val);
    println!("✓ Permutation check: {}", if is_identity { "IDENTITY ✓" } else { "NOT IDENTITY ✗" });

    // Step 4: Apply simplification at least 10 times
    println!("\n[STEP 4] Applying canonicalization + duplicate removal (at least 10 rounds)...");

    let mut round = 0;
    let mut stable_count = 0;

    while round < 10 || stable_count < 3 {
        round += 1;
        let before_gates = combined.gates.len();

        let changed = canonicalize_and_remove_duplicates(&mut combined);

        let after_gates = combined.gates.len();

        if changed {
            println!("  Round {:2}: {} gates → {} gates (removed {})",
                     round, before_gates, after_gates, before_gates - after_gates);
            stable_count = 0;
        } else {
            stable_count += 1;
            println!("  Round {:2}: {} gates → {} gates (STABLE {}/3)",
                     round, before_gates, after_gates, stable_count);
        }

        if stable_count >= 3 && round >= 10 {
            break;
        }
    }

    visualize_circuit(&combined, num_wires as usize, &format!("Final Circuit after {} rounds", round));

    // Final verification
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  FINAL RESULTS                                            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!("Original circuit gates: {} gates", num_gates * 2);
    println!("Final circuit gates:    {} gates", combined.gates.len());
    println!("Reduction:              {} gates removed", (num_gates * 2) - combined.gates.len());
    println!("Expected final:         0 gates (complete identity)");

    if combined.gates.is_empty() {
        println!("\n✅ SUCCESS! Circuit fully simplified to identity (0 gates)");
    } else {
        println!("\n⚠️  Circuit not fully simplified. Remaining {} gates.", combined.gates.len());
        println!("This may be due to gate reordering creating a non-trivial but equivalent identity.");
    }

    // Now test with larger circuit
    println!("\n\n╔═══════════════════════════════════════════════════════════╗");
    println!("║  LARGER TEST: 5 wires, 50 gates                           ║");
    println!("╚═══════════════════════════════════════════════════════════╝");

    let num_gates_large = 50;
    let random_circ_large = random_circuit(num_wires, num_gates_large);
    println!("\nRandom circuit: {} gates", random_circ_large.gates.len());

    let mut inverse_circ_large = random_circ_large.clone();
    inverse_circ_large.gates.reverse();
    println!("Inverse circuit: {} gates", inverse_circ_large.gates.len());

    let mut combined_large = random_circ_large.concat(&inverse_circ_large);
    println!("Combined (R ⊙ R⁻¹): {} gates", combined_large.gates.len());

    // Verify identity
    let perm_large = combined_large.permutation(num_wires as usize);
    let is_identity_large = perm_large.data.iter().enumerate().all(|(i, &val)| i == val);
    println!("Identity check: {}", if is_identity_large { "✓ IDENTITY" } else { "✗ NOT IDENTITY" });

    // Apply simplification
    println!("\nSimplifying...");
    let mut round_large = 0;
    let mut stable_count_large = 0;

    while round_large < 10 || stable_count_large < 3 {
        round_large += 1;
        let before_gates = combined_large.gates.len();

        let changed = canonicalize_and_remove_duplicates(&mut combined_large);

        let after_gates = combined_large.gates.len();

        if changed {
            println!("  Round {:2}: {} gates → {} gates (removed {})",
                     round_large, before_gates, after_gates, before_gates - after_gates);
            stable_count_large = 0;
        } else {
            stable_count_large += 1;
            if stable_count_large <= 3 {
                println!("  Round {:2}: {} gates → {} gates (STABLE {}/3)",
                         round_large, before_gates, after_gates, stable_count_large);
            }
        }

        if stable_count_large >= 3 && round_large >= 10 {
            break;
        }
    }

    println!("\nFinal results:");
    println!("  Original: {} gates", num_gates_large * 2);
    println!("  Final:    {} gates", combined_large.gates.len());
    println!("  Removed:  {} gates", (num_gates_large * 2) - combined_large.gates.len());

    if combined_large.gates.is_empty() {
        println!("\n✅ SUCCESS! Fully simplified to 0 gates");
    } else {
        println!("\n⚠️  {} gates remain", combined_large.gates.len());
    }
}
