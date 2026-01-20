use crate::{
    algorithms::butterfly::replace::{
        compress, compress_big, compress_big_sat, compress_big_sat_lmdb, expand_big, obfuscate,
        outward_compress, random_id, replace_pairs,
    },
    config::ObfuscationConfig,
    infra::circuit::circuit::CircuitSeq,
    infra::random::random_data::shoot_random_gate,
};
// use crate::infra::random::random_data::random_walk_no_skeleton;

use itertools::Itertools;
use rand::Rng;
use rayon::prelude::*;

use crate::algorithms::butterfly::replace::random_gate_replacements;
use rusqlite::{Connection, OpenFlags};

use once_cell::sync::Lazy;

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};
fn obfuscate_and_target_compress(
    c: &CircuitSeq,
    conn: &mut Connection,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    // Obfuscate circuit, get positions of inverses
    let (mut final_circuit, inverse_starts) = obfuscate(c, n);
    println!("{}", final_circuit.to_string(n));
    //let (mut final_circuit, inverse_starts) = obfuscate(&_final_circuit, n);
    println!(
        "{:?} Obf Len: {}",
        pin_counts(&final_circuit, n),
        final_circuit.gates.len()
    );
    // For each gate, compress its "inverse+gate+next_random" slice
    // Reverse iteration to avoid index shifting issues

    for i in (0..c.gates.len()).rev() {
        // ri^-1 start
        let start = inverse_starts[i];

        // r_{i+1} start is the next inverse start
        let end = inverse_starts[i + 1]; // safe because i < c.gates.len()
        // Slice the subcircuit: r_i^-1 ⋅ g_i ⋅ r_{i+1}
        let sub_slice = &final_circuit.gates[start..end];

        // Wrap it into a CircuitSeq
        let sub_circuit = CircuitSeq {
            gates: sub_slice.to_vec(),
        };

        // Compress the subcircuit
        let compressed_sub = compress(&sub_circuit, 100_000, conn, &bit_shuf, n); // compress doesn't take config yet, might need update or bypass
        // Replace the slice in the final circuit
        if sub_circuit.gates != compressed_sub.gates {
            println!("The compression hid g_{}", i);
        }
        final_circuit.gates.splice(start..end, compressed_sub.gates);
    }
    let mut com_len = final_circuit.gates.len();
    let mut count = 0;
    while count < 3 {
        final_circuit = compress(&final_circuit, 100_000, conn, &bit_shuf, n);
        if final_circuit.gates.len() == com_len {
            count += 1;
        } else {
            com_len = final_circuit.gates.len();
        }
    }
    println!(
        "{:?} Compressed Len: {}",
        pin_counts(&final_circuit, n),
        final_circuit.gates.len()
    );
    final_circuit
}

pub fn pin_counts(circuit: &CircuitSeq, num_wires: usize) -> Vec<usize> {
    let mut counts = vec![0; num_wires];
    for gate in &circuit.gates {
        counts[gate[0] as usize] += 1;
        counts[gate[1] as usize] += 1;
        counts[gate[2] as usize] += 1;
    }
    counts
}

pub fn butterfly(
    c: &CircuitSeq,
    conn: &mut Connection,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    // Pick one random R
    let mut rng = rand::rng();
    let (r, r_inv) = random_id(n as u8, rng.random_range(3..=25));

    println!("Butterfly start: {} gates", c.gates.len());

    let r = &r; // reference is enough; read-only
    let r_inv = &r_inv; // same
    let bit_shuf = &bit_shuf;

    // Parallel processing of gates
    let blocks: Vec<_> = c
        .gates
        .par_iter()
        .enumerate()
        .map(|(i, &g)| {
            // wrap single gate as CircuitSeq
            let gi = CircuitSeq { gates: vec![g] };

            // create a read-only connection per thread
            let mut conn =
                Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("Failed to open read-only connection");

            // compress the block
            let compressed_block = outward_compress(&gi, r, 100_000, &mut conn, bit_shuf, n);

            println!(
                "  Block {}: before {} gates → after {} gates",
                i,
                r_inv.gates.len() * 2 + 1, // approximate size
                compressed_block.gates.len()
            );

            println!("  {}", compressed_block.repr());

            compressed_block
        })
        .collect();

    // Combine blocks hierarchically
    let mut acc = blocks[0].clone();
    println!("Start combining: {}", acc.gates.len());

    for (i, b) in blocks.into_iter().skip(1).enumerate() {
        let combined = acc.concat(&b);
        let before = combined.gates.len();
        acc = compress(&combined, 500_000, conn, bit_shuf, n);
        let after = acc.gates.len();

        println!("  Combine step {}: {} → {} gates", i + 1, before, after);
    }

    // Add bookends: R ... R*
    acc = r.concat(&acc).concat(&r_inv);
    println!("After adding bookends: {} gates", acc.gates.len());

    // Final global compression (until stable 3x)
    let mut stable_count = 0;
    while stable_count < 3 {
        let before = acc.gates.len();
        acc = compress(&acc, 1_000_000, conn, bit_shuf, n); // update compress signature if needed
        let after = acc.gates.len();

        if after == before {
            stable_count += 1;
            println!(
                "  Final compression stable {}/3 at {} gates",
                stable_count, after
            );
        } else {
            println!("  Final compression reduced: {} → {} gates", before, after);
            stable_count = 0;
        }
    }

    let mut i = 0;
    while i < acc.gates.len().saturating_sub(1) {
        if acc.gates[i] == acc.gates[i + 1] {
            // remove elements at i and i+1
            acc.gates.drain(i..=i + 1);

            // step back up to 2 indices, but not below 0
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    //writeln!(file, "Permutation after remove identities 2 is: \n{:?}", acc.permutation(n).data).unwrap();
    println!("Compressed len: {}", acc.gates.len());

    println!("Butterfly done: {} gates", acc.gates.len());

    acc
}

pub fn merge_combine_blocks(
    blocks: &[CircuitSeq],
    n: usize,
    db_path: &str,
    progress: &Arc<AtomicUsize>,
    _total: usize,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    println!("Phase 1: Pairwise merge");
    // let total_1 = (blocks.len()+1)/2;
    let pairs: Vec<CircuitSeq> = blocks
        .par_chunks(2)
        .map(|chunk| {
            let mut conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("Failed to open DB");

            let combined = if chunk.len() == 2 {
                chunk[0].concat(&chunk[1])
            } else {
                chunk[0].clone()
            };

            let compressed = compress_big(
                &combined,
                30,
                n,
                &mut conn,
                env,
                &bit_shuf_list,
                dbs,
                config,
            );
            compressed
        })
        .collect();

    println!("Phase 2: Offset pairwise merge");

    // Skip the first block
    let mut phase2_blocks = Vec::new();
    phase2_blocks.push(pairs[0].clone());

    // Pair the rest starting from index 1
    let rest = &pairs[1..];

    let phase2_pairs: Vec<CircuitSeq> = rest
        .par_chunks(2)
        .map(|chunk| {
            let mut conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("Failed to open DB");

            let combined = if chunk.len() == 2 {
                chunk[0].concat(&chunk[1])
            } else {
                chunk[0].clone()
            };

            let compressed = compress_big(
                &combined,
                30,
                n,
                &mut conn,
                env,
                &bit_shuf_list,
                dbs,
                config,
            );

            // let _done = progress2.fetch_add(1, Ordering::Relaxed) + 1;
            // if done % 10 == 0 {
            //     println!("Phase 2 progress: {}/{}", done, total_2);
            // }

            compressed
        })
        .collect();

    // Prepend the untouched first block
    phase2_blocks.extend(phase2_pairs);

    println!("Phase 3: 4-way merge");
    progress.store(0, Ordering::Relaxed);
    let chunk_size = (pairs.len() + 3) / 4;
    let phase2_results: Vec<CircuitSeq> = pairs
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("Failed to open DB");

            let mut combined = CircuitSeq { gates: vec![] };
            for block in chunk {
                combined = combined.concat(block);
            }

            let compressed = compress_big(
                &combined,
                200,
                n,
                &mut conn,
                env,
                &bit_shuf_list,
                dbs,
                config,
            );

            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            println!("Phase 2 partial done: {}/4", done);

            compressed
        })
        .collect();

    println!("Phase 4: Final merge");
    // Final combination and compression
    let mut conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("Failed to open DB");

    let mut final_combined = CircuitSeq { gates: vec![] };
    for part in phase2_results {
        final_combined = final_combined.concat(&part);
    }

    let final_compressed = compress_big(
        &final_combined,
        1000,
        n,
        &mut conn,
        env,
        &bit_shuf_list,
        dbs,
        config,
    );

    println!("All phases complete");
    final_compressed
}

