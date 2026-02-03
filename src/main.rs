use clap::{Arg, ArgAction, Command};
use itertools::Itertools;
use plotters::prelude::*;

use rusqlite::{Connection, OpenFlags};

use local_mixing::algorithms::butterfly::mixing::open_all_dbs;
use local_mixing::algorithms::butterfly::replace::compress_big_ancillas;
use local_mixing::analysis::alignment::{distance_matrix, dtw_alignment, trace_states};
use local_mixing::{
    algorithms::annealing::local::{MixConfig, ReduceConfig, mix_circuit, reduce_circuit},
    algorithms::butterfly::{
        mixing::{
            install_kill_handler, main_butterfly, main_butterfly_big,
            main_butterfly_big_bookendsless, main_mix, main_rac_big,
        },
        replace::random_canonical_id,
    },
    algorithms::identity_growth::{IdentityGrowthConfig, TemplateSource, grow_identity},
    config::ObfuscationConfig,
    infra::circuit::{CircuitSeq, Gate},
    infra::ids_index::build_ids_indexes,
    infra::random::random_data::{
        build_from_sql, build_initial_table, generate_heatmap_data, main_random, random_circuit,
    },
};
use rand::Rng;

use std::{
    fs::{self},
    io::Write,
    path::Path,
};
fn main() {
    let matches = Command::new("rainbow")
        .about("Rainbow circuit generator")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("new")
                .about("Build a new database")
                .arg(Arg::new("n").short('n').long("n").required(true).value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("m").short('m').long("m").required(true).value_parser(clap::value_parser!(usize))),
        )
        .subcommand(
            Command::new("load")
                .about("Load an existing database")
                .arg(Arg::new("n").short('n').long("n").required(true).value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("m").short('m').long("m").required(true).value_parser(clap::value_parser!(usize))),
        )
        .subcommand(
            Command::new("init")
                .about("Initialize a new database")
                .arg(Arg::new("n").short('n').long("n").required(true).value_parser(clap::value_parser!(usize))),
        )
        .subcommand(
            Command::new("explore")
                .about("Explore an existing database")
                .arg(Arg::new("n").short('n').long("n").required(true).value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("m").short('m').long("m").required(true).value_parser(clap::value_parser!(usize))),
        )
        .subcommand(
            Command::new("random")
                .about("Generate random circuits and store in DB")
                .arg(Arg::new("n").short('n').long("n").required(true).value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("m").short('m').long("m").required(true).value_parser(clap::value_parser!(usize)))
                .arg(
                    Arg::new("count")
                        .short('c')
                        .long("count")
                        .value_parser(clap::value_parser!(usize))
                        .conflicts_with("sliding"),
                )
                .arg(
                    Arg::new("sliding")
                        .short('C')
                        .long("sliding")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("count"),
                ),
        )
        .subcommand(
            Command::new("mix")
                .about("Obfuscate and compress an existing circuit")
                .arg(
                    Arg::new("rounds")
                        .short('r')
                        .long("rounds")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                ),
        )
        .subcommand(
            Command::new("butterfly")
                .about("Obfuscate and compress an existing circuit via butterfly method")
                .arg(
                    Arg::new("rounds")
                        .short('r')
                        .long("rounds")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                ),
        )
        .subcommand(
        Command::new("bbutterfly")
            .about("Obfuscate and compress an existing circuit via butterfly_big method")
            .arg(
                Arg::new("rounds")
                    .short('r')
                    .long("rounds")
                    .required(true)
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                Arg::new("path")
                    .short('p')
                    .long("path")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .help("Path to the circuit file"),
            )
            .arg(
                Arg::new("n")
                    .short('n')
                    .long("n")
                    .required(false)
                    .default_value("32")
                    .value_parser(clap::value_parser!(usize))
                    .help("Number of wires (default: 32)"),
            )
            .arg(
                Arg::new("shooting")
                    .long("shooting")
                    .default_value("500000")
                    .value_parser(clap::value_parser!(usize))
                    .help("Shooting intensity"),
            )
            .arg(
                Arg::new("single-gate")
                    .long("single-gate")
                    .action(ArgAction::SetTrue)
                    .help("Enable single gate replacements"),
            )
            .arg(
                Arg::new("no-ancilla")
                    .long("no-ancilla")
                    .action(ArgAction::SetTrue)
                    .help("Disable ancilla expansion"),
            )
            .arg(
                Arg::new("config")
                    .long("config")
                    .value_parser(clap::value_parser!(String))
                    .help("Path to configuration JSON file"),
            ),
    )
    .subcommand(
        Command::new("rac")
            .about("Obfuscate via RAC (Replace And Compress) method")
            .arg(
                Arg::new("rounds")
                    .short('r')
                    .long("rounds")
                    .required(true)
                    .value_parser(clap::value_parser!(usize))
                    .help("Number of RAC rounds"),
            )
            .arg(
                Arg::new("path")
                    .short('p')
                    .long("path")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .help("Path to the circuit file"),
            )
            .arg(
                Arg::new("n")
                    .short('n')
                    .long("n")
                    .default_value("32")
                    .value_parser(clap::value_parser!(usize))
                    .help("Number of wires (default: 32)"),
            )
            .arg(
                Arg::new("save")
                    .short('s')
                    .long("save")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .help("Output file path"),
            )
            .arg(
                Arg::new("intermediate")
                    .short('i')
                    .long("intermediate")
                    .default_value("./progress/rac_intermediate.txt")
                    .value_parser(clap::value_parser!(String))
                    .help("Intermediate progress file path"),
            ),
    )
    .subcommand(
        Command::new("ids-index")
            .about("Build reverse index + witness prefilter for ids_n* LMDBs")
            .arg(
                Arg::new("db")
                    .long("db")
                    .default_value("./db")
                    .value_parser(clap::value_parser!(String))
                    .help("LMDB directory containing ids_n* databases"),
            )
            .arg(
                Arg::new("min-n")
                    .long("min-n")
                    .default_value("3")
                    .value_parser(clap::value_parser!(usize))
                    .help("Minimum ids_n* width to include"),
            )
            .arg(
                Arg::new("max-n")
                    .long("max-n")
                    .default_value("8")
                    .value_parser(clap::value_parser!(usize))
                    .help("Maximum ids_n* width to include"),
            ),
    )
    .subcommand(
        Command::new("abbutterfly")
            .about("Obfuscate and compress an existing circuit via asymmetric butterfly_big method")
            .arg(
                Arg::new("rounds")
                    .short('r')
                    .long("rounds")
                    .required(false)  // Optional when using --config
                    .value_parser(clap::value_parser!(usize))
                    .help("Number of rounds (uses config value if not specified)"),
            )
            .arg(
                Arg::new("path")
                    .short('p')
                    .long("path")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .help("Path to the circuit file"),
            )
            .arg(
                Arg::new("n")
                    .short('n')
                    .long("n")
                    .required(false)
                    .default_value("32")
                    .value_parser(clap::value_parser!(usize))
                    .help("Number of wires (default: 32)"),
            )
            .arg(
                Arg::new("bookendless")
                    .short('b')
                    .long("bookendless")
                    .help("Enable bookendless mode")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("sat")
                    .short('S')
                    .long("sat")
                    .help("Enable SAT mode")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("shooting")
                    .long("shooting")
                    .value_parser(clap::value_parser!(usize))
                    .help("Shooting intensity (uses config value if not specified)"),
            )
            .arg(
                Arg::new("single-gate")
                    .long("single-gate")
                    .action(ArgAction::SetTrue)
                    .help("Enable single gate replacements"),
            )
            .arg(
                Arg::new("no-ancilla")
                    .long("no-ancilla")
                    .action(ArgAction::SetTrue)
                    .help("Disable ancilla expansion"),
            )
            .arg(
                Arg::new("lmdb-db")
                    .long("lmdb-db")
                    .value_parser(clap::value_parser!(String))
                    .help("Path to LMDB database for templates (enables fast lookup)"),
            )
            .arg(
                Arg::new("no-pairs")
                    .long("no-pairs")
                    .action(ArgAction::SetTrue)
                    .help("Disable pair replacements"),
            )
            .arg(
                Arg::new("no-equal")
                    .long("no-equal")
                    .action(ArgAction::SetTrue)
                    .help("Disable equal-length replacements (blurring)"),
            )
            .arg(
                Arg::new("config")
                    .long("config")
                    .value_parser(clap::value_parser!(String))
                    .help("Path to configuration JSON file"),
            ),
    )
        .subcommand(
            Command::new("gen")
                .about("Generate random circuit")
                .arg(
                    Arg::new("wires")
                        .short('w')
                        .long("wires")
                        .value_parser(clap::value_parser!(u8))
                        .help("Number of wires"),
                )
                .arg(
                    Arg::new("length")
                        .short('l')
                        .long("length")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of gates"),
                )
        )
        .subcommand(
            Command::new("heatmap")
                .about("Run the circuit distinguisher and produce a heatmap")
                .arg(
                    Arg::new("inputs")
                        .short('i')
                        .long("inputs")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of random inputs to test"),
                )
                .arg(
                    Arg::new("num_wires")
                        .short('n')
                        .long("num_wires")
                        .required(true)
                        .value_parser(clap::value_parser!(usize)),
                )
                .arg(
                    Arg::new("xlabel")
                        .short('x')
                        .long("xlabel")
                        .value_parser(clap::value_parser!(String))
                        .help("Label for X axis"),
                )
                .arg(
                    Arg::new("ylabel")
                        .short('y')
                        .long("ylabel")
                        .value_parser(clap::value_parser!(String))
                        .help("Label for Y axis"),
                )
                .arg(
                    Arg::new("std")
                        .short('s')
                        .help("Use standard deviation (if given) or raw otherwise")
                        .action(ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("c1")
                        .long("c1")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to first circuit"),
                )
                .arg(
                    Arg::new("c2")
                        .long("c2")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to second circuit"),
                )
                .arg(
                    Arg::new("raw")
                        .long("raw")
                        .help("Do not canonicalize circuits before processing (default: canonicalize)")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("align")
                .about("Run DTW alignment between two circuits")
                .arg(
                    Arg::new("c1")
                        .long("c1")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to first circuit"),
                )
                .arg(
                    Arg::new("c2")
                        .long("c2")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to second circuit"),
                )
                .arg(
                    Arg::new("n")
                        .short('n')
                        .long("num_wires")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires"),
                )
                .arg(
                    Arg::new("inputs")
                        .short('i')
                        .long("inputs")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of random inputs to test"),
                )
                .arg(
                    Arg::new("raw")
                        .long("raw")
                        .help("Do not canonicalize circuits before processing (default: canonicalize)")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("compare")
                .about("Compare two circuits for functional equivalence")
                .arg(
                    Arg::new("c1")
                        .long("c1")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to first circuit"),
                )
                .arg(
                    Arg::new("c2")
                        .long("c2")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to second circuit"),
                )
                .arg(
                    Arg::new("n")
                        .short('n')
                        .long("num_wires")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires"),
                )
                .arg(
                    Arg::new("inputs")
                        .short('i')
                        .long("inputs")
                        .default_value("1000")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of inputs to test"),
                ),
        )
        .subcommand(
            Command::new("reverse")
                .about("Reverse the order of gates in a circuit file")
                .arg(
                    Arg::new("source")
                        .short('s')
                        .long("source")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to the source circuit file"),
                )
                .arg(
                    Arg::new("dest")
                        .short('d')
                        .long("dest")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to write the reversed circuit file"),
                ),
        )
        .subcommand(
            Command::new("binload")
                .about("Load a binary circuit file")
                .arg(
                    Arg::new("n")
                        .short('n')
                        .long("n")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires in the circuit"),
                )
                .arg(
                    Arg::new("m")
                        .short('m')
                        .long("m")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of gates in the circuit"),
                )
        )
        .subcommand(
            Command::new("compress")
                .about("Run compression trials on a circuit file")
                .arg(
                    Arg::new("p")
                        .short('p')
                        .long("path")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to the starting circuit file"),
                )
                .arg(
                    Arg::new("n")
                        .short('n')
                        .long("wires")
                        .required(true)
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires in the circuit"),
                )
                .arg(
                    Arg::new("window")
                        .long("window")
                        .short('w')
                        .value_parser(clap::value_parser!(usize))
                        .default_value("50")
                        .help("Compression window size (default: 50)"),
                )
                .arg(
                    Arg::new("db")
                        .long("db")
                        .default_value("./db-old")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to database directory"),
                )
                .arg(
                    Arg::new("passes")
                        .long("passes")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("100")
                        .help("Max reducer passes"),
                ),
        )
        .subcommand(
            Command::new("wiredot")
                .about("Run the circuit counter and produce a dotplot")
                .arg(
                    Arg::new("num_wires")
                        .short('n')
                        .long("num_wires")
                        .required(true)
                        .value_parser(clap::value_parser!(usize)),
                )
                .arg(
                    Arg::new("xlabel")
                        .short('x')
                        .long("xlabel")
                        .value_parser(clap::value_parser!(String))
                        .help("Label for X axis"),
                )
                .arg(
                    Arg::new("path")
                        .short('p')
                        .long("path")
                        .value_parser(clap::value_parser!(String))
                        .help("Circuit to analyze path"),
                ),
        )
        .subcommand(
            Command::new("lmdb")
                .about("Run reducer on a circuit file")
                .arg(
                    Arg::new("p")
                        .short('p')
                        .long("path")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to the circuit file"),
                )
                .arg(
                    Arg::new("db")
                        .long("db")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to database directory"),
                )
                .arg(
                    Arg::new("passes")
                        .long("passes")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("100")
                        .help("Max reducer passes"),
                ),
        )
        .subcommand(
            Command::new("lmdbcounts")
            .about("Generate table for generating canon ids")
        )
        .subcommand(
            Command::new("string")
                .about("Reverse the order of gates in a circuit file")
                .arg(
                    Arg::new("source")
                        .short('s')
                        .long("source")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to the source circuit file"),
                )
                .arg(
                    Arg::new("dest")
                        .short('d')
                        .long("dest")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to write the reversed circuit file"),
                ),
        )
        .subcommand(
            Command::new("obfuscate")
                .about("Apply reducer-resistant obfuscation to a circuit")
                .arg(
                    Arg::new("input")
                        .short('i')
                        .long("input")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to input circuit file"),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .required(true)
                        .value_parser(clap::value_parser!(String))
                        .help("Path to output obfuscated circuit"),
                )
                .arg(
                    Arg::new("level")
                        .short('l')
                        .long("level")
                        .default_value("3")
                        .value_parser(clap::value_parser!(u8))
                        .help("Obfuscation level 1-5 (default: 3)"),
                )
                .arg(
                    Arg::new("seed")
                        .short('s')
                        .long("seed")
                        .default_value("0")
                        .value_parser(clap::value_parser!(u64))
                        .help("Random seed (0 = random)"),
                )
                .arg(
                    Arg::new("wires")
                        .short('n')
                        .long("wires")
                        .default_value("64")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires"),
                )
                .arg(
                    Arg::new("report")
                        .short('r')
                        .long("report")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to write JSON report"),
                ),
        )
        .subcommand(
            Command::new("local-mix")
                .about("Generate and obfuscate a 64-wire identity using local mixing")
                .arg(
                    Arg::new("wires")
                        .long("wires")
                        .default_value("64")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of wires"),
                )
                .arg(
                    Arg::new("rounds")
                        .long("rounds")
                        .default_value("100")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of mixing rounds"),
                )
        )
        .subcommand(
            Command::new("grow-identity")
                .about("Grow a large identity circuit from small templates")
                .arg(
                    Arg::new("wires")
                        .short('w')
                        .long("wires")
                        .default_value("32")
                        .value_parser(clap::value_parser!(usize))
                        .help("Target number of wires (default: 32)"),
                )
                .arg(
                    Arg::new("rounds")
                        .short('r')
                        .long("rounds")
                        .default_value("5")
                        .value_parser(clap::value_parser!(usize))
                        .help("Number of growth rounds"),
                )
                .arg(
                    Arg::new("placements")
                        .short('p')
                        .long("placements")
                        .default_value("10")
                        .value_parser(clap::value_parser!(usize))
                        .help("Template placements per round"),
                )
                .arg(
                    Arg::new("diffusion")
                        .short('d')
                        .long("diffusion")
                        .default_value("100000")
                        .value_parser(clap::value_parser!(usize))
                        .help("Diffusion passes (shoot_random_gate intensity)"),
                )
                .arg(
                    Arg::new("template-source")
                        .long("template-source")
                        .value_parser(["perm", "template-db", "mixed"])
                        .help("Template source: perm, template-db, mixed"),
                )
                .arg(
                    Arg::new("template-db")
                        .long("template-db")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to TemplateDB (collection.lmdb)"),
                )
                .arg(
                    Arg::new("template-min-gates")
                        .long("template-min-gates")
                        .value_parser(clap::value_parser!(usize))
                        .help("Minimum template gate count"),
                )
                .arg(
                    Arg::new("template-max-gates")
                        .long("template-max-gates")
                        .value_parser(clap::value_parser!(usize))
                        .help("Maximum template gate count"),
                )
                .arg(
                    Arg::new("template-attempts")
                        .long("template-attempts")
                        .value_parser(clap::value_parser!(usize))
                        .help("Template sampling attempts per placement"),
                )
                .arg(
                    Arg::new("conjugation-min")
                        .long("conjugation-min")
                        .value_parser(clap::value_parser!(usize))
                        .help("Minimum conjugation depth (0 disables)"),
                )
                .arg(
                    Arg::new("conjugation-max")
                        .long("conjugation-max")
                        .value_parser(clap::value_parser!(usize))
                        .help("Maximum conjugation depth (0 disables)"),
                )
                .arg(
                    Arg::new("template-hardness-passes")
                        .long("template-hardness-passes")
                        .value_parser(clap::value_parser!(usize))
                        .help("Reducer passes for template hardness check (0 disables)"),
                )
                .arg(
                    Arg::new("template-min-reducer-ratio")
                        .long("template-min-reducer-ratio")
                        .value_parser(clap::value_parser!(f64))
                        .help("Minimum survival ratio for template acceptance"),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_parser(clap::value_parser!(String))
                        .help("Output file path (prints to stdout if not specified)"),
                )
                .arg(
                    Arg::new("config")
                        .long("config")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to JSON config file"),
                )
                .arg(
                    Arg::new("db")
                        .long("db")
                        .default_value("./db-old")
                        .value_parser(clap::value_parser!(String))
                        .help("Path to database directory (with SQLite + LMDB)"),
                )
        )

        .get_matches();

    match matches.subcommand() {
        Some(("load", sub)) => {
            let n: usize = *sub.get_one("n").unwrap();
            let m: usize = *sub.get_one("m").unwrap();
            // main_rainbow_load(n, m, "./db");

            // Open DB connection
            let mut conn = Connection::open("./db/circuits.db").expect("Failed to open DB");
            conn.execute_batch(
                "
                    PRAGMA synchronous = OFF;
                    PRAGMA journal_mode = WAL;
                    PRAGMA temp_store = MEMORY;
                    PRAGMA cache_size = -200000;
                    ",
            )
            .unwrap();
            let perms: Vec<Vec<usize>> = (0..n).permutations(n).collect();
            let bit_shuf = perms.into_iter().skip(1).collect::<Vec<_>>();
            build_from_sql(&mut conn, n, m, &bit_shuf).expect("Unknown error occured");
        }
        Some(("init", sub)) => {
            let n: usize = *sub.get_one("n").unwrap();
            let mut conn = Connection::open("./db/circuits.db").expect("Failed to open DB");
            build_initial_table(&mut conn, n).expect("Failed to init table");
        }
        Some(("random", sub)) => {
            let n: usize = *sub.get_one("n").unwrap();
            let m: usize = *sub.get_one("m").unwrap();

            if let Some(count) = sub.get_one::<usize>("count") {
                // Fixed-count mode
                main_random(n, m, *count, false);
            } else if sub.get_flag("sliding") {
                // Sliding-window fail-rate mode
                main_random(n, m, 0, true);
            } else {
                panic!("You must provide either -c <count> or -C for sliding-window mode");
            }
        }
        Some(("mix", sub)) => {
            let rounds: usize = *sub.get_one("rounds").unwrap();

            let data = fs::read_to_string("initial.txt").expect("Failed to read initial.txt");
            // let seed = OsRng.try_next_u64().unwrap_or_else(|e| {
            //     panic!("Failed to generate random seed: {}", e);
            // });
            // println!("Using seed: {}", seed);
            if data.trim().is_empty() {
                // Open DB connection
                let mut conn = Connection::open_with_flags(
                    "./db/circuits.db",
                    OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .expect("Failed to open DB (read-only)");
                conn.execute_batch(
                    "
                    PRAGMA synchronous = NORMAL;
                    PRAGMA journal_mode = WAL;
                    PRAGMA temp_store = MEMORY;
                    PRAGMA cache_size = -200000;
                    PRAGMA locking_mode = EXCLUSIVE;
                    ",
                )
                .unwrap();
                let lmdb = "./db";
                let env = lmdb::Environment::new()
                    .set_max_dbs(50)
                    .set_map_size(700 * 1024 * 1024 * 1024)
                    .open(Path::new(lmdb))
                    .expect("Failed to open lmdb");

                // Fallback when file is empty
                let c1 = random_canonical_id(&env, &conn, 5).unwrap();
                println!(
                    "{:?} Starting Len: {}",
                    c1.permutation(5).data,
                    c1.gates.len()
                );
                let config = ObfuscationConfig::default();
                main_mix(&c1, rounds, &mut conn, 5, &config);
            } else {
                let c = CircuitSeq::from_string(&data);

                // Open DB connection
                let mut conn = Connection::open_with_flags(
                    "./db/circuits.db",
                    OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .expect("Failed to open DB (read-only)");
                conn.execute_batch(
                    "
                    PRAGMA synchronous = NORMAL;
                    PRAGMA journal_mode = WAL;
                    PRAGMA temp_store = MEMORY;
                    PRAGMA cache_size = -200000;
                    PRAGMA locking_mode = EXCLUSIVE;
                    ",
                )
                .unwrap();
                let config = ObfuscationConfig::default();
                main_mix(&c, rounds, &mut conn, 5, &config);
            }
        }
        Some(("butterfly", sub)) => {
            let rounds: usize = *sub.get_one("rounds").unwrap();
            let data = fs::read_to_string("initial.txt").expect("Failed to read initial.txt");
            // let seed = OsRng.try_next_u64().unwrap_or_else(|e| {
            //     panic!("Failed to generate random seed: {}", e);
            // });
            // println!("Using seed: {}", seed);
            if data.trim().is_empty() {
                // Open DB connection
                let mut conn = Connection::open_with_flags(
                    "./db/db/circuits.db",
                    OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .expect("Failed to open DB (read-only)");
                conn.execute_batch(
                    "
                    PRAGMA synchronous = NORMAL;
                    PRAGMA journal_mode = WAL;
                    PRAGMA temp_store = MEMORY;
                    PRAGMA cache_size = -200000;
                    PRAGMA locking_mode = EXCLUSIVE;
                    ",
                )
                .unwrap();
                // Fallback when file is empty
                // Fallback when file is empty
                println!("Generating random");
                let c1 = random_circuit(6, 30);
                // let perms: Vec<Vec<usize>> = (0..5).permutations(5).collect();
                // let bit_shuf = perms.into_iter().skip(1).collect::<Vec<_>>();
                // let c1 = compress(&random_circuit(5,128), 100_000, &mut conn, &bit_shuf,5 );
                println!(
                    "{:?} Starting Len: {}",
                    c1.permutation(6).data,
                    c1.gates.len()
                );
                let config = ObfuscationConfig::default();
                main_butterfly(&c1, rounds, &mut conn, 6, &config);
            } else {
                let c = CircuitSeq::from_string(&data);

                // Open DB connection
                let mut conn = Connection::open_with_flags(
                    "./db/db/circuits.db",
                    OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .expect("Failed to open DB (read-only)");
                conn.execute_batch(
                    "
                    PRAGMA synchronous = NORMAL;
                    PRAGMA journal_mode = WAL;
                    PRAGMA temp_store = MEMORY;
                    PRAGMA cache_size = -200000;
                    PRAGMA locking_mode = EXCLUSIVE;
                    ",
                )
                .unwrap();
                let config = ObfuscationConfig::default();
                main_butterfly(&c, rounds, &mut conn, 6, &config);
            }
        }
        Some(("rac", sub)) => {
            let rounds: usize = *sub.get_one("rounds").unwrap();
            let path: &String = sub.get_one("path").unwrap();
            let n: usize = *sub.get_one("n").unwrap();
            let save: &String = sub.get_one("save").unwrap();
            let intermediate: &String = sub.get_one("intermediate").unwrap();

            // Read the input circuit
            let data = fs::read_to_string(path)
                .unwrap_or_else(|_| panic!("Failed to read circuit file: {}", path));
            let c = CircuitSeq::from_string(&data);

            println!("Running RAC on circuit with {} wires, {} gates", n, c.gates.len());

            // Open DB connection
            let mut conn = Connection::open_with_flags(
                "db/circuits.db",
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("Failed to open DB (read-only)");
            conn.execute_batch(
                "
                PRAGMA synchronous = NORMAL;
                PRAGMA journal_mode = WAL;
                PRAGMA temp_store = MEMORY;
                PRAGMA cache_size = -200000;
                PRAGMA locking_mode = EXCLUSIVE;
                ",
            )
            .unwrap();

            // Open LMDB environment
            let lmdb = "./db";
            let env = lmdb::Environment::new()
                .set_max_dbs(60)
                .set_map_size(700 * 1024 * 1024 * 1024)
                .open(Path::new(lmdb))
                .expect("Failed to open lmdb");

            // Run RAC
            main_rac_big(&c, rounds, &mut conn, n, save, &env, intermediate);
        }
        Some(("ids-index", sub)) => {
            let db_path: &String = sub.get_one("db").unwrap();
            let min_n: usize = *sub.get_one("min-n").unwrap();
            let max_n: usize = *sub.get_one("max-n").unwrap();

            let env = lmdb::Environment::new()
                .set_max_dbs(100)
                .set_map_size(700 * 1024 * 1024 * 1024)
                .open(Path::new(db_path))
                .expect("Failed to open lmdb");

            let mut ids_db_names = Vec::new();
            for n in min_n..=max_n {
                ids_db_names.push(format!("ids_n{}", n));
            }

            println!(
                "Building ids reverse index + witness prefilter from: {:?}",
                ids_db_names
            );
            let stats = build_ids_indexes(&env, &ids_db_names)
                .expect("Failed to build ids reverse index");
            println!(
                "ids-index done: dbs_scanned={} ids_seen={} rev_inserted={} token_inserted={}",
                stats.dbs_scanned, stats.ids_seen, stats.rev_inserted, stats.token_inserted
            );
        }
        Some(("bbutterfly", sub)) => {
            // let rounds: usize = *sub.get_one("rounds").unwrap();
            let path: &str = sub.get_one::<String>("path").unwrap().as_str();
            let n: usize = *sub.get_one("n").unwrap_or(&32); // default to 32 if not provided
            let data = fs::read_to_string(path).expect("Failed to read initial circuit file");

            // Load Config
            let mut config = if let Some(config_path) = sub.get_one::<String>("config") {
                let file = std::fs::File::open(config_path).expect("Failed to open config file");
                let reader = std::io::BufReader::new(file);
                serde_json::from_reader(reader).expect("Failed to parse config JSON")
            } else {
                ObfuscationConfig::default()
            };

            // Overlay CLI arguments
            if let Some(r) = sub.get_one::<usize>("rounds") {
                config.rounds = *r;
            }
            if let Some(s) = sub.get_one::<usize>("shooting") {
                config.shooting_count = *s;
            }
            if sub.get_flag("single-gate") {
                config.single_gate_mode = true;
            }
            if sub.get_flag("no-ancilla") {
                config.no_ancilla_mode = true;
            }

            let mut conn = Connection::open("./db/circuits.db").expect("Failed to open DB");
            conn.execute_batch(
                "
                PRAGMA temp_store = MEMORY;
                PRAGMA cache_size = -200000;
                ",
            )
            .unwrap();

            let lmdb = "./db";
            let _ = std::fs::create_dir_all(lmdb);

            let env = lmdb::Environment::new()
                .set_max_dbs(50)
                .set_map_size(1 * 1024 * 1024 * 1024)
                .open(Path::new(lmdb))
                .expect("Failed to open lmdb");

            if data.trim().is_empty() {
                println!("Generating random");
                let c1 = random_circuit(n as u8, 30);
                println!("Starting Len: {}", c1.gates.len());
                main_butterfly_big(&c1, &mut conn, n, false, path, &env, &config, None);
            } else {
                let c = CircuitSeq::from_string(&data);
                main_butterfly_big(&c, &mut conn, n, false, path, &env, &config, None);
            }
        }

        Some(("abbutterfly", sub)) => {
            // let rounds: usize = *sub.get_one("rounds").unwrap(); // Now handled via config overlay
            let path: &str = sub.get_one::<String>("path").unwrap().as_str();
            let n: usize = *sub.get_one("n").unwrap_or(&32); // default to 32 if not provided
            let data = fs::read_to_string(path).expect("Failed to read initial circuit file");
            let bookendless = sub.get_flag("bookendless");

            // Load Config
            let mut config = if let Some(config_path) = sub.get_one::<String>("config") {
                let file = std::fs::File::open(config_path).expect("Failed to open config file");
                let reader = std::io::BufReader::new(file);
                serde_json::from_reader(reader).expect("Failed to parse config JSON")
            } else {
                ObfuscationConfig::default()
            };

            // Overlay CLI arguments
            if let Some(r) = sub.get_one::<usize>("rounds") {
                config.rounds = *r;
            }
            if let Some(s) = sub.get_one::<usize>("shooting") {
                config.shooting_count = *s;
            }
            if sub.get_flag("sat") {
                config.sat_mode = true;
            }
            if sub.get_flag("single-gate") {
                config.single_gate_mode = true;
            }
            if sub.get_flag("no-ancilla") {
                config.no_ancilla_mode = true;
            }
            if sub.get_flag("no-pairs") {
                config.pair_replacement_mode = false;
            }
            if sub.get_flag("no-equal") {
                config.equal_replacement_mode = false;
            }

            let lmdb_db_path = sub.get_one::<String>("lmdb-db");

            let template_db = if let Some(path) = lmdb_db_path {
                println!("Loading TemplateDB from {}", path);
                Some(
                    local_mixing::infra::store::reader::TemplateDB::open(path)
                        .expect("Failed to open TemplateDB"),
                )
            } else {
                None
            };

            let mut conn =
                Connection::open_with_flags("./db/circuits.db", OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("Failed to open db/circuits.db (read-only)");
            conn.execute_batch(
                "
                PRAGMA temp_store = MEMORY;
                PRAGMA cache_size = -200000;
                ",
            )
            .unwrap();
            let lmdb = "./db";
            let _ = std::fs::create_dir_all(lmdb);

            let env = lmdb::Environment::new()
                .set_max_readers(10000)
                .set_max_dbs(60)
                .set_map_size(1 * 1024 * 1024 * 1024)
                .open(Path::new(lmdb))
                .expect("Failed to open lmdb");

            install_kill_handler();
            if data.trim().is_empty() {
                println!("Generating random");
                let c1 = random_circuit(n as u8, 30);
                println!("Starting Len: {}", c1.gates.len());
                if bookendless {
                    main_butterfly_big_bookendsless(
                        &c1,
                        &mut conn,
                        n,
                        true,
                        path,
                        &env,
                        &config,
                        template_db.as_ref(),
                    );
                } else {
                    main_butterfly_big(
                        &c1,
                        &mut conn,
                        n,
                        true,
                        path,
                        &env,
                        &config,
                        template_db.as_ref(),
                    );
                }
            } else {
                let c = CircuitSeq::from_string(&data);
                if bookendless {
                    main_butterfly_big_bookendsless(
                        &c,
                        &mut conn,
                        n,
                        true,
                        path,
                        &env,
                        &config,
                        template_db.as_ref(),
                    );
                } else {
                    main_butterfly_big(
                        &c,
                        &mut conn,
                        n,
                        true,
                        path,
                        &env,
                        &config,
                        template_db.as_ref(),
                    );
                }
            }
        }
        Some(("reverse", sub)) => {
            let from_path = sub.get_one::<String>("source").unwrap();
            let dest_path = sub.get_one::<String>("dest").unwrap();
            reverse(from_path, dest_path);
        }
        Some(("compress", sub)) => {
            let p: &String = sub.get_one("p").expect("Missing -p <path>");
            let n: usize = *sub.get_one("n").expect("Missing -n <wires>");

            let contents = fs::read_to_string(p)
                .unwrap_or_else(|_| panic!("Failed to read circuit file at {}", p));

            let mut acc = CircuitSeq::from_string(&contents);

            let mut conn = Connection::open_with_flags(
                "./db/circuits.db",
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("Failed to open ./db/circuits.db in read-only mode");
            let lmdb = "./db";
            let _ = std::fs::create_dir_all(lmdb);

            let env = Environment::new()
                .set_max_dbs(60)
                .set_map_size(700 * 1024 * 1024 * 1024)
                .open(Path::new(lmdb))
                .expect("Failed to open lmdb");
            let dbs = open_all_dbs(&env);
            let bit_shuf_list = (3..=7)
                .map(|n| {
                    (0..n)
                        .permutations(n)
                        .filter(|p| !p.iter().enumerate().all(|(i, &x)| i == x))
                        .collect::<Vec<Vec<usize>>>()
                })
                .collect();
            // Call compression logic
            let mut stable_count = 0;
            while stable_count < 6 {
                let before = acc.gates.len();
                acc = compress_big_ancillas(&acc, 1_000, n, &mut conn, &env, &bit_shuf_list, &dbs);
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
            let mut file =
                fs::File::create("compressed.txt").expect("Failed to create compressed.txt");
            write!(file, "{}", acc.repr()).expect("Failed to write compressed circuit to file");

            println!("Compressed circuit written to compressed.txt");
        }
        Some(("wiredot", sub)) => {
            let n: usize = *sub.get_one("num_wires").unwrap();

            let path = sub.get_one::<String>("path").map(|s| s.as_str()).unwrap();

            let xlabel = sub
                .get_one::<String>("xlabel")
                .map(|s| s.as_str())
                .unwrap_or("Circuit 1 gate index");

            let e = format!("Failed to read {}", path);
            let c = fs::read_to_string(path).expect(&e);
            let c = CircuitSeq::from_string(&c);

            analyze_gate_to_wires(&c, n, xlabel).unwrap();
        }
        Some(("lmdb", sub)) => {
            let path: &String = sub.get_one("p").unwrap();
            // let n: usize = *sub.get_one("n").unwrap(); // Not used
            // let window: usize = *sub.get_one("window").unwrap(); // Removed from config
            // let active: Option<usize> = sub.get_one("active").copied(); // Removed from config

            let data = fs::read_to_string(path).expect("Failed to read circuit file");
            let mut c = CircuitSeq::from_string(&data);

            let initial_len = c.gates.len();

            // Open DBs
            let db_path = sub
                .get_one::<String>("db")
                .map(|s| s.as_str())
                .unwrap_or("./db-old");
            let env = lmdb::Environment::new()
                .set_max_dbs(50)
                .set_map_size(1 * 1024 * 1024 * 1024)
                .open(Path::new(db_path))
                .ok(); // Optional, ignore if fail (or log warning?)

            let sqlite_path = format!("{}/circuits.db", db_path);
            let conn =
                Connection::open_with_flags(&sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();

            let passes: usize = *sub.get_one("passes").unwrap();
            let config = ReduceConfig {
                max_passes: passes,
                max_stall: 10,
            };

            println!(
                "Compressing {} gates (DB: {:?}, Passes: {})...",
                initial_len, db_path, passes
            );
            let final_len = reduce_circuit(&mut c, &config, env.as_ref(), conn.as_ref());
            println!("Final Length: {}", final_len);
            println!("Ratio: {:.4}", final_len as f64 / initial_len as f64);
        }
        Some(("lmdbcounts", _)) => {
            let env_path = "./db";

            let env = Environment::new()
                .set_max_dbs(50)
                .set_map_size(64 * 1024 * 1024 * 1024)
                .open(Path::new(env_path))
                .expect("Failed to open lmdb");

            let ns_and_ms = [(3, 10), (4, 6), (5, 5), (6, 5), (7, 4)];

            for (n, max_m) in ns_and_ms {
                let tables: Vec<String> = (1..=max_m).map(|m| format!("n{}m{}", n, m)).collect();

                // println!("tables: {:?}", tables);
                let perms_to_m =
                    perm_tables_with_duplicates(&env, &tables).expect("Failed to compute perms");

                let db_name = format!("perm_tables_n{}", n);
                save_perm_tables_to_lmdb(&env_path, &db_name, &perms_to_m)
                    .expect("Failed to save perms");

                println!("Saved perm_tables_n{}", n);
            }
        }
        Some(("string", sub)) => {
            let from_path = sub.get_one::<String>("source").unwrap();
            let dest_path = sub.get_one::<String>("dest").unwrap();
            let input_str = fs::read_to_string(from_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", from_path, e));
            let circuit = CircuitSeq::from_string(input_str.trim());
            let string = circuit.to_string(circuit.used_wires().len());
            fs::write(dest_path, string)
                .unwrap_or_else(|e| panic!("Failed to write {}: {}", dest_path, e));
        }
        Some(("obfuscate", sub)) => {
            use local_mixing::obfuscate::{ObfConfig, obfuscate};

            let input_path: &String = sub.get_one("input").unwrap();
            let output_path: &String = sub.get_one("output").unwrap();
            let level: u8 = *sub.get_one("level").unwrap();
            let seed: u64 = *sub.get_one("seed").unwrap();
            let num_wires: usize = *sub.get_one("wires").unwrap();
            let report_path = sub.get_one::<String>("report");

            // Load input circuit
            println!("Loading circuit from {}...", input_path);
            let input_str = fs::read_to_string(input_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", input_path, e));
            let circuit = CircuitSeq::from_string(input_str.trim());
            println!("Loaded circuit with {} gates", circuit.len());

            // Configure obfuscation
            let cfg = ObfConfig::level(level)
                .with_wires(num_wires)
                .with_seed(seed);

            println!("Obfuscating at level {} (seed={})...", level, seed);
            let result = obfuscate(&circuit, &cfg);

            // Report
            println!("Obfuscation complete:");
            println!("  Original: {} gates", result.report.orig_len);
            println!("  Obfuscated: {} gates", result.report.obf_len);
            println!("  Overhead: {:.2}x", result.report.overhead);
            println!("  Coverage: {:.2}", result.report.wire_coverage);
            println!("  Verified: {}", result.report.verified);

            // Save obfuscated circuit
            let output_str = result.circuit.repr();
            fs::write(output_path, &output_str)
                .unwrap_or_else(|e| panic!("Failed to write {}: {}", output_path, e));
            println!("Saved obfuscated circuit to {}", output_path);

            // Save report if requested
            if let Some(path) = report_path {
                let json = result.report.to_json();
                fs::write(path, &json)
                    .unwrap_or_else(|e| panic!("Failed to write report {}: {}", path, e));
                println!("Saved report to {}", path);
            }
        }
        Some(("local-mix", sub)) => {
            let wires = *sub.get_one("wires").unwrap();
            let rounds = *sub.get_one("rounds").unwrap();

            // Open DB if available (read-only for templates)
            let conn = Connection::open_with_flags(
                "./db/circuits.db",
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .ok();

            let lmdb_path = Path::new("./db");
            let env = if lmdb_path.exists() {
                use lmdb::Environment;
                Environment::new()
                    .set_max_dbs(50)
                    .set_map_size(700 * 1024 * 1024 * 1024)
                    .open(lmdb_path)
                    .ok()
            } else {
                None
            };

            let mut circuit = CircuitSeq { gates: Vec::new() };

            let mix_cfg = MixConfig {
                num_wires: wires,
                num_rounds: rounds,
                template_size_max: 8,
            };

            println!(
                "Generating and mixing identity on {} wires for {} rounds...",
                wires, rounds
            );
            mix_circuit(&mut circuit, &mix_cfg, env.as_ref(), conn.as_ref());

            let len_mixed = circuit.gates.len();
            let used = local_mixing::infra::circuit::CircuitSeq::count_used_wires(&circuit);
            let coverage = if wires > 0 {
                used as f32 / wires as f32
            } else {
                0.0
            };

            println!("Mixed Length: {}", len_mixed);
            println!("Wire Coverage: {:.2}% ({} wires)", coverage * 100.0, used);

            // Reducer Score
            let reduce_cfg = ReduceConfig {
                max_passes: 200,
                max_stall: 10,
            };
            println!("Running reducer (attacker model)...");

            // Clone to keep original mixed circuit
            let mut reduced_circuit = circuit.clone();
            let len_reduced = reduce_circuit(
                &mut reduced_circuit,
                &reduce_cfg,
                env.as_ref(),
                conn.as_ref(),
            );

            println!("Reduced Length: {}", len_reduced);
            let ratio = if len_mixed > 0 {
                len_reduced as f32 / len_mixed as f32
            } else {
                1.0
            };
            println!("Compression Ratio: {:.4}", ratio);
        }
        Some(("gen", sub)) => {
            let n: u8 = *sub.get_one("wires").unwrap_or(&64);
            let l: usize = *sub.get_one("length").unwrap_or(&100);
            let c = random_circuit(n, l);
            print!("{}", c.repr());
        }
        Some(("heatmap", sub)) => {
            let n = *sub.get_one::<usize>("num_wires").unwrap();
            let inputs = *sub.get_one::<usize>("inputs").unwrap();
            let c1_path = sub.get_one::<String>("c1").unwrap();
            let c2_path = sub.get_one::<String>("c2").unwrap();

            let c1_str = fs::read_to_string(c1_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c1_path, e));
            let c2_str = fs::read_to_string(c2_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c2_path, e));

            let c1 = CircuitSeq::from_string(c1_str.trim());
            let c2 = CircuitSeq::from_string(c2_str.trim());

            // Defaulting to true for canonicalization, unless --raw is passed
            let canonicalize = !sub.get_flag("raw");
            let data = generate_heatmap_data(&c1, &c2, n, inputs, canonicalize);

            // Output JSON to stdout
            let output = serde_json::json!({
                "heatmap_data": data,
                "x_size": c1.gates.len() + 1,
                "y_size": c2.gates.len() + 1
            });
            println!("{}", output);
        }

        Some(("align", sub)) => {
            let n = *sub.get_one::<usize>("n").unwrap();
            let num_inputs = *sub.get_one::<usize>("inputs").unwrap();
            let c1_path = sub.get_one::<String>("c1").unwrap();
            let c2_path = sub.get_one::<String>("c2").unwrap();

            let c1_str = fs::read_to_string(c1_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c1_path, e));
            let c2_str = fs::read_to_string(c2_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c2_path, e));

            let mut c1 = CircuitSeq::from_string(c1_str.trim());
            let mut c2 = CircuitSeq::from_string(c2_str.trim());

            // Canonicalize to match heatmap behavior, unless --raw is passed
            if !sub.get_flag("raw") {
                c1.canonicalize();
                c2.canonicalize();
            }

            // Generate random inputs
            let mut rng = rand::rng();
            let inputs: Vec<u64> = (0..num_inputs).map(|_| rng.random()).collect();

            // Trace states
            let trace1 = trace_states(&c1, &inputs);
            let trace2 = trace_states(&c2, &inputs);

            // Distance matrix
            let d = distance_matrix(&trace1, &trace2, n);

            // DTW Alignment
            // Using small penalties as typically used in this research context
            let result = dtw_alignment(&d, 0.1, 0.1);

            // Output JSON
            println!("{}", serde_json::to_string(&result).unwrap());
        }
        Some(("compare", sub)) => {
            let n = *sub.get_one::<usize>("n").unwrap();
            let num_inputs = *sub.get_one::<usize>("inputs").unwrap();
            let c1_path = sub.get_one::<String>("c1").unwrap();
            let c2_path = sub.get_one::<String>("c2").unwrap();

            let c1_str = fs::read_to_string(c1_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c1_path, e));
            let c2_str = fs::read_to_string(c2_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", c2_path, e));

            use local_mixing::infra::circuit::Gate;

            let c1 = CircuitSeq::from_string(c1_str.trim());
            let c2 = CircuitSeq::from_string(c2_str.trim());

            println!("Comparing logic tables on {} random inputs...", num_inputs);

            // Print 5 examples
            let mut rng = rand::rng();
            let mask: usize = if n < usize::BITS as usize {
                (1 << n) - 1
            } else {
                usize::MAX - 1
            };

            println!(
                "{:<10} | {:<10} | {:<10} | Match?",
                "Input", "C1 Out", "C2 Out"
            );
            println!("{:-<10}-+-{:-<10}-+-{:-<10}-+-------", "", "", "");

            for _ in 0..5 {
                let input = (rng.random::<u64>() as usize) & mask;
                let out1 = Gate::evaluate_index_list(input, &c1.gates);
                let out2 = Gate::evaluate_index_list(input, &c2.gates);
                let match_str = if out1 == out2 { "✅" } else { "❌" };
                println!(
                    "0x{:<08x} | 0x{:<08x} | 0x{:<08x} | {}",
                    input, out1, out2, match_str
                );
            }
            println!("...");

            match c1.probably_equal(&c2, n, num_inputs) {
                Ok(_) => println!(
                    "Circuits are functionally EQUIVALENT (checked {} inputs)",
                    num_inputs
                ),
                Err(e) => println!("Circuits DIFFERENT: {}", e),
            }
        }
        Some(("grow-identity", sub)) => {
            let wires: usize = *sub.get_one("wires").unwrap_or(&32);
            let rounds: usize = *sub.get_one("rounds").unwrap_or(&5);
            let placements: usize = *sub.get_one("placements").unwrap_or(&10);
            let diffusion: usize = *sub.get_one("diffusion").unwrap_or(&100_000);
            let output_path = sub.get_one::<String>("output");

            // Build config from CLI args or JSON file
            let mut config = if let Some(config_path) = sub.get_one::<String>("config") {
                let file = std::fs::File::open(config_path).expect("Failed to open config file");
                let reader = std::io::BufReader::new(file);
                serde_json::from_reader(reader).expect("Failed to parse config JSON")
            } else {
                IdentityGrowthConfig {
                    target_width: wires,
                    rounds,
                    placements_per_round: placements,
                    diffusion_passes: diffusion,
                    ..Default::default()
                }
            };

            // Overlay CLI arguments
            if let Some(source) = sub.get_one::<String>("template-source") {
                config.template_source = match source.as_str() {
                    "perm" => TemplateSource::PermTables,
                    "template-db" => TemplateSource::TemplateDB,
                    "mixed" => TemplateSource::Mixed,
                    _ => TemplateSource::PermTables,
                };
            }
            if let Some(min_gates) = sub.get_one::<usize>("template-min-gates") {
                config.template_gate_count_min = *min_gates;
            }
            if let Some(max_gates) = sub.get_one::<usize>("template-max-gates") {
                config.template_gate_count_max = *max_gates;
            }
            if let Some(attempts) = sub.get_one::<usize>("template-attempts") {
                config.template_attempts = *attempts;
            }
            if let Some(min_depth) = sub.get_one::<usize>("conjugation-min") {
                config.template_conjugation_depth_min = *min_depth;
            }
            if let Some(max_depth) = sub.get_one::<usize>("conjugation-max") {
                config.template_conjugation_depth_max = *max_depth;
            }
            if let Some(passes) = sub.get_one::<usize>("template-hardness-passes") {
                config.template_hardness_passes = *passes;
            }
            if let Some(ratio) = sub.get_one::<f64>("template-min-reducer-ratio") {
                config.template_min_reducer_ratio = *ratio;
            }

            // Open LMDB
            let db_path = sub
                .get_one::<String>("db")
                .map(|s| s.as_str())
                .unwrap_or("./db-old");
            let env = lmdb::Environment::new()
                .set_max_dbs(50)
                .set_map_size(1 * 1024 * 1024 * 1024)
                .open(Path::new(db_path))
                .expect("Failed to open LMDB");

            // Open SQLite (for random_canonical_id compatibility)
            let sqlite_path = format!("{}/circuits.db", db_path);
            let conn = Connection::open_with_flags(&sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect(&format!("Failed to open SQLite DB at {}", sqlite_path));

            let template_db = sub.get_one::<String>("template-db").map(|path| {
                local_mixing::infra::store::reader::TemplateDB::open(path)
                    .expect("Failed to open TemplateDB")
            });
            if matches!(config.template_source, TemplateSource::TemplateDB) && template_db.is_none()
            {
                eprintln!("Error: --template-source template-db requires --template-db");
                std::process::exit(1);
            }
            if matches!(config.template_source, TemplateSource::Mixed) && template_db.is_none() {
                eprintln!(
                    "Warning: --template-source mixed without --template-db; using perm tables only"
                );
            }

            // Run identity growth
            match grow_identity(&config, &env, &conn, template_db.as_ref()) {
                Ok((circuit, metrics)) => {
                    // Output circuit
                    let circuit_str = circuit.repr();

                    // Determine output path
                    let final_output_path = if let Some(path) = output_path {
                        path.clone()
                    } else {
                        // Default: save to experiments/identities/ with timestamped name
                        let identities_dir = Path::new("experiments/identities");
                        std::fs::create_dir_all(identities_dir).ok();

                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        format!(
                            "{}/identity_{}w_{}g_{}.gate",
                            identities_dir.display(),
                            wires,
                            metrics.final_gates,
                            timestamp
                        )
                    };

                    std::fs::write(&final_output_path, &circuit_str)
                        .expect("Failed to write output file");
                    eprintln!(
                        "Saved {} gates to {}",
                        metrics.final_gates, final_output_path
                    );
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => unreachable!(),
    }
}

pub fn reverse(from_path: &str, dest_path: &str) {
    if !Path::new(from_path).exists() {
        panic!("Source file {} does not exist", from_path);
    }

    // Read circuit string
    let input_str = fs::read_to_string(from_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", from_path, e));

    // Parse into CircuitSeq
    let mut circuit = CircuitSeq::from_string(input_str.trim());

    // Reverse the gates
    circuit.gates.reverse();

    // Convert back to string
    let reversed_str = circuit.repr();

    // Write to destination file
    fs::write(dest_path, reversed_str)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", dest_path, e));

    println!("Reversed circuit written to {}", dest_path);
}

pub fn analyze_gate_to_wires(
    circuit: &CircuitSeq,
    num_wires: usize,
    x: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut total_counts = vec![0u32; num_wires];
    let mut active_counts = vec![0u32; num_wires];
    for gate in &circuit.gates {
        for (i, &w) in gate.iter().enumerate() {
            total_counts[w as usize] += 1;
            if i == 0 {
                active_counts[w as usize] += 1;
            }
        }
    }

    let root = BitMapBackend::new("wire_plot.png", (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_count = *total_counts.iter().max().unwrap_or(&1);

    let mut chart = ChartBuilder::on(&root)
        .caption("Gate Touches per Wire", "sans-serif, 24")
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(0f64..num_wires as f64, 0f64..(max_count as f64 + 1.0))?;

    let x_label = format!("Wire Index ({})", x);
    let _ = chart
        .configure_mesh()
        .x_desc(x_label)
        .y_desc("Gate Touch Count")
        .draw();

    chart
        .draw_series(
            (0..num_wires)
                .map(|i| Circle::new((i as f64, total_counts[i] as f64), 6, BLUE.filled())),
        )?
        .label("Total gate count")
        .legend(|(x, y)| Circle::new((x, y), 5, BLUE.filled()));

    chart
        .draw_series(
            (0..num_wires)
                .map(|i| Circle::new((i as f64, active_counts[i] as f64), 6, RED.filled())),
        )?
        .label("Active gate count")
        .legend(|(x, y)| Circle::new((x, y), 5, BLUE.filled()));

    root.present()?;
    println!("Saved to wire_plot.png");
    Ok(())
}

use lmdb::Cursor;
use lmdb::{Database, Environment, Transaction, WriteFlags};
use local_mixing::infra::circuit::Permutation;

pub fn sql_to_lmdb(n: usize, m: usize) -> Result<(), ()> {
    let sqlite_path = "db/circuits.db";
    let lmdb_path = "./db";
    let map_size_bytes: usize = 800 * 1024 * 1024 * 1024;
    let batch_max_entries: usize = 100_000;

    let conn = Connection::open(sqlite_path).expect("Failed to open sqlite database");
    let table = format!("n{}m{}", n, m);

    let query = format!("SELECT * FROM {}", table);
    let mut stmt = conn
        .prepare(&query)
        .expect("Failed to prepare SQLite query");
    let mut rows = stmt.query([]).expect("Failed to query SQLite rows");

    fs::create_dir_all(lmdb_path).expect("Failed to create LMDB directory");
    let env = Environment::new()
        .set_max_dbs(50)
        .set_map_size(map_size_bytes)
        .open(Path::new(lmdb_path))
        .expect("Failed to open LMDB environment");

    let db = env
        .create_db(Some(&table), lmdb::DatabaseFlags::empty())
        .expect("Failed to create LMDB database");

    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(batch_max_entries);
    let mut rows_processed: u64 = 0;

    let flush = |env: &Environment, db: Database, batch: &mut Vec<Vec<u8>>| {
        if batch.is_empty() {
            return;
        }
        let mut txn = env
            .begin_rw_txn()
            .expect("Failed to begin LMDB RW transaction");
        for key in batch.iter() {
            txn.put(db, key, &[], WriteFlags::empty())
                .expect("Failed to write LMDB entry");
        }
        txn.commit().expect("Failed to commit LMDB transaction");
        batch.clear();
    };

    while let Some(row) = rows.next().expect("Failed getting next SQLite row") {
        rows_processed += 1;

        let circuit: Vec<u8> = row.get(0).expect("Failed to read column 'circuit'");
        let perm: Vec<u8> = row.get(1).expect("Failed to read column 'perm'");
        let shuf: Vec<u8> = row.get(2).expect("Failed to read column 'shuf'");

        // check inverse
        let inv = crate::Permutation::from_blob(&perm).invert().repr_blob();
        let mut inv_key = inv.clone();
        inv_key.extend_from_slice(&0u32.to_le_bytes());
        let ro_txn = env.begin_ro_txn().expect("Failed to begin LMDB RO txn");
        if ro_txn.get(db, &inv_key).is_ok() {
            continue;
        }

        let mut key = perm.clone();

        let mut circuit_seq = CircuitSeq::from_blob(&circuit);
        circuit_seq.rewire(&Permutation::from_blob(&shuf), n);
        circuit_seq.canonicalize();
        if circuit_seq.gates.windows(2).any(|w| w[0] == w[1]) {
            continue;
        }
        // compute key = perm || circuit
        key.extend_from_slice(&circuit_seq.repr_blob());

        batch.push(key);

        if batch.len() >= batch_max_entries {
            flush(&env, db, &mut batch);
        }

        if rows_processed % 100_000 == 0 {
            println!("Processed {} in {}", rows_processed, table);
        }
    }

    if !batch.is_empty() {
        flush(&env, db, &mut batch);
    }

    println!(
        "Finished copying {} rows into LMDB table {}",
        rows_processed, table
    );

    Ok(())
}

pub fn sql_to_lmdb_perms(n: usize, m: usize) -> Result<(), ()> {
    let sqlite_path = "db/circuits.db";
    let lmdb_path = "./db";
    let map_size_bytes: usize = 800 * 1024 * 1024 * 1024;
    let batch_max_entries: usize = 100_000;

    // Open SQLite
    let conn = Connection::open(sqlite_path).expect("Failed to open SQLite database");
    let table = format!("n{}m{}", n, m);
    let table2 = format!("n{}m{}perms", n, m);
    let query = format!("SELECT * FROM {}", table);
    let mut stmt = conn
        .prepare(&query)
        .expect("Failed to prepare SQLite query");
    let mut rows = stmt.query([]).expect("Failed to query SQLite rows");

    // Open LMDB
    fs::create_dir_all(lmdb_path).expect("Failed to create LMDB directory");
    let env = Environment::new()
        .set_max_dbs(50)
        .set_map_size(map_size_bytes)
        .open(Path::new(lmdb_path))
        .expect("Failed to open LMDB environment");

    let db = env
        .create_db(Some(&table2), lmdb::DatabaseFlags::empty())
        .expect("Failed to create LMDB database");

    let mut batch: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(batch_max_entries);
    let mut rows_processed: u64 = 0;

    // Flush function writes batch to LMDB
    let flush = |env: &Environment, db: Database, batch: &mut Vec<(Vec<u8>, Vec<u8>)>| {
        if batch.is_empty() {
            return;
        }
        let mut txn = env
            .begin_rw_txn()
            .expect("Failed to begin LMDB RW transaction");
        for (key, val) in batch.iter() {
            txn.put(db, key, val, WriteFlags::empty())
                .expect("Failed to write LMDB entry");
        }
        txn.commit().expect("Failed to commit LMDB transaction");
        batch.clear();
    };

    // Iterate SQLite rows
    while let Some(row) = rows.next().expect("Failed getting next SQLite row") {
        rows_processed += 1;

        let circuit: Vec<u8> = row.get(0).expect("Failed to read 'circuit'");
        let perm: Vec<u8> = row.get(1).expect("Failed to read 'perm'");
        let shuf: Vec<u8> = row.get(2).expect("Failed to read 'shuf'");

        // Serialize (perm, shuf) together
        let mut val = Vec::with_capacity(perm.len() + shuf.len());
        val.extend_from_slice(&perm);
        val.extend_from_slice(&shuf);

        batch.push((circuit, val));

        if batch.len() >= batch_max_entries {
            flush(&env, db, &mut batch);
        }

        if rows_processed % 100_000 == 0 {
            println!("Processed {} rows in {}", rows_processed, table);
        }
    }

    if !batch.is_empty() {
        flush(&env, db, &mut batch);
    }

    println!(
        "Finished copying {} rows into LMDB table {}",
        rows_processed, table
    );
    Ok(())
}

/// Scans all tables and creates a DB of perms with multiple circuits
use std::collections::HashMap;

fn perm_tables_with_duplicates(
    env: &Environment,
    tables: &[String], // tables like n{num_wires}m{m}
) -> Result<HashMap<Vec<u8>, Vec<u8>>, lmdb::Error> {
    let mut perms_to_m: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();

    for table in tables {
        // parse num_wires and m from table name
        let t = table.strip_prefix('n').unwrap();
        let (n_str, m_str) = t.split_once('m').unwrap();
        let num_wires: usize = n_str.parse().unwrap();
        let m: u8 = m_str.parse().unwrap();

        let perm_len = 1usize << num_wires;

        let db = env.open_db(Some(table))?;
        let ro_txn = env.begin_ro_txn()?;
        let mut cursor = ro_txn.open_ro_cursor(db)?;

        for (k, _) in cursor.iter() {
            let perm = &k[..perm_len];

            // push every occurrence of perm, even duplicates in same table
            perms_to_m.entry(perm.to_vec()).or_default().push(m);
        }
    }

    perms_to_m.retain(|_, ms| ms.len() > 1);

    Ok(perms_to_m)
}

fn save_perm_tables_to_lmdb(
    env_path: &str,
    db_name: &str,
    perms_to_m: &HashMap<Vec<u8>, Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(env_path)?;
    let env = Environment::new()
        .set_max_dbs(50)
        .set_map_size(800 * 1024 * 1024 * 1024)
        .open(Path::new(env_path))?;

    let db = env.create_db(Some(db_name), lmdb::DatabaseFlags::empty())?;

    let batch_size = 100_000;
    let mut batch: Vec<(&Vec<u8>, Vec<u8>)> = Vec::with_capacity(batch_size);

    let flush_batch = |env: &Environment, db: Database, batch: &mut Vec<(&Vec<u8>, Vec<u8>)>| {
        if batch.is_empty() {
            return;
        }
        let mut txn = env.begin_rw_txn().expect("Failed to begin LMDB txn");
        for (key, value) in batch.iter() {
            txn.put(db, key, value, WriteFlags::empty())
                .expect("Failed to write LMDB entry");
        }
        txn.commit().expect("Failed to commit LMDB txn");
        batch.clear();
    };

    for (perm, ms) in perms_to_m.iter() {
        let value = bincode::serialize(ms)?;
        batch.push((perm, value));

        if batch.len() >= batch_size {
            flush_batch(&env, db, &mut batch);
        }
    }

    flush_batch(&env, db, &mut batch);

    Ok(())
}
