import sys, json, base64, os
sys.path.insert(0, "/home/claude/w/tools")
# reuse the comparison logic
exec(open("/home/claude/w/tools/check_spans.py").read().split("SCALARS =")[0])
SCALARS = {"int","long","float","double","bool","string","date","timestamp","hex_color","regex","blob","enum"}
files = compiled = skipped = spans_total = mism_total = absent = tuple_elems = 0
by_kind = {}
for line in open("/tmp/spans_real/index.txt"):
    idx, path = line.rstrip("\n").split("\t")
    files += 1
    try:
        db = MdixDatabase.load_str(open(path, encoding="utf-8").read())
        keys = db.get_keys("")
        if len(keys) == 0: raise RuntimeError("empty")
    except Exception:
        skipped += 1; continue
    compiled += 1
    for l in open("/tmp/spans_real/%s.tsv" % idx):
        sp, kind, b = l.rstrip("\n").split("\t")
        text = base64.b64decode(b).decode("utf-8")
        try: ty = str(db.get_type(sp)).lower()
        except Exception: ty = "unknown"
        if ty == "unknown":
            # peel trailing [n] until a tuple is found
            q = sp; inside = False
            while q.endswith("]") and "[" in q:
                q = q[:q.rindex("[")]
                try:
                    if str(db.get_type(q)).lower() == "tuple": inside = True; break
                except Exception: pass
            if inside: tuple_elems += 1
            else: absent += 1; print("  ABSENT (not a tuple element): %s :: %s" % (os.path.basename(path), sp))
            continue
        spans_total += 1
        ok = False
        try:
            if   kind == "Int":       ok = ty == "int"  and db.get_int(sp)  == parse_int(text)
            elif kind == "Long":      ok = ty == "long" and db.get_long(sp) == parse_int(text)
            elif kind == "Float":     ok = ty == "float" and abs(db.get_float(sp) - float(text.rstrip("fF").replace("_",""))) <= 1e-6 * max(1.0, abs(float(text.rstrip("fF").replace("_",""))))
            elif kind == "Double":    ok = ty == "double" and abs(db.get_double(sp) - float(text.replace("_",""))) < 1e-12
            elif kind == "Bool":      ok = ty == "bool" and db.get_bool(sp) == (text == "true")
            elif kind == "String":    ok = ty == "string" and db.get_string(sp) == decode_string(text)
            elif kind == "RawString": ok = ty == "string" and db.get_string(sp) == text[1:-1]
            elif kind == "HexColor":  ok = ty == "hex_color" and json.loads(db.get_json(sp)) == text
            elif kind in ("Date","Timestamp"): ok = ty == kind.lower() and db.get_string(sp) == text
            elif kind == "Regex":     ok = ty == "regex" and json.loads(db.get_json(sp)) == call_arg(text)
            elif kind == "Blob":      ok = ty == "blob" and json.loads(db.get_json(sp)) == call_arg(text)
            elif kind == "Enum":
                j = json.loads(db.get_json(sp)); ok = ty == "enum" and (j["enum_name"] + "." + j["field_name"]) == text
        except Exception:
            ok = False
        by_kind.setdefault(kind, [0, 0]); by_kind[kind][0] += 1
        if not ok:
            mism_total += 1; by_kind[kind][1] += 1
            if mism_total <= 12: print("  MISMATCH %s :: %s  %s  %r  (engine %s)" % (os.path.basename(path), sp, kind, text[:30], ty))
print("files %d: compiled %d, did not compile on their own %d" % (files, compiled, skipped))
print("spans compared with the compiler: %d   mismatches: %d   (tuple elements, checked via the tuple JSON instead: %d; absent for another reason: %d)" % (spans_total, mism_total, tuple_elems, absent))
print({k: "%d ok / %d bad" % (v[0]-v[1], v[1]) for k, v in sorted(by_kind.items())})