// fn initial_milestone(acc: usize) -> usize {
//     if acc >= 10_000 {
//         (acc / 10_000) * 10_000   // nearest 10k below
//     } else if acc >= 5_000 {
//         5_000
//     } else if acc >= 2_000 {
//         2_000
//     } else if acc >= 1_000 {
//         1_000
//     } else {
//         0
//     }
// }

/// Given the previous milestone, decide the next lower one
// fn next_milestone(prev: usize) -> usize {
//     match prev {
//         x if x > 10_000 => x - 10_000,
//         10_000 => 5_000,
//         5_000 => 2_000,
//         2_000 => 1_000,
//         _ => 0,
//     }
// }

pub fn butterfly_big(
    c: &CircuitSeq,
    _conn: &mut Connection,
    n: usize,
    last: bool,
    stop: usize,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    // Pick one random R
    let mut rng = rand::rng();
    let (r, r_inv) = random_id(
        n as u8,
        rng.random_range(config.structure_block_size_min..=config.structure_block_size_max),
    );
    let mut c = c.clone();
    replace_pairs(&mut c, n, _conn, &env, None);
    if config.single_gate_mode {
        random_gate_replacements(&mut c, config.single_gate_replacements, n, _conn, env);
    }
    shoot_random_gate(&mut c, config.shooting_count);
    println!("Butterfly start: {} gates", c.gates.len());

    let r = &r; // reference is enough; read-only
    let r_inv = &r_inv; // same
    // Parallel processing of gates
    let blocks: Vec<CircuitSeq> = c
        .gates
        .par_iter()
        .enumerate()
        .map(|(i, &g)| {
            // wrap single gate as CircuitSeq
            let mut gi = r_inv.concat(&CircuitSeq { gates: vec![g] }).concat(&r);
            shoot_random_gate(&mut gi, config.shooting_count_inner);
            // create a read-only connection per thread
            let mut conn =
                Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("Failed to open read-only connection");
            //shoot_random_gate(&mut gi, 100_000);
            let compressed_block = if config.sat_mode {
                compress_big_sat(
                    &gi,
                    config.compression_window_size_sat,
                    n,
                    config.compression_sat_limit as u64,
                )
            } else {
                let expanded = if config.no_ancilla_mode {
                    gi.clone()
                } else {
                    expand_big(
                        &gi,
                        config.compression_window_size,
                        n,
                        &mut conn,
                        env,
                        &bit_shuf_list,
                        dbs,
                    )
                };
                compress_big(
                    &expanded,
                    config.compression_window_size,
                    n,
                    &mut conn,
                    env,
                    &bit_shuf_list,
                    dbs,
                    config,
                )
            };
            let before_len = r_inv.gates.len() * 2 + 1;
            let after_len = compressed_block.gates.len();

            let color_line = if after_len < before_len {
                "\x1b[32m──────────────\x1b[0m" // green
            } else if after_len > before_len {
                "\x1b[31m──────────────\x1b[0m" // red
            } else if gi.gates != compressed_block.gates {
                "\x1b[35m──────────────\x1b[0m" // purple
            } else {
                "\x1b[90m──────────────\x1b[0m" // gray
            };

            println!(
                "  Block {}: before {} gates → after {} gates  {}",
                i, before_len, after_len, color_line
            );

            // println!("  {}", compressed_block.repr());

            compressed_block
        })
        .collect();

    let progress = Arc::new(AtomicUsize::new(0));
    let _total = 2 * blocks.len() - 1;

    println!("Beginning merge");

    let mut acc = merge_combine_blocks(
        &blocks,
        n,
        "./db/circuits.db",
        &progress,
        _total,
        env,
        &bit_shuf_list,
        dbs,
        config,
    );

    // Add bookends: R ... R*
    acc = r.concat(&acc).concat(&r_inv);
    println!("After adding bookends: {} gates", acc.gates.len());
    // let mut milestone = initial_milestone(acc.gates.len());
    // Final global compression (until stable 3x)
    let mut stable_count = 0;
    while stable_count < config.final_stability_threshold {
        // if acc.gates.len() <= milestone {
        //     let mut f = OpenOptions::new()
        //         .create(true)
        //         .append(true)
        //         .open("bcircuitlist.txt")
        //         .expect("Could not open bcircuitlist.txt");

        //     writeln!(f, "{}", acc.repr()).unwrap();
        //     milestone = next_milestone(milestone);
        // }
        let before = acc.gates.len();

        let k = if before <= config.chunk_split_base {
            1
        } else {
            (before + config.chunk_split_base - 1) / config.chunk_split_base
        };

        let mut rng = rand::rng();

        let chunks = split_into_random_chunks(&acc.gates, k, &mut rng);
        let compressed_chunks: Vec<Vec<[u8; 3]>> = chunks
            .into_par_iter()
            .map(|chunk| {
                let sub = CircuitSeq { gates: chunk };
                let mut thread_conn =
                    Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .expect("Failed to open read-only connection");
                compress_big(
                    &sub,
                    config.compression_window_size,
                    n,
                    &mut thread_conn,
                    env,
                    &bit_shuf_list,
                    dbs,
                    config,
                )
                .gates
            })
            .collect();

        let new_gates: Vec<[u8; 3]> = compressed_chunks.into_iter().flatten().collect();
        acc.gates = new_gates;

        let after = acc.gates.len();
        if last && acc.gates.len() <= stop {
            break;
        }
        if after == before {
            stable_count += 1;
            println!(
                "  Final compression stable {}/3 at {} gates",
                stable_count, after
            );
        } else {
            println!("  Final compression reduced: {} → {} gates", before, after);
            stable_count = 0;
        }
    }
    println!("Compressed len: {}", acc.gates.len());

    println!("Butterfly done: {} gates", acc.gates.len());

    acc
}

