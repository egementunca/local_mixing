use crate::infra::ids_index::canonicalize_first_occurrence;
use lmdb::{Database, Environment, EnvironmentFlags, Transaction};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

const BASIS_ECA57_ID: u8 = 1;
const DB_TEMPLATES_BY_HASH: &str = "templates_by_hash";
const DB_WITNESS_PREFILTER: &str = "witness_prefilter";

pub struct SatWitnessDb {
    env: Environment,
    templates_by_hash: Database,
    witness_prefilter: Database,
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

fn prefilter_key(width: u8, token: u64) -> [u8; 10] {
    let mut key = [0u8; 10];
    key[0] = BASIS_ECA57_ID;
    key[1] = width;
    key[2..10].copy_from_slice(&token.to_le_bytes());
    key
}

fn template_key(width: u8, gate_count: usize, canonical_hash: &[u8; 32]) -> [u8; 36] {
    let mut key = [0u8; 36];
    key[0] = BASIS_ECA57_ID;
    key[1] = width;
    key[2..4].copy_from_slice(&(gate_count as u16).to_le_bytes());
    key[4..36].copy_from_slice(canonical_hash);
    key
}

fn window_token_with_width(window: &[[u8; 3]], width: u8) -> u64 {
    let (canon, _) = canonicalize_first_occurrence(window);
    let digest = blake3_digest(width, canon.len(), &canon);
    u64::from_le_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

fn open_sat_env(path: &Path) -> Option<SatWitnessDb> {
    if !path.exists() {
        return None;
    }
    let env = Environment::new()
        .set_max_dbs(10)
        .set_max_readers(10_000)
        .set_flags(EnvironmentFlags::READ_ONLY)
        .open(path)
        .ok()?;
    let templates_by_hash = env.open_db(Some(DB_TEMPLATES_BY_HASH)).ok()?;
    let witness_prefilter = env.open_db(Some(DB_WITNESS_PREFILTER)).ok()?;
    Some(SatWitnessDb {
        env,
        templates_by_hash,
        witness_prefilter,
    })
}

fn default_sat_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SAT_WITNESS_DB") {
        return Some(PathBuf::from(path));
    }
    let fallback = Path::new("../sat_revsynth/data/collection.lmdb");
    if fallback.exists() {
        return Some(fallback.to_path_buf());
    }
    None
}

static SAT_WITNESS_DB: OnceCell<Option<SatWitnessDb>> = OnceCell::new();

pub fn get_sat_witness_db() -> Option<&'static SatWitnessDb> {
    SAT_WITNESS_DB
        .get_or_init(|| default_sat_path().and_then(|p| open_sat_env(&p)))
        .as_ref()
}

pub fn prefilter_hit(db: &SatWitnessDb, gates: &[[u8; 3]], width: u8, k_sizes: &[usize]) -> bool {
    if gates.is_empty() {
        return false;
    }
    let txn = match db.env.begin_ro_txn() {
        Ok(txn) => txn,
        Err(_) => return false,
    };
    for &k in k_sizes {
        if gates.len() < k {
            continue;
        }
        for i in 0..=(gates.len() - k) {
            let token = window_token_with_width(&gates[i..i + k], width);
            let key = prefilter_key(width, token);
            if txn.get(db.witness_prefilter, &key).is_ok() {
                return true;
            }
        }
    }
    false
}

pub fn template_contains(db: &SatWitnessDb, gates: &[[u8; 3]], width: u8) -> bool {
    if gates.is_empty() {
        return false;
    }
    let (canon, _) = canonicalize_first_occurrence(gates);
    let hash = blake3_digest(width, canon.len(), &canon);
    let key = template_key(width, canon.len(), &hash);
    let txn = match db.env.begin_ro_txn() {
        Ok(txn) => txn,
        Err(_) => return false,
    };
    txn.get(db.templates_by_hash, &key).is_ok()
}

pub fn remove_identity_window(
    subcircuit: &mut crate::infra::circuit::circuit::CircuitSeq,
    db: &SatWitnessDb,
    width: u8,
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
            if template_contains(db, window, width) {
                subcircuit.gates.drain(start..start + window_len);
                return true;
            }
        }
    }
    false
}
