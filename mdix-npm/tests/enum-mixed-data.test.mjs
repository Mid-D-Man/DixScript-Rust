// mdix-npm has no test suite at all yet -- package.json has no test
// runner configured. This uses Node's built-in `node:test` so it needs
// zero new devDependencies.
//
// mdix-npm/src/index.ts is a thin re-export of the mdix-wasm bundler
// build plus a small Result-wrapping layer (ok/err/tryGet/unwrap) --
// that Result layer is the one piece of logic that's actually unique
// to this package (mdix-wasm's own #[wasm_bindgen_test] suite in
// tests/enum_mixed_data.rs already covers the underlying getters
// directly). This file exercises enum reads *through* tryGet/unwrap,
// since that's the path real consumers of @midmanstudio/mdix will use.
//
// Prerequisite: `npm run build` (builds the wasm bundle AND the TS
// layer into dist/) must have been run first -- this imports from the
// built output, not from src/, since that's what a real consumer sees.
//
// Run with: node --test test/enum-mixed-data.test.mjs
//
// Imports from dist/index.node.js, not dist/index.js: the latter is the
// --target bundler build, which contains a raw `.wasm` ESM import only an
// actual bundler knows how to resolve -- plain `node --test` throws
// ERR_UNKNOWN_FILE_EXTENSION on it before this file's own code even runs
// (confirmed directly). dist/index.node.js is the --target nodejs build,
// which is also exactly what package.json's "node" export condition
// resolves real `import ... from "@midmanstudio/mdix"` consumers to under
// plain Node -- so this exercises the same code path they get, not a
// test-only shortcut.
//
// Status.PENDING (= 2) and Role.EDITOR (= 1) are deliberately non-zero,
// non-conventional-default variants -- dixscript's AST resolver falls
// back to 0 on an enum-table lookup miss, and 0 is a different,
// valid-looking variant (ACTIVE/ADMIN) in both enums below, so a
// fallback bug would otherwise hide behind a coincidentally-correct 0.

import test from "node:test";
import assert from "node:assert/strict";
import { MdixDatabase, tryGet, unwrap } from "../dist/index.node.js";

const SOURCE = `
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2, ARCHIVED = 3 }
  Role   { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)
@DATA(
  app = "enum-mixed-data-fixture"

  user:
    name = "Alice",
    age<int> = 30,
    score<double> = 98.5,
    active<bool> = true,
    tags = ["admin", "verified"],
    status<enum> = Status.PENDING

  user.permissions::
    { role<enum> = Role.EDITOR, scope = "team" },
    { role<enum> = Role.ADMIN,  scope = "global" }
)
`;

function loadFixture() {
  return MdixDatabase.loadStr(SOURCE);
}

test("sibling fields are unaffected by the enum field", () => {
  const db = loadFixture();
  try {
    assert.equal(unwrap(tryGet(() => db.getString("user.name"))), "Alice");
    assert.equal(unwrap(tryGet(() => db.getInt("user.age"))), 30);
    assert.ok(Math.abs(unwrap(tryGet(() => db.getDouble("user.score"))) - 98.5) < 1e-9);
    assert.equal(unwrap(tryGet(() => db.getBool("user.active"))), true);
    assert.equal(unwrap(tryGet(() => db.getArrayLength("user.tags"))), 2);
  } finally {
    db.free?.();
  }
});

test("enum field resolves name, field, and value together", () => {
  const db = loadFixture();
  try {
    assert.equal(unwrap(tryGet(() => db.getEnumName("user.status"))), "Status");
    assert.equal(unwrap(tryGet(() => db.getEnumField("user.status"))), "PENDING");
    // PENDING is declared as 2 -- this is the assertion that would
    // actually catch a silent enum-table lookup-miss fallback to 0.
    assert.equal(unwrap(tryGet(() => db.getInt("user.status"))), 2);
  } finally {
    db.free?.();
  }
});

test("group array elements resolve their enum fields independently", () => {
  const db = loadFixture();
  try {
    assert.equal(unwrap(tryGet(() => db.getEnumField("user.permissions[0].role"))), "EDITOR");
    assert.equal(unwrap(tryGet(() => db.getInt("user.permissions[0].role"))), 1);
    assert.equal(unwrap(tryGet(() => db.getString("user.permissions[0].scope"))), "team");

    assert.equal(unwrap(tryGet(() => db.getEnumField("user.permissions[1].role"))), "ADMIN");
    assert.equal(unwrap(tryGet(() => db.getInt("user.permissions[1].role"))), 0);
    assert.equal(unwrap(tryGet(() => db.getString("user.permissions[1].scope"))), "global");
  } finally {
    db.free?.();
  }
});

test("a failed read comes back through tryGet as an error, not a throw", () => {
  const db = loadFixture();
  try {
    const result = tryGet(() => db.getEnumName("user.name")); // "name" is a string, not an enum
    assert.equal(result.ok, false);
  } finally {
    db.free?.();
  }
});
