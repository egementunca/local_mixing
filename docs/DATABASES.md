# Local Mixing Data Infrastructure

This document details the database schemas and storage formats used by the `local_mixing` circuit obfuscation framework.

Note: there are **two different LMDB schemas** in this repo with overlapping names:
- `./db` (local_mixing perm tables): used for perm-based lookups and identity construction.
- `collection.lmdb` (sat_revsynth TemplateDB): used for template hash lookups in LMDB-first SAT compression.

The compressors use these differently; see Section 4 for the mapping.

## 1. SQLite Database (`circuits.db`)

**Purpose**: Stores enumerated circuits (Rainbow Tables) to "bridge" finding the canonical ID of a circuit. It maps complex circuits to their simple canonical forms.

**Location**: `circuits.db` (usually in the execution directory, e.g., `./` or `./db/`).
**Manager**: `rusqlite` (Rust).

### Schema (Dynamic Tables)
The system creates dynamic tables for each width `N` and gate count `M`, named `n{N}m{M}` (e.g., `n3m5` for 3-wire, 5-gate circuits).

**Table: `n{N}m{M}`**
```sql
CREATE TABLE IF NOT EXISTS n{N}m{M} (
    circuit BLOB UNIQUE,   -- The circuit structure, serialized (3 bytes per gate)
    perm BLOB NOT NULL,    -- The Canonical Truth Table (Permutation Vector)
    shuf BLOB NOT NULL     -- The Wire Shuffle/Mapping used to achieve the canonical form
);
```

### Indexing
Indices are created on the `perm` column to allow fast "Meet-in-the-Middle" searches (finding all circuits that implement a specific logic).
```sql
CREATE INDEX IF NOT EXISTS idx_perm_{table} ON {table} (perm);
```

### Usage Workflow
1.  **Generation**: We exhaustively generate circuits of size $M$.
2.  **Canonicalization**: For each circuit, we compute its `perm` (canonical truth table) and `shuf` (wire mapping).
3.  **Storage**: We store `(circuit, perm, shuf)` in the table.
4.  **Lookup**: When we have a subcircuit `C`, we compute its `perm` and query `SELECT circuit, shuf FROM tables WHERE perm = ?` to find an equivalent representation.

---

## 2. LMDB (local_mixing perm tables, `./db`)

**Purpose**: Fast perm-based lookups for compression and identity construction. These tables are **not** the same as the TemplateDB used by SAT compression.

**Location**: `./db` (hard-coded in current CLI paths). The `ObfuscationConfig.lmdb_path` field exists but is not wired to the CLI yet.

### 2.1 Tables

#### `perm_tables_n{N}`
- Key: `perm` (length `2^N` bytes).
- Value: `bincode` list of gate counts (`m`) that share the same permutation.
- Built by: `local_mixing_bin lmdbcounts` or `save_perm_tables_to_lmdb`.
- Used by: `random_canonical_id` (identity construction).

#### `n{N}m{M}`
- Key: `perm || circuit` (perm bytes + circuit blob).
- Value: empty.
- Built by: `sql_to_lmdb` (from SQLite).
- Used by: `compress_lmdb` and `expand_lmdb` via prefix searches; `random_perm_lmdb` returns the circuit suffix.

#### `n{N}m{M}perms`
- Key: `circuit` blob.
- Value: `perm || shuf` (perm bytes + shuffle bytes).
- Built by: `sql_to_lmdb_perms` (from SQLite).
- Used by: `compress_lmdb` and `expand_lmdb` to map a subcircuit to canonical `perm` and `shuf` quickly.

### 2.2 Generation
- SQLite -> LMDB: `local_mixing_bin lmdb -n N -m M` calls `sql_to_lmdb`.
- Perm tables: `local_mixing_bin lmdbcounts` builds `perm_tables_n{N}`.
- JSON import: `local_mixing/src/bin/import_rainbow.rs` can import exported JSON into these tables.

### 2.3 Usage notes
- These tables live in `./db` and are opened directly in `main.rs` (`bbutterfly`, `abbutterfly`, `compress`, `local-mix`).
- The CLI flag `--lmdb-db` does **not** point to these tables; it points to the TemplateDB schema below.

