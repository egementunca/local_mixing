# Local Mixing Data Infrastructure

This document details the database schemas and storage formats used by the `local_mixing` circuit obfuscation framework. The system uses a hybrid storage model: **SQLite** for structure-aware enumeration and **LMDB** for fast template lookups.

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

## 2. LMDB Database (`collection.lmdb`)

**Purpose**: Stores "Identity Templates" and optimized circuits. This is a Key-Value store optimized for extremely fast random reads (millions/sec) during the obfuscation process.

**Location**: `./db` or specified via `--lmdb-db`.
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

## 3. Serialization Details

### Circuit Blob (SQLite & LMDB)
Circuits are stored as raw byte arrays of gates:
-   Each gate is 3 bytes: `[Target, Control1, Control2]`.
-   A 10-gate circuit is a `BLOB` of 30 bytes.
-   Wire indices are `u8` (0-255).

### Canonical Hash
-   Computed in `src/rainbow/canonical.rs`.
-   Represents the lexicographically smallest truth table reachable by permuting wires.
-   Ensures that structurally different but functionally identical circuits map to the same Key.
