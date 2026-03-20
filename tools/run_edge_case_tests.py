#!/usr/bin/env python3
"""
Edge case test runner: imports, cyclic detection, DLM pipeline, binary capture.
Produces edge-case-results.json consumed by the HTML report.
"""
import subprocess, json, os, sys, time, glob
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

def run(cmd, cwd=None, timeout=120, extra_env=None):
    start = time.time()
    env = os.environ.copy()
    if extra_env:
        env.update(extra_env)
    try:
        r = subprocess.run(
            cmd, capture_output=True, text=True,
            cwd=cwd, timeout=timeout, env=env
        )
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
            "id":         t["id"],
            "name":       t["name"],
            "file":       t["file"],
            "status":     "passed" if passed else "failed",
            "exit_code":  r["exit_code"],
            "stdout":     r["stdout"],
            "stderr":     r["stderr"],
            "elapsed_ms": r["elapsed_ms"],
        })
        print(f"  {'PASS' if passed else 'FAIL'} [{t['id']}] {t['name']}")
    return results


# ── Cyclic import detection ───────────────────────────────────────────────────

def run_cyclic_tests():
    results = []
    path = os.path.join(IMPORTS_DIR, "circular_a.mdix")
    r    = run([BINARY, "compile", path])
    combined = (r["stderr"] + r["stdout"]).lower()
    error_detected  = r["exit_code"] != 0
    keyword_found   = any(k in combined for k in ["circular", "cyclic", "cycle", "import chain"])
    passed = error_detected
    results.append({
        "name":           "Cyclic import A → B → A",
        "status":         "passed" if passed else "failed",
        "exit_code":      r["exit_code"],
        "error_detected": error_detected,
        "keyword_found":  keyword_found,
        "stderr":         r["stderr"],
        "elapsed_ms":     r["elapsed_ms"],
    })
    print(f"  {'PASS' if passed else 'FAIL'} Cyclic A→B→A (exit={r['exit_code']}, keyword={keyword_found})")
    return results


# ── DLM tests ─────────────────────────────────────────────────────────────────

DLM_TESTS = [
    {"id": "01", "file": "01_gzip.mdix",            "name": "GZip compression",                  "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "02", "file": "02_bzip2.mdix",           "name": "BZip2 compression",                 "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "03", "file": "03_lzma.mdix",            "name": "LZMA compression",                  "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "04", "file": "04_aes128_password.mdix", "name": "AES-128 password encryption",       "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "05", "file": "05_aes256_password.mdix", "name": "AES-256 password encryption",       "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "06", "file": "06_chacha20_password.mdix","name": "ChaCha20 password encryption",     "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "07", "file": "07_diy_audit.mdix",       "name": "DIY auditor (.mdix.au)",            "encrypted": False, "compressed": False, "audited": True,  "password_mode": False},
    {"id": "08", "file": "08_enhanced_audit.mdix",  "name": "Enhanced auditor (.mdix.au)",       "encrypted": False, "compressed": False, "audited": True,  "password_mode": False},
    {"id": "09", "file": "09_gzip_aes256.mdix",     "name": "GZip then AES-256",                 "encrypted": True,  "compressed": True,  "audited": False, "password_mode": True},
    {"id": "10", "file": "10_full_pipeline.mdix",   "name": "Full pipeline: GZip+AES-256+Audit", "encrypted": True,  "compressed": True,  "audited": True,  "password_mode": True},
]

def collect_output_files(output_subdir, source_file_path=None):
    """
    Collect produced files from the output directory AND the source directory
    (auditor puts .au files next to the source).
    """
    files = []
    if os.path.isdir(output_subdir):
        for f in sorted(os.listdir(output_subdir)):
            # Skip subdirectories and placeholder files
            fp = os.path.join(output_subdir, f)
            if os.path.isfile(fp) and not f.endswith(".placeholder") and not f.startswith("."):
                files.append({"name": f, "size_bytes": os.path.getsize(fp), "path": fp})

    # Auditor writes .au file next to the source — check there too
    if source_file_path:
        source_dir  = os.path.dirname(source_file_path)
        source_stem = os.path.splitext(os.path.basename(source_file_path))[0]
        au_path = os.path.join(source_dir, f"{source_stem}.mdix.au")
        if os.path.isfile(au_path):
            # Copy it to the output subdir so it appears in the results
            import shutil
            dest = os.path.join(output_subdir, f"{source_stem}.mdix.au")
            shutil.copy2(au_path, dest)
            if not any(f["path"] == dest for f in files):
                files.append({
                    "name":       f"{source_stem}.mdix.au",
                    "size_bytes": os.path.getsize(dest),
                    "path":       dest,
                })

    return files

