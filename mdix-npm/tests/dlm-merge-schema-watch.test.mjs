// Coverage for the API surface that index.ts previously left un-exported:
// DLM compile/decompile, AST merge, schema validation, and the content-hash
// watcher. mdix-wasm's own #[wasm_bindgen_test] suite (tests/web.rs) already
// covers the underlying Rust behavior in depth — this file only confirms
// each binding is actually *reachable* through @dixscript/core's public
// export surface and that the JS-facing shapes (camelCase methods, JSON
// conflict/error arrays) look the way consumers will actually use them.
//
// Prerequisite: `npm run build` (wasm bundle + TS layer into dist/) must
// have been run first — this imports from the built output, not src/.
//
// Run with: node --test tests/

import test from "node:test";
import assert from "node:assert/strict";
import {
  MdixDatabase,
  compileWithDlm,
  decompileWithDlm,
  mergeSources,
  mergeSourcesWeighted,
  MdixSchema,
  MdixWatcher,
} from "../dist/index.js";

// ── DLM (compress / encrypt / audit) ─────────────────────────────────────

test("compileWithDlm passes through cleanly when source has no @DLM section", () => {
  const source = `@DATA(plain = "just plain data, no @DLM section")`;

  const outcome = compileWithDlm(source, "dlm-passthrough-test");
  assert.equal(outcome.isSuccess(), true);
  assert.ok(outcome.processedData().length > 0);
  assert.equal(outcome.keyFileContent(), undefined);
  assert.equal(outcome.executedModules().length, 0);

  // Empty string keyFileContent tells decompileWithDlm to unpack directly
  // rather than attempt decryption -- the mirror image of the no-modules
  // case above.
  const db = decompileWithDlm(outcome.processedData(), "", "dlm-passthrough-test");
  assert.equal(db.getString("plain"), "just plain data, no @DLM section");
  db.free?.();
});

test("compileWithDlm rejects empty source", () => {
  assert.throws(() => compileWithDlm("", "empty-test"));
});

test("decompileWithDlm rejects empty data", () => {
  assert.throws(() => decompileWithDlm(new Uint8Array(), "", "empty-test"));
});

// ── Merge ─────────────────────────────────────────────────────────────────

test("mergeSources combines disjoint data from two sources", () => {
  const outcome = mergeSources(["@DATA(x = 1)", "@DATA(y = 2)"]);
  const db = outcome.database();
  assert.equal(db.getInt("x"), 1);
  assert.equal(db.getInt("y"), 2);
  db.free?.();
});

test("mergeSources primary_wins keeps the first source's value on conflict", () => {
  const outcome = mergeSources(["@DATA(x = 1)", "@DATA(x = 2)"], "primary_wins");
  const db = outcome.database();
  assert.equal(db.getInt("x"), 1);
  db.free?.();
});

test("mergeSources secondary_wins keeps the second source's value on conflict", () => {
  const outcome = mergeSources(["@DATA(x = 1)", "@DATA(x = 2)"], "secondary_wins");
  const db = outcome.database();
  assert.equal(db.getInt("x"), 2);
  db.free?.();
});

test("mergeSources conflicts() reports the winning source and path", () => {
  const outcome = mergeSources(["@DATA(x = 1)", "@DATA(x = 2)"], "primary_wins");
  const conflicts = outcome.conflicts();
  assert.ok(Array.isArray(conflicts));
  assert.ok(conflicts.length > 0, "a genuine key conflict should show up in the report");
  assert.ok("path" in conflicts[0]);
  assert.ok("winningSource" in conflicts[0]);
});

test("mergeSources rejects an empty source list", () => {
  assert.throws(() => mergeSources([]));
});

test("mergeSourcesWeighted respects explicit per-source weights", () => {
  const outcome = mergeSourcesWeighted([
    ["@DATA(x = 1)", 0.9],
    ["@DATA(x = 2)", 0.1],
  ]);
  const db = outcome.database();
  // Higher explicit weight should win under the default "weighted" strategy.
  assert.equal(db.getInt("x"), 1);
  db.free?.();
});

