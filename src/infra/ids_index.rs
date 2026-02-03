use crate::infra::circuit::circuit::CircuitSeq;
use lmdb::{Cursor, Database, DatabaseFlags, Environment, Transaction, WriteFlags};
use std::collections::{HashMap, HashSet};
use std::error::Error;

pub const IDS_REV_DB: &str = "ids_rev";
pub const IDS_WIT_PREFILTER_DB: &str = "ids_wit_prefilter";
pub const BASIS_ECA57_ID: u8 = 1;

#[derive(Debug, Default)]
pub struct IdsIndexStats {
    pub ids_seen: usize,
    pub rev_inserted: usize,
    pub token_inserted: usize,
    pub dbs_scanned: usize,
}

pub fn canonicalize_first_occurrence(gates: &[[u8; 3]]) -> (Vec<[u8; 3]>, u8) {
    let mut map: HashMap<u8, u8> = HashMap::new();
    let mut next: u8 = 0;
    for gate in gates {
        for &wire in gate {
            if !map.contains_key(&wire) {
                map.insert(wire, next);
                next = next.saturating_add(1);
            }
        }
    }
    let canonical = gates
        .iter()
        .map(|g| [map[&g[0]], map[&g[1]], map[&g[2]]])
        .collect::<Vec<_>>();
    (canonical, next)
}

fn gates_to_blob(gates: &[[u8; 3]]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(gates.len() * 3);
    for g in gates {
        blob.push(g[0]);
        blob.push(g[1]);
        blob.push(g[2]);
    }
    blob
}

fn blake3_digest(width: u8, len: usize, gates: &[[u8; 3]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    let prefix = format!("eca57:{}:{}:", width, len);
    hasher.update(prefix.as_bytes());
    for g in gates {
        hasher.update(&[g[0], g[1], g[2]]);
    }
    hasher.finalize().as_bytes().to_owned()
}

fn window_token(window: &[[u8; 3]]) -> (u8, u64) {
    let (canon, width) = canonicalize_first_occurrence(window);
    let digest = blake3_digest(width, canon.len(), &canon);
    let token = u64::from_le_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    (width, token)
}

fn prefilter_key(width: u8, token: u64) -> [u8; 10] {
    let mut key = [0u8; 10];
    key[0] = BASIS_ECA57_ID;
    key[1] = width;
    key[2..10].copy_from_slice(&token.to_le_bytes());
    key
}

pub fn build_ids_indexes(
    env: &Environment,
    ids_db_names: &[String],
) -> Result<IdsIndexStats, Box<dyn Error>> {
    let rev_db = env.create_db(Some(IDS_REV_DB), DatabaseFlags::empty())?;
    let wit_db = env.create_db(Some(IDS_WIT_PREFILTER_DB), DatabaseFlags::empty())?;
    let mut stats = IdsIndexStats::default();

    let k_sizes = [2usize, 3usize];
    let mut pending = 0usize;
    let mut wtxn = env.begin_rw_txn()?;

    for db_name in ids_db_names {
        let db = match env.open_db(Some(db_name)) {
            Ok(db) => db,
            Err(lmdb::Error::NotFound) => continue,
            Err(e) => return Err(Box::new(e)),
        };
        stats.dbs_scanned += 1;

        let rtxn = env.begin_ro_txn()?;
        let mut cursor = rtxn.open_ro_cursor(db)?;
        for (_key, value) in cursor.iter() {
            let circuits: Vec<Vec<u8>> = bincode::deserialize(value)?;
            for blob in circuits {
                stats.ids_seen += 1;
                let circuit = CircuitSeq::from_blob(&blob);
                let (canon_gates, _) = canonicalize_first_occurrence(&circuit.gates);
                let canon_blob = gates_to_blob(&canon_gates);

                if wtxn
                    .put(rev_db, &canon_blob, &[], WriteFlags::NO_OVERWRITE)
                    .is_ok()
                {
                    stats.rev_inserted += 1;
                }

                let mut token_set: HashSet<(u8, u64)> = HashSet::new();
                for &k in &k_sizes {
                    if canon_gates.len() < k {
                        continue;
                    }
                    for i in 0..=(canon_gates.len() - k) {
                        token_set.insert(window_token(&canon_gates[i..i + k]));
                    }
                }
                for (width, token) in token_set {
                    let key = prefilter_key(width, token);
                    if wtxn
                        .put(wit_db, &key, &[1u8], WriteFlags::NO_OVERWRITE)
                        .is_ok()
                    {
                        stats.token_inserted += 1;
                    }
                }

                pending += 1;
                if pending >= 1000 {
                    wtxn.commit()?;
                    wtxn = env.begin_rw_txn()?;
                    pending = 0;
                }
            }
        }
    }

    wtxn.commit()?;
    Ok(stats)
}

pub fn ids_rev_contains(env: &Environment, db: Database, gates: &[[u8; 3]]) -> bool {
    let (canon, _) = canonicalize_first_occurrence(gates);
    let blob = gates_to_blob(&canon);
    ids_rev_contains_blob(env, db, &blob)
}

pub fn ids_rev_contains_blob(env: &Environment, db: Database, blob: &[u8]) -> bool {
    let txn = match env.begin_ro_txn() {
        Ok(txn) => txn,
        Err(_) => return false,
    };
    match txn.get(db, &blob) {
        Ok(_) => true,
        Err(lmdb::Error::NotFound) => false,
        Err(_) => false,
    }
}

pub fn ids_prefilter_hit(
    env: &Environment,
    db: Database,
    gates: &[[u8; 3]],
    k_sizes: &[usize],
) -> bool {
    let txn = match env.begin_ro_txn() {
        Ok(txn) => txn,
        Err(_) => return false,
    };
    for &k in k_sizes {
        if gates.len() < k {
            continue;
        }
        for i in 0..=(gates.len() - k) {
            let (width, token) = window_token(&gates[i..i + k]);
            let key = prefilter_key(width, token);
            if txn.get(db, &key).is_ok() {
                return true;
            }
        }
    }
    false
}

pub fn remove_identity_window(
    subcircuit: &mut CircuitSeq,
    env: &Environment,
    ids_rev_db: Database,
    max_len: usize,
) -> bool {
    let len = subcircuit.gates.len();
    if len == 0 {
        return false;
    }
    let max_len = max_len.min(len);
    for window_len in (5..=max_len).rev() {
        for start in 0..=(len - window_len) {
            let window = &subcircuit.gates[start..start + window_len];
            let (canon, _) = canonicalize_first_occurrence(window);
            let blob = gates_to_blob(&canon);
            if ids_rev_contains_blob(env, ids_rev_db, &blob) {
                subcircuit.gates.drain(start..start + window_len);
                return true;
            }
        }
    }
    false
}
