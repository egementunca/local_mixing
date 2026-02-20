use crate::{
    infra::circuit::circuit::{CircuitSeq, Permutation},
    infra::ids_index::{self, IDS_REV_DB, IDS_WIT_PREFILTER_DB},
    infra::sat_witness,
    infra::rainbow::canonical::Canonicalization,
    infra::random::random_data::{
        contiguous_convex, find_convex_subcircuit, get_canonical, random_circuit,
        simple_find_convex_subcircuit, shoot_left_vec, targeted_convex_subcircuit,
    },
};

use crate::config::ObfuscationConfig;

use crate::infra::store::reader::TemplateDB;
use itertools::Itertools;
use rand::{Rng, seq::{SliceRandom, IndexedRandom}};

use rusqlite::{Connection, Statement};

use lmdb::{Cursor, Database, RoCursor, RoTransaction, Transaction};

use libc::{c_uint, fcntl, F_GETFL, F_SETFL, O_NONBLOCK};
extern crate lmdb_sys;
use lmdb_sys as ffi;

use std::{
    cmp::{max, min},
    collections::{HashMap, HashSet},
    // fs::OpenOptions, // used for testing
    io::{self, Read},
    // sync::Arc,
    marker::PhantomData,
    os::unix::io::AsRawFd,
    ptr,
    slice,
    time::Instant,
};

use std::sync::atomic::{AtomicU64, Ordering};

pub struct Iter<'txn> {
    cursor: *mut ffi::MDB_cursor,
    op: c_uint,
    next_op: c_uint,
    finished: bool,
    _marker: PhantomData<&'txn ()>,
}

impl<'txn> Iter<'txn> {
    pub fn new(cursor: *mut ffi::MDB_cursor, op: c_uint, next_op: c_uint) -> Self {
        Self {
            cursor,
            op,
            next_op,
            finished: false,
            _marker: PhantomData,
        }
    }
}

impl<'txn> Iterator for Iter<'txn> {
    type Item = (&'txn [u8], &'txn [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        unsafe {
            let mut key = ffi::MDB_val {
                mv_size: 0,
                mv_data: ptr::null_mut(),
            };
            let mut data = ffi::MDB_val {
                mv_size: 0,
                mv_data: ptr::null_mut(),
            };

            let rc = ffi::mdb_cursor_get(self.cursor, &mut key, &mut data, self.op);
            self.op = self.next_op;

            if rc == ffi::MDB_NOTFOUND {
                self.finished = true;
                return None;
            } else if rc != ffi::MDB_SUCCESS {
                panic!("LMDB error: {}", rc);
            }

            let key_slice = slice::from_raw_parts(key.mv_data as *const u8, key.mv_size);
            let data_slice = slice::from_raw_parts(data.mv_data as *const u8, data.mv_size);
            Some((key_slice, data_slice))
        }
    }
}

pub trait RoCursorExt<'txn> {
    fn iter_from_safe<K>(&mut self, key: K) -> Iter<'txn>
    where
        K: AsRef<[u8]>;
}

impl<'txn> RoCursorExt<'txn> for RoCursor<'txn> {
    fn iter_from_safe<K>(&mut self, key: K) -> Iter<'txn>
    where
        K: AsRef<[u8]>,
    {
        let rc = unsafe {
            let mut key_val = lmdb_sys::MDB_val {
                mv_size: key.as_ref().len(),
                mv_data: key.as_ref().as_ptr() as *mut _,
            };
            lmdb_sys::mdb_cursor_get(
                self.cursor(),
                &mut key_val,
                std::ptr::null_mut(),
                lmdb_sys::MDB_SET_RANGE,
            )
        };

        if rc == lmdb_sys::MDB_NOTFOUND {
            Iter {
                cursor: self.cursor(),
                op: lmdb_sys::MDB_GET_CURRENT,
                next_op: lmdb_sys::MDB_NEXT,
                finished: true,
                _marker: std::marker::PhantomData,
            }
        } else if rc != lmdb_sys::MDB_SUCCESS {
            panic!("LMDB error: {}", rc);
        } else {
            Iter::new(self.cursor(), lmdb_sys::MDB_GET_CURRENT, lmdb_sys::MDB_NEXT)
        }
    }
}

fn random_perm_from_perm_table(txn: &RoTransaction, db: Database) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut cursor = txn.open_ro_cursor(db).ok()?;
    let mut entries = Vec::new();

    for (k, v) in cursor.iter() {
        entries.push((k.to_vec(), v.to_vec()));
    }

    if entries.is_empty() {
        return None;
    }

    let idx = rand::rng().random_range(0..entries.len());
    Some(entries.swap_remove(idx))
}

// Helper function to make stdin non-blocking for interactive debugging
// Ported from many_thread branch for RAC integration
fn make_stdin_nonblocking() {
    let fd = io::stdin().as_raw_fd();
    unsafe {
        let flags = fcntl(fd, F_GETFL);
        fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    }
}

// Get a random identity circuit from LMDB database matching gate pair taxonomy
// Ported from many_thread branch for RAC integration
fn get_random_identity(
    n: usize,
    gate_pair: GatePair,
    env: &lmdb::Environment,
    dbs: &HashMap<String, lmdb::Database>,
) -> Result<CircuitSeq, Box<dyn std::error::Error>> {
    let db_name = format!("ids_n{}", n);
    let db = match dbs.get(&db_name) {
        Some(db) => *db,
        None => panic!("No db {}", db_name),
    };

    let txn = env.begin_ro_txn()?;

    // Serialize the gate_pair to use as the key
    let key_bytes = bincode::serialize(&gate_pair)
        .unwrap_or_else(|e| panic!("Failed to serialize gate pair: {}", e));

    // Lookup the circuits
    let value_bytes = txn.get(db, &key_bytes)?;

    let circuits: Vec<Vec<u8>> = bincode::deserialize(value_bytes)
        .unwrap_or_else(|e| panic!("Failed to deserialize circuit list: {}", e));

    let mut rng = rand::rng();
    let blob = circuits
        .choose(&mut rng)
        .expect("Failed to choose a random circuit");

    Ok(CircuitSeq::from_blob(blob))
}

// Returns a nontrivial identity circuit built from two "friend" circuits
pub fn random_canonical_id(
    env: &lmdb::Environment,
    _conn: &Connection,
    n: usize,
) -> Result<CircuitSeq, Box<dyn std::error::Error>> {
    let mut rng = rand::rng();

    loop {
        let perm_db_name = format!("perm_tables_n{}", n);
        let perm_db = match env.open_db(Some(&perm_db_name)) {
            Ok(db) => db,
            Err(_) => return Err(format!("LMDB DB '{}' not found", perm_db_name).into()),
        };
        let (perm_blob, ms_blob) = {
            let txn = match env.begin_ro_txn() {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(
                        format!("Failed to begin RO txn on '{}': {:?}", perm_db_name, e).into(),
                    );
                }
            };
            match random_perm_from_perm_table(&txn, perm_db) {
                Some(x) => x,
                None => return Err(format!("perm_tables_n{} is empty or malformed", n).into()),
            }
        };

        let mut ms: Vec<u8> = bincode::deserialize(&ms_blob)
            .unwrap_or_else(|_| panic!("Failed to deserialize ms_blob for n={}", n));

        ms.retain(|&x| x != 0);

        if ms.len() < 2 {
            panic!("ms.len() < 2 for perm in perm_tables_n{}", n);
        }

        // println!("perm: {:?}", Permutation::from_blob(&perm_blob));
        // println!("ms: {:?}", ms);

        let i = rng.random_range(0..ms.len());
        let mut j = rng.random_range(0..ms.len());
        while j == i {
            j = rng.random_range(0..ms.len());
        }
        let m1 = ms[i];
        let m2 = ms[j];

        let db1_name = format!("n{}m{}", n, m1);
        let db2_name = format!("n{}m{}", n, m2);

        // println!("Searching for perm_len {} in {}", perm_blob.len().trailing_zeros(), db1_name);

        let circuit1_blob = {
            let db1 = env
                .open_db(Some(&db1_name))
                .unwrap_or_else(|e| panic!("LMDB DB1 '{}' failed to open: {:?}", db1_name, e));
            let txn = env
                .begin_ro_txn()
                .unwrap_or_else(|e| panic!("Failed to begin RO txn on '{}': {:?}", db1_name, e));
            random_perm_lmdb(&txn, db1, &perm_blob)
                .unwrap_or_else(|| panic!("perm not found in {}", db1_name))
        };
        let mut ca = CircuitSeq::from_blob(&circuit1_blob);

        let circuit2_blob = {
            let db2 = env
                .open_db(Some(&db2_name))
                .unwrap_or_else(|e| panic!("LMDB DB2 '{}' failed to open: {:?}", db2_name, e));
            let txn = env
                .begin_ro_txn()
                .unwrap_or_else(|e| panic!("Failed to begin RO txn on '{}': {:?}", db2_name, e));
            random_perm_lmdb(&txn, db2, &perm_blob)
                .unwrap_or_else(|| panic!("perm not found in {}", db2_name))
        };
        let mut cb = CircuitSeq::from_blob(&circuit2_blob);

        cb.gates.reverse();
        ca.gates.extend(cb.gates);

        let mut shuf: Vec<usize> = (0..n).collect();
        shuf.shuffle(&mut rng);

        let bit_shuf = Permutation { data: shuf };
        ca.rewire(&bit_shuf, n);
        return Ok(ca);
    }
}

// To just get a completely random circuit and reverse for identity, rather than using canonical ones from our rainbow table
pub fn random_id(n: u8, m: usize) -> (CircuitSeq, CircuitSeq) {
    let circuit = random_circuit(n, m);

    // Preallocate reversed gates so we don't need to run through circuit twice
    let mut rev_gates = Vec::with_capacity(circuit.gates.len());
    for g in circuit.gates.iter().rev() {
        rev_gates.push(*g); // copy [u8;3]
    }

    let rev = CircuitSeq { gates: rev_gates };
    (circuit, rev)
}

// Return a random subcircuit, its starting index (gate), and ending index
pub fn random_subcircuit(circuit: &CircuitSeq) -> (CircuitSeq, usize, usize) {
    let len = circuit.gates.len();

    if circuit.gates.len() == 0 {
        return (CircuitSeq { gates: Vec::new() }, 0, 0);
    }

    let mut rng = rand::rng();
    //get size with more bias to lower length subcircuits
    let a = rng.random_range(0..len);

    // pick one of 1, 2, 4, 8
    let shift = rng.random_range(0..4);
    let upper = 1 << shift;

    let mut b = (a + (1 + rng.random_range(0..upper))) as usize;

    if b > len {
        b = len;
    }

    if a == b {
        if b < len - 1 {
            b += 1;
        } else {
            b -= 1;
        }
    }

    let start = min(a, b);
    let end = max(a, b);

    let subcircuit = circuit.gates[start..end].to_vec();

    (CircuitSeq { gates: subcircuit }, start, end)
}

pub fn random_subcircuit_max(circuit: &CircuitSeq, max_len: usize) -> (CircuitSeq, usize, usize) {
    let len = circuit.gates.len();
    if len == 0 {
        return (CircuitSeq { gates: Vec::new() }, 0, 0);
    }

    let mut rng = rand::rng();

    let start = rng.random_range(0..len);

    let remaining = len - start;
    let allowed_len = remaining.min(max_len);

    let shift = rng.random_range(0..4); // 0..3
    let mut sub_len = 1 << shift; // 1,2,4,8
    if sub_len > allowed_len {
        sub_len = allowed_len;
    }

    sub_len = sub_len.max(1);

    let end = start + sub_len;
    let subcircuit = circuit.gates[start..end].to_vec();

    (CircuitSeq { gates: subcircuit }, start, end)
}