---

## 3. LMDB TemplateDB (`collection.lmdb`)

**Purpose**: Stores identity templates and optimized circuits by canonical hash. This is a Key-Value store optimized for fast random reads.

**Location**: typically `data/collection.lmdb` (built by `sat_revsynth`). Provided to `abbutterfly` via `--lmdb-db`.
**Manager**: `lmdb` crate.
**Map Size**: Configured typically to 10GB - 700GB depending on command (see `main.rs`).

### Key-Value Structure
The database `templates_by_hash` stores mappings from a circuit's **Canonical Hash** to its **Optimized Implementation**.

**Key Format (36 bytes)**
```
[ BasisID (u8) ] [ Width (u8) ] [ GateCount (u16, LE) ] [ CanonicalHash (32 bytes) ]
```
-   **BasisID**: `1` for ECA57, other IDs for other gate sets.
-   **Width**: Number of wires (e.g., 3, 4, 5).
-   **GateCount**: Number of gates in the template.
-   **CanonicalHash**: A distinct hash representing the function implemented.

**Value Format (Binary Struct, ~91 bytes + gates)**
Binary layout matches `src/store/schema.rs` `TemplateRecord`:

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 0 | 8 | `template_id` | Unique ID of this template |
| 8 | 1 | `basis_id` | Gate set ID |
| 9 | 1 | `width` | Number of wires |
| 10 | 2 | `gate_count` | Number of gates |
| 12 | 32 | `canonical_hash` | Function hash |
| 44 | 32 | `family_hash` | Structure/Equivalence hash |
| 76 | 1 | `origin` | Source enumeration |
| 77 | 8 | `origin_template_id` | Parent ID |
| 85 | 4 | `unroll_ops` | Optimization metadata |
| 89 | 2 | `gates_len` | Size of gate data in bytes |
| 91 | N*3 | `gates` | List of gates (Target, C1, C2) |

### Optimization Strategy
During `abbutterfly` (obfuscation):
1.  The mixer identifies a sub-window of the circuit.
2.  It computes the **Canonical Hash** of that window.
3.  It queries LMDB: `get_smaller_equivalent(width, current_len, hash)`.
4.  If a smaller record exists (e.g., length 6 vs current length 10), it replaces the window.

## 4. Compressor/database mapping (current code)

- `compress` / `compress_exhaust`: SQLite `circuits.db` tables `n{N}m{M}`.
- `compress_lmdb`: local_mixing LMDB `n{N}m{M}` for replacements; uses `n{N}m{M}perms` or SQLite for perm/shuf (special cases `n7m4`, `n6m5`).
- `expand_lmdb`: same DB usage as `compress_lmdb`.
- `compress_big`: currently does **not** call `compress_lmdb` (subcircuit path is commented out); only local dedup is applied.
- `compress_big_ancillas`: uses `compress_lmdb` (local_mixing LMDB).
- `compress_big_sat`: SAT only (no DB).
- `compress_big_sat_lmdb`: TemplateDB `templates_by_hash` (collection.lmdb) with SAT fallback.
- `random_canonical_id`: local_mixing LMDB `perm_tables_n{N}` + `n{N}m{M}`.
- `replace_pairs`: uses TemplateDB when `--lmdb-db` is provided; otherwise falls back to `random_canonical_id` (local_mixing LMDB).

## 5. Serialization Details

### Circuit Blob (SQLite & LMDB)
Circuits are stored as raw byte arrays of gates:
-   Each gate is 3 bytes: `[Target, Control1, Control2]`.
-   A 10-gate circuit is a `BLOB` of 30 bytes.
-   Wire indices are `u8` (0-255).

### Canonical Hash (TemplateDB)
-   For `collection.lmdb`, the `canonical_hash` is a function hash used by sat_revsynth.
-   Local lookups compute it in `local_mixing/src/optimize/compress_sat.rs::compute_canonical_hash` (truth table hash).
-   This is **separate** from `infra/rainbow/canonical.rs`, which canonicalizes permutations for the local_mixing perm tables.
