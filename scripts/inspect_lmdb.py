import sys
import os
import lmdb
from database.lmdb_env import TemplateDBEnv
from database.templates import TemplateRecord

def inspect(db_path):
    print(f"Opening LMDB: {db_path}")
    env = TemplateDBEnv(db_path, config=None)

    with env._env.begin() as txn:
        # The main DB contains keys that correspond to named DBs
        print("\n--- All Databases ---")
        main_db_cursor = txn.cursor()
        for key, value in main_db_cursor:
             try:
                 print(f"  {key.decode()}")
             except:
                 print(f"  {key}")
        
    with env.read_txn() as txn:
        # Check Meta
        basis = env.get_meta(txn, "basis")
        count = env.get_meta(txn, "template_count")
        print(f"Meta - Basis: {basis}, Template Count: {count}")

        # Check raw stats
        print("\n--- Raw DB Stats ---")
        for db_name in ["meta", "templates_by_hash", "template_families", "templates_by_dims", "witnesses_by_hash", "witness_prefilter"]:
            try:
                db = env._env.open_db(db_name.encode(), txn=txn)
                stat = txn.stat(db)
                print(f"  {db_name}: {stat['entries']} entries")
            except:
                pass
        
        # Check new tables
        for db_name in ["perm_tables_n3", "perm_tables_n4", "perm_tables_n5", "n3m6", "n4m6", "n5m4"]:
            try:
                db = env._env.open_db(db_name.encode(), txn=txn)
                stat = txn.stat(db)
                print(f"  {db_name}: {stat['entries']} entries")
            except:
                pass

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 inspect_lmdb.py <db_path>")
        sys.exit(1)
    inspect(sys.argv[1])
