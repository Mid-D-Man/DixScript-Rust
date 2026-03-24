#!/usr/bin/env python3
"""
Edge case test runner: imports, cyclic detection, DLM pipeline, binary capture,
cloud imports, and cloud cyclic detection.
Produces edge-case-results.json consumed by the HTML report.
"""

import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

BINARY        = os.environ.get("MDIX_BINARY",         "target/debug/mdix")
BUILD_NUM     = os.environ.get("BUILD_NUM",            "0")
BRANCH        = os.environ.get("BRANCH",               "unknown")
COMMIT        = os.environ.get("COMMIT",               "unknown")[:8]
BUILD_DATE    = os.environ.get("BUILD_DATE",           "")
DLM_PW        = os.environ.get("DLM_TEST_PASSWORD",    "dixscript_ci_test_password_2025")
REGISTRY_BASE = os.environ.get(
    "REGISTRY_BASE", "https://dixscript-docs.pages.dev/api/registry"
)

IMPORTS_DIR     = "mdix_files/tests/imports"
DLM_DIR         = "mdix_files/tests/dlm"
BINARY_FIXTURE  = "mdix_files/tests/dlm/serialize_target.mdix"
OUT_DLM         = "edge-case-output/dlm"
OUT_BINARY      = "binary-output"
OUT_INSPECT     = "edge-case-output/inspect"
OUT_CLOUD       = "edge-case-output/cloud"


# ── Subprocess helper ─────────────────────────────────────────────────────────

def run(cmd, cwd=None, timeout=120, extra_env=None):
    start = time.time()
    env = os.environ.copy()
    if extra_env:
        env.update(extra_env)
    try:
        r = subprocess.run(
            cmd, capture_output=True, text=True,
            cwd=cwd, timeout=timeout, env=env,
        )
        return {
            "exit_code":  r.returncode,
            "stdout":     r.stdout.strip()[:4000],
            "stderr":     r.stderr.strip()[:4000],
            "elapsed_ms": int((time.time() - start) * 1000),
        }
    except subprocess.TimeoutExpired:
        return {
            "exit_code": -1, "stdout": "",
            "stderr": "TIMEOUT", "elapsed_ms": timeout * 1000,
        }
    except Exception as e:
        return {"exit_code": -1, "stdout": "", "stderr": str(e), "elapsed_ms": 0}


# ── Cloud helpers ─────────────────────────────────────────────────────────────

def check_cloud_connectivity():
    """Ping the registry and return (reachable, latency_ms, error_or_None)."""
    url = f"{REGISTRY_BASE}/base_types.mdix"
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "DixScript-CI/1.0"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            resp.read(64)
        return True, int((time.time() - start) * 1000), None
    except urllib.error.HTTPError as e:
        return False, int((time.time() - start) * 1000), f"HTTP {e.code}"
    except Exception as e:
        return False, int((time.time() - start) * 1000), str(e)


def fetch_file_sha256(filename):
    """Download a registry file and return its SHA-256 hex digest, or None."""
    url = f"{REGISTRY_BASE}/{filename}"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "DixScript-CI/1.0"})
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = resp.read()
        return hashlib.sha256(data).hexdigest()
    except Exception:
        return None


def _skipped(name, file_or_none=None):
    """Return a uniform skipped-result dict."""
    r = {
        "name":    name,
        "status":  "skipped",
        "skipped": True,
        "reason":  "Registry unreachable",
        "exit_code": None,
        "stdout":  "",
        "stderr":  "",
        "elapsed_ms": 0,
    }
    if file_or_none:
        r["file"] = file_or_none
    return r


# ── 1. Local import tests ─────────────────────────────────────────────────────

LOCAL_IMPORT_TESTS = [
    {"id": "01", "file": "01_basic_import.mdix",
     "name": "Basic enum and type access via alias"},
    {"id": "02", "file": "02_alias_funcs_in_quickfuncs.mdix",
     "name": "Imported funcs called inside local QuickFuncs"},
    {"id": "03", "file": "03_alias_funcs_in_data.mdix",
     "name": "Imported funcs called directly in DATA"},
    {"id": "04", "file": "04_funcs_in_funcs.mdix",
     "name": "Imported function calls nested inside other imported calls"},
    {"id": "05", "file": "05_enum_in_funcs.mdix",
     "name": "Imported enum values passed as arguments to imported functions"},
]


