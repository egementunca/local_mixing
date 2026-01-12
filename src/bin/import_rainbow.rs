use lmdb::{Transaction, WriteFlags};
use local_mixing::infra::circuit::{CircuitSeq, Permutation};
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize, Debug)]
struct ExportGate(u8, u8, u8);

#[derive(Deserialize, Debug)]
struct ExportEntry {
    perm: Vec<usize>,
    circuits: Vec<Vec<ExportGate>>,
}

#[derive(Deserialize, Debug)]
struct ExportData {
    n: usize,
    m: usize,
    entries: Vec<ExportEntry>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: import_rainbow <json_file> <lmdb_path>");
        std::process::exit(1);
    }

    let json_path = &args[1];
    let lmdb_path = &args[2];

    println!("Reading JSON from {}", json_path);
    let mut file = File::open(json_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let data: ExportData = serde_json::from_str(&contents)?;

    println!(
        "Loaded n={}, m={}, entries={}",
        data.n,
        data.m,
        data.entries.len()
    );

    let env = lmdb::Environment::new()
        .set_max_dbs(20)
        .set_map_size(10 * 1024 * 1024 * 1024) // 10GB
        .open(Path::new(lmdb_path))?;

    let n = data.n;
    let m = data.m;

    let perm_db_name = format!("perm_tables_n{}", n);
    let db_nq_name = format!("n{}m{}", n, m);
    let db_perms_name = format!("n{}m{}perms", n, m);

    let perm_db = env.create_db(Some(&perm_db_name), lmdb::DatabaseFlags::empty())?;
    let db_nq = env.create_db(Some(&db_nq_name), lmdb::DatabaseFlags::empty())?;
    let db_perms = env.create_db(Some(&db_perms_name), lmdb::DatabaseFlags::empty())?;

    let mut txn = env.begin_rw_txn()?;

    let id_shuf = (0..n).collect::<Vec<usize>>();
    let id_shuf_blob: Vec<u8> = id_shuf.iter().map(|&x| x as u8).collect();

    let mut count = 0;

    for entry in data.entries {
        let perm = Permutation::new(entry.perm);
        let perm_blob = perm.repr_blob();

        let ms = vec![m as u8];
        let ms_blob = bincode::serialize(&ms)?;

        txn.put(perm_db, &perm_blob, &ms_blob, WriteFlags::empty())?;

        for ex_circuit in entry.circuits {
            let mut gates = Vec::with_capacity(ex_circuit.len());
            for g in ex_circuit {
                gates.push([g.0, g.1, g.2]);
            }
            let circuit = CircuitSeq { gates };
            let circuit_blob = circuit.repr_blob();

            let mut key_nq = perm_blob.clone();
            key_nq.extend_from_slice(&circuit_blob);
            txn.put(db_nq, &key_nq, &[], WriteFlags::empty())?;

            let mut val_perms = perm_blob.clone();
            val_perms.extend_from_slice(&id_shuf_blob);
            txn.put(db_perms, &circuit_blob, &val_perms, WriteFlags::empty())?;

            count += 1;
        }
    }

    txn.commit()?;

    println!("Imported {} circuits into LMDB.", count);
    Ok(())
}