pub static SHOOT_RANDOM_GATE_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static REPLACE_PAIRS_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static RANDOM_ID_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static EXPAND_BIG_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static COMPRESS_BIG_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
pub static MERGE_COMBINE_BLOCKS_TIME: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));

static CURRENT_ACC: Lazy<Mutex<Option<CircuitSeq>>> = Lazy::new(|| Mutex::new(None));

static SHOULD_DUMP: AtomicBool = AtomicBool::new(false);
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::process::exit;
use std::thread;

pub fn install_kill_handler() {
    let mut signals = Signals::new([SIGINT, SIGTERM]).expect("signals");

    thread::spawn(move || {
        for _ in signals.forever() {
            eprintln!("Received termination signal, dumping acc...");
            SHOULD_DUMP.store(true, Ordering::SeqCst);
            break;
        }
    });
}

fn dump_and_exit() -> ! {
    if let Some(acc) = CURRENT_ACC.lock().unwrap().as_ref() {
        let mut f = File::create("killed.txt").expect("create killed.txt");
        writeln!(f, "{}", acc.repr()).expect("write");
        eprintln!("Wrote killed.txt");
    }
    exit(1);
}

pub fn abutterfly_big(
    c: &CircuitSeq,
    _conn: &mut Connection,
    n: usize,
    last: bool,
    stop: usize,
    env: &lmdb::Environment,
    curr_round: usize,
    last_round: usize,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
    config: &ObfuscationConfig,
    template_db: Option<&crate::infra::store::reader::TemplateDB>,
) -> CircuitSeq {
    println!("Current round: {}/{}", curr_round, last_round);
    println!("Butterfly start: {} gates", c.gates.len());
    let mut rng = rand::rng();

    let mut c = c.clone();
    let t0 = Instant::now();
    let mut c = c.clone();
    let t0 = Instant::now();
    shoot_random_gate(&mut c, config.shooting_count);
    SHOOT_RANDOM_GATE_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    // c = random_walk_no_skeleton(&c, &mut rng);
    let (first_r, first_r_inv) = random_id(
        n as u8,
        rng.random_range(config.structure_block_size_min..=config.structure_block_size_max),
    );
    let mut prev_r_inv = first_r_inv.clone();
    let t1 = Instant::now();
    if config.pair_replacement_mode {
        replace_pairs(&mut c, n, _conn, &env, template_db);
    }
    REPLACE_PAIRS_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);
    if config.single_gate_mode {
        random_gate_replacements(&mut c, config.single_gate_replacements, n, _conn, env);
        println!("Single gate replacements would happen here if imported");
    }

    let mut pre_blocks: Vec<CircuitSeq> = Vec::with_capacity(c.gates.len());

    for &g in &c.gates {
        let t2 = Instant::now();
        let (r, r_inv) = random_id(
            n as u8,
            rng.random_range(config.structure_block_size_min..=config.structure_block_size_max),
        );
        RANDOM_ID_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);
        let mut block = prev_r_inv
            .clone()
            .concat(&CircuitSeq { gates: vec![g] })
            .concat(&r);
        shoot_random_gate(&mut block, config.shooting_count_inner);
        // block = random_walk_no_skeleton(&block, &mut rng);
        pre_blocks.push(block);
        prev_r_inv = r_inv;
    }
    let grew = Arc::new(AtomicUsize::new(0));
    let reduced = Arc::new(AtomicUsize::new(0));
    let swapped = Arc::new(AtomicUsize::new(0));
    let no_change = Arc::new(AtomicUsize::new(0));
    // Parallel compression of each block
    let compressed_blocks: Vec<CircuitSeq> = pre_blocks
        .into_par_iter()
        .enumerate()
        .map(|(_, block)| {
            // let mut rng = rand::rng();
            // let mut thread_conn = Connection::open("db/circuits.db").unwrap();
            let mut thread_conn =
                Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("Failed to open read-only connection");

            let t3 = Instant::now();
            let expanded = if config.no_ancilla_mode {
                block.clone()
            } else {
                expand_big(
                    &block,
                    config.compression_window_size,
                    n,
                    &mut thread_conn,
                    env,
                    &bit_shuf_list,
                    dbs,
                )
            };
            EXPAND_BIG_TIME.fetch_add(t3.elapsed().as_nanos() as u64, Ordering::Relaxed);
            let t4 = Instant::now();
            let before_len = expanded.gates.len();
            let compressed_block = if config.skip_compression {
                expanded.clone()
            } else if config.sat_mode {
                // compress_big_sat(&expanded, 100, n, 1000)
                compress_big_sat_lmdb(
                    &expanded,
                    config.compression_window_size_sat,
                    n,
                    config.compression_sat_limit as u64,
                    template_db,
                    config,
                )
            } else {
                compress_big(
                    &expanded,
                    config.compression_window_size,
                    n,
                    &mut thread_conn,
                    env,
                    &bit_shuf_list,
                    dbs,
                    config,
                )
            };
            COMPRESS_BIG_TIME.fetch_add(t4.elapsed().as_nanos() as u64, Ordering::Relaxed);
            let after_len = compressed_block.gates.len();

            if after_len < before_len {
                reduced.fetch_add(1, Ordering::Relaxed);
            } else if after_len > before_len {
                grew.fetch_add(1, Ordering::Relaxed);
            } else if block.gates != compressed_block.gates {
                swapped.fetch_add(1, Ordering::Relaxed);
            } else {
                no_change.fetch_add(1, Ordering::Relaxed);
            };
            compressed_block
        })
        .collect();

    println!("Summary:");
    println!("\x1b[32mGrew:      {}\x1b[0m", grew.load(Ordering::Relaxed));
    println!(
        "\x1b[31mReduced:   {}\x1b[0m",
        reduced.load(Ordering::Relaxed)
    );
    println!(
        "\x1b[34mSwapped:   {}\x1b[0m",
        swapped.load(Ordering::Relaxed)
    );
    println!(
        "\x1b[90mNo change: {}\x1b[0m",
        no_change.load(Ordering::Relaxed)
    );

    let progress = Arc::new(AtomicUsize::new(0));
    let _total = 2 * compressed_blocks.len() - 1;

    println!("Beginning merge");
    let t5 = Instant::now();
    let mut acc = merge_combine_blocks(
        &compressed_blocks,
        n,
        "./db/circuits.db",
        &progress,
        _total,
        env,
        &bit_shuf_list,
        dbs,
        config,
    );
    MERGE_COMBINE_BLOCKS_TIME.fetch_add(t5.elapsed().as_nanos() as u64, Ordering::Relaxed);

    // Add global bookends: first_r ... last_r_inv
    acc = first_r.concat(&acc).concat(&prev_r_inv);

    acc = CircuitSeq {
        gates: acc.gates.clone(),
    };
    println!("After adding bookends: {} gates", acc.gates.len());

    // let mut milestone = initial_milestone(acc.gates.len());
    // Final global compression until stable 12×
    let mut rng = rand::rng();
    let mut stable_count = 0;
    while stable_count < config.final_stability_threshold {
        // if acc.gates.len() <= milestone {
        //     let mut f = OpenOptions::new()
        //         .create(true)
        //         .append(true)d
        //         .open("circuitlist.txt")
        //         .expect("Could not open circuitlist.txt");

        //     writeln!(f, "{}", acc.repr()).unwrap();
        //     milestone = next_milestone(milestone);
        // }

        let before = acc.gates.len();

        let k = if before <= config.chunk_split_base {
            1
        } else {
            (before + config.chunk_split_base - 1) / config.chunk_split_base
        };

        let chunks = split_into_random_chunks(&acc.gates, k, &mut rng);

        let compressed_chunks: Vec<Vec<[u8; 3]>> = chunks
            .into_par_iter()
            .map(|chunk| {
                let sub = CircuitSeq { gates: chunk };
                let mut thread_conn =
                    Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .expect("Failed to open read-only connection");
                if config.sat_mode {
                    compress_big_sat(
                        &sub,
                        config.compression_window_size_sat,
                        n,
                        config.compression_sat_limit as u64,
                    )
                    .gates
                } else {
                    compress_big(
                        &sub,
                        config.compression_window_size,
                        n,
                        &mut thread_conn,
                        env,
                        &bit_shuf_list,
                        dbs,
                        config,
                    )
                    .gates
                }
            })
            .collect();

        let new_gates: Vec<[u8; 3]> = compressed_chunks.into_iter().flatten().collect();
        acc.gates = new_gates;
        if SHOULD_DUMP.load(Ordering::SeqCst) {
            {
                let mut guard = CURRENT_ACC.lock().unwrap();
                *guard = Some(acc.clone());
            }

            dump_and_exit();
        }
        let after = acc.gates.len();
        if last && acc.gates.len() <= stop {
            break;
        }
        if after == before {
            stable_count += 1;
            println!(
                "  {}/{} Final compression stable {}/{} at {} gates",
                curr_round, last_round, stable_count, config.final_stability_threshold, after
            );
        } else {
            println!(
                "  {}/{}: {} → {} gates",
                curr_round, last_round, before, after
            );
            stable_count = 0;
        }
    }

    println!("Compressed len: {}", acc.gates.len());
    println!("Butterfly done: {} gates", acc.gates.len());
    println!("Timers (minutes):");
    println!(
        "  shoot_random_gate:      {:.3}",
        SHOOT_RANDOM_GATE_TIME.load(Ordering::Relaxed) as f64 / 1e9 / 60.0
    );
    println!(
        "  replace_pairs:          {:.3}",
        REPLACE_PAIRS_TIME.load(Ordering::Relaxed) as f64 / 1e9 / 60.0
    );
    println!(
        "  random_id:              {:.3}",
        RANDOM_ID_TIME.load(Ordering::Relaxed) as f64 / 1e9 / 60.0
    );
    println!(
        "  compress_big:           {:.3}",
        COMPRESS_BIG_TIME.load(Ordering::Relaxed) as f64 / 1e9 / 60.0
    );
    println!(
        "  merge_combine_blocks:   {:.3}",
        MERGE_COMBINE_BLOCKS_TIME.load(Ordering::Relaxed) as f64 / 1e9 / 60.0
    );

    crate::algorithms::butterfly::replace::print_compress_timers();
    acc
}