def run_import_tests():
    results = []
    for t in LOCAL_IMPORT_TESTS:
        path   = os.path.join(IMPORTS_DIR, t["file"])
        r      = run([BINARY, "compile", path])
        passed = r["exit_code"] == 0
        results.append({
            "id":         t["id"],
            "name":       t["name"],
            "file":       t["file"],
            "status":     "passed" if passed else "failed",
            "skipped":    False,
            "exit_code":  r["exit_code"],
            "stdout":     r["stdout"],
            "stderr":     r["stderr"],
            "elapsed_ms": r["elapsed_ms"],
        })
        print(f"  {'PASS' if passed else 'FAIL'} [{t['id']}] {t['name']}")
    return results


# ── 2. Local cyclic detection ─────────────────────────────────────────────────

def run_cyclic_tests():
    results = []
    path     = os.path.join(IMPORTS_DIR, "circular_a.mdix")
    r        = run([BINARY, "compile", path])
    combined = (r["stderr"] + r["stdout"]).lower()
    error_detected = r["exit_code"] != 0
    keyword_found  = any(
        k in combined for k in ["circular", "cyclic", "cycle", "import chain"]
    )
    passed = error_detected
    results.append({
        "name":           "Cyclic import A → B → A (local filesystem)",
        "status":         "passed" if passed else "failed",
        "skipped":        False,
        "exit_code":      r["exit_code"],
        "error_detected": error_detected,
        "keyword_found":  keyword_found,
        "stderr":         r["stderr"],
        "elapsed_ms":     r["elapsed_ms"],
    })
    print(f"  {'PASS' if passed else 'FAIL'} Local cyclic A→B→A "
          f"(exit={r['exit_code']}, keyword={keyword_found})")
    return results


# ── 3. Cloud import tests ─────────────────────────────────────────────────────

# expected_pass=True  → compiler exit 0 is a pass
# expected_pass=False → any non-zero exit is a pass (error expected)
CLOUD_IMPORT_TESTS = [
    {
        "id":            "06",
        "file":          "06_cloud_basic.mdix",
        "name":          "Basic cloud import — no hash verification",
        "expected_pass": True,
    },
    {
        "id":            "07",
        "file":          "07_cloud_verify_invalid.mdix",
        "name":          "Cloud import with deliberately wrong hash (HashMismatch expected)",
        "expected_pass": False,
        "error_keywords": ["hash", "mismatch", "verify", "checksum", "integrity"],
    },
    {
        "id":            "08",
        "file":          "08_cloud_404.mdix",
        "name":          "Cloud import — 404 not found (CloudFetchFailed expected)",
        "expected_pass": False,
        "error_keywords": ["404", "not found", "fetch", "cloud", "http"],
    },
    {
        "id":            "09",
        "file":          "09_cloud_transitive.mdix",
        "name":          "Transitive cloud import — game_helpers → base_types",
        "expected_pass": True,
    },
]


def run_cloud_import_tests(cloud_available):
    os.makedirs(OUT_CLOUD, exist_ok=True)
    results = []

    if not cloud_available:
        for t in CLOUD_IMPORT_TESTS:
            r = _skipped(t["name"], t["file"])
            r["id"] = t["id"]
            results.append(r)
            print(f"  SKIP [{t['id']}] {t['name']}")
        return results

    for t in CLOUD_IMPORT_TESTS:
        path = os.path.join(IMPORTS_DIR, t["file"])
        r    = run([BINARY, "compile", path], timeout=60)

        passed = (r["exit_code"] == 0) if t["expected_pass"] else (r["exit_code"] != 0)

        keyword_found = None
        if not t["expected_pass"] and "error_keywords" in t:
            combined = (r["stderr"] + r["stdout"]).lower()
            keyword_found = any(k in combined for k in t["error_keywords"])

        entry = {
            "id":            t["id"],
            "name":          t["name"],
            "file":          t["file"],
            "expected_pass": t["expected_pass"],
            "status":        "passed" if passed else "failed",
            "skipped":       False,
            "exit_code":     r["exit_code"],
            "stdout":        r["stdout"],
            "stderr":        r["stderr"],
            "elapsed_ms":    r["elapsed_ms"],
        }
        if keyword_found is not None:
            entry["error_keyword_found"] = keyword_found

        results.append(entry)

        if not t["expected_pass"] and passed:
            tag = "PASS (error correctly detected)"
        else:
            tag = "PASS" if passed else "FAIL"
        print(f"  {tag} [{t['id']}] {t['name']}")

    return results


# ── 4. Cloud cyclic detection ─────────────────────────────────────────────────

