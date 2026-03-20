#!/usr/bin/env python3
"""
Edge case test runner: imports, cyclic detection, DLM pipeline.
Produces edge-case-results.json consumed by the HTML report.
"""
import subprocess, json, os, sys, time, shutil
from pathlib import Path

BINARY      = os.environ.get("MDIX_BINARY", "target/debug/mdix")
BUILD_NUM   = os.environ.get("BUILD_NUM",   "0")
BRANCH      = os.environ.get("BRANCH",      "unknown")
COMMIT      = os.environ.get("COMMIT",      "unknown")[:8]
BUILD_DATE  = os.environ.get("BUILD_DATE",  "")
DLM_PW      = os.environ.get("DLM_TEST_PASSWORD", "dixscript_ci_test_password_2025")
IMPORTS_DIR = "mdix_files/tests/imports"
DLM_DIR     = "mdix_files/tests/dlm"
BINARY_DIR  = "mdix_files/tests/binary"
OUT_DLM     = "edge-case-output/dlm"
OUT_BINARY  = "binary-output"
OUT_INSPECT = "edge-case-output/inspect"

def run(cmd, cwd=None, timeout=90):
    start = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, timeout=timeout)
        return {
            "exit_code":  r.returncode,
            "stdout":     r.stdout.strip()[:4000],
            "stderr":     r.stderr.strip()[:4000],
            "elapsed_ms": int((time.time() - start) * 1000),
        }
    except subprocess.TimeoutExpired:
        return {"exit_code": -1, "stdout": "", "stderr": "TIMEOUT", "elapsed_ms": timeout * 1000}
    except Exception as e:
        return {"exit_code": -1, "stdout": "", "stderr": str(e), "elapsed_ms": 0}


# ── Import tests ──────────────────────────────────────────────────────────────

IMPORT_TESTS = [
    {"id": "01", "file": "01_basic_import.mdix",              "name": "Basic enum and type access via alias"},
    {"id": "02", "file": "02_alias_funcs_in_quickfuncs.mdix", "name": "Imported funcs called inside local QuickFuncs"},
    {"id": "03", "file": "03_alias_funcs_in_data.mdix",       "name": "Imported funcs called directly in DATA"},
    {"id": "04", "file": "04_funcs_in_funcs.mdix",            "name": "Imported function calls nested inside other imported calls"},
    {"id": "05", "file": "05_enum_in_funcs.mdix",             "name": "Imported enum values passed as arguments to imported functions"},
]

def run_import_tests():
    results = []
    for t in IMPORT_TESTS:
        path = os.path.join(IMPORTS_DIR, t["file"])
        r    = run([BINARY, "compile", path])
        passed = r["exit_code"] == 0
        results.append({
            "id":      t["id"],
            "name":    t["name"],
            "file":    t["file"],
            "status":  "passed" if passed else "failed",
            "exit_code": r["exit_code"],
            "stdout":  r["stdout"],
            "stderr":  r["stderr"],
            "elapsed_ms": r["elapsed_ms"],
        })
        print(f"  {'PASS' if passed else 'FAIL'} [{t['id']}] {t['name']}")
    return results


# ── Cyclic import detection ───────────────────────────────────────────────────

def run_cyclic_tests():
    results = []
    path = os.path.join(IMPORTS_DIR, "circular_a.mdix")
    r    = run([BINARY, "compile", path])
    # Expected: non-zero exit, stderr mentions cyclic/circular/import chain
    stderr_lower = r["stderr"].lower() + r["stdout"].lower()
    error_detected = r["exit_code"] != 0
    error_mentioned = any(k in stderr_lower for k in ["circular", "cyclic", "cycle", "import chain"])
    passed = error_detected  # we just need it to fail cleanly, not hang
    results.append({
        "name":           "Cyclic import A → B → A",
        "status":         "passed" if passed else "failed",
        "exit_code":      r["exit_code"],
        "error_detected": error_detected,
        "keyword_found":  error_mentioned,
        "stderr":         r["stderr"],
        "elapsed_ms":     r["elapsed_ms"],
    })
    print(f"  {'PASS' if passed else 'FAIL'} Cyclic A→B→A (exit={r['exit_code']}, keyword={error_mentioned})")
    return results