pub fn abutterfly_big_delay_bookends(
    c: &CircuitSeq,
    _conn: &mut Connection,
    n: usize,
    env: &lmdb::Environment,
    config: &ObfuscationConfig,
    template_db: Option<&crate::infra::store::reader::TemplateDB>,
) -> (CircuitSeq, CircuitSeq, CircuitSeq) {
    let dbs = open_all_dbs(env);
    let bit_shuf_list = (3..=7)
        .map(|k| (0..k).permutations(k).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    println!("Butterfly start: {} gates", c.gates.len());
    let mut rng = rand::rng();
    let mut c = c.clone();
    replace_pairs(&mut c, n, _conn, &env, template_db);
    if config.single_gate_mode {
        random_gate_replacements(&mut c, config.single_gate_replacements, n, _conn, env);
    }
    let mut pre_blocks: Vec<CircuitSeq> = Vec::with_capacity(c.gates.len());
    // let (first_r, first_r_inv) = random_id(n as u8, rng.random_range(20..=100));
    let (first_r, first_r_inv) = random_id(
        n as u8,
        rng.random_range(config.structure_block_size_min..=config.structure_block_size_max),
    );
    let mut prev_r_inv = first_r_inv.clone();
    shoot_random_gate(&mut c, config.shooting_count);
    for &g in &c.gates {
        // let (r, r_inv) = random_id(n as u8, rng.random_range(20..=100));
        let (r, r_inv) = random_id(
            n as u8,
            rng.random_range(config.structure_block_size_min..=config.structure_block_size_max),
        );
        let mut block = prev_r_inv
            .clone()
            .concat(&CircuitSeq { gates: vec![g] })
            .concat(&r);
        shoot_random_gate(&mut block, config.shooting_count_inner);
        pre_blocks.push(block);
        prev_r_inv = r_inv;
    }

    // Parallel compression of each block
    let compressed_blocks: Vec<CircuitSeq> = pre_blocks
        .into_par_iter()
        .enumerate()
        .map(|(i, block)| {
            let mut thread_conn =
                Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("Failed to open read-only connection");

            let before_len = block.gates.len();
            let compressed_block = if config.sat_mode {
                compress_big_sat(
                    &block,
                    config.compression_window_size_sat,
                    n,
                    config.compression_sat_limit as u64,
                )
            } else {
                let expanded = if config.no_ancilla_mode {
                    block.clone()
                } else {
                    expand_big(
                        &block,
                        config.compression_window_size,
                        n,
                        &mut thread_conn,
                        env,
                        &bit_shuf_list,
                        &dbs,
                    )
                };
                compress_big(
                    &expanded,
                    config.compression_window_size,
                    n,
                    &mut thread_conn,
                    env,
                    &bit_shuf_list,
                    &dbs,
                    config,
                )
            };
            let after_len = compressed_block.gates.len();

            let color_line = if after_len < before_len {
                "\x1b[32m──────────────\x1b[0m" // green
            } else if after_len > before_len {
                "\x1b[31m──────────────\x1b[0m" // red
            } else if block.gates != compressed_block.gates {
                "\x1b[35m──────────────\x1b[0m" // purple
            } else {
                "\x1b[90m──────────────\x1b[0m" // gray
            };

            println!(
                "  Block {}: before {} gates → after {} gates  {}",
                i, before_len, after_len, color_line
            );
            //println!("  {}", compressed_block.repr());

            compressed_block
        })
        .collect();

    let progress = Arc::new(AtomicUsize::new(0));
    let _total = 2 * compressed_blocks.len() - 1;

    println!("Beginning merge");
    let mut acc = merge_combine_blocks(
        &compressed_blocks,
        n,
        "./db/circuits.db",
        &progress,
        _total,
        env,
        &bit_shuf_list,
        &dbs,
        config,
    );

    println!("After merging: {} gates", acc.gates.len());

    // Final global compression until stable 3×
    let mut stable_count = 0;
    while stable_count < 3 {
        let before = acc.gates.len();

        let k = if before > 10_000 {
            16
        } else if before > 5_000 {
            8
        } else if before > 1_000 {
            4
        } else if before > 500 {
            2
        } else {
            1
        };

        let mut rng = rand::rng();

        let chunks = split_into_random_chunks(&acc.gates, k, &mut rng);

        let compressed_chunks: Vec<Vec<[u8; 3]>> = chunks
            .into_par_iter()
            .map(|chunk| {
                let sub = CircuitSeq { gates: chunk };
                let mut thread_conn =
                    Connection::open_with_flags("db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .expect("Failed to open read-only connection");
                if config.sat_mode {
                    // compress_big_sat(&sub, 1_000, n, 1000).gates
                    compress_big_sat_lmdb(
                        &sub,
                        config.compression_window_size_sat,
                        n,
                        config.compression_sat_limit as u64,
                        template_db,
                        config,
                    )
                    .gates
                } else {
                    compress_big(
                        &sub,
                        config.compression_window_size,
                        n,
                        &mut thread_conn,
                        env,
                        &bit_shuf_list,
                        &dbs,
                        config,
                    )
                    .gates
                }
            })
            .collect();

        let new_gates: Vec<[u8; 3]> = compressed_chunks.into_iter().flatten().collect();
        acc.gates = new_gates;

        let after = acc.gates.len();

        if after == before {
            stable_count += 1;
            println!(
                "  Final compression stable {}/3 at {} gates",
                stable_count, after
            );
        } else {
            println!("  Final compression reduced: {} → {} gates", before, after);
            stable_count = 0;
        }
    }

    println!("Compressed len: {}", acc.gates.len());
    println!("Butterfly done: {} gates", acc.gates.len());

    (acc, first_r, prev_r_inv)
}

pub fn split_into_random_chunks<T: Clone>(
    v: &[T],
    k: usize,
    rng: &mut impl rand::Rng,
) -> Vec<Vec<T>> {
    let n = v.len();
    if k <= 1 || n <= 1 {
        return vec![v.to_vec()];
    }

    // Minimum chunk size
    let min_size = 100;

    let max_chunks = k;
    let mut cuts = Vec::with_capacity(max_chunks - 1);
    let mut start = 0;

    // Generate cuts ensuring each chunk >= min_size
    for _ in 0..(max_chunks - 1) {
        // Remaining length that can still accommodate remaining cuts
        let remaining_chunks = max_chunks - cuts.len();
        let max_cut = n - (remaining_chunks * min_size);
        if start >= max_cut {
            break;
        }
        let cut = rng.random_range(start + min_size..=max_cut);
        cuts.push(cut);
        start = cut;
    }

    let mut boundaries = vec![0];
    boundaries.extend(cuts);
    boundaries.push(n);

    boundaries
        .windows(2)
        .map(|w| v[w[0]..w[1]].to_vec())
        .collect()
}

pub fn open_all_dbs(env: &lmdb::Environment) -> HashMap<String, lmdb::Database> {
    let mut dbs = HashMap::new();
    let db_names = [
        "n3m1",
        "n3m2",
        "n3m3",
        "n3m4",
        "n3m5",
        "n3m6",
        "n3m7",
        "n3m8",
        "n3m9",
        "n3m10",
        "n4m1",
        "n4m2",
        "n4m3",
        "n4m4",
        "n4m5",
        "n4m6",
        "n5m1",
        "n5m2",
        "n5m3",
        "n5m4",
        "n5m5",
        "n6m1",
        "n6m2",
        "n6m3",
        "n6m4",
        "n6m5",
        "n7m1",
        "n7m2",
        "n7m3",
        "n7m4",
        "perm_tables_n3",
        "perm_tables_n4",
        "perm_tables_n5",
        "perm_tables_n6",
        "perm_tables_n7",
        "n4m1perms",
        "n4m2perms",
        "n4m3perms",
        "n4m4perms",
        "n4m5perms",
        "n4m6perms",
        "n5m1perms",
        "n5m2perms",
        "n5m3perms",
        "n5m4perms",
        "n5m5perms",
        "n6m1perms",
        "n6m2perms",
        "n6m3perms",
        "n6m4perms",
        "n7m1perms",
        "n7m2perms",
        "n7m3perms",
    ];

    for name in db_names.iter() {
        match env.open_db(Some(name)) {
            Ok(db) => {
                dbs.insert(name.to_string(), db);
            }
            Err(lmdb::Error::NotFound) => continue,
            Err(e) => panic!("Failed to open LMDB database {}: {:?}", name, e),
        }
    }

    dbs
}

pub fn main_mix(
    c: &CircuitSeq,
    rounds: usize,
    conn: &mut Connection,
    n: usize,
    config: &ObfuscationConfig,
) {
    // Start with the input circuit
    println!("Starting len: {}", c.gates.len());
    let mut circuit = c.clone();
    let perms: Vec<Vec<usize>> = (0..n).permutations(n).collect();
    let bit_shuf = perms.into_iter().skip(1).collect::<Vec<_>>();
    // Repeat obfuscate + compress 'rounds' times
    let mut post_len = 0;
    let mut count = 0;
    for _ in 0..rounds {
        circuit = obfuscate_and_target_compress(&circuit, conn, &bit_shuf, n, config);
        if circuit.gates.len() == 0 {
            break;
        }

        if circuit.gates.len() == post_len {
            count += 1;
        } else {
            post_len = circuit.gates.len();
            count = 0;
        }

        if count > 2 {
            break;
        }
    }
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            // remove elements at i and i+1
            circuit.gates.drain(i..=i + 1);

            // step back up to 2 indices, but not below 0
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    // Convert the final circuit to string
    let circuit_str = circuit.to_string(n);
    println!("{:?}", circuit.permutation(n).data);
    // Write to file
    let mut file = File::create("recent_circuit.txt").expect("Failed to create file");
    file.write_all(circuit_str.as_bytes())
        .expect("Failed to write circuit to file");

    let circuit_str = circuit.repr(); // or however you stringify your circuit

    if !circuit.gates.is_empty() {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true) // append instead of overwriting
            .open("good_id.txt")
            .expect("Failed to open good_id.txt");

        let line = format!("{} : {}\n", circuit.gates.len(), circuit_str);

        file.write_all(line.as_bytes())
            .expect("Failed to write circuit to good_ids.txt");

        println!("Wrote good circuit to good_id.txt");
    }

    if circuit.gates == c.gates {
        println!("The obfuscation didn't do anything");
    }

    println!("Final circuit written to recent_circuit.txt");
}

pub fn main_butterfly(
    c: &CircuitSeq,
    rounds: usize,
    conn: &mut Connection,
    n: usize,
    config: &ObfuscationConfig,
) {
    // Start with the input circuit
    println!("Starting len: {}", c.gates.len());
    let mut circuit = c.clone();
    let perms: Vec<Vec<usize>> = (0..n).permutations(n).collect();
    let bit_shuf = perms.into_iter().skip(1).collect::<Vec<_>>();
    // Repeat obfuscate + compress 'rounds' times
    let mut post_len = 0;
    let mut count = 0;
    for _ in 0..rounds {
        circuit = butterfly(&circuit, conn, &bit_shuf, n, config);
        if circuit.gates.len() == 0 {
            break;
        }

        if circuit.gates.len() == post_len {
            count += 1;
        } else {
            post_len = circuit.gates.len();
            count = 0;
        }

        if count > 2 {
            break;
        }
        let mut i = 0;
        while i < circuit.gates.len().saturating_sub(1) {
            if circuit.gates[i] == circuit.gates[i + 1] {
                // remove elements at i and i+1
                circuit.gates.drain(i..=i + 1);

                // step back up to 2 indices, but not below 0
                i = i.saturating_sub(2);
            } else {
                i += 1;
            }
        }
    }
    println!("Final len: {}", circuit.gates.len());
    println!("Final cycle: {:?}", circuit.permutation(n).to_cycle());
    // Convert the final circuit to string
    let circuit_str = circuit.to_string(n);
    println!("Final Permutation: {:?}", circuit.permutation(n).data);
    if circuit.permutation(n).data != c.permutation(n).data {
        panic!(
            "The permutation differs from the original.\nOriginal: {:?}\nNew: {:?}",
            c.permutation(n).data,
            circuit.permutation(n).data
        );
    }
    // Write to file
    let mut file = File::create("recent_circuit.txt").expect("Failed to create file");
    file.write_all(circuit_str.as_bytes())
        .expect("Failed to write circuit to file");

    let circuit_str = circuit.repr(); // or however you stringify your circuit

    if !circuit.gates.is_empty() {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true) // append instead of overwriting
            .open("good_id.txt")
            .expect("Failed to open good_id.txt");

        let line = format!("{} : {}\n", circuit.gates.len(), circuit_str);

        file.write_all(line.as_bytes())
            .expect("Failed to write circuit to good_ids.txt");

        println!("Wrote good circuit to good_id.txt");
    }

    if circuit.gates == c.gates {
        println!("The obfuscation didn't do anything");
    }

    println!("Final circuit written to recent_circuit.txt");
}