def run_cloud_cyclic_tests(cloud_available):
    results = []

    if not cloud_available:
        r = _skipped("Cloud cyclic import (cloud_a → cloud_b → cloud_a)")
        results.append(r)
        print("  SKIP Cloud cyclic detection (registry offline)")
        return results

    path     = os.path.join(IMPORTS_DIR, "10_cloud_cyclic.mdix")
    r        = run([BINARY, "compile", path], timeout=60)
    combined = (r["stderr"] + r["stdout"]).lower()

    error_detected = r["exit_code"] != 0
    keyword_found  = any(
        k in combined for k in ["circular", "cyclic", "cycle", "import chain"]
    )
    passed = error_detected

    results.append({
        "name":           "Cloud cyclic import (cloud_a → cloud_b → cloud_a)",
        "status":         "passed" if passed else "failed",
        "skipped":        False,
        "exit_code":      r["exit_code"],
        "error_detected": error_detected,
        "keyword_found":  keyword_found,
        "stderr":         r["stderr"],
        "elapsed_ms":     r["elapsed_ms"],
    })
    print(f"  {'PASS' if passed else 'FAIL'} Cloud cyclic "
          f"(exit={r['exit_code']}, keyword={keyword_found})")
    return results


# ── 5. DLM pipeline ───────────────────────────────────────────────────────────

DLM_TESTS = [
    {"id": "01", "file": "01_gzip.mdix",             "name": "GZip compression",
     "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "02", "file": "02_bzip2.mdix",            "name": "BZip2 compression",
     "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "03", "file": "03_lzma.mdix",             "name": "LZMA compression",
     "encrypted": False, "compressed": True,  "audited": False, "password_mode": False},
    {"id": "04", "file": "04_aes128_password.mdix",  "name": "AES-128 password encryption",
     "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "05", "file": "05_aes256_password.mdix",  "name": "AES-256 password encryption",
     "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "06", "file": "06_chacha20_password.mdix","name": "ChaCha20 password encryption",
     "encrypted": True,  "compressed": False, "audited": False, "password_mode": True},
    {"id": "07", "file": "07_diy_audit.mdix",        "name": "DIY auditor (.mdix.au)",
     "encrypted": False, "compressed": False, "audited": True,  "password_mode": False},
    {"id": "08", "file": "08_enhanced_audit.mdix",   "name": "Enhanced auditor (.mdix.au)",
     "encrypted": False, "compressed": False, "audited": True,  "password_mode": False},
    {"id": "09", "file": "09_gzip_aes256.mdix",      "name": "GZip then AES-256",
     "encrypted": True,  "compressed": True,  "audited": False, "password_mode": True},
    {"id": "10", "file": "10_full_pipeline.mdix",    "name": "Full pipeline: GZip+AES-256+Audit",
     "encrypted": True,  "compressed": True,  "audited": True,  "password_mode": True},
]


def collect_output_files(output_subdir, source_file_path=None):
    files = []
    if os.path.isdir(output_subdir):
        for f in sorted(os.listdir(output_subdir)):
            fp = os.path.join(output_subdir, f)
            if os.path.isfile(fp) and not f.endswith(".placeholder") and not f.startswith("."):
                files.append({"name": f, "size_bytes": os.path.getsize(fp), "path": fp})

    if source_file_path:
        import shutil
        source_dir  = os.path.dirname(source_file_path)
        source_stem = os.path.splitext(os.path.basename(source_file_path))[0]
        au_path = os.path.join(source_dir, f"{source_stem}.mdix.au")
        if os.path.isfile(au_path):
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

        compile_cmd = [BINARY, "compile", src_path, "--output", out_subdir]
        extra_env   = {}
        if t["password_mode"]:
            compile_cmd += ["--password", DLM_PW]
            extra_env["MDIX_DLM_PASSWORD"] = DLM_PW

        compile_r  = run(compile_cmd, extra_env=extra_env)
        compile_ok = compile_r["exit_code"] == 0

        output_files = collect_output_files(out_subdir, src_path)

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

                rt_files     = collect_output_files(rt_out)
                insp_orig_r  = run([BINARY, "inspect", src_path, "--json"])
                inspect_orig = insp_orig_r["stdout"] if insp_orig_r["exit_code"] == 0 else None
                inspect_rt   = None

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
                    "match": (inspect_orig == inspect_rt)
                        if (inspect_orig and inspect_rt) else None,
                }

        passed = compile_ok
        results.append({
            "id":            t["id"],
            "name":          t["name"],
            "file":          t["file"],
            "encrypted":     t["encrypted"],
            "compressed":    t["compressed"],
            "audited":       t["audited"],
            "password_mode": t["password_mode"],
            "status":        "passed" if passed else "failed",
            "exit_code":     compile_r["exit_code"],
            "stdout":        compile_r["stdout"],
            "stderr":        compile_r["stderr"],
            "elapsed_ms":    compile_r["elapsed_ms"],
            "output_files":  output_files,
            "roundtrip":     roundtrip,
        })
        print(f"  {'PASS' if passed else 'FAIL'} [{t['id']}] {t['name']}")

    return results


