// Coverage for MdixDatabase.query()/queryMany() and the query.ts
// convenience wrappers.
//
// Fixture is copied verbatim from dixscript/tests/query_tests.rs (the
// core Rust reference tests for DixQuery) on purpose — every assertion
// below reproduces one of that file's test names using native JS Array
// methods (.filter/.sort/.slice/.some/.every/.reduce) instead of
// DixQuery's Rust closure chain (.where_/.order_by_desc/.select/...).
// Matching expected values against the exact same fixture is the actual
// proof that "use native Array methods on the decoded result" is a
// complete substitute for porting the chain itself — not just a claim
// in a comment.
//
// Prerequisite: `npm run build` must have been run first.
// Run with: node --test tests/
//
// Imports from dist/index.node.js — see enum-mixed-data.test.mjs's
// header comment for why (dist/index.js's --target bundler build isn't
// loadable by plain Node at all).

import test from "node:test";
import assert from "node:assert/strict";
import { MdixDatabase, query, queryMany } from "../dist/index.node.js";

const SOURCE = `
@DATA(
  app_name = "QueryTestApp"

  tasks::
  {
    name = "Backup",
    priority = 3
  },
  {
    name = "Docs",
    priority = 1
  },
  {
    name = "Audit",
    priority = 3
  },
  {
    name = "Deploy",
    priority = 2
  }

  servers.web1:
  status = "up"

  servers.db1:
  status = "down"

  servers.web2:
  status = "up"
)
`;

function loadFixture() {
  return MdixDatabase.loadStr(SOURCE);
}

// ── query() basics ─────────────────────────────────────────────────────

test("query returns None-equivalent ([]) for a missing or non-array path", () => {
  const db = loadFixture();
  assert.deepEqual(query(db, "does_not_exist"), []);
  assert.deepEqual(
    query(db, "app_name"),
    [],
    "app_name is a String, not an Array — query() should return [], not throw",
  );
});

test("query covers GroupArray items via the base path, no pattern needed", () => {
  const db = loadFixture();
  const tasks = query(db, "tasks");
  assert.equal(tasks.length, 4);
});

// ── where_ ⇒ .filter() ───────────────────────────────────────────────────

test("filter reproduces where_'s field-based matching", () => {
  const db = loadFixture();
  const highPriority = query(db, "tasks").filter((t) => t.priority === 3);
  assert.equal(highPriority.length, 2);
});

// ── select ⇒ .map() ──────────────────────────────────────────────────────

test("filter + map reproduces where_ followed by select", () => {
  const db = loadFixture();
  const names = query(db, "tasks")
    .filter((t) => t.priority === 3)
    .map((t) => t.name);
  assert.deepEqual(names, ["Backup", "Audit"]);
});

// ── order_by_desc + take(1) ⇒ .sort() + [0] ──────────────────────────────

test("sort descending + [0] reproduces order_by_desc().take(1)", () => {
  const db = loadFixture();
  // Backup and Audit are tied at priority 3. Array.prototype.sort is
  // stable (guaranteed since ES2019 — true in every Node version this
  // package supports), so ties keep their original relative order, same
  // as Rust's sort_by_key underneath order_by_desc — Backup (index 0)
  // wins the tie over Audit (index 2) in both, for the same reason.
  const top = [...query(db, "tasks")].sort((a, b) => b.priority - a.priority)[0];
  assert.equal(top.name, "Backup");
});

test("sort ascending + [0] reproduces order_by().take(1)", () => {
  const db = loadFixture();
  const bottom = [...query(db, "tasks")].sort((a, b) => a.priority - b.priority)[0];
  assert.equal(bottom.name, "Docs", "priority 1 is the lowest");
});

// ── skip ⇒ .slice() ──────────────────────────────────────────────────────

test("slice reproduces skip", () => {
  const db = loadFixture();
  const rest = query(db, "tasks").slice(3);
  assert.equal(rest.length, 1);
});

// ── group_by ⇒ reduce into a Map ─────────────────────────────────────────