pub fn main_butterfly_big(
    c: &CircuitSeq,
    // rounds: usize, // Now in config
    conn: &mut Connection,
    n: usize,
    asymmetric: bool,
    save: &str,
    env: &lmdb::Environment,
    config: &ObfuscationConfig,
    template_db: Option<&crate::infra::store::reader::TemplateDB>,
) {
    let dbs = open_all_dbs(env);

    let bit_shuf_list = (3..=7)
        .map(|k| (0..k).permutations(k).collect::<Vec<_>>())
        .collect::<Vec<_>>();

    let mut current_circuit = c.clone();
    let mut post_len = 0;
    let mut count = 0;
    for i in 0..config.rounds {
        println!("Round {}/{}", i + 1, config.rounds);
        current_circuit = abutterfly_big(
            &current_circuit,
            conn,
            n,
            i == config.rounds - 1,
            2 * current_circuit.gates.len(),
            env,
            i + 1,
            config.rounds,
            &bit_shuf_list,
            &dbs,
            config,
            template_db,
        );
        if current_circuit.gates.len() == 0 {
            break;
        }

        if current_circuit.gates.len() == post_len {
            count += 1;
        } else {
            post_len = current_circuit.gates.len();
            count = 0;
        }

        if count > 2 {
            break;
        }
        let mut i: usize = 0;
        while i < current_circuit.gates.len().saturating_sub(1) {
            if current_circuit.gates[i] == current_circuit.gates[i + 1] {
                // remove elements at i and i+1
                current_circuit.gates.drain(i..=i + 1);

                // step back up to 2 indices, but not below 0
                i = i.saturating_sub(2);
            } else {
                i += 1;
            }
        }
    }
    println!("Final len: {}", current_circuit.gates.len());
    if let Err(e) = current_circuit.probably_equal(&c, n, 150_000) {
        println!("Warning: The circuits differ somewhere! Error: {:?}", e);
    }

    // Write to file
    let c_str = c.repr();
    let circuit_str = current_circuit.repr();
    let long_str = format!("{}:{}", c.repr(), current_circuit.repr());
    // let good_str = format!("{}: {}", good_id.gates.len(), good_id.repr());
    // Write start.txt
    File::create("start.txt")
        .and_then(|mut f| f.write_all(c_str.as_bytes()))
        .expect("Failed to write start.txt");

    // Write recent_circuit.txt
    File::create("recent_circuit.txt")
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    File::create(save)
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    // Write butterfly_recent.txt (overwrite)
    File::create("butterfly_recent.txt")
        .and_then(|mut f| f.write_all(long_str.as_bytes()))
        .expect("Failed to write butterfly_recent.txt");

    // Append to butterfly.txt
    OpenOptions::new()
        .append(true)
        .create(true)
        .open("butterfly.txt")
        .and_then(|mut f| writeln!(f, "{}", long_str))
        .expect("Failed to append to butterfly.txt");
    if current_circuit.gates == c.gates {
        println!("The obfuscation didn't do anything");
    }

    println!("Final circuit written to recent_circuit.txt");
}

