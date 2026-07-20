"""
Enum-only @DATA (no strings/ints/other primitives at all, and zero
@QUICKFUNCS anywhere) round-tripped through DLM compression alone,
encryption alone, and both together.

test_enum_mixed_data.py covers enum fields sitting alongside ordinary
sibling data via load_str -- direct source parsing, no DLM at all. This
file closes the gap that left: this project has zero DLM coverage of any
kind before this, let alone the specific "enum(s) are the *only* @DATA
content" shape that let a real bug through in dixscript's core (see the
header comment in mdix_files/tests/dlm/11_enum_only_gzip.mdix for the full
mechanics of why "has enums, has nothing else" is the shape that mattered
-- mdix-wasm's dlm_compression.rs found and fixed it there first; this
mirrors that coverage on the Python side against the same underlying
dixscript pipeline via load_encrypted).

Unlike mdix-wasm (which has compile_with_dlm and can build its own
fixtures in-memory at test time), this binding's exposed surface is
load-only -- MdixDatabase never wraps a "compile" entry point, only
load_encrypted / load_encrypted_password / load_encrypted_bytes. So these
fixtures are pre-compiled binaries (fixtures/enum_dlm/*.mdix.enc +
*.mdix.key), generated from the source .mdix files in
mdix_files/tests/dlm/ by scripts/generate_enum_dlm_fixtures.sh, and
checked in rather than produced at test time. Re-run that script after any
change to those source files, or after any fix that touches DLM/enum
resolution, so these binaries stay current.

Adjust the import path below if `midmanstudio.mdix` differs from what's
actually installed in your environment.
"""
from pathlib import Path

import pytest
from midmanstudio.mdix import MdixDatabase

FIXTURES_DIR = Path(__file__).parent / "fixtures" / "enum_dlm"


def _fixture_paths(base_name: str) -> tuple[str, str]:
    enc_path = FIXTURES_DIR / f"{base_name}.mdix.enc"
    key_path = FIXTURES_DIR / f"{base_name}.mdix.key"
    assert enc_path.exists(), (
        f"fixture should exist at {enc_path} -- "
        f"run scripts/generate_enum_dlm_fixtures.sh if missing"
    )
    return str(enc_path), str(key_path)


def _assert_enum_only_data_resolves_correctly(db: MdixDatabase) -> None:
    """Shared assertions for all three fixtures -- they're compiled from
    sources that differ only in their @DLM module list (see
    mdix_files/tests/dlm/11..13_enum_only_*.mdix), so the resolved @DATA
    content is identical in every case. If any one of the three tests
    below fails while the others pass, the failure is specific to that DLM
    module combination, not to enum resolution in general.
    """
    assert db.get_enum_name("status") == "Status"
    assert db.get_enum_field("status") == "PENDING"
    # PENDING is declared as 2. A silent enum-table lookup-miss falls back
    # to 0, which happens to be a different, valid-looking variant
    # (ACTIVE) -- this is the assertion that would actually catch that.
    assert db.get_int("status") == 2

    assert db.get_enum_name("role") == "Role"
    assert db.get_enum_field("role") == "EDITOR"
    assert db.get_int("role") == 1

    # Nested inside a GroupArray -- the exact spot nested-path resolution
    # has broken before (mdix-scaffold GroupArray regression), now
    # combined with the DLM round trip too.
    assert db.get_enum_field("assignments[0].role") == "ADMIN"
    assert db.get_int("assignments[0].role") == 0
    assert db.get_enum_field("assignments[1].role") == "VIEWER"
    assert db.get_int("assignments[1].role") == 2


class TestEnumOnlyDataThroughDlm:
    def test_compression_alone_round_trips_correctly(self):
        enc_path, key_path = _fixture_paths("11_enum_only_gzip")
        db = MdixDatabase.load_encrypted(enc_path, key_path)
        try:
            _assert_enum_only_data_resolves_correctly(db)
        finally:
            db.close()

    def test_encryption_alone_round_trips_correctly(self):
        enc_path, key_path = _fixture_paths("12_enum_only_aes256")
        db = MdixDatabase.load_encrypted(enc_path, key_path)
        try:
            _assert_enum_only_data_resolves_correctly(db)
        finally:
            db.close()

    def test_compression_and_encryption_round_trip_correctly(self):
        enc_path, key_path = _fixture_paths("13_enum_only_gzip_aes256")
        db = MdixDatabase.load_encrypted(enc_path, key_path)
        try:
            _assert_enum_only_data_resolves_correctly(db)
        finally:
            db.close()

    def test_auto_detected_key_file_also_round_trips_correctly(self):
        # Same as test_compression_and_encryption_round_trip_correctly but
        # omitting key_path -- load_encrypted(enc_path) should auto-detect
        # the sibling .mdix.key file the same way `mdix decrypt` does
        # without --key.
        enc_path, _key_path = _fixture_paths("13_enum_only_gzip_aes256")
        db = MdixDatabase.load_encrypted(enc_path)
        try:
            _assert_enum_only_data_resolves_correctly(db)
        finally:
            db.close()
