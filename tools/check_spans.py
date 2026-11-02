import sys, json, base64
from midmanstudio.mdix import MdixDatabase

def decode_string(lit):
    body, out, i = lit[1:-1], [], 0
    while i < len(body):
        c = body[i]
        if c == "\\" and i + 1 < len(body):
            e = body[i + 1]; out.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(e, e)); i += 2
        else:
            out.append(c); i += 1
    return "".join(out)

def call_arg(lit):                       # r:("...") / b:("...") / r:(`...`)
    inner = lit[3:].strip()
    assert inner.endswith(")"), lit[:20]
    inner = inner[:-1].strip()
    return inner[1:-1] if inner[0] == "`" else decode_string(inner)

def parse_int(text):
    t = text.strip(); long_suffix = t[-1:] in "Ll" and len(t) > 1 and not t.lower().startswith("0x") or (t[-1:] in "Ll" and t.lower().startswith(("0x","0b")))
    if t[-1:] in "Ll": t = t[:-1]
    t = t.replace("_", "")
    if t[:2].lower() == "0x": return int(t[2:], 16)
    if t[:2].lower() == "0b": return int(t[2:], 2)
    return int(t)

SCALARS = {"int","long","float","double","bool","string","date","timestamp","hex_color","regex","blob","enum"}
total_bad = 0
for f in sys.argv[1:]:
    name = f.split("/")[-1]
    db = MdixDatabase.load_str(open(f, encoding="utf-8").read())
    spans = {}
    for line in open("/tmp/spans_%s.tsv" % name):
        path, kind, b = line.rstrip("\n").split("\t")
        spans[path] = (kind, base64.b64decode(b).decode("utf-8"))

    bad = 0
    counts = {}
    for path, (kind, text) in spans.items():
        ty = str(db.get_type(path)).lower()
        if ty in ("unknown", "none", "?"):
            q = path; inside = False
            while q.endswith("]") and "[" in q:           # a tuple element: the compiler cannot address it
                q = q[:q.rindex("[")]
                if str(db.get_type(q)).lower() == "tuple": inside = True; break
            if inside:
                counts["(tuple element)"] = counts.get("(tuple element)", 0) + 1
                continue
        ok = False
        try:
            if   kind == "Int":       ok = ty == "int"  and db.get_int(path)  == parse_int(text)
            elif kind == "Long":      ok = ty == "long" and db.get_long(path) == parse_int(text)
            elif kind == "Float":     ok = ty == "float" and abs(db.get_float(path) - float(text.rstrip("fF").replace("_",""))) <= 1e-6 * max(1.0, abs(float(text.rstrip("fF").replace("_",""))))
            elif kind == "Double":    ok = ty == "double" and abs(db.get_double(path) - float(text.replace("_",""))) < 1e-12
            elif kind == "Bool":      ok = ty == "bool" and db.get_bool(path) == (text == "true")
            elif kind == "String":    ok = ty == "string" and db.get_string(path) == decode_string(text)
            elif kind == "RawString": ok = ty == "string" and db.get_string(path) == text[1:-1]
            elif kind == "HexColor":  ok = ty == "hex_color" and json.loads(db.get_json(path)) == text
            elif kind in ("Date","Timestamp"): ok = ty == kind.lower() and db.get_string(path) == text
            elif kind == "Regex":     ok = ty == "regex" and json.loads(db.get_json(path)) == call_arg(text)
            elif kind == "Blob":      ok = ty == "blob" and json.loads(db.get_json(path)) == call_arg(text)
            elif kind == "Enum":
                j = json.loads(db.get_json(path)); ok = ty == "enum" and (j["enum_name"] + "." + j["field_name"]) == text
        except Exception as e:
            ok = False; print("   exception at", path, kind, str(e)[:80])
        counts[kind] = counts.get(kind, 0) + 1
        if not ok:
            bad += 1; print("  MISMATCH %-26s %-9s %-24s engine type=%s" % (path, kind, text[:24].replace("\n","\\n"), ty))
    total_bad += bad
    print("%s: %d spans checked against the compiler, %d mismatches   %s" % (name, len(spans), bad, dict(sorted(counts.items()))))

    unmapped = []
    for k in db.get_keys(""):
        ty = str(db.get_type(k)).lower()
        if ty in SCALARS and k not in spans:
            unmapped.append((k, ty))
    print("  scalars the editor cannot edit (%d):" % len(unmapped))
    for k, ty in unmapped: print("     %-24s %s" % (k, ty))
sys.exit(1 if total_bad else 0)