test("MdixDatabase.mergeWith merges two already-loaded databases", () => {
  const primary = MdixDatabase.loadStr("@DATA(x = 1)");
  const secondary = MdixDatabase.loadStr("@DATA(y = 2)");
  const outcome = primary.mergeWith(secondary);
  const merged = outcome.database();
  assert.equal(merged.getInt("x"), 1);
  assert.equal(merged.getInt("y"), 2);
  primary.free?.();
  secondary.free?.();
  merged.free?.();
});

// ── Schema validation ─────────────────────────────────────────────────────

test("validateSchema passes for data matching every required field", () => {
  const db = MdixDatabase.loadStr(`@DATA(app_name = "MyApp", port = 8080)`);
  const schema = new MdixSchema().requireString("app_name").requireInt("port");

  const report = db.validateSchema(schema);
  assert.equal(report.isValid, true);
  assert.equal(report.errorCount, 0);
  db.free?.();
});

test("validateSchema reports a missing required field", () => {
  const db = MdixDatabase.loadStr(`@DATA(app_name = "MyApp")`);
  const schema = new MdixSchema().requireString("app_name").requireInt("port");

  const report = db.validateSchema(schema);
  assert.equal(report.isValid, false);
  assert.deepEqual(report.failedPaths(), ["port"]);
  db.free?.();
});

test("validateSchema errors() returns a typed, inspectable array", () => {
  const db = MdixDatabase.loadStr(`@DATA(port = "not-a-number")`);
  const schema = new MdixSchema().requireInt("port");

  const report = db.validateSchema(schema);
  const errors = report.errors();
  assert.ok(Array.isArray(errors));
  assert.ok(errors.length > 0);
  assert.equal(errors[0].path, "port");
  db.free?.();
});

test("the same MdixSchema instance can validate more than one database", () => {
  const schema = new MdixSchema().requireString("name");
  const dbA = MdixDatabase.loadStr(`@DATA(name = "Alice")`);
  const dbB = MdixDatabase.loadStr(`@DATA(name = "Bob")`);

  assert.equal(dbA.validateSchema(schema).isValid, true);
  assert.equal(dbB.validateSchema(schema).isValid, true);
  dbA.free?.();
  dbB.free?.();
});

// ── Hot reload / watch ────────────────────────────────────────────────────

test("MdixWatcher reports unchanged on repeated identical content", () => {
  const watcher = new MdixWatcher();
  const source = `@DATA(count = 1)`;

  const first = watcher.check(source);
  assert.equal(first.changed, true);
  const db1 = first.database();
  assert.equal(db1.getInt("count"), 1);
  db1.free?.();

  const second = watcher.check(source);
  assert.equal(second.changed, false);
});

test("MdixWatcher reports changed when content differs", () => {
  const watcher = new MdixWatcher();
  watcher.check(`@DATA(count = 1)`);

  const outcome = watcher.check(`@DATA(count = 2)`);
  assert.equal(outcome.changed, true);
  const db = outcome.database();
  assert.equal(db.getInt("count"), 2);
  db.free?.();
});

test("MdixWatcher.hasChanged is a cheap pre-check that doesn't update state", () => {
  const watcher = new MdixWatcher();
  const source = `@DATA(count = 1)`;

  assert.equal(watcher.hasChanged(source), true, "nothing seen yet -- should report changed");
  assert.equal(watcher.hasChanged(source), true, "hasChanged must not itself update last-seen state");

  watcher.check(source);
  assert.equal(watcher.hasChanged(source), false, "same content as the last check() call");
});

test("MdixWatcher.reset forgets previously seen content", () => {
  const watcher = new MdixWatcher();
  const source = `@DATA(count = 1)`;
  watcher.check(source);
  assert.equal(watcher.hasChanged(source), false);

  watcher.reset();
  assert.equal(watcher.hasChanged(source), true, "reset should forget the last-seen hash");
});