pub fn main_butterfly_big_bookendsless(
    c: &CircuitSeq,
    // rounds: usize,
    conn: &mut Connection,
    n: usize,
    _asymmetric: bool,
    save: &str,
    env: &lmdb::Environment,
    config: &ObfuscationConfig,
    template_db: Option<&crate::infra::store::reader::TemplateDB>,
) {
    let dbs = open_all_dbs(env);

    let bit_shuf_list = (3..=7)
        .map(|k| (0..k).permutations(k).collect::<Vec<_>>())
        .collect::<Vec<_>>();

    println!("Starting len: {}", c.gates.len());
    let mut circuit = c.clone();
    // Repeat obfuscate + compress 'rounds' times
    let mut post_len = 0;
    let mut count = 0;
    let mut beginning = CircuitSeq { gates: Vec::new() };
    let mut end = CircuitSeq { gates: Vec::new() };

    // let mut current_circuit = c.clone();
    for i in 0..config.rounds {
        println!("Round {}/{}", i + 1, config.rounds);
        let (new_circuit, b, e) =
            abutterfly_big_delay_bookends(&circuit, conn, n, env, config, template_db);
        // beginning = beginning.concat(&b);
        // end = e.concat(&end);
        circuit = new_circuit;

        if circuit.gates.len() == post_len {
            count += 1;
        } else {
            post_len = circuit.gates.len();
            count = 0;
        }

        if count > 2 {
            break;
        }
        let mut i: usize = 0;
        while i < circuit.gates.len().saturating_sub(1) {
            if circuit.gates[i] == circuit.gates[i + 1] {
                // remove elements at i and i+1
                circuit.gates.drain(i..=i + 1);

                // step back up to 2 indices, but not below 0
                i = i.saturating_sub(2);
            } else {
                i += 1;
            }
        }
    }

    // println!("Adding bookends");
    // beginning = compress_big(&beginning, 100, n, conn, env, &bit_shuf_list, &dbs, config);
    // end = compress_big(&end, 100, n, conn, env, &bit_shuf_list, &dbs, config);
    // circuit = beginning.concat(&circuit).concat(&end);
    if !config.skip_compression {
        let mut c1 = CircuitSeq {
            gates: circuit.gates[0..circuit.gates.len() / 2].to_vec(),
        };
        let mut c2 = CircuitSeq {
            gates: circuit.gates[circuit.gates.len() / 2..].to_vec(),
        };
        c1 = compress_big(&c1, 1_000, n, conn, env, &bit_shuf_list, &dbs, config);
        c2 = compress_big(&c2, 1_000, n, conn, env, &bit_shuf_list, &dbs, config);
        circuit = c1.concat(&c2);
        let mut stable_count = 0;
        while stable_count < 3 {
            let before = circuit.gates.len();
            //shoot_random_gate(&mut acc, 100_000);
            circuit = compress_big(&circuit, 1_000, n, conn, env, &bit_shuf_list, &dbs, config);
            let after = circuit.gates.len();

            if after == before {
                stable_count += 1;
                println!(
                    "  Final compression stable {}/3 at {} gates",
                    stable_count, after
                );
            } else {
                println!("  Final compression reduced: {} → {} gates", before, after);
                stable_count = 0;
            }
        }
    }

    println!("Final len: {}", circuit.gates.len());

    if let Err(e) = circuit.probably_equal(&c, n, 150_000) {
        println!("Warning: The circuits differ somewhere! Error: {:?}", e);
    }

    // Write to file
    let c_str = c.repr();
    let circuit_str = circuit.repr();
    let long_str = format!("{}:{}", c.repr(), circuit.repr());
    // let good_str = format!("{}: {}", good_id.gates.len(), good_id.repr());
    // Write start.txt
    File::create("start.txt")
        .and_then(|mut f| f.write_all(c_str.as_bytes()))
        .expect("Failed to write start.txt");

    // Write recent_circuit.txt
    File::create("recent_circuit.txt")
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    File::create(save)
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    // Write butterfly_recent.txt (overwrite)
    File::create("butterfly_recent.txt")
        .and_then(|mut f| f.write_all(long_str.as_bytes()))
        .expect("Failed to write butterfly_recent.txt");

    // Append to butterfly.txt
    OpenOptions::new()
        .append(true)
        .create(true)
        .open("butterfly.txt")
        .and_then(|mut f| writeln!(f, "{}", long_str))
        .expect("Failed to append to butterfly.txt");
    if circuit.gates == c.gates {
        println!("The obfuscation didn't do anything");
    }

    println!("Final circuit written to recent_circuit.txt");
}

