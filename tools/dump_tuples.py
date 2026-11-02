import os, sys, glob
from midmanstudio.mdix import MdixDatabase
os.makedirs("/tmp/tuples", exist_ok=True)
for f in glob.glob("/tmp/tuples/*.tsv"): os.remove(f)

jobs = []                                    # (id, source path, spans tsv)
for line in open("/tmp/spans_real/index.txt"):
    idx, path = line.rstrip("\n").split("\t"); jobs.append((idx, path, "/tmp/spans_real/%s.tsv" % idx))
for f in glob.glob("/home/claude/w/samples_out/*.mdix"):
    name = os.path.basename(f); jobs.append(("S_" + name[:-5], f, "/tmp/spans_%s.tsv" % name))

files = tuples = 0
for idx, path, spans in jobs:
    try:
        db = MdixDatabase.load_str(open(path, encoding="utf-8").read())
        if len(db.get_keys("")) == 0: continue
    except Exception: continue
    found = {}
    for l in open(spans):
        sp = l.split("\t")[0]
        try: ty = str(db.get_type(sp)).lower()
        except Exception: ty = "unknown"
        if ty != "unknown": continue
        q = sp
        while q.endswith("]") and "[" in q:       # peel [n] until the enclosing tuple
            q = q[:q.rindex("[")]
            try:
                if str(db.get_type(q)).lower() == "tuple":
                    if q not in found: found[q] = db.get_json(q)
                    break
            except Exception: pass
    if found:
        open("/tmp/tuples/%s.tsv" % idx, "w", encoding="utf-8").write("\n".join(k + "\t" + v for k, v in found.items()) + "\n")
        files += 1; tuples += len(found)
print("tuples dumped: %d tuples in %d files" % (tuples, files))