test("reduce into a Map reproduces group_by, first-seen key order", () => {
  const db = loadFixture();
  const groups = query(db, "tasks").reduce((map, t) => {
    const key = t.priority;
    if (!map.has(key)) map.set(key, []);
    map.get(key).push(t);
    return map;
  }, new Map());

  // First-seen distinct priorities in fixture order: 3 (Backup), 1 (Docs), 2 (Deploy).
  assert.deepEqual([...groups.keys()], [3, 1, 2]);
  assert.equal(groups.get(3).length, 2);
});

// ── any / all ⇒ .some() / .every() ───────────────────────────────────────

test("some/every reproduce any/all", () => {
  const db = loadFixture();
  const tasks = query(db, "tasks");
  assert.ok(tasks.some((t) => t.priority === 1));
  assert.ok(!tasks.every((t) => t.priority === 3));
});

// ── first / last / nth ⇒ [0] / .at(-1) / [n] ─────────────────────────────

test("[0]/.at(-1)/[n] reproduce first/last/nth", () => {
  const db = loadFixture();
  const tasks = query(db, "tasks");
  assert.equal(tasks[0].name, "Backup");
  assert.equal(tasks.at(-1).name, "Deploy");
  assert.equal(tasks[1].name, "Docs");
});

// ── queryMany — sibling-wildcard matching ────────────────────────────────

test("queryMany matches sibling wildcarded paths", () => {
  const db = loadFixture();
  const upCount = queryMany(db, "servers.*.status").filter((s) => s === "up").length;
  assert.equal(upCount, 2);
  assert.equal(queryMany(db, "servers.*.status").length, 3, "all three servers should match the wildcard");
});

// ── sum / avg ⇒ .reduce() ─────────────────────────────────────────────────

test("reduce reproduces sum_int / avg_float", () => {
  const db = loadFixture();
  const priorities = query(db, "tasks").map((t) => t.priority);
  const sum = priorities.reduce((a, b) => a + b, 0);
  assert.equal(sum, 9); // 3 + 1 + 3 + 2
  assert.equal(sum / priorities.length, 2.25);
});

// ── min_by_key / max_by_key — first-max vs last-max tie-breaking ────────

test("reduce reproduces min_by_key (no tie in this fixture)", () => {
  const db = loadFixture();
  const cheapest = query(db, "tasks").reduce((min, t) => (t.priority < min.priority ? t : min));
  assert.equal(cheapest.name, "Docs");
});

test("max needs >= (not >) in the reducer to match max_by_key's tie-break", () => {
  const db = loadFixture();
  const tasks = query(db, "tasks");

  // Backup and Audit are tied at priority 3. Rust's Iterator::max_by_key
  // (which order_by_desc/max_by_key sit on) documents that it returns the
  // *last* maximum element on a tie -- so the Rust reference test expects
  // Audit, not Backup, here. A reducer using strict `>` gives the *first*
  // max instead (Backup) -- `>=` is what actually reproduces Rust's
  // behavior. Both are shown below so the difference is explicit, not a
  // trap someone finds out about later.
  const firstMax = tasks.reduce((max, t) => (t.priority > max.priority ? t : max));
  const lastMax = tasks.reduce((max, t) => (t.priority >= max.priority ? t : max));

  assert.equal(firstMax.name, "Backup", "strict > keeps the first-seen max");
  assert.equal(lastMax.name, "Audit", "matches Rust max_by_key's documented last-max tie-break");
});

// ── empty query terminal ops don't throw ─────────────────────────────────

test("terminal ops on an empty result behave the same as on DixQuery::new(vec![])", () => {
  const empty = query(loadFixture(), "does_not_exist");
  assert.equal(empty.length, 0);
  assert.equal(empty[0], undefined);
  assert.equal(empty.reduce((a, b) => a + b, 0), 0);
  assert.ok(!empty.some(() => true));
  assert.ok(empty.every(() => false), "vacuous truth — matches Array.prototype.every's own behavior on []");
});
