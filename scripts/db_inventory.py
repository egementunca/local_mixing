#!/usr/bin/env python3
"""Quick inventory for LMDB databases used by local_mixing.

- Perm tables: ./db (n{N}m{M}, n{N}m{M}perms, perm_tables_nN)
- TemplateDB: ./collection.lmdb (templates_by_hash / templates_by_dims)
"""
import argparse
import os
import re
import lmdb


def list_named_dbs(env):
    names = []
    with env.begin() as txn:
        cur = txn.cursor()
        for k, _ in cur:
            try:
                names.append(k.decode())
            except Exception:
                names.append(repr(k))
    return names


def stat_db(env, name):
    with env.begin() as txn:
        db = env.open_db(name.encode(), txn=txn)
        st = txn.stat(db)
        return st.get("entries", 0)


def inspect_perm_db(path):
    if not os.path.isdir(path):
        print(f"Perm-table LMDB not found at {path}")
        return

    env = lmdb.open(path, readonly=True, max_dbs=200, lock=False)
    names = list_named_dbs(env)

    print(f"Perm-table LMDB: {path}")
    print("Named DBs:")
    for name in names:
        try:
            entries = stat_db(env, name)
            print(f"  {name}: entries={entries}")
        except Exception as e:
            print(f"  {name}: failed to stat ({e})")

    # Summaries
    n_m = re.compile(r"^n(\d+)m(\d+)$")
    perm_tables = re.compile(r"^perm_tables_n(\d+)$")

    by_width = {}
    for name in names:
        m = n_m.match(name)
        if m:
            w, g = int(m.group(1)), int(m.group(2))
            by_width.setdefault(w, []).append(g)

    if by_width:
        print("\nSummary: n{N}m{M} tables")
        for w in sorted(by_width):
            lens = sorted(by_width[w])
            print(f"  width {w}: m range {lens[0]}..{lens[-1]} (count={len(lens)})")

    pt = [int(m.group(1)) for name in names for m in [perm_tables.match(name)] if m]
    if pt:
        print("\nperm_tables_n* present for widths:", sorted(pt))


def inspect_template_db(path):
    if not os.path.isdir(path):
        print(f"TemplateDB not found at {path}")
        return

    env = lmdb.open(path, readonly=True, max_dbs=20, lock=False)
    names = list_named_dbs(env)

    print(f"TemplateDB: {path}")
    for name in names:
        try:
            entries = stat_db(env, name)
            print(f"  {name}: entries={entries}")
        except Exception as e:
            print(f"  {name}: failed to stat ({e})")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--perm-db", default="./db", help="Path to perm-table LMDB (default: ./db)")
    ap.add_argument("--template-db", default="./collection.lmdb", help="Path to TemplateDB LMDB (default: ./collection.lmdb)")
    args = ap.parse_args()

    inspect_perm_db(args.perm_db)
    print("")
    inspect_template_db(args.template_db)


if __name__ == "__main__":
    main()