def run_dlm_tests():
    os.makedirs(OUT_DLM, exist_ok=True)
    os.makedirs(OUT_INSPECT, exist_ok=True)
    results = []

    for t in DLM_TESTS:
        src_path   = os.path.join(DLM_DIR, t["file"])
        out_subdir = os.path.join(OUT_DLM, t["id"])
        os.makedirs(out_subdir, exist_ok=True)

        # Build compile command — pass --password only when supported and needed
        compile_cmd = [BINARY, "compile", src_path, "--output", out_subdir]
        extra_env   = {}
        if t["password_mode"]:
            compile_cmd += ["--password", DLM_PW]
            # Also set env var so encryptors that read from environment work
            extra_env["MDIX_DLM_PASSWORD"] = DLM_PW

        compile_r  = run(compile_cmd, extra_env=extra_env)
        compile_ok = compile_r["exit_code"] == 0

        output_files = collect_output_files(out_subdir, src_path)

        # Round-trip for encrypted files
        roundtrip = None
        if compile_ok and t["encrypted"] and output_files:
            enc_files = [f for f in output_files if f["name"].endswith(".enc")]
            if enc_files:
                enc_path = enc_files[0]["path"]
                rt_out   = os.path.join(out_subdir, "roundtrip")
                os.makedirs(rt_out, exist_ok=True)

                decrypt_cmd = [BINARY, "decrypt", enc_path, "--output", rt_out]
                decrypt_env = {}
                if t["password_mode"]:
                    decrypt_cmd += ["--password", DLM_PW]
                    decrypt_env["MDIX_DLM_PASSWORD"] = DLM_PW

                rt_r  = run(decrypt_cmd, extra_env=decrypt_env)
                rt_ok = rt_r["exit_code"] == 0

                rt_files        = collect_output_files(rt_out)
                insp_orig_r     = run([BINARY, "inspect", src_path, "--json"])
                inspect_orig    = insp_orig_r["stdout"] if insp_orig_r["exit_code"] == 0 else None
                inspect_rt      = None

                if rt_ok and rt_files:
                    restored_path = rt_files[0]["path"]
                    insp_rt_r     = run([BINARY, "inspect", restored_path, "--json"])
                    inspect_rt    = insp_rt_r["stdout"] if insp_rt_r["exit_code"] == 0 else None

                roundtrip = {
                    "status":           "passed" if rt_ok else "failed",
                    "exit_code":        rt_r["exit_code"],
                    "stderr":           rt_r["stderr"],
                    "original_inspect": inspect_orig,
                    "restored_inspect": inspect_rt,
                    "match":            inspect_orig == inspect_rt if (inspect_orig and inspect_rt) else None,
                }

        passed = compile_ok
        results.append({
            "id":           t["id"],
            "name":         t["name"],
            "file":         t["file"],
            "encrypted":    t["encrypted"],
            "compressed":   t["compressed"],
            "audited":      t["audited"],
            "password_mode": t["password_mode"],
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

    # First try running the pre-built example binary directly
    example_binary = "target/debug/examples/binary_capture"
    if os.path.isfile(example_binary):
        r = run([
            example_binary,
            "mdix_files/tests/binary/serialize_target.mdix",
            OUT_BINARY,
        ])
    else:
        # Fall back to cargo run
        r = run([
            "cargo", "run", "--example", "binary_capture",
            "--manifest-path", "dixscript/Cargo.toml", "--",
            "mdix_files/tests/binary/serialize_target.mdix",
            OUT_BINARY,
        ], timeout=180)

    result = {
        "status":     "passed" if r["exit_code"] == 0 else "failed",
        "exit_code":  r["exit_code"],
        "stderr":     r["stderr"],
        "size_bytes": None,
        "sections":   None,
        "b64":        None,
        "hex_dump":   None,
        "bin_path":   None,
    }

    if r["exit_code"] == 0:
        for line in r["stdout"].splitlines():
            if line.startswith("BINARY_SIZE:"):
                result["size_bytes"] = int(line.split(":", 1)[1])
            elif line.startswith("BINARY_SECTIONS:"):
                result["sections"] = int(line.split(":", 1)[1])
            elif line.startswith("BINARY_B64:"):
                result["b64"] = line.split(":", 1)[1]

    stem    = "serialize_target"
    hex_p   = f"{OUT_BINARY}/{stem}.hex"
    bin_p   = f"{OUT_BINARY}/{stem}.bin"
    result["bin_path"] = bin_p if os.path.isfile(bin_p) else None
    if os.path.isfile(hex_p):
        with open(hex_p) as f:
            result["hex_dump"] = f.read()

    status_str = result["status"].upper()
    size_str   = f"{result['size_bytes']} bytes" if result["size_bytes"] else "n/a"
    print(f"  {status_str} Binary capture ({size_str})")
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
        "import_tests":   import_results,
        "cyclic_tests":   cyclic_results,
        "binary_capture": binary_result,
        "dlm_tests":      dlm_results,
    }

    with open("edge-case-results.json", "w") as f:
        json.dump(output, f, indent=2)

    total_p = output["summary"]["total_passed"]
    total_f = output["summary"]["total_failed"]
    print(f"\nDone — passed={total_p} failed={total_f}")
    sys.exit(0 if total_f == 0 else 1)


if __name__ == "__main__":
    main()