# ── 6. Binary capture ─────────────────────────────────────────────────────────

def run_binary_capture():
    os.makedirs(OUT_BINARY, exist_ok=True)

    example_binary = "target/debug/examples/binary_capture"
    if os.path.isfile(example_binary):
        r = run([example_binary, BINARY_FIXTURE, OUT_BINARY])
    else:
        r = run([
            "cargo", "run", "--example", "binary_capture",
            "--manifest-path", "dixscript/Cargo.toml", "--",
            BINARY_FIXTURE, OUT_BINARY,
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

    stem  = "serialize_target"
    hex_p = f"{OUT_BINARY}/{stem}.hex"
    bin_p = f"{OUT_BINARY}/{stem}.bin"
    result["bin_path"] = bin_p if os.path.isfile(bin_p) else None
    if os.path.isfile(hex_p):
        with open(hex_p) as fh:
            result["hex_dump"] = fh.read()

    status_str = result["status"].upper()
    size_str   = f"{result['size_bytes']} bytes" if result["size_bytes"] else "n/a"
    print(f"  {status_str} Binary capture ({size_str})")
    return result


# ── Main ──────────────────────────────────────────────────────────────────────

def counts(lst):
    p = sum(1 for x in lst if x.get("status") == "passed")
    s = sum(1 for x in lst if x.get("status") == "skipped")
    f = len(lst) - p - s
    return p, f, s


def main():
    print("=== DixScript Edge Case Tests ===")

    print("\n[1/7] Cloud connectivity check")
    cloud_ok, latency_ms, cloud_err = check_cloud_connectivity()
    if cloud_ok:
        print(f"  ONLINE  Registry reachable ({latency_ms}ms) — {REGISTRY_BASE}")
    else:
        print(f"  OFFLINE Registry unreachable: {cloud_err}")
        print("         Cloud import and cloud cyclic tests will be skipped.")

    print("\n[2/7] Local import tests")
    import_results = run_import_tests()

    print("\n[3/7] Local cyclic detection")
    cyclic_results = run_cyclic_tests()

    print("\n[4/7] Cloud import tests")
    cloud_import_results = run_cloud_import_tests(cloud_ok)

    print("\n[5/7] Cloud cyclic detection")
    cloud_cyclic_results = run_cloud_cyclic_tests(cloud_ok)

    print("\n[6/7] DLM pipeline tests")
    dlm_results = run_dlm_tests()

    print("\n[7/7] Binary serialization capture")
    binary_result = run_binary_capture()

    ip,  if_,  _   = counts(import_results)
    cp,  cf,   _   = counts(cyclic_results)
    cip, cif,  cis = counts(cloud_import_results)
    ccp, ccf,  ccs = counts(cloud_cyclic_results)
    dp,  df,   _   = counts(dlm_results)
    bp             = 1 if binary_result["status"] == "passed" else 0

    output = {
        "build":  BUILD_NUM,
        "branch": BRANCH,
        "commit": COMMIT,
        "date":   BUILD_DATE,
        "cloud": {
            "available":     cloud_ok,
            "latency_ms":    latency_ms,
            "error":         cloud_err,
            "registry_base": REGISTRY_BASE,
        },
        "summary": {
            "imports_passed":        ip,  "imports_failed":        if_,
            "cyclic_passed":         cp,  "cyclic_failed":         cf,
            "cloud_imports_passed":  cip, "cloud_imports_failed":  cif,
            "cloud_imports_skipped": cis,
            "cloud_cyclic_passed":   ccp, "cloud_cyclic_failed":   ccf,
            "cloud_cyclic_skipped":  ccs,
            "binary_passed":         bp,
            "dlm_passed":            dp,  "dlm_failed":            df,
            "total_passed":  ip + cp + cip + ccp + bp + dp,
            "total_failed":  if_ + cf + cif + ccf + (1 - bp) + df,
            "total_skipped": cis + ccs,
        },
        "import_tests":       import_results,
        "cyclic_tests":       cyclic_results,
        "cloud_import_tests": cloud_import_results,
        "cloud_cyclic_tests": cloud_cyclic_results,
        "binary_capture":     binary_result,
        "dlm_tests":          dlm_results,
    }

    with open("edge-case-results.json", "w") as fh:
        json.dump(output, fh, indent=2)

    total_p = output["summary"]["total_passed"]
    total_f = output["summary"]["total_failed"]
    total_s = output["summary"]["total_skipped"]
    print(f"\nDone — passed={total_p}  failed={total_f}  skipped={total_s}")
    sys.exit(0 if total_f == 0 else 1)


if __name__ == "__main__":
    main()
