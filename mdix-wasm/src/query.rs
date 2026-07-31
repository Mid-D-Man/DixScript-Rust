// Structural typing on purpose, not `import type { MdixDatabase } from
// "../wasm-pkg/mdix_wasm.js"` — that would tie this file to one specific
// wasm-pack target. Both index.ts (bundler target) and index.node.ts
// (nodejs target) re-export this same file; any object with query()/
// queryMany() methods shaped like this satisfies it, regardless of which
// wasm-pkg it actually came from.
interface Queryable {
  query(path: string): string;
  queryMany(pattern: string): string;
}

/**
 * Parses `db.query(path)`'s JSON-string result into a typed array.
 *
 * DixQuery's Rust-side chain (`.where_()/.orderBy_desc()/.select()/
 * .groupBy()/.any()/.all()/.minByKey()/.maxByKey()`, all in
 * `dixscript/src/Runtime/query.rs`) isn't ported to JS as its own
 * chainable API — every one of those operations works on plain
 * `DixValue` data, which once decoded here is exactly what you already
 * have. Use native `Array` methods on the result instead:
 *
 * ```ts
 * query<Task>(db, "tasks")
 *   .filter(t => t.priority === 3)               // where_
 *   .sort((a, b) => b.priority - a.priority)      // order_by_desc
 *   .map(t => t.name);                            // select
 * ```
 *
 * Cheat sheet for the rest of `query.rs`'s methods, once you have the
 * array:
 * - `count()` → `.length`
 * - `any(pred)` / `all(pred)` → `.some(pred)` / `.every(pred)`
 * - `first()` / `last()` / `nth(i)` → `[0]` / `.at(-1)` / `[i]`
 * - `sum_int()` / `avg_float()` → `.reduce((a, b) => a + b.field, 0)`,
 *   divide by `.length` for the average
 * - `group_by(key_fn)` → build a `Map` keyed however you like:
 *   `arr.reduce((m, x) => (m.set(k(x), [...(m.get(k(x)) ?? []), x]), m), new Map())`
 * - `distinct()` → dedupe via `Set` (primitives) or a `Map` keyed by
 *   whatever makes two elements "the same" for your data (objects)
 *
 * Returns `[]` for a path that doesn't exist or isn't an `Array` — same
 * as `query()`'s own JSON output; that's a normal "no match" outcome
 * here, not an error.
 */
export function query<T = unknown>(db: Queryable, path: string): T[] {
  return JSON.parse(db.query(path)) as T[];
}

/**
 * Same as {@link query}, but gathers every *sibling* path matched by a
 * glob `pattern` (whole-segment `*` only, e.g. `"servers.*.status"` —
 * same syntax as the core's `select_many`) into one array, instead of
 * one Array/GroupArray path's own items. See `dixscript/src/Runtime/
 * query.rs`'s module doc for exactly what does and doesn't match.
 */
export function queryMany<T = unknown>(db: Queryable, pattern: string): T[] {
  return JSON.parse(db.queryMany(pattern)) as T[];
}