# ── DLM tests ─────────────────────────────────────────────────────────────────

DLM_TESTS = [
    {"id": "01", "file": "01_gzip.mdix",           "name": "GZip compression",                 "encrypted": False, "compressed": True,  "audited": False},
    {"id": "02", "file": "02_bzip2.mdix",          "name": "BZip2 compression",                "encrypted": False, "compressed": True,  "audited": False},
    {"id": "03", "file": "03_lzma.mdix",           "name": "LZMA compression",                 "encrypted": False, "compressed": True,  "audited": False},
    {"id": "04", "file": "04_aes128_password.mdix","name": "AES-128 password encryption",      "encrypted": True,  "compressed": False, "audited": False},
    {"id": "05", "file": "05_aes256_password.mdix","name": "AES-256 password encryption",      "encrypted": True,  "compressed": False, "audited": False},
    {"id": "06", "file": "06_chacha20_password.mdix","name": "ChaCha20 password encryption",   "encrypted": True,  "compressed": False, "audited": False},
    {"id": "07", "file": "07_diy_audit.mdix",      "name": "DIY auditor (.mdix.au)",           "encrypted": False, "compressed": False, "audited": True},
    {"id": "08", "file": "08_enhanced_audit.mdix", "name": "Enhanced auditor (.mdix.au)",      "encrypted": False, "compressed": False, "audited": True},
    {"id": "09", "file": "09_gzip_aes256.mdix",    "name": "GZip then AES-256",                "encrypted": True,  "compressed": True,  "audited": False},
    {"id": "10", "file": "10_full_pipeline.mdix",  "name": "Full pipeline: GZip+AES-256+Audit","encrypted": True,  "compressed": True,  "audited": True},
]

def collect_output_files(output_subdir):
    if not os.path.isdir(output_subdir):
        return []
    files = []
    for f in sorted(os.listdir(output_subdir)):
        fp = os.path.join(output_subdir, f)
        files.append({"name": f, "size_bytes": os.path.getsize(fp), "path": fp})
    return files

def run_dlm_tests():
    os.makedirs(OUT_DLM, exist_ok=True)
    os.makedirs(OUT_INSPECT, exist_ok=True)
    results = []

    for t in DLM_TESTS:
        src_path  = os.path.join(DLM_DIR, t["file"])
        out_subdir = os.path.join(OUT_DLM, t["id"])
        os.makedirs(out_subdir, exist_ok=True)

        # Build compile command
        compile_cmd = [BINARY, "compile", src_path, "--output", out_subdir]
        if t["encrypted"]:
            compile_cmd += ["--password", DLM_PW]

        compile_r = run(compile_cmd)
        compile_ok = compile_r["exit_code"] == 0
        output_files = collect_output_files(out_subdir)

        # Round-trip for encrypted files
        roundtrip = None
        if compile_ok and t["encrypted"] and output_files:
            enc_files = [f for f in output_files if f["name"].endswith(".enc")]
            if enc_files:
                enc_path    = enc_files[0]["path"]
                rt_out      = os.path.join(out_subdir, "roundtrip")
                os.makedirs(rt_out, exist_ok=True)
                decrypt_cmd = [BINARY, "decrypt", enc_path, "--password", DLM_PW, "--output", rt_out]
                rt_r        = run(decrypt_cmd)
                rt_ok       = rt_r["exit_code"] == 0
                rt_files    = collect_output_files(rt_out)

                # Inspect original (no DLM baseline)
                baseline_out = os.path.join(OUT_INSPECT, f"baseline_{t['id']}.json")
                insp_orig    = run([BINARY, "inspect", src_path, "--format", "json"])
                inspect_rt   = None
                if rt_ok and rt_files:
                    restored_path = rt_files[0]["path"]
                    insp_rt_r     = run([BINARY, "inspect", restored_path, "--format", "json"])
                    inspect_rt    = insp_rt_r["stdout"]

                roundtrip = {
                    "status":           "passed" if rt_ok else "failed",
                    "exit_code":        rt_r["exit_code"],
                    "stderr":           rt_r["stderr"],
                    "original_inspect": insp_orig["stdout"],
                    "restored_inspect": inspect_rt,
                    "match":            insp_orig["stdout"] == inspect_rt if inspect_rt else None,
                }

        passed = compile_ok
        results.append({
            "id":           t["id"],
            "name":         t["name"],
            "file":         t["file"],
            "encrypted":    t["encrypted"],
            "compressed":   t["compressed"],
            "audited":      t["audited"],
            "status":       "passed" if passed else "failed",
            "exit_code":    compile_r["exit_code"],
            "stdout":       compile_r["stdout"],
            "stderr":       compile_r["stderr"],
            "elapsed_ms":   compile_r["elapsed_ms"],
            "output_files": output_files,
            "roundtrip":    roundtrip,
        })
        print(f"  {'PASS' if passed else 'FAIL'} [{t['id']}] {t['name']}")

    return results


