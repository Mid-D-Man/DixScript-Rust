import sys, json
from midmanstudio.mdix import MdixDatabase
ok_all = True
for f in sys.argv[1:]:
    src = open(f, encoding="utf-8").read()
    try:
        db = MdixDatabase.load_str(src)
        keys = db.get_keys("")
        if len(keys) == 0: raise RuntimeError("compiled to an empty database (see the compiler log)")
        types = {}
        for k in keys:
            try: types[k] = str(db.get_type(k)).lower()
            except Exception: types[k] = "?"
        bad = [k for k,t in types.items() if t in ("unknown","?","none")]
        print("%s: compiled, %d paths%s" % (f.split("/")[-1], len(keys), "" if not bad else "  (UNKNOWN: %s)" % bad[:8]))
        json.dump(types, open("/tmp/types_%s.json" % f.split("/")[-1], "w"), indent=0)
    except Exception as e:
        ok_all = False
        print("%s: FAILED to compile: %s" % (f.split("/")[-1], str(e)[:300]))
sys.exit(0 if ok_all else 1)