static PERMUTATION_TIME: AtomicU64 = AtomicU64::new(0);
static SQL_TIME: AtomicU64 = AtomicU64::new(0);
static CANON_TIME: AtomicU64 = AtomicU64::new(0);
static CONVEX_FIND_TIME: AtomicU64 = AtomicU64::new(0);
static CONTIGUOUS_TIME: AtomicU64 = AtomicU64::new(0);
static REWIRE_TIME: AtomicU64 = AtomicU64::new(0);
static COMPRESS_TIME: AtomicU64 = AtomicU64::new(0);
static UNREWIRE_TIME: AtomicU64 = AtomicU64::new(0);
static REPLACE_TIME: AtomicU64 = AtomicU64::new(0);
static DEDUP_TIME: AtomicU64 = AtomicU64::new(0);
static PICK_SUBCIRCUIT_TIME: AtomicU64 = AtomicU64::new(0);
static CANONICALIZE_TIME: AtomicU64 = AtomicU64::new(0);
static ROW_FETCH_TIME: AtomicU64 = AtomicU64::new(0);
static SROW_FETCH_TIME: AtomicU64 = AtomicU64::new(0);
static SIXROW_FETCH_TIME: AtomicU64 = AtomicU64::new(0);
static LROW_FETCH_TIME: AtomicU64 = AtomicU64::new(0);
static DB_OPEN_TIME: AtomicU64 = AtomicU64::new(0);
static TXN_TIME: AtomicU64 = AtomicU64::new(0);
static LMDB_LOOKUP_TIME: AtomicU64 = AtomicU64::new(0);
static FROM_BLOB_TIME: AtomicU64 = AtomicU64::new(0);
static SPLICE_TIME: AtomicU64 = AtomicU64::new(0);
static TRIAL_TIME: AtomicU64 = AtomicU64::new(0);

pub fn compress(
    c: &CircuitSeq,
    trials: usize,
    conn: &mut Connection,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
) -> CircuitSeq {
    let id = Permutation::id_perm(n);

    // let t0 = Instant::now();
    let c_perm = c.permutation(n);
    // PERMUTATION_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

    if c_perm == id {
        return CircuitSeq { gates: Vec::new() };
    }

    let mut compressed = c.clone();
    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    let mut i = 0;
    while i < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[i] == compressed.gates[i + 1] {
            compressed.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    for _ in 0..trials {
        let (mut subcircuit, start, end) = random_subcircuit(&compressed);
        subcircuit.canonicalize();

        let max = if n == 7 {
            4
        } else if n == 5 || n == 6 {
            5
        } else if n == 4 {
            6
        } else {
            12
        };

        let sub_m = subcircuit.gates.len();
        let min = min(sub_m, max);

        let (canon_perm_blob, canon_shuf_blob) = if subcircuit.gates.len() <= max && n == 7 {
            let table = format!("n{}m{}", n, min);
            let query = format!(
                "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
                table
            );

            // let sql_t0 = Instant::now();
            let mut stmt = match conn.prepare(&query) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let rows = stmt.query([&subcircuit.repr_blob()]);
            // SQL_TIME.fetch_add(sql_t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

            let mut r = match rows {
                Ok(r) => r,
                Err(_) => continue,
            };

            if let Some(row_result) = r.next().unwrap() {
                (
                    row_result.get(0).expect("Failed to get blob"),
                    row_result.get(1).expect("Failed to get blob"),
                )
            } else {
                continue;
            }
        } else {
            // let t1 = Instant::now();
            let sub_perm = subcircuit.permutation(n);
            // PERMUTATION_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);

            // let t2 = Instant::now();
            let canon_perm = get_canonical(&sub_perm, bit_shuf);
            // CANON_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);

            (canon_perm.perm.repr_blob(), canon_perm.shuffle.repr_blob())
        };

        for smaller_m in 1..=sub_m {
            let table = format!("n{}m{}", n, smaller_m);
            let query = format!(
                "SELECT * FROM {} WHERE perm = ?1 ORDER BY RANDOM() LIMIT 1",
                table
            );

            // let sql_t0 = Instant::now();
            let mut stmt = match conn.prepare(&query) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let rows = stmt.query([&canon_perm_blob]);
            // SQL_TIME.fetch_add(sql_t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

            let mut r = match rows {
                Ok(r) => r,
                Err(_) => continue,
            };

            if let Some(row_result) = r.next().unwrap() {
                let blob: Vec<u8> = row_result.get(0).expect("Failed to get blob");
                let mut repl = CircuitSeq::from_blob(&blob);

                let repl_perm: Vec<u8> = row_result.get(1).expect("Failed to get blob");

                let repl_shuf: Vec<u8> = row_result.get(2).expect("Failed to get blob");

                if repl.gates.len() <= subcircuit.gates.len() {
                    let rc = Canonicalization {
                        perm: Permutation::from_blob(&repl_perm),
                        shuffle: Permutation::from_blob(&repl_shuf),
                    };

                    if !rc.shuffle.data.is_empty() {
                        repl.rewire(&rc.shuffle, n);
                    }

                    repl.rewire(&Permutation::from_blob(&canon_shuf_blob).invert(), n);

                    compressed.gates.splice(start..end, repl.gates);
                    break;
                }
            }
        }
    }

    let mut j = 0;
    while j < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[j] == compressed.gates[j + 1] {
            compressed.gates.drain(j..=j + 1);
            j = j.saturating_sub(2);
        } else {
            j += 1;
        }
    }

    compressed
}

pub fn expand_lmdb<'a>(
    c: &CircuitSeq,
    trials: usize,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
    env: &lmdb::Environment,
    _old_n: usize,
    dbs: &HashMap<String, lmdb::Database>,
    prepared_stmt: &mut rusqlite::Statement<'a>,
    prepared_stmt2: &mut rusqlite::Statement<'a>,
    conn: &Connection,
) -> CircuitSeq {
    let mut compressed = c.clone();
    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }
    let perm_len = 1 << n;
    for _ in 0..trials {
        let (mut subcircuit, start, end) = random_subcircuit(&compressed);
        subcircuit.canonicalize();

        let max = if n == 7 {
            4
        } else if n == 5 || n == 6 {
            5
        } else if n == 4 {
            6
        } else {
            10
        };

        let sub_m = subcircuit.gates.len();
        let (canon_perm_blob, canon_shuf_blob) = if sub_m <= max
            && ((n == 6 && sub_m == 5) || (n == 7 && sub_m == 4))
        {
            if n == 7 && sub_m == 4 {
                let stmt: &mut Statement<'_> = &mut *prepared_stmt;

                let row_start = Instant::now();
                let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> = stmt
                    .query_row([&subcircuit.repr_blob()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    });

                SROW_FETCH_TIME.fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                match blobs_result {
                    Ok(b) => b,
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => panic!("SQL query failed: {:?}", e),
                }
            } else if n == 6 && sub_m == 5 {
                let stmt: &mut Statement<'_> = &mut *prepared_stmt2;

                let row_start = Instant::now();
                let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> = stmt
                    .query_row([&subcircuit.repr_blob()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    });

                SIXROW_FETCH_TIME
                    .fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                match blobs_result {
                    Ok(b) => b,
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => panic!("SQL query failed: {:?}", e),
                }
            } else {
                let table = format!("n{}m{}", n, sub_m);
                let query = format!(
                    "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
                    table
                );

                let row_start = Instant::now();
                let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> =
                    conn.query_row(&query, [&subcircuit.repr_blob()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    });

                ROW_FETCH_TIME.fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                match blobs_result {
                    Ok(b) => b,
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => panic!("SQL query failed: {:?}", e),
                }
            }
        } else if sub_m <= max && (n >= 4) {
            let db_name = format!("n{}m{}perms", n, sub_m);
            let db = match dbs.get(&db_name) {
                Some(db) => *db,
                None => continue,
            };

            let txn = env.begin_ro_txn().expect("lmdb ro txn");

            let row_start = Instant::now();
            let val = match txn.get(db, &subcircuit.repr_blob()) {
                Ok(v) => v,
                Err(lmdb::Error::NotFound) => continue,
                Err(e) => panic!("LMDB get failed: {:?}", e),
            };
            LROW_FETCH_TIME.fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            let perm = val[..perm_len].to_vec();
            let shuf = val[perm_len..].to_vec();

            (perm, shuf)
        } else {
            // let t1 = Instant::now();
            let sub_perm = subcircuit.permutation(n);
            // PERMUTATION_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);

            // let t2 = Instant::now();
            let canon_perm = get_canonical(&sub_perm, bit_shuf);
            // CANON_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);

            (canon_perm.perm.repr_blob(), canon_perm.shuffle.repr_blob())
        };

        let prefix = canon_perm_blob.as_slice();
        for smaller_m in (1..=max).rev() {
            let db_name = format!("n{}m{}", n, smaller_m);
            let &db = match dbs.get(&db_name) {
                Some(db) => db,
                None => continue,
            };
            let mut invert = false;
            let hit = {
                let txn = env.begin_ro_txn().expect("txn");

                // let t0 = Instant::now();

                let mut res = random_perm_lmdb(&txn, db, prefix);
                if res.is_none() {
                    let prefix_inv_blob = Permutation::from_blob(&prefix).invert().repr_blob();
                    invert = true;
                    res = random_perm_lmdb(&txn, db, &prefix_inv_blob);
                }

                // SQL_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

                res.map(|val_blob| val_blob)
            };

            if let Some(val_blob) = hit {
                let repl_blob: Vec<u8> = val_blob;

                let mut repl = CircuitSeq::from_blob(&repl_blob);

                if invert {
                    repl.gates.reverse();
                }

                repl.rewire(&Permutation::from_blob(&canon_shuf_blob).invert(), n);

                if repl.gates.len() == end - start {
                    compressed.gates[start..end].copy_from_slice(&repl.gates);
                } else {
                    compressed.gates.splice(start..end, repl.gates);
                }
                break;
            }
        }
    }

    compressed
}