//do targeted compression
pub fn main_compression(
    c: &CircuitSeq,
    conn: &mut Connection,
    n: usize,
    save: &str,
    env: &lmdb::Environment,
    config: &ObfuscationConfig,
) {
    let dbs = open_all_dbs(env);
    // Start with the input circuit
    let bit_shuf_list = (3..=7)
        .map(|k| (0..k).permutations(k).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    println!("Starting len: {}", c.gates.len());
    let mut circuit = c.clone();
    // Repeat obfuscate + compress 'rounds' times
    let mut post_len = 0;
    let mut count = 0;
    for _ in 0..config.rounds {
        butterfly_big(
            &circuit,
            conn,
            n,
            false,
            0,
            env,
            &bit_shuf_list,
            &dbs,
            config,
        );
        if circuit.gates.len() == 0 {
            break;
        }

        if circuit.gates.len() == post_len {
            count += 1;
        } else {
            post_len = circuit.gates.len();
            count = 0;
        }

        if count > 2 {
            break;
        }
        let mut i = 0;
        while i < circuit.gates.len().saturating_sub(1) {
            if circuit.gates[i] == circuit.gates[i + 1] {
                // remove elements at i and i+1
                circuit.gates.drain(i..=i + 1);

                // step back up to 2 indices, but not below 0
                i = i.saturating_sub(2);
            } else {
                i += 1;
            }
        }
    }
    println!("Final len: {}", circuit.gates.len());

    circuit
        .probably_equal(&c, n, 150_000)
        .expect("The circuits differ somewhere!");

    // Write to file
    let c_str = c.repr();
    let circuit_str = circuit.repr();
    let long_str = format!("{}:{}", c.repr(), circuit.repr());
    // Write start.txt
    File::create("start.txt")
        .and_then(|mut f| f.write_all(c_str.as_bytes()))
        .expect("Failed to write start.txt");

    // Write recent_circuit.txt
    File::create("recent_circuit.txt")
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    File::create(save)
        .and_then(|mut f| f.write_all(circuit_str.as_bytes()))
        .expect("Failed to write recent_circuit.txt");

    // Write butterfly_recent.txt (overwrite)
    File::create("butterfly_recent.txt")
        .and_then(|mut f| f.write_all(long_str.as_bytes()))
        .expect("Failed to write butterfly_recent.txt");

    // Append to butterfly.txt
    OpenOptions::new()
        .append(true)
        .create(true)
        .open("butterfly.txt")
        .and_then(|mut f| writeln!(f, "{}", long_str))
        .expect("Failed to append to butterfly.txt");
    if circuit.gates == c.gates {
        println!("The obfuscation didn't do anything");
    }

    println!("Final circuit written to recent_circuit.txt");
}
