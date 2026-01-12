use crate::infra::store::schema::TemplateRecord;
use lmdb::{Cursor, Database, Environment, Transaction};
use std::path::Path;
extern crate lmdb_sys as sys;

pub struct TemplateDB {
    env: Environment,
    db_templates: Database,
}

impl TemplateDB {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, lmdb::Error> {
        let env = Environment::new()
            .set_max_dbs(10)
            .set_map_size(10 * 1024 * 1024 * 1024) // 10GB, simplistic
            .open(path.as_ref())?;

        let db_templates = env.open_db(Some("templates_by_hash"))?;

        Ok(Self { env, db_templates })
    }

    /// Returns a list of available gate counts for the given width.
    pub fn get_available_gate_counts(&self, width: u8) -> Vec<u16> {
        let txn = match self.env.begin_ro_txn() {
            Ok(txn) => txn,
            Err(e) => {
                println!("Txn error: {:?}", e);
                return Vec::new();
            }
        };
        let cursor = match txn.open_ro_cursor(self.db_templates) {
            Ok(c) => c,
            Err(e) => {
                println!("Cursor error: {:?}", e);
                return Vec::new();
            }
        };

        let mut counts = Vec::new();
        let basis_id = 1u8; // ECA57

        // Start searching at GC=0
        let mut next_gc = 0u16;

        loop {
            // Construct probe key: basis + width + next_gc + [0; 32]
            let mut key = Vec::with_capacity(36);
            key.push(basis_id);
            key.push(width);
            key.extend_from_slice(&next_gc.to_le_bytes());
            key.extend_from_slice(&[0u8; 32]);

            // Pass Some(slice) to coerce Option<&Vec> to Option<&[u8]>
            let result = cursor.get(Some(key.as_slice()), None, sys::MDB_SET_RANGE);

            match result {
                Ok((Some(k), _)) => {
                    if k.len() < 4 {
                        break;
                    } // Should check min len
                    // println!("Found key: {:?}", &k[0..min(10, k.len())]);
                    if k[0] != basis_id || k[1] != width {
                        break; // No more for this width
                    }

                    let found_gc = u16::from_le_bytes([k[2], k[3]]);
                    counts.push(found_gc);

                    // Advance search to found_gc + 1
                    // If found_gc is MAX, we are done
                    if found_gc == u16::MAX {
                        break;
                    }
                    next_gc = found_gc + 1;
                }
                _ => break, // End of DB or Error
            }
        }

        counts
    }

    /// Tries to get a random identity template for the given width and gate_count.
    /// Uses a "random probe" strategy since we cannot seek by index easily.
    pub fn get_random_identity(&self, width: u8, gate_count: u16) -> Option<TemplateRecord> {
        // Use cached DB handle
        let txn = match self.env.begin_ro_txn() {
            Ok(t) => t,
            Err(e) => {
                println!("get_random_identity txn error: {:?}", e);
                return None;
            }
        };
        let cursor = match txn.open_ro_cursor(self.db_templates) {
            Ok(c) => c,
            Err(e) => {
                println!("get_random_identity cursor error: {:?}", e);
                return None;
            }
        };

        // Key Format: basis(u8) + width(u8) + gc(u16) + hash([u8; 32])
        // We want to scan keys where width and gc match.
        // Basis 1 (ECA57)
        let basis = 1u8;

        let mut rng = rand::rng();
        use rand::Rng;

        // Try probing 10 times at random hash positions
        for _ in 0..10 {
            let mut probe_key = Vec::with_capacity(36);
            probe_key.push(basis);
            probe_key.push(width);
            probe_key.extend_from_slice(&gate_count.to_le_bytes());

            let mut random_hash = [0u8; 32];
            rng.fill(&mut random_hash);
            probe_key.extend_from_slice(&random_hash);

            // Manual get with SET_RANGE to avoid iter_from panics
            // Pass Some(slice) explicitly to satisfy Option<&[u8]>
            match cursor.get(Some(probe_key.as_slice()), None, sys::MDB_SET_RANGE) {
                Ok((Some(k), v)) => {
                    // Check if k matches prefix (basis, width, gc)
                    let gc_bytes = gate_count.to_le_bytes();
                    if k.len() >= 4
                        && k[0] == basis
                        && k[1] == width
                        && k[2] == gc_bytes[0]
                        && k[3] == gc_bytes[1]
                    {
                        return TemplateRecord::from_bytes(v);
                    }
                    // If not match, we landed outside (or at end of) the block.
                }
                _ => {
                    // NotFound or error.
                }
            }
        }

        // Fallback: Try probing at the very start of the block (hash 0)
        // This guarantees finding a record if one exists, even if random probe missed.
        let mut probe_key = Vec::with_capacity(36);
        probe_key.push(basis);
        probe_key.push(width);
        probe_key.extend_from_slice(&gate_count.to_le_bytes());
        probe_key.extend_from_slice(&[0u8; 32]);

        if let Ok((Some(k), v)) = cursor.get(Some(probe_key.as_slice()), None, sys::MDB_SET_RANGE) {
            let gc_bytes = gate_count.to_le_bytes();
            if k.len() >= 4
                && k[0] == basis
                && k[1] == width
                && k[2] == gc_bytes[0]
                && k[3] == gc_bytes[1]
            {
                return TemplateRecord::from_bytes(v);
            }
        }

        None
    }

    /// Looks up a template by its canonical hash.
    /// This is used for LMDB-first compression: try to find a pre-computed optimized version.
    ///
    /// Key format: basis(1) + width(1) + gate_count(2) + canonical_hash(32) = 36 bytes
    pub fn get_by_canonical_hash(
        &self,
        width: u8,
        gate_count: u16,
        canonical_hash: &[u8; 32],
    ) -> Option<TemplateRecord> {
        let txn = self.env.begin_ro_txn().ok()?;

        // Construct the exact key
        let basis = 1u8; // ECA57
        let mut key = Vec::with_capacity(36);
        key.push(basis);
        key.push(width);
        key.extend_from_slice(&gate_count.to_le_bytes());
        key.extend_from_slice(canonical_hash);

        // Direct lookup
        match txn.get(self.db_templates, &key) {
            Ok(v) => TemplateRecord::from_bytes(v),
            Err(_) => None,
        }
    }

    /// Looks up any template with fewer gates than the given count for the same function.
    /// Returns the smallest available template if one exists.
    pub fn get_smaller_equivalent(
        &self,
        width: u8,
        current_gate_count: u16,
        canonical_hash: &[u8; 32],
    ) -> Option<TemplateRecord> {
        // Try progressively smaller gate counts
        for gc in (2..current_gate_count).rev() {
            if let Some(record) = self.get_by_canonical_hash(width, gc, canonical_hash) {
                return Some(record);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_read_real_db() {
        // Path relative to local_mixing/
        let path = Path::new("../sat_revsynth/data/collection.lmdb");
        if !path.exists() {
            println!("Skipping test: DB not found at {:?}", path);
            return;
        }

        let db = TemplateDB::open(path).expect("Failed to open DB");
        // Try getting an identity for width 5, GC 6 (common)
        // Adjust width/gc if needed based on what you generated
        if let Some(record) = db.get_random_identity(5, 6) {
            println!("Found record: {:?}", record);
            assert_eq!(record.width, 5);
            assert_eq!(record.gate_count, 6);
        } else {
            println!("No record found for 5/6, trying others...");
        }
    }
}
