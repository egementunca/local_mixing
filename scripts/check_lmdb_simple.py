import lmdb
import sys

def check_lmdb(path):
    env = lmdb.open(path, max_dbs=20)
    print(f"Checking LMDB at: {path}")
    
    with env.begin() as txn:
        # The main DB contains names of other DBs as keys
        main_db = env.open_db(None)
        with txn.cursor(main_db) as cursor:
            print("\n--- Named Databases ---")
            for key, value in cursor:
                try:
                    name = key.decode()
                    db = env.open_db(key, txn=txn)
                    stat = txn.stat(db)
                    print(f"  {name}: {stat['entries']} entries")
                except Exception as e:
                    print(f"  [Error decoding key {key}]: {e}")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 check_lmdb_simple.py <db_path>")
        sys.exit(1)
    check_lmdb(sys.argv[1])