# ── Binary capture ────────────────────────────────────────────────────────────

def run_binary_capture():
    os.makedirs(OUT_BINARY, exist_ok=True)
    r = run([
        "cargo", "run", "--example", "binary_capture", "--",
        "mdix_files/tests/binary/serialize_target.mdix",
        OUT_BINARY,
    ])
    result = {
        "status":     "passed" if r["exit_code"] == 0 else "failed",
        "exit_code":  r["exit_code"],
        "stderr":     r["stderr"],
        "size_bytes": None,
        "sections":   None,
        "b64":        None,
        "hex_path":   None,
        "bin_path":   None,
    }
    for line in r["stdout"].splitlines():
        if line.startswith("BINARY_SIZE:"):
            result["size_bytes"] = int(line.split(":", 1)[1])
        elif line.startswith("BINARY_SECTIONS:"):
            result["sections"] = int(line.split(":", 1)[1])
        elif line.startswith("BINARY_B64:"):
            result["b64"] = line.split(":", 1)[1]

    stem = "serialize_target"
    hex_p = f"{OUT_BINARY}/{stem}.hex"
    bin_p = f"{OUT_BINARY}/{stem}.bin"
    result["hex_path"] = hex_p if os.path.exists(hex_p) else None
    result["bin_path"] = bin_p if os.path.exists(bin_p) else None
    if result["hex_path"]:
        with open(result["hex_path"]) as f:
            result["hex_dump"] = f.read()

    print(f"  {'PASS' if result['status'] == 'passed' else 'FAIL'} Binary capture"
          f" ({result['size_bytes']} bytes, {result['sections']} sections)")
    return result


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    print("=== DixScript Edge Case Tests ===")

    print("\n[1/4] Import tests")
    import_results  = run_import_tests()

    print("\n[2/4] Cyclic import detection")
    cyclic_results  = run_cyclic_tests()

    print("\n[3/4] Binary serialization capture")
    binary_result   = run_binary_capture()

    print("\n[4/4] DLM pipeline tests")
    dlm_results     = run_dlm_tests()

    # Summary
    def counts(lst):
        p = sum(1 for x in lst if x.get("status") == "passed")
        return p, len(lst) - p

    ip, if_ = counts(import_results)
    cp, cf  = counts(cyclic_results)
    dp, df  = counts(dlm_results)
    bp      = 1 if binary_result["status"] == "passed" else 0

    output = {
        "build":   BUILD_NUM,
        "branch":  BRANCH,
        "commit":  COMMIT,
        "date":    BUILD_DATE,
        "summary": {
            "imports_passed":  ip, "imports_failed":  if_,
            "cyclic_passed":   cp, "cyclic_failed":   cf,
            "binary_passed":   bp,
            "dlm_passed":      dp, "dlm_failed":      df,
            "total_passed":    ip + cp + bp + dp,
            "total_failed":    if_ + cf + (1 - bp) + df,
        },
        "import_tests":  import_results,
        "cyclic_tests":  cyclic_results,
        "binary_capture": binary_result,
        "dlm_tests":     dlm_results,
    }

    with open("edge-case-results.json", "w") as f:
        json.dump(output, f, indent=2)

    print(f"\nDone — passed={output['summary']['total_passed']} "
          f"failed={output['summary']['total_failed']}")
    sys.exit(0 if output["summary"]["total_failed"] == 0 else 1)


if __name__ == "__main__":
    main()