pub fn compress_exhaust(
    c: &CircuitSeq,
    conn: &mut Connection,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
) -> CircuitSeq {
    let id = Permutation::id_perm(n);

    if c.permutation(n) == id {
        return CircuitSeq { gates: Vec::new() };
    }

    let mut compressed = c.clone();
    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    // Initial cleanup of consecutive duplicates
    let mut i = 0;
    while i < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[i] == compressed.gates[i + 1] {
            compressed.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    let mut changed = true;
    let mut seen_positions: HashSet<(usize, usize)> = HashSet::new(); // Track replaced positions globally

    while changed {
        changed = false;
        let len = compressed.gates.len();

        'outer: for start in 0..len - 2 {
            for end in (start + 2)..len {
                // skip length 1
                if seen_positions.contains(&(start, end)) {
                    continue; // skip positions already replaced in this pass
                }
                let subcircuit = CircuitSeq {
                    gates: compressed.gates[start..end].to_vec(),
                };

                let sub_perm = subcircuit.permutation(n);
                let canon_perm = get_canonical(&sub_perm, bit_shuf);
                let sub_blob = canon_perm.perm.repr_blob();

                let sub_m = subcircuit.gates.len();

                for smaller_m in 1..=sub_m {
                    let table = format!("n{}m{}", n, smaller_m);
                    let query = format!(
                        "SELECT circuit FROM {} WHERE perm = ?1 ORDER BY RANDOM() LIMIT 1",
                        table
                    );

                    let mut stmt = match conn.prepare(&query) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let rows = stmt.query([&sub_blob]);

                    if let Ok(mut r) = rows {
                        if let Some(row) = r.next().unwrap() {
                            let blob: Vec<u8> = row.get(0).expect("Failed to get blob");
                            let mut repl = CircuitSeq::from_blob(&blob);

                            if repl.gates.len() <= subcircuit.gates.len() {
                                let repl_perm = repl.permutation(n);
                                let rc = get_canonical(&repl_perm, bit_shuf);

                                if !rc.shuffle.data.is_empty() {
                                    repl.rewire(&rc.shuffle, n);
                                }
                                repl.rewire(&canon_perm.shuffle.invert(), n);

                                if repl.permutation(n) != sub_perm {
                                    panic!("Replacement permutation mismatch!");
                                }

                                // Only perform replacement if it actually changes the gates
                                if repl.gates != subcircuit.gates {
                                    let old_len = end - start;
                                    let repl_len = repl.gates.len();
                                    let delta = repl_len as isize - old_len as isize; // ≤ 0 always
                                    let r_len = repl.gates.len();
                                    compressed.gates.splice(start..end, repl.gates);

                                    if r_len < subcircuit.gates.len() {
                                        // Update seen_positions
                                        let mut updated = HashSet::new();

                                        for &(a, b) in &seen_positions {
                                            // If it overlaps the replaced region, discard it
                                            if !(b <= start || a >= end) {
                                                continue;
                                            }

                                            // If it comes after the replaced region, shift back
                                            if a >= end {
                                                let new_a = (a as isize + delta) as usize;
                                                let new_b = (b as isize + delta) as usize;
                                                if new_a < new_b {
                                                    updated.insert((new_a, new_b));
                                                }
                                            } else {
                                                // Unaffected before the replacement
                                                updated.insert((a, b));
                                            }
                                        }

                                        seen_positions = updated;
                                    }

                                    // Mark the new replaced range
                                    seen_positions.insert((start, end));

                                    changed = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Final cleanup of consecutive duplicates
    let mut i = 0;
    while i < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[i] == compressed.gates[i + 1] {
            compressed.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    compressed
}

pub fn compress_big(
    c: &CircuitSeq,
    trials: usize,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    if config.skip_compression {
        return c.clone();
    }
    let table_7m4 = format!("n{}m{}", 7, 4);
    let query_limit_7m4 = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table_7m4
    );
    let mut stmt_7m4 = conn.prepare(&query_limit_7m4).ok();
    let table_6m5 = format!("n{}m{}", 6, 5);
    let query_limit_6m5 = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table_6m5
    );
    let mut stmt_6m5 = conn.prepare(&query_limit_6m5).ok();
    let mut circuit = c.clone();
    let mut rng = rand::rng();

    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    if circuit.gates.is_empty() {
        return circuit;
    }

    for _ in 0..trials {
        let t0 = Instant::now();
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(5..=7);
        let size = if random_max_wires == 7 {
            6
        } else if random_max_wires == 6 {
            4
        } else {
            3
        };
        for set_size in (3..=size).rev() {
            let (gates, _) =
                find_convex_subcircuit(set_size, random_max_wires, num_wires, &circuit, &mut rng);
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
        }
        CONVEX_FIND_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if subcircuit_gates.is_empty() {
            continue;
        }

        let gates: Vec<[u8; 3]> = subcircuit_gates.iter().map(|&g| circuit.gates[g]).collect();
        subcircuit_gates.sort();

        let t1 = Instant::now();
        let (start, end) =
            contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires).unwrap();
        CONTIGUOUS_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let mut subcircuit = CircuitSeq { gates };

        let expected_slice: Vec<_> = subcircuit_gates.iter().map(|&i| circuit.gates[i]).collect();
        let actual_slice = &circuit.gates[start..=end];
        if actual_slice != &expected_slice[..] {
            continue;
        }

        let t2 = Instant::now();
        let used_wires = subcircuit.used_wires();
        subcircuit =
            CircuitSeq::rewire_subcircuit(&mut circuit, &mut subcircuit_gates, &used_wires);
        REWIRE_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let sub_num_wires = used_wires.len();
        if let Some(&ids_rev_db) = dbs.get(IDS_REV_DB) {
            if ids_index::ids_rev_contains(env, ids_rev_db, &subcircuit.gates) {
                subcircuit.gates.clear();
            } else if let Some(&ids_wit_db) = dbs.get(IDS_WIT_PREFILTER_DB) {
                if ids_index::ids_prefilter_hit(env, ids_wit_db, &subcircuit.gates, &[2, 3]) {
                    ids_index::remove_identity_window(&mut subcircuit, env, ids_rev_db, 7);
                }
            }
        }
        if let Some(db) = sat_witness::get_sat_witness_db() {
            let width = sub_num_wires as u8;
            if sat_witness::template_contains(db, &subcircuit.gates, width) {
                subcircuit.gates.clear();
            } else if sat_witness::prefilter_hit(db, &subcircuit.gates, width, &[2, 3]) {
                sat_witness::remove_identity_window(&mut subcircuit, db, width, 7);
            }
        }

        let t3 = Instant::now();
        let bit_shuf = &bit_shuf_list[sub_num_wires - 3];
        PERMUTATION_TIME.fetch_add(t3.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let t4 = Instant::now();
        let subcircuit_temp = compress_lmdb(
            &subcircuit,
            20,
            bit_shuf,
            sub_num_wires,
            env,
            dbs,
            stmt_7m4.as_mut(),
            stmt_6m5.as_mut(),
            conn,
        );
        COMPRESS_TIME.fetch_add(t4.elapsed().as_nanos() as u64, Ordering::Relaxed);

        subcircuit = subcircuit_temp;

        let t5 = Instant::now();
        subcircuit = CircuitSeq::unrewire_subcircuit(&subcircuit, &used_wires);
        UNREWIRE_TIME.fetch_add(t5.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let t6 = Instant::now();
        let repl_len = subcircuit.gates.len();
        let old_len = end - start + 1;

        if repl_len == old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
        } else if repl_len < old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
            for i in (end + 1)..circuit.gates.len() {
                circuit.gates[i - (old_len - repl_len)] = circuit.gates[i];
            }
            circuit
                .gates
                .truncate(circuit.gates.len() - (old_len - repl_len));
        } else {
            panic!("Replacement grew, which is not allowed");
        }
        REPLACE_TIME.fetch_add(t6.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    let t7 = Instant::now();
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    DEDUP_TIME.fetch_add(t7.elapsed().as_nanos() as u64, Ordering::Relaxed);

    circuit
}

fn random_perm_lmdb(txn: &RoTransaction, db: Database, prefix: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = txn.open_ro_cursor(db).ok()?;
    let mut rng = rand::rng();
    let mut chosen: Option<Vec<u8>> = None;
    let mut count = 0;

    for (key, _) in cursor.iter_from_safe(prefix) {
        if !key.starts_with(prefix) {
            break;
        }
        count += 1;
        if rng.random_range(0..count) == 0 {
            chosen = Some(key[prefix.len()..].to_vec());
        }
    }
    chosen
}

pub fn compress_lmdb<'a>(
    c: &CircuitSeq,
    trials: usize,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
    env: &lmdb::Environment,
    dbs: &HashMap<String, lmdb::Database>,
    mut prepared_stmt: Option<&mut rusqlite::Statement<'a>>,
    mut prepared_stmt2: Option<&mut rusqlite::Statement<'a>>,
    conn: &Connection,
) -> CircuitSeq {
    let id = Permutation::id_perm(n);
    let perm_len = 1 << n;
    // Timer for initial permutation
    let t0 = Instant::now();
    let c_perm = c.permutation(n);
    PERMUTATION_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

    if c_perm == id {
        return CircuitSeq { gates: Vec::new() };
    }

    let mut compressed = c.clone();
    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    // Timer for initial deduplication
    let dedup_start = Instant::now();
    let mut i = 0;
    while i < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[i] == compressed.gates[i + 1] {
            compressed.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    DEDUP_TIME.fetch_add(dedup_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

    if compressed.gates.is_empty() {
        return CircuitSeq { gates: Vec::new() };
    }

    let (do_subcircuit, trial_count) = if compressed.gates.len() < 5 {
        (false, 2)
    } else {
        (true, trials)
    };

    for _ in 0..trial_count {
        let trial_start = Instant::now();

        // Pick subcircuit
        let pick_start = Instant::now();
        let (subcircuit, start, end) = if do_subcircuit {
            random_subcircuit(&compressed)
        } else {
            (compressed.clone(), 0, compressed.gates.len())
        };
        PICK_SUBCIRCUIT_TIME.fetch_add(pick_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let mut subcircuit = subcircuit;

        // Canonicalize
        let canon_start = Instant::now();
        subcircuit.canonicalize();
        CANONICALIZE_TIME.fetch_add(canon_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let max = if n == 7 {
            4
        } else if n == 5 || n == 6 {
            5
        } else if n == 4 {
            6
        } else {
            10
        };
        let sub_m = subcircuit.gates.len();
        let min = min(sub_m, max);

        let compute_canonical = || {
            let perm_start = Instant::now();
            let sub_perm = subcircuit.permutation(n);
            PERMUTATION_TIME.fetch_add(perm_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            let canon_start = Instant::now();
            let canon_perm = get_canonical(&sub_perm, bit_shuf);
            CANON_TIME.fetch_add(canon_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            (canon_perm.perm.repr_blob(), canon_perm.shuffle.repr_blob())
        };

        let (canon_perm_blob, canon_shuf_blob) = if sub_m <= max
            && ((n == 6 && sub_m == 5) || (n == 7 && sub_m == 4))
        {
            if n == 7 && sub_m == 4 {
                if let Some(stmt_ref) = prepared_stmt.as_mut() {
                    let stmt: &mut Statement<'_> = *stmt_ref;
                    let row_start = Instant::now();
                    let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> = stmt
                        .query_row([&subcircuit.repr_blob()], |row| {
                            Ok((row.get(0)?, row.get(1)?))
                        });

                    SROW_FETCH_TIME
                        .fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                    match blobs_result {
                        Ok(b) => b,
                        Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                        Err(e) => panic!("SQL query failed: {:?}", e),
                    }
                } else {
                    compute_canonical()
                }
            } else if n == 6 && sub_m == 5 {
                if let Some(stmt_ref) = prepared_stmt2.as_mut() {
                    let stmt: &mut Statement<'_> = *stmt_ref;
                    let row_start = Instant::now();
                    let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> = stmt
                        .query_row([&subcircuit.repr_blob()], |row| {
                            Ok((row.get(0)?, row.get(1)?))
                        });

                    SIXROW_FETCH_TIME
                        .fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                    match blobs_result {
                        Ok(b) => b,
                        Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                        Err(e) => panic!("SQL query failed: {:?}", e),
                    }
                } else {
                    compute_canonical()
                }
            } else {
                let table = format!("n{}m{}", n, sub_m);
                let query = format!(
                    "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
                    table
                );
                let row_start = Instant::now();
                let blobs_result: rusqlite::Result<(Vec<u8>, Vec<u8>)> =
                    conn.query_row(&query, [&subcircuit.repr_blob()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    });

                ROW_FETCH_TIME.fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                match blobs_result {
                    Ok(b) => {
                        println!("{}", table);
                        b
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => panic!("SQL query failed: {:?}", e),
                }
            }
        } else if sub_m <= max && (n >= 4) {
            let db_name = format!("n{}m{}perms", n, min);
            if let Some(db) = dbs.get(&db_name) {
                let txn = env.begin_ro_txn().expect("lmdb ro txn");

                let row_start = Instant::now();
                match txn.get(*db, &subcircuit.repr_blob()) {
                    Ok(val) => {
                        LROW_FETCH_TIME
                            .fetch_add(row_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        let perm = val[..perm_len].to_vec();
                        let shuf = val[perm_len..].to_vec();
                        (perm, shuf)
                    }
                    Err(lmdb::Error::NotFound) => {
                        // Fallback: compute canonical directly when perm table is missing coverage.
                        compute_canonical()
                    }
                    Err(e) => panic!("LMDB get failed: {:?}", e),
                }
            } else {
                // Fallback: compute canonical directly when perm table is missing.
                compute_canonical()
            }
        } else {
            compute_canonical()
        };

        let prefix = canon_perm_blob.as_slice();

        for smaller_m in 1..=min {
            let db_open_start = Instant::now();
            let db_name = format!("n{}m{}", n, smaller_m);
            let &db = match dbs.get(&db_name) {
                Some(db) => db,
                None => continue,
            };
            DB_OPEN_TIME.fetch_add(db_open_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            let txn_start = Instant::now();
            let txn = env.begin_ro_txn().expect("txn");
            TXN_TIME.fetch_add(txn_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            let lookup_start = Instant::now();
            let mut invert = false;
            let mut res = random_perm_lmdb(&txn, db, prefix);
            if res.is_none() {
                let prefix_inv_blob = Permutation::from_blob(&prefix).invert().repr_blob();
                invert = true;
                res = random_perm_lmdb(&txn, db, &prefix_inv_blob);
            }
            LMDB_LOOKUP_TIME.fetch_add(lookup_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            if let Some(val_blob) = res {
                let from_blob_start = Instant::now();
                let mut repl = CircuitSeq::from_blob(&val_blob);
                FROM_BLOB_TIME.fetch_add(
                    from_blob_start.elapsed().as_nanos() as u64,
                    Ordering::Relaxed,
                );

                let rewire_start = Instant::now();
                if invert {
                    repl.gates.reverse();
                }
                repl.rewire(&Permutation::from_blob(&canon_shuf_blob).invert(), n);
                REWIRE_TIME.fetch_add(rewire_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                let splice_start = Instant::now();
                if repl.gates.len() == end - start {
                    compressed.gates[start..end].copy_from_slice(&repl.gates);
                } else {
                    compressed.gates.splice(start..end, repl.gates);
                }
                SPLICE_TIME.fetch_add(splice_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

                break;
            }
        }

        TRIAL_TIME.fetch_add(trial_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    // Final deduplication
    let dedup2_start = Instant::now();
    let mut j = 0;
    while j < compressed.gates.len().saturating_sub(1) {
        if compressed.gates[j] == compressed.gates[j + 1] {
            compressed.gates.drain(j..=j + 1);
            j = j.saturating_sub(2);
        } else {
            j += 1;
        }
    }
    DEDUP_TIME.fetch_add(dedup2_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

    compressed
}

pub fn expand_big(
    c: &CircuitSeq,
    trials: usize,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
) -> CircuitSeq {
    let table = format!("n{}m{}", 7, 4);
    let query_limit = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table
    );
    let mut stmt = match conn.prepare(&query_limit) {
        Ok(s) => s,
        Err(_) => return c.clone(),
    };
    let table2 = format!("n{}m{}", 6, 5);
    let query_limit = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table2
    );
    let mut stmt2 = match conn.prepare(&query_limit) {
        Ok(s) => s,
        Err(_) => return c.clone(),
    };
    let mut circuit = c.clone();
    if circuit.gates.is_empty() {
        return circuit;
    }
    let mut rng = rand::rng();

    for _i in 0..trials {
        // if i % 20 == 0 {
        //     println!("{} trials so far, {} more to go", i, trials - i);
        // }
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(3..=7);
        for set_size in (3..=7).rev() {
            let (gates, _) =
                find_convex_subcircuit(set_size, random_max_wires, num_wires, &circuit, &mut rng);
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
        }

        if subcircuit_gates.is_empty() {
            return circuit;
        }

        let mut gates: Vec<[u8; 3]> = vec![[0, 0, 0]; subcircuit_gates.len()];
        for (i, g) in subcircuit_gates.iter().enumerate() {
            gates[i] = circuit.gates[*g];
        }

        subcircuit_gates.sort();
        let (start, end) =
            contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires).unwrap();
        let mut subcircuit = CircuitSeq { gates };
        // let sub_ref = subcircuit.clone();
        let expected_slice: Vec<_> = subcircuit_gates.iter().map(|&i| circuit.gates[i]).collect();
        let actual_slice = &circuit.gates[start..=end];

        if actual_slice != &expected_slice[..] {
            break;
        }

        let mut used_wires = subcircuit.used_wires();
        let n_wires = used_wires.len();
        let max = 7;
        let new_wires = rng.random_range(n_wires..=max);

        if new_wires > n_wires {
            let mut count = n_wires;
            while count < new_wires {
                let random = rng.random_range(0..num_wires);
                if used_wires.contains(&(random as u8)) {
                    continue;
                }
                used_wires.push(random as u8);
                count += 1;
            }
        }
        used_wires.sort();
        subcircuit =
            CircuitSeq::rewire_subcircuit(&mut circuit, &mut subcircuit_gates, &used_wires);

        let bit_shuf = &bit_shuf_list[new_wires - 3];

        let subcircuit_temp = expand_lmdb(
            &subcircuit,
            10,
            &bit_shuf,
            new_wires,
            &env,
            n_wires,
            dbs,
            &mut stmt,
            &mut stmt2,
            conn,
        );
        subcircuit = subcircuit_temp;

        subcircuit = CircuitSeq::unrewire_subcircuit(&subcircuit, &used_wires);
        if subcircuit.gates.len() == end + 1 - start {
            circuit.gates[start..end + 1].copy_from_slice(&subcircuit.gates);
        } else {
            circuit.gates.splice(start..end + 1, subcircuit.gates);
        }
        // if c.permutation(num_wires).data != circuit.permutation(num_wires).data {
        //     panic!("splice changed something");
        // }
    }
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    circuit
}

pub fn obfuscate(c: &CircuitSeq, num_wires: usize) -> (CircuitSeq, Vec<usize>) {
    if c.gates.len() == 0 {
        return (CircuitSeq { gates: Vec::new() }, Vec::new());
    }
    let mut obfuscated = CircuitSeq { gates: Vec::new() };
    let mut inverse_starts = Vec::new();

    let mut rng = rand::rng();

    // for butterfly
    let (r, r_inv) = random_id(num_wires as u8, rng.random_range(3..=25));

    for gate in &c.gates {
        // Generate a random identity r ⋅ r⁻¹
        // let (r, r_inv) = random_id(num_wires as u8, rng.random_range(3..=25), seed);

        // Add r
        obfuscated.gates.extend(&r.gates);

        // Record where r⁻¹ starts
        inverse_starts.push(obfuscated.gates.len());

        // Add r⁻¹
        obfuscated.gates.extend(&r_inv.gates);

        // Now add the original gate
        obfuscated.gates.push(*gate);
    }

    // Add a final padding random identity
    //let (r0, r0_inv) = random_id(num_wires as u8, rng.random_range(3..=5), seed);
    //obfuscated.gates.extend(&r0.gates);
    obfuscated.gates.extend(&r.gates);
    inverse_starts.push(obfuscated.gates.len());
    //obfuscated.gates.extend(&r0_inv.gates);
    obfuscated.gates.extend(&r_inv.gates);

    (obfuscated, inverse_starts)
}

pub fn outward_compress(
    g: &CircuitSeq,
    r: &CircuitSeq,
    trials: usize,
    conn: &mut Connection,
    bit_shuf: &Vec<Vec<usize>>,
    n: usize,
) -> CircuitSeq {
    let mut g = g.clone();
    for gate in r.gates.iter() {
        let wrapper = CircuitSeq { gates: vec![*gate] };
        g = compress(
            &wrapper.concat(&g).concat(&wrapper),
            trials,
            conn,
            bit_shuf,
            n,
        );
    }
    g
}

pub fn compress_big_sat(
    c: &CircuitSeq,
    trials: usize,
    num_wires: usize,
    timeout: u64,
) -> CircuitSeq {
    let mut circuit = c.clone();
    let mut rng = rand::rng();

    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    for _ in 0..trials {
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(3..=6);
        for set_size in (3..=random_max_wires).rev() {
            let (gates, _) =
                find_convex_subcircuit(set_size, random_max_wires, num_wires, &circuit, &mut rng);
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
        }

        if subcircuit_gates.is_empty() {
            continue;
        }

        let gates: Vec<[u8; 3]> = subcircuit_gates.iter().map(|&g| circuit.gates[g]).collect();
        subcircuit_gates.sort();

        let (start, end) = match contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires) {
            Some(se) => se,
            None => continue,
        };

        let subcircuit = CircuitSeq { gates };

        // Used wires analysis
        let mut used_wires = subcircuit.used_wires();
        let sub_num_wires = used_wires.len();
        used_wires.sort();

        // Rewire to reduce wire count for SAT
        let subcircuit = CircuitSeq::rewire_subcircuit(
            &mut CircuitSeq {
                gates: subcircuit.gates.clone(),
            },
            &mut (0..subcircuit.gates.len()).collect::<Vec<usize>>(),
            &used_wires,
        );

        // Call SAT Optimizer
        if let Some(optimized) =
            crate::optimize::compress_sat::compress_sat_run(&subcircuit, sub_num_wires, timeout)
        {
            if optimized.gates.len() < subcircuit.gates.len() {
                // Unrewire
                let optimized = CircuitSeq::unrewire_subcircuit(&optimized, &used_wires);

                // Replace
                let repl_len = optimized.gates.len();
                let old_len = end - start + 1;

                if repl_len <= old_len {
                    // Always true if checked above, but rigorous
                    // Replace logic
                    // If size reduced, we need to shift tail
                    if repl_len < old_len {
                        circuit.gates.drain(start + repl_len..end + 1);
                    }
                    for k in 0..repl_len {
                        circuit.gates[start + k] = optimized.gates[k];
                    }
                    // No, drain logic above handles the size difference.
                    // Wait, drain(range) removes items.
                    // Indices: start..end+1 are the old gates.
                    // We want new gates to be at start..start+repl_len.
                    // If we overwrite first repl_len, then we drain start+repl_len..end+1.
                }
            }
        }
    }

    // Final cleanup
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    circuit
}

/// LMDB-first compression: tries pre-computed template lookup before falling back to SAT.
/// This is much faster when the LMDB database contains matching templates.
pub fn compress_big_sat_lmdb(
    c: &CircuitSeq,
    trials: usize,
    num_wires: usize,
    timeout: u64,
    template_db: Option<&crate::infra::store::reader::TemplateDB>,
    config: &ObfuscationConfig,
) -> CircuitSeq {
    let mut circuit = c.clone();
    let mut rng = rand::rng();

    // Stats for logging
    let mut lmdb_hits = 0usize;
    let mut sat_calls = 0usize;

    // Initial cleanup: remove adjacent identical gates
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    for _ in 0..trials {
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(3..=6);
        for set_size in (3..=random_max_wires).rev() {
            let (gates, _) =
                find_convex_subcircuit(set_size, random_max_wires, num_wires, &circuit, &mut rng);
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
        }

        if subcircuit_gates.is_empty() {
            continue;
        }

        let gates: Vec<[u8; 3]> = subcircuit_gates.iter().map(|&g| circuit.gates[g]).collect();
        subcircuit_gates.sort();

        let (start, end) = match contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires) {
            Some(se) => se,
            None => continue,
        };

        let subcircuit = CircuitSeq { gates };

        // Used wires analysis
        let mut used_wires = subcircuit.used_wires();
        let sub_num_wires = used_wires.len();
        used_wires.sort();

        // Rewire to reduce wire count
        let rewired = CircuitSeq::rewire_subcircuit(
            &mut CircuitSeq {
                gates: subcircuit.gates.clone(),
            },
            &mut (0..subcircuit.gates.len()).collect::<Vec<usize>>(),
            &used_wires,
        );

        // Try LMDB lookup first (if available)
        let optimized = if let Some(db) = template_db {
            // Compute canonical hash of the rewired subcircuit
            let hash =
                crate::optimize::compress_sat::compute_canonical_hash(&rewired, sub_num_wires);

            // Try to find a smaller equivalent in LMDB
            if let Some(record) =
                db.get_smaller_equivalent(sub_num_wires as u8, rewired.gates.len() as u16, &hash)
            {
                lmdb_hits += 1;
                Some(record.to_circuit_seq())
            } else {
                // Fall back to SAT
                sat_calls += 1;
                crate::optimize::compress_sat::compress_sat_run(&rewired, sub_num_wires, timeout)
            }
        } else {
            // No LMDB, use SAT directly
            sat_calls += 1;
            crate::optimize::compress_sat::compress_sat_run(&rewired, sub_num_wires, timeout)
        };

        if let Some(optimized) = optimized {
            if optimized.gates.len() < rewired.gates.len()
                || (config.equal_replacement_mode && optimized.gates.len() == rewired.gates.len())
            {
                // Unrewire
                let optimized = CircuitSeq::unrewire_subcircuit(&optimized, &used_wires);

                // Replace
                let repl_len = optimized.gates.len();
                let old_len = end - start + 1;

                if repl_len <= old_len {
                    if repl_len < old_len {
                        circuit.gates.drain(start + repl_len..end + 1);
                    }
                    for k in 0..repl_len {
                        circuit.gates[start + k] = optimized.gates[k];
                    }
                }
            }
        }
    }

    // Log stats if any LMDB hits
    if lmdb_hits > 0 {
        println!("LMDB hits: {}, SAT calls: {}", lmdb_hits, sat_calls);
    }

    // Final cleanup
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    circuit
}

pub fn compress_big_ancillas(
    c: &CircuitSeq,
    trials: usize,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
) -> CircuitSeq {
    let table = format!("n{}m{}", 7, 4);
    let query_limit = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table
    );
    let mut stmt = conn.prepare(&query_limit).ok();
    let table = format!("n{}m{}", 6, 5);
    let query_limit = format!(
        "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
        table
    );
    let mut stmt2 = conn.prepare(&query_limit).ok();
    let mut circuit = c.clone();
    let mut rng = rand::rng();

    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    for _ in 0..trials {
        // let t0 = Instant::now();
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(3..=7);
        for set_size in (3..=6).rev() {
            let (gates, _) =
                find_convex_subcircuit(set_size, random_max_wires, num_wires, &circuit, &mut rng);
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
        }
        // CONVEX_FIND_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if subcircuit_gates.is_empty() {
            continue;
        }

        let gates: Vec<[u8; 3]> = subcircuit_gates.iter().map(|&g| circuit.gates[g]).collect();
        subcircuit_gates.sort();

        // let t1 = Instant::now();
        let (start, end) =
            contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires).unwrap();
        // CONTIGUOUS_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let mut subcircuit = CircuitSeq { gates };

        let expected_slice: Vec<_> = subcircuit_gates.iter().map(|&i| circuit.gates[i]).collect();
        let actual_slice = &circuit.gates[start..=end];
        if actual_slice != &expected_slice[..] {
            continue;
        }

        // let t2 = Instant::now();
        let mut used_wires = subcircuit.used_wires();
        let n_wires = used_wires.len();
        let max = 7;
        let new_wires = rng.random_range(n_wires..=max);
        if new_wires > n_wires {
            let mut count = n_wires;
            while count < new_wires {
                let random = rng.random_range(0..num_wires);
                if used_wires.contains(&(random as u8)) {
                    continue;
                }
                used_wires.push(random as u8);
                count += 1;
            }
        }
        // used_wires.sort();
        subcircuit =
            CircuitSeq::rewire_subcircuit(&mut circuit, &mut subcircuit_gates, &used_wires);
        // REWIRE_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // let t3 = Instant::now();
        let sub_num_wires = used_wires.len();
        let bit_shuf = &bit_shuf_list[sub_num_wires - 3];

        // PERMUTATION_TIME.fetch_add(t3.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // let t4 = Instant::now();
        let subcircuit_temp = compress_lmdb(
            &subcircuit,
            20,
            &bit_shuf,
            sub_num_wires,
            env,
            dbs,
            stmt.as_mut(),
            stmt2.as_mut(),
            conn,
        );
        // COMPRESS_TIME.fetch_add(t4.elapsed().as_nanos() as u64, Ordering::Relaxed);

        subcircuit = subcircuit_temp;

        // let t5 = Instant::now();
        subcircuit = CircuitSeq::unrewire_subcircuit(&subcircuit, &used_wires);
        // UNREWIRE_TIME.fetch_add(t5.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // let t6 = Instant::now();
        let repl_len = subcircuit.gates.len();
        let old_len = end - start + 1;

        if repl_len == old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
        } else if repl_len < old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
            for i in (end + 1)..circuit.gates.len() {
                circuit.gates[i - (old_len - repl_len)] = circuit.gates[i];
            }
            circuit
                .gates
                .truncate(circuit.gates.len() - (old_len - repl_len));
        } else {
            panic!("Replacement grew, which is not allowed");
        }
        // REPLACE_TIME.fetch_add(t6.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    // let t7 = Instant::now();
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    // DEDUP_TIME.fetch_add(t7.elapsed().as_nanos() as u64, Ordering::Relaxed);

    circuit
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollisionType {
    OnActive,
    OnCtrl1,
    OnCtrl2,
    OnNew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GatePair {
    a: CollisionType,
    c1: CollisionType,
    c2: CollisionType,
}

impl GatePair {
    pub fn is_none(gate_pair: &Self) -> bool {
        gate_pair.a == CollisionType::OnNew
            && gate_pair.c1 == CollisionType::OnNew
            && gate_pair.c2 == CollisionType::OnNew
    }
}

pub fn get_collision_type(g1: &[u8; 3], pin: u8) -> CollisionType {
    match pin {
        x if x == g1[0] => CollisionType::OnActive,
        x if x == g1[1] => CollisionType::OnCtrl1,
        x if x == g1[2] => CollisionType::OnCtrl2,
        _ => CollisionType::OnNew,
    }
}

pub fn gate_pair_taxonomy(g1: &[u8; 3], g2: &[u8; 3]) -> GatePair {
    GatePair {
        a: get_collision_type(&g1, g2[0]),
        c1: get_collision_type(&g1, g2[1]),
        c2: get_collision_type(&g1, g2[2]),
    }
}

pub fn random_stored_id(db: &TemplateDB, width: u8) -> Option<CircuitSeq> {
    let mut rng = rand::rng();
    use rand::Rng;

    // Get valid counts from DB index to avoid blind guessing
    let valid_counts = db.get_available_gate_counts(width);

    // Filter pertinent range (e.g. 6..=40 gates) and pick random
    let candidates: Vec<u16> = valid_counts
        .into_iter()
        .filter(|&gc| gc >= 6 && gc <= 40)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Try a few times
    for _ in 0..5 {
        let idx = rng.random_range(0..candidates.len());
        let gc = candidates[idx];
        if let Some(record) = db.get_random_identity(width, gc) {
            return Some(record.to_circuit_seq());
        }
    }
    None
}

pub fn replace_pairs(
    circuit: &mut CircuitSeq,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    template_db: Option<&TemplateDB>,
) {
    println!(
        "Starting replace_pairs, circuit length: {}",
        circuit.gates.len()
    );

    let mut pairs: HashMap<GatePair, Vec<usize>> = HashMap::new();
    let gates = circuit.gates.clone();
    let m = circuit.gates.len();
    let mut replaced = 0;
    let mut to_replace: Vec<Vec<[u8; 3]>> = vec![Vec::new(); m / 2];
    if m < 2 {
        println!("Circuit too small, returning");
        return;
    }

    println!("Building taxonomy pairs...");
    let mut i = 0;
    while i + 1 < m {
        let g1 = gates[i];
        let g2 = gates[i + 1];
        let taxonomy = gate_pair_taxonomy(&g1, &g2);

        if !GatePair::is_none(&taxonomy) {
            pairs.entry(taxonomy).or_default().push(i);
        }
        i += 2;
    }
    let num_pairs: usize = pairs.values().map(|v| v.len()).sum();
    println!("Pairs collected: {}", num_pairs);

    let mut rng = rand::rng();
    let mut fail = 0;
    while !pairs.is_empty() && fail < 100 {
        let n = rng.random_range(3..=5);
        let mut id = if let Some(tdb) = template_db {
            if let Some(c) = random_stored_id(tdb, n as u8) {
                c
            } else {
                fail += 1;
                continue;
            }
        } else {
            match random_canonical_id(&env, conn, n) {
                Ok(c) => c,
                Err(_) => {
                    // println!("random_canonical_id failed {}, continuing", fail);
                    fail += 1;
                    continue;
                }
            }
        };
        // println!("Generated random canonical id of length {}", id.gates.len());

        let tax = gate_pair_taxonomy(&id.gates[0], &id.gates[1]);
        if let Some(v) = pairs.get_mut(&tax) {
            if !v.is_empty() {
                let idx = fastrand::usize(..v.len());
                let chosen = v.swap_remove(idx);
                to_replace[chosen / 2] = id.gates.clone();
                // println!("Replaced pair at index {} with new circuit", chosen);
                if v.is_empty() {
                    pairs.remove(&tax);
                }
                continue;
            }
        }

        let id_len = id.gates.len();
        let tax_rev = gate_pair_taxonomy(&id.gates[id_len - 1], &id.gates[id_len - 2]);
        if let Some(v) = pairs.get_mut(&tax_rev) {
            if !v.is_empty() {
                let idx = fastrand::usize(..v.len());
                let chosen = v.swap_remove(idx);
                id.gates.reverse();
                to_replace[chosen / 2] = id.gates.clone();
                // println!("Reversed and replaced pair at index {}", chosen);
                if v.is_empty() {
                    pairs.remove(&tax_rev);
                }
                continue;
            }
        }

        fail += 1;
        // println!("Failed to match pair, fail count: {}", fail);
    }

    println!("Applying replacements...");
    for (i, replacement) in to_replace.into_iter().enumerate().rev() {
        if replacement.is_empty() {
            continue;
        }

        // println!("Replacing at pair index {}", i);
        replaced += 1;
        let index = 2 * i;
        let (g1, g2) = (circuit.gates[index], circuit.gates[index + 1]);
        let replacement = CircuitSeq { gates: replacement };
        let mut used_wires: Vec<u8> = vec![(num_wires + 1) as u8; replacement.max_wire() + 1];

        used_wires[replacement.gates[0][0] as usize] = g1[0];
        used_wires[replacement.gates[0][1] as usize] = g1[1];
        used_wires[replacement.gates[0][2] as usize] = g1[2];

        // println!("Original wires: {:?}, used_wires initialized", used_wires);

        // println!("Gates g1: {:?} g2: {:?}", g1, g2);

        let tax = gate_pair_taxonomy(&g1, &g2);
        if tax.a == CollisionType::OnNew
            || tax.c1 == CollisionType::OnNew
            || tax.c2 == CollisionType::OnNew
        {
            // println!("Found OnNew collision, assigning new wires...");
        }

        // Assign new wires if OnNew
        let mut i = 0;
        for collision in &[tax.a, tax.c1, tax.c2] {
            if *collision == CollisionType::OnNew {
                used_wires[replacement.gates[1][i] as usize] = g2[i]
            }
            i += 1;
        }

        // Fill any remaining placeholders
        for i in 0..used_wires.len() {
            if used_wires[i] == (num_wires + 1) as u8 {
                let mut attempts = 0;
                loop {
                    let wire = rng.random_range(0..num_wires) as u8;
                    if !used_wires.contains(&wire) {
                        used_wires[i] = wire;
                        break;
                    }
                    attempts += 1;
                    if attempts > 100 {
                        // fallback or error
                        break;
                    }
                }
            }
        }

        // Check for duplicates in used_wires
        let mut sorted_wires = used_wires.clone();
        sorted_wires.sort();
        if sorted_wires.windows(2).any(|w| w[0] == w[1]) {
            println!("Collision in used_wires! Skipping replacement.");
            continue;
        }

        // println!("Final used_wires for this replacement: {:?}", used_wires);

        let final_replacement = CircuitSeq::unrewire_subcircuit(&replacement, &used_wires);

        // Verify replacement is identity
        if final_replacement
            .probably_equal(&CircuitSeq { gates: vec![] }, num_wires, 100)
            .is_err()
        {
            println!("Replacement Check Failed!");
            println!("Sorted wires: {:?}", sorted_wires);
            // panic!("Replacement is not an id");
            continue;
        }
        circuit
            .gates
            .splice(index + 1..index + 1, final_replacement.gates);

        // println!("Replacement: {:?}", CircuitSeq::unrewire_subcircuit(&replacement, &used_wires));
        // println!("Replacement applied at indices {}..{}", index, index + 1);
        // println!("Replacements so far: {}/{}", replaced, num_pairs);
    }
    println!("Replaced {}/{} pairs", replaced, num_pairs);
    // println!("Starting single gate replacements");
    // random_gate_replacements(circuit, min((num_pairs - replaced)/20 + (m/2 - num_pairs)/20, 1000), num_wires, conn, env);
    println!("Finished replace_pairs");
}

pub fn random_gate_replacements(
    c: &mut CircuitSeq,
    x: usize,
    n: usize,
    _conn: &Connection,
    env: &lmdb::Environment,
) {
    let mut rng = rand::rng();
    for _ in 0..x {
        if c.gates.is_empty() {
            break;
        }

        let i = rng.random_range(0..c.gates.len());
        let g = &c.gates[i];

        let num = rng.random_range(3..=7);
        if let Ok(mut id) = random_canonical_id(env, &_conn, num) {
            let mut used_wires = vec![g[0], g[1], g[2]];
            let mut count = 3;
            while count < num {
                let random = rng.random_range(0..n);
                if used_wires.contains(&(random as u8)) {
                    continue;
                }
                used_wires.push(random as u8);
                count += 1;
            }
            used_wires.sort();
            let rewired_g = CircuitSeq::rewire_subcircuit(&c, &vec![i], &used_wires);
            // println!("rewired_g {:?} vs len: {}", rewired_g, num);
            id.rewire_first_gate(rewired_g.gates[0], num);
            id = CircuitSeq::unrewire_subcircuit(&id, &used_wires);
            id.gates.remove(0);
            c.gates.splice(i..i + 1, id.gates);
        }
    }
}

pub fn print_compress_timers() {
    let perm = PERMUTATION_TIME.load(Ordering::Relaxed);
    let sql = SQL_TIME.load(Ordering::Relaxed);
    let canon = CANON_TIME.load(Ordering::Relaxed);
    let compress = COMPRESS_TIME.load(Ordering::Relaxed);
    let rewire = REWIRE_TIME.load(Ordering::Relaxed);
    let unrewire = UNREWIRE_TIME.load(Ordering::Relaxed);
    let convex_find = CONVEX_FIND_TIME.load(Ordering::Relaxed);
    let contiguous = CONTIGUOUS_TIME.load(Ordering::Relaxed);
    let replace = REPLACE_TIME.load(Ordering::Relaxed);
    let dedup = DEDUP_TIME.load(Ordering::Relaxed);
    let pick = PICK_SUBCIRCUIT_TIME.load(Ordering::Relaxed);
    let canonicalize = CANONICALIZE_TIME.load(Ordering::Relaxed);
    let row_fetch = ROW_FETCH_TIME.load(Ordering::Relaxed);
    let srow_fetch = SROW_FETCH_TIME.load(Ordering::Relaxed);
    let sixrow_fetch = SIXROW_FETCH_TIME.load(Ordering::Relaxed);
    let lrow_fetch = LROW_FETCH_TIME.load(Ordering::Relaxed);
    let db_open = DB_OPEN_TIME.load(Ordering::Relaxed);
    let txn = TXN_TIME.load(Ordering::Relaxed);
    let lmdb_lookup = LMDB_LOOKUP_TIME.load(Ordering::Relaxed);
    let from_blob = FROM_BLOB_TIME.load(Ordering::Relaxed);
    let splice = SPLICE_TIME.load(Ordering::Relaxed);
    let trial = TRIAL_TIME.load(Ordering::Relaxed);

    println!("--- Compression Timing Totals (minutes) ---");
    println!(
        "Permutation computation time: {:.2} min",
        perm as f64 / 60_000_000_000.0
    );
    println!("SQL lookup time: {:.2} min", sql as f64 / 60_000_000_000.0);
    println!(
        "Canonicalization time: {:.2} min",
        canon as f64 / 60_000_000_000.0
    );
    println!(
        "Compress LMDB time: {:.2} min",
        compress as f64 / 60_000_000_000.0
    );
    println!(
        "Rewire subcircuit time: {:.2} min",
        rewire as f64 / 60_000_000_000.0
    );
    println!(
        "Unrewire subcircuit time: {:.2} min",
        unrewire as f64 / 60_000_000_000.0
    );
    println!(
        "Convex subcircuit find time: {:.2} min",
        convex_find as f64 / 60_000_000_000.0
    );
    println!(
        "Contiguous convex subcircuit time: {:.2} min",
        contiguous as f64 / 60_000_000_000.0
    );
    println!(
        "Replacement time: {:.2} min",
        replace as f64 / 60_000_000_000.0
    );
    println!(
        "Deduplication time: {:.2} min",
        dedup as f64 / 60_000_000_000.0
    );
    println!(
        "Pick subcircuit time: {:.2} min",
        pick as f64 / 60_000_000_000.0
    );
    println!(
        "Subcircuit canonicalize time: {:.2} min",
        canonicalize as f64 / 60_000_000_000.0
    );
    println!(
        "SQL row fetch time: {:.2} min",
        row_fetch as f64 / 60_000_000_000.0
    );
    println!(
        "SQL n7m4 prepared row fetch time: {:.2} min",
        srow_fetch as f64 / 60_000_000_000.0
    );
    println!(
        "SQL n6m5 prepared row fetch time: {:.2} min",
        sixrow_fetch as f64 / 60_000_000_000.0
    );
    println!(
        "LMDB row fetch time: {:.2} min",
        lrow_fetch as f64 / 60_000_000_000.0
    );
    println!(
        "LMDB DB open time: {:.2} min",
        db_open as f64 / 60_000_000_000.0
    );
    println!(
        "LMDB transaction begin time: {:.2} min",
        txn as f64 / 60_000_000_000.0
    );
    println!(
        "LMDB lookup time: {:.2} min",
        lmdb_lookup as f64 / 60_000_000_000.0
    );
    println!(
        "CircuitSeq from_blob time: {:.2} min",
        from_blob as f64 / 60_000_000_000.0
    );
    println!(
        "Gate splice time: {:.2} min",
        splice as f64 / 60_000_000_000.0
    );
    println!(
        "Trial loop time: {:.2} min",
        trial as f64 / 60_000_000_000.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::time::Instant;

    fn require_file(path: &str) -> bool {
        if !Path::new(path).exists() {
            eprintln!("Skipping test: missing {}", path);
            return false;
        }
        true
    }

    fn require_dir(path: &str) -> bool {
        if !Path::new(path).is_dir() {
            eprintln!("Skipping test: missing directory {}", path);
            return false;
        }
        true
    }

    fn db_tests_enabled() -> bool {
        std::env::var("LOCAL_MIXING_DB_TESTS")
            .ok()
            .as_deref()
            == Some("1")
    }

    fn slow_tests_enabled() -> bool {
        std::env::var("LOCAL_MIXING_SLOW_TESTS")
            .ok()
            .as_deref()
            == Some("1")
    }

    #[test]
    fn test_shoot_left_vec_stops_on_collision() {
        // g2 should shoot left past g1 and stop just right of g0 (first collision).
        let mut gates = vec![[0, 1, 2], [3, 4, 5], [1, 6, 7]];
        let new_index = shoot_left_vec(&mut gates, 2);

        assert_eq!(new_index, 1);
        assert_eq!(gates, vec![[0, 1, 2], [1, 6, 7], [3, 4, 5]]);
    }

    #[test]
    fn test_replace_sequential_pairs_preserves_function() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "lmdb_ids_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time went backwards")
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).expect("Failed to create temp LMDB dir");

        let env = lmdb::Environment::new()
            .set_max_dbs(10)
            .set_map_size(10 * 1024 * 1024)
            .open(&tmp_dir)
            .expect("Failed to open temp LMDB");

        let g1 = [0, 1, 2];
        let g2 = [1, 2, 3];
        let g3 = [2, 3, 4];
        let identity = CircuitSeq {
            gates: vec![g1, g2, g3, g3, g2, g1],
        };

        let key = bincode::serialize(&gate_pair_taxonomy(&g1, &g2))
            .expect("Failed to serialize gate taxonomy");
        let value = bincode::serialize(&vec![identity.repr_blob()])
            .expect("Failed to serialize identity blob list");

        let mut dbs = HashMap::new();
        for name in ["ids_n5", "ids_n6", "ids_n7"] {
            let db = env
                .create_db(Some(name), lmdb::DatabaseFlags::empty())
                .expect("Failed to create temp LMDB db");
            let mut txn = env.begin_rw_txn().expect("Failed to begin LMDB txn");
            txn.put(db, &key, &value, lmdb::WriteFlags::empty())
                .expect("Failed to write identity to LMDB");
            txn.commit().expect("Failed to commit LMDB txn");
            dbs.insert(name.to_string(), db);
        }

        let before = CircuitSeq { gates: vec![g1, g2] };
        let mut after = before.clone();
        let mut conn = Connection::open_in_memory().expect("Failed to open in-memory SQLite");

        let (already_collided, shoot_count, curr_zero, traverse_left) =
            replace_sequential_pairs(&mut after, 5, &mut conn, &env, &Vec::new(), &dbs);

        assert_eq!((already_collided, shoot_count, curr_zero, traverse_left), (1, 0, 0, 0));
        assert_eq!(before.permutation(5), after.permutation(5));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    #[test]
    fn random_circuit_exists_in_db() {
        if !db_tests_enabled() {
            eprintln!("Skipping DB test (set LOCAL_MIXING_DB_TESTS=1)");
            return;
        }
        if !require_file("db/circuits.db") {
            return;
        }
        // Open the SQLite DB
        let conn = Connection::open("db/circuits.db").expect("Failed to open DB");

        let perms: Vec<Vec<usize>> = (0..5).permutations(5).collect();
        let bit_shuf = perms.into_iter().skip(1).collect::<Vec<_>>();

        let n = 5;
        let len = 4;

        // Generate a random circuit of length 4
        let c = random_circuit(n, len);
        println!("Random circuit: {:?}", c.gates);

        // Compute its permutation and canonical form
        let perm = c.permutation(n as usize);
        let canon = perm.canon_simple(&bit_shuf);
        let perm_blob = canon.perm.repr_blob();

        let mut found = false;

        // Check tables for lengths 1..=len
        for m in 1..=len {
            let table = format!("n{}m{}", n, m);
            let query = format!("SELECT COUNT(*) FROM {} WHERE perm = ?1", table);

            if let Ok(count) =
                conn.query_row(&query, [perm_blob.as_slice()], |row| row.get::<_, i64>(0))
            {
                if count > 0 {
                    println!("Found permutation in table {}!", table);
                    found = true;
                    break;
                }
            }
        }

        // Assert that the permutation exists in at least one table
        assert!(found, "Permutation not found in any table!");
    }
    use crate::algorithms::butterfly::mixing::open_all_dbs;
    use lmdb::Environment;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    #[test]
    fn test_compression_big_time() {
        if !slow_tests_enabled() {
            eprintln!("Skipping slow test (set LOCAL_MIXING_SLOW_TESTS=1)");
            return;
        }
        if !db_tests_enabled() {
            eprintln!("Skipping DB test (set LOCAL_MIXING_DB_TESTS=1)");
            return;
        }
        if !require_file("compressed.txt") || !require_file("db/circuits.db") || !require_dir("./db") {
            return;
        }
        // let total_start = Instant::now();

        // // ---------- FIRST TEST ----------
        // let t1_start = Instant::now();
        // let n = 64;
        // let str1 = "circuitQQF_64.txt";
        // let data1 = fs::read_to_string(str1).expect("Failed to read circuitQQF_64.txt");
        // let mut stable_count = 0;
        // let mut conn = Connection::open("db/circuits.db").expect("Failed to open DB");
        // let mut acc = CircuitSeq::from_string(&data1);
        // while stable_count < 3 {
        //     let before = acc.gates.len();
        //     acc = compress_big(&acc, 1_000, n, &mut conn);
        //     let after = acc.gates.len();

        //     if after == before {
        //         stable_count += 1;
        //         println!("  Final compression stable {}/3 at {} gates", stable_count, after);
        //     } else {
        //         println!("  Final compression reduced: {} → {} gates", before, after);
        //         stable_count = 0;
        //     }
        // }
        // let t1_duration = t1_start.elapsed();
        // println!(" First compression finished in {:.2?}", t1_duration);

        // ---------- SECOND TEST ----------
        let t2_start = Instant::now();
        let str2 = "compressed.txt";
        let lmdb = "./db";
        let _ = std::fs::create_dir_all(lmdb);

        let env = Environment::new()
            .set_max_readers(10000)
            .set_max_dbs(50)
            .set_map_size(700 * 1024 * 1024 * 1024)
            .open(Path::new(lmdb))
            .expect("Failed to open lmdb");

        let data2 = fs::read_to_string(str2).expect("Failed to read circuitF.txt");
        let mut stable_count = 0;
        let conn = Connection::open("db/circuits.db").expect("Failed to open DB");
        let mut acc = CircuitSeq::from_string(&data2);
        let bit_shuf_list = (3..=7)
            .map(|n| {
                (0..n)
                    .permutations(n)
                    .filter(|p| !p.iter().enumerate().all(|(i, &x)| i == x))
                    .collect::<Vec<Vec<usize>>>()
            })
            .collect();
        let dbs = open_all_dbs(&env);
        let mut stmts_prepared = HashMap::new();
        let mut stmts_prepared_limit1 = HashMap::new();
        let ns_and_ms = vec![(3, 10), (4, 6), (5, 5), (6, 5), (7, 4)];
        for &(n, max_m) in &ns_and_ms {
            for m in 1..=max_m {
                let table = format!("n{}m{}", n, m);
                let query = format!("SELECT perm, shuf FROM {} WHERE circuit = ?", table);
                let stmt = conn.prepare(&query).unwrap();
                stmts_prepared.insert((n, m), stmt);

                let query_limit = format!(
                    "SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1",
                    table
                );
                let stmt_limit = conn.prepare(&query_limit).unwrap();
                stmts_prepared_limit1.insert((n, m), stmt_limit);
            }
        }
        let mut conn = Connection::open("db/circuits.db").expect("Failed to open DB");
        while stable_count < 6 {
            let before = acc.gates.len();
            let config = ObfuscationConfig::default();
            acc = compress_big(
                &acc,
                1_000,
                64,
                &mut conn,
                &env,
                &bit_shuf_list,
                &dbs,
                &config,
            );
            let after = acc.gates.len();

            if after == before {
                stable_count += 1;
                println!(
                    "  Final compression stable {}/6 at {} gates",
                    stable_count, after
                );
            } else {
                println!("  Final compression reduced: {} → {} gates", before, after);
                stable_count = 0;
            }
        }

        File::create("compressed.txt")
            .and_then(|mut f| f.write_all(acc.repr().as_bytes()))
            .expect("Failed to write butterfly_recent.txt");
        let t2_duration = t2_start.elapsed();
        println!(" Second compression finished in {:.2?}", t2_duration);

        // ---------- TOTAL ----------
        // let total_duration = total_start.elapsed();
        // println!(" Total test duration: {:.2?}", total_duration);
    }

    #[test]
    fn test_random_canon_id() {
        if !db_tests_enabled() {
            eprintln!("Skipping DB test (set LOCAL_MIXING_DB_TESTS=1)");
            return;
        }
        if !require_dir("./db") || !require_file("db/circuits.db") {
            return;
        }
        let env = Environment::new()
            .set_max_readers(10000)
            .set_max_dbs(50)
            .set_map_size(700 * 1024 * 1024 * 1024)
            .open(Path::new("./db"))
            .expect("Failed to open lmdb");
        let conn = Connection::open("db/circuits.db").expect("Failed to open DB");
        let circuit = random_canonical_id(&env, &conn, 3)
            .unwrap_or_else(|_| panic!("Failed to run random_canon_id"));
        if circuit
            .probably_equal(
                &CircuitSeq {
                    gates: vec![[1, 2, 3], [1, 2, 3]],
                },
                10,
                10000,
            )
            .is_err()
        {
            panic!("Not id");
        }
        println!("circuit {:?}", circuit.gates);
    }

    #[test]
    fn print_lmdb_keys() -> Result<(), Box<dyn std::error::Error>> {
        if !db_tests_enabled() {
            eprintln!("Skipping DB test (set LOCAL_MIXING_DB_TESTS=1)");
            return Ok(());
        }
        if !require_dir("./db") {
            return Ok(());
        }
        let env_path = "./db";
        let db_name = "n6m5";

        let env = Environment::new()
            .set_max_dbs(50)
            .open(Path::new(env_path))?;

        let db = env.open_db(Some(db_name))?;

        let txn = env.begin_ro_txn()?;
        let mut cursor = txn.open_ro_cursor(db)?;
        let perm_len = 1 << 6;
        for (key, _value) in cursor.iter() {
            let circuit_blob = &key[perm_len..];
            let circuit = CircuitSeq::from_blob(&circuit_blob);
            println!("{:?}", circuit.gates);
        }

        Ok(())
    }

    #[test]
    fn test_find_perm_lmdb() {
        if !db_tests_enabled() {
            eprintln!("Skipping DB test (set LOCAL_MIXING_DB_TESTS=1)");
            return;
        }
        if !require_dir("./db") {
            return;
        }
        let perm = Permutation {
            data: vec![
                3, 2, 5, 4, 7, 6, 1, 0, 11, 10, 13, 12, 15, 14, 9, 8, 19, 18, 21, 20, 23, 22, 17,
                16, 27, 26, 29, 28, 31, 30, 25, 24, 37, 36, 35, 34, 33, 32, 39, 38, 43, 42, 45, 44,
                47, 46, 41, 40, 53, 52, 51, 50, 49, 48, 55, 54, 59, 58, 61, 60, 63, 62, 57, 56, 71,
                70, 68, 69, 67, 66, 64, 65, 79, 78, 76, 77, 75, 74, 72, 73, 87, 86, 84, 85, 83, 82,
                80, 81, 95, 94, 92, 93, 91, 90, 88, 89, 100, 101, 103, 102, 96, 97, 99, 98, 111,
                110, 108, 109, 107, 106, 104, 105, 116, 117, 119, 118, 112, 113, 115, 114, 127,
                126, 124, 125, 123, 122, 120, 121,
            ],
        };
        let prefix = perm.repr_blob();
        let env_path = "./db";
        let db_name = "n4m2";
        let env = Environment::new()
            .set_max_dbs(50)
            .open(Path::new(env_path))
            .expect("Failed to open db");
        let db = env
            .open_db(Some(&db_name))
            .unwrap_or_else(|e| panic!("LMDB DB '{}' failed to open: {:?}", db_name, e));
        let txn = env
            .begin_ro_txn()
            .unwrap_or_else(|e| panic!("Failed to begin RO txn on '{}': {:?}", "perm_db_name", e));
        let mut cursor = txn.open_ro_cursor(db).ok().expect("Failed to open cursor");
        let mut circuits = Vec::new();
        let mut count = 0;
        for (key, _) in cursor.iter() {
            if key.starts_with(&prefix) {
                circuits.push(key[prefix.len()..].to_vec());
                count += 1;
                println!("count: {}", count);
            }
        }
    }

    #[test]
    fn test_sat_integration_pipeline() {
        let path = Path::new("../sat_revsynth/data/collection.lmdb");
        if !path.exists() {
            println!("Skipping integration test: DB not found");
            return;
        }
        let db = TemplateDB::open(path).expect("Failed to open DB");

        // 1. Fetch random ID from SAT DB
        // We try width 3 since our DB currently contains width 3 data.
        if let Some(id) = random_stored_id(&db, 3) {
            println!("Fetched SAT ID len: {}", id.gates.len());

            // 2. Verify it is an identity
            // We use an empty circuit as reference for identity check?
            // existing probably_equal check:
            let empty = CircuitSeq { gates: vec![] };
            // Use smaller trials for speed
            // probably_equal returns Ok(()) if equal, Err if not.
            id.probably_equal(&empty, 3, 100)
                .expect("Fetched circuit is not identity");
        } else {
            println!("Could not fetch random identity (might need more retries or data)");
        }

        // 3. Test SAT Compression (using compress_sat module)
        // Construct [g, g] which is identity for ECA57 (gates are self-inverse).
        // [0, 1, 2] -> target 0, c1 1, c2 2.
        let g = [0, 1, 2];
        let redundant = CircuitSeq { gates: vec![g, g] };

        use crate::optimize::compress_sat::compress_sat_run;
        let optimized = compress_sat_run(&redundant, 3, 10);

        if let Some(opt) = optimized {
            println!(
                "SAT Compressed: {} -> {}",
                redundant.gates.len(),
                opt.gates.len()
            );
            assert!(
                opt.gates.is_empty(),
                "Should compress to empty for self-inverse pair"
            );
        } else {
            println!("SAT Compression return None (script failed or timeout)");
            // Check if script exists
            if !Path::new("../sat_revsynth/scripts/synthesize_from_tt.py").exists() {
                println!("Script not found, failure expected.");
            } else {
                panic!("SAT compression failed unexpectedly");
            }
        }
    }
}

// Sequential version of gate pair replacement with collision detection
// Ported from many_thread branch for RAC integration
pub fn replace_sequential_pairs(
    circuit: &mut CircuitSeq,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
) -> (usize, usize, usize, usize) {
    make_stdin_nonblocking();
    let gates = circuit.gates.clone();
    let n = gates.len();
    if n < 2 {
        println!("Circuit too small, returning");
        return (0, 0, 0, 0);
    }

    let mut already_collided = 0;
    let mut shoot_count = 0;
    let mut curr_zero = 0;
    let mut traverse_left = 0;

    let mut rng = rand::rng();
    let mut out: Vec<[u8; 3]> = Vec::new();

    // rolling state
    let mut left = gates[0];
    let mut i = 1;
    let mut fail = 0;
    while i < n {
        let right = gates[i];
        let tax = gate_pair_taxonomy(&left, &right);

        if !GatePair::is_none(&tax) {
            already_collided += 1;
            let mut produced: Option<Vec<[u8; 3]>> = None;

            while produced.is_none() && fail < 100 {
                {
                    let mut buf = [0u8; 1];
                    if let Ok(n) = io::stdin().read(&mut buf) {
                        if n > 0 && buf[0] == b'\n' {
                            println!("i = {}\n fail = {}", i, fail);
                        }
                    }
                }
                fail += 1;
                let id_len = rng.random_range(5..=7);
                let id = match get_random_identity(id_len, tax, env, dbs) {
                    Ok(id) => id,
                    Err(_) => {
                        fail += 1;
                        continue;
                    }
                };

                let new_circuit = id.gates[2..].to_vec();

                let replacement_circ = CircuitSeq {
                    gates: new_circuit,
                };

                let mut used_wires: Vec<u8> = vec![
                    (num_wires + 1) as u8;
                    std::cmp::max(
                        replacement_circ.max_wire(),
                        CircuitSeq {
                            gates: vec![id.gates[0], id.gates[1]],
                        }
                        .max_wire(),
                    ) + 1
                ];

                used_wires[id.gates[0][0] as usize] = left[0];
                used_wires[id.gates[0][1] as usize] = left[1];
                used_wires[id.gates[0][2] as usize] = left[2];

                let mut k = 0;
                for collision in &[tax.a, tax.c1, tax.c2] {
                    if *collision == CollisionType::OnNew {
                        used_wires[id.gates[1][k] as usize] = right[k];
                    }
                    k += 1;
                }

                let mut available_wires: Vec<u8> = (0..num_wires as u8)
                    .filter(|w| !used_wires.contains(w))
                    .collect();

                available_wires.shuffle(&mut rng);
                for w in 0..used_wires.len() {
                    if used_wires[w] == (num_wires + 1) as u8 {
                        if let Some(&wire) = available_wires.get(0) {
                            used_wires[w] = wire;
                            available_wires.remove(0);
                        } else {
                            panic!("No available wires left to assign!");
                        }
                    }
                }

                produced = Some(
                    CircuitSeq::unrewire_subcircuit(&replacement_circ, &used_wires)
                        .gates
                        .into_iter()
                        .rev()
                        .collect(),
                );

                if produced.is_none() {
                    fail += 1;
                }
                fail += 1;
            }

            if let Some(mut gates_out) = produced {
                out.append(&mut gates_out);
                left = out.pop().unwrap();
            } else {
                // extremely unlikely fallback
                out.push(left);
                left = right;
            }
            fail = 0;
            i += 1;
        } else {
            shoot_count += 1;
            out.push(gates[i]);
            let out_len = out.len();

            let new_index = shoot_left_vec(&mut out, out_len - 1);
            traverse_left += out_len - 1 - new_index;
            if new_index == 0 {
                // nothing to collide with so single gate replacement
                curr_zero += 1;
                let g = &out[0];
                let temp_out_circ = CircuitSeq { gates: out.clone() };
                let num = rng.random_range(3..=7);

                if let Ok(mut id) = random_canonical_id(env, &conn, num) {
                    let mut used_wires = vec![g[0], g[1], g[2]];
                    let mut count = 3;

                    while count < num {
                        let random = rng.random_range(0..num_wires);
                        if used_wires.contains(&(random as u8)) {
                            continue;
                        }
                        used_wires.push(random as u8);
                        count += 1;
                    }
                    used_wires.sort();

                    let rewired_g =
                        CircuitSeq::rewire_subcircuit(&temp_out_circ, &vec![0], &used_wires);
                    id.rewire_first_gate(rewired_g.gates[0], num);
                    id = CircuitSeq::unrewire_subcircuit(&id, &used_wires);
                    id.gates.remove(0);

                    out.splice(0..1, id.gates);
                }

                fail = 0;
                i += 1;
                continue;
            }

            // collision found, make pair replacement
            let left_gate = out[new_index - 1];
            let right_gate = out[new_index];

            let tax = gate_pair_taxonomy(&left_gate, &right_gate);

            if !GatePair::is_none(&tax) {
                let mut produced: Option<Vec<[u8; 3]>> = None;

                while produced.is_none() && fail < 100 {
                    fail += 1;
                    let id_len = rng.random_range(5..=7);
                    let id = match get_random_identity(id_len, tax, env, dbs) {
                        Ok(id) => id,
                        Err(_) => {
                            fail += 1;
                            continue;
                        }
                    };

                    let new_circuit = id.gates[2..].to_vec();

                    let replacement_circ = CircuitSeq {
                        gates: new_circuit,
                    };

                    let mut used_wires: Vec<u8> = vec![
                        (num_wires + 1) as u8;
                        std::cmp::max(
                            replacement_circ.max_wire(),
                            CircuitSeq {
                                gates: vec![id.gates[0], id.gates[1]],
                            }
                            .max_wire(),
                        ) + 1
                    ];

                    used_wires[id.gates[0][0] as usize] = left_gate[0];
                    used_wires[id.gates[0][1] as usize] = left_gate[1];
                    used_wires[id.gates[0][2] as usize] = left_gate[2];

                    let mut k = 0;
                    for collision in &[tax.a, tax.c1, tax.c2] {
                        if *collision == CollisionType::OnNew {
                            used_wires[id.gates[1][k] as usize] = right_gate[k];
                        }
                        k += 1;
                    }

                    let mut available_wires: Vec<u8> = (0..num_wires as u8)
                        .filter(|w| !used_wires.contains(w))
                        .collect();

                    available_wires.shuffle(&mut rng);
                    for w in 0..used_wires.len() {
                        if used_wires[w] == (num_wires + 1) as u8 {
                            if let Some(&wire) = available_wires.get(0) {
                                used_wires[w] = wire;
                                available_wires.remove(0);
                            } else {
                                panic!("No available wires left to assign!");
                            }
                        }
                    }

                    produced = Some(
                        CircuitSeq::unrewire_subcircuit(&replacement_circ, &used_wires)
                            .gates
                            .into_iter()
                            .rev()
                            .collect(),
                    );

                    if produced.is_none() {
                        fail += 1;
                    }
                    fail += 1;
                }

                if let Some(mut gates_out) = produced {
                    out.splice((new_index - 1)..=new_index, gates_out.drain(..));
                    fail = 0;
                    i += 1;
                } else {
                    fail = 0;
                    i += 1;
                }
            }
        }
    }

    // flush final carried gate
    out.push(left);
    let out_circ = CircuitSeq { gates: out };
    circuit.gates = out_circ.gates;

    (already_collided, shoot_count, curr_zero, traverse_left)
}

// Sequential compression using convex subcircuits
// Ported from many_thread branch for RAC integration
pub fn sequential_compress_big(
    c: &CircuitSeq,
    num_wires: usize,
    conn: &mut Connection,
    env: &lmdb::Environment,
    bit_shuf_list: &Vec<Vec<Vec<usize>>>,
    dbs: &HashMap<String, lmdb::Database>,
) -> CircuitSeq {
    let table = format!("n{}m{}", 7, 4);
    let query_limit = format!("SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1", table);
    let mut stmt = conn.prepare(&query_limit).ok();
    let table2 = format!("n{}m{}", 6, 5);
    let query_limit = format!("SELECT perm, shuf FROM {} WHERE circuit = ?1 LIMIT 1", table2);
    let mut stmt2 = conn.prepare(&query_limit).ok();
    let mut circuit = c.clone();
    let mut rng = rand::rng();

    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }

    let mut len = circuit.gates.len();
    let mut i = 0;
    while i < len {
        let t0 = Instant::now();
        let mut subcircuit_gates = vec![];
        let random_max_wires = rng.random_range(5..=7);
        let size = if random_max_wires == 7 {
            6
        } else if random_max_wires == 6 {
            4
        } else {
            3
        };
        for set_size in (3..=size).rev() {
            let (gates, _) = targeted_convex_subcircuit(
                set_size,
                random_max_wires,
                num_wires,
                &circuit,
                &mut rng,
                i,
            );
            if !gates.is_empty() {
                subcircuit_gates = gates;
                break;
            }
            if set_size == 3 {
                let (gates, _) =
                    targeted_convex_subcircuit(set_size, 7, num_wires, &circuit, &mut rng, i);
                subcircuit_gates = gates;
            }
        }
        CONVEX_FIND_TIME.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if subcircuit_gates.is_empty() {
            i += 1;
            continue;
        }

        let gates: Vec<[u8; 3]> = subcircuit_gates
            .iter()
            .map(|&g| circuit.gates[g])
            .collect();
        subcircuit_gates.sort();

        let t1 = Instant::now();
        let (start, end) = contiguous_convex(&mut circuit, &mut subcircuit_gates, num_wires).unwrap();
        CONTIGUOUS_TIME.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let mut subcircuit = CircuitSeq { gates };

        let expected_slice: Vec<_> = subcircuit_gates
            .iter()
            .map(|&i| circuit.gates[i])
            .collect();
        let actual_slice = &circuit.gates[start..=end];
        if actual_slice != &expected_slice[..] {
            i += 1;
            continue;
        }

        let t2 = Instant::now();
        let used_wires = subcircuit.used_wires();
        subcircuit = CircuitSeq::rewire_subcircuit(&mut circuit, &mut subcircuit_gates, &used_wires);
        REWIRE_TIME.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let t3 = Instant::now();
        let sub_num_wires = used_wires.len();
        let bit_shuf = &bit_shuf_list[sub_num_wires - 3];
        PERMUTATION_TIME.fetch_add(t3.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let t4 = Instant::now();
        let subcircuit_temp = compress_lmdb(
            &subcircuit,
            20,
            &bit_shuf,
            sub_num_wires,
            env,
            dbs,
            stmt.as_mut(),
            stmt2.as_mut(),
            conn,
        );
        COMPRESS_TIME.fetch_add(t4.elapsed().as_nanos() as u64, Ordering::Relaxed);

        subcircuit = subcircuit_temp;

        let t5 = Instant::now();
        subcircuit = CircuitSeq::unrewire_subcircuit(&subcircuit, &used_wires);
        UNREWIRE_TIME.fetch_add(t5.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let t6 = Instant::now();
        let repl_len = subcircuit.gates.len();
        let old_len = end - start + 1;

        if repl_len == old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
        } else if repl_len < old_len {
            for i in 0..repl_len {
                circuit.gates[start + i] = subcircuit.gates[i];
            }
            for i in (end + 1)..circuit.gates.len() {
                circuit.gates[i - (old_len - repl_len)] = circuit.gates[i];
            }
            circuit
                .gates
                .truncate(circuit.gates.len() - (old_len - repl_len));
        } else {
            panic!("Replacement grew, which is not allowed");
        }
        REPLACE_TIME.fetch_add(t6.elapsed().as_nanos() as u64, Ordering::Relaxed);
        i += 1;
        len = circuit.gates.len();
    }

    let t7 = Instant::now();
    let mut i = 0;
    while i < circuit.gates.len().saturating_sub(1) {
        if circuit.gates[i] == circuit.gates[i + 1] {
            circuit.gates.drain(i..=i + 1);
            i = i.saturating_sub(2);
        } else {
            i += 1;
        }
    }
    DEDUP_TIME.fetch_add(t7.elapsed().as_nanos() as u64, Ordering::Relaxed);

    circuit
}
