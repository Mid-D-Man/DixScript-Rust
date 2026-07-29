# @dixscript/core

WebAssembly runtime for DixScript (`.mdix`) — works in the browser,
Node.js, and any bundler that supports WASM (Vite, webpack, Rollup).

## Installation
```bash
npm install @dixscript/core
```

## Docs

Full language reference, `.mdix` syntax, and per-binding guides:
**https://dixscript-docs.pages.dev**

This README covers the `@dixscript/core` JS/TS API specifically — every
example below maps 1:1 onto a `#[wasm_bindgen]` binding in `mdix-wasm`.

## Quick start
```typescript
import { MdixDatabase, MdixBuilder, tryGet } from "@dixscript/core";

// Load from a .mdix source string
const db = MdixDatabase.loadStr(`
  @DATA(
    app_name = "MyApp"
    port     = 8080
    ssl      = true
  )
`);

// Direct access — throws on error
const name = db.getString("app_name"); // "MyApp"
const port = db.getInt("port");        // 8080

// Safe access — returns MdixResult<T>, never throws
const result = tryGet(() => db.getString("missing_key"));
if (result.ok) console.log(result.value);
else           console.error(result.error);

db.free();
```

## Building programmatically
```typescript
import { MdixBuilder } from "@dixscript/core";

const db = new MdixBuilder()
  .setConfigVersion("1.0.0")
  .addEnum("LogLevel", JSON.stringify(["DEBUG", "INFO", "WARN", "ERROR"]))
  .withString("app_name", "MyGame")
  .withInt("port", 8080)
  .withBool("ssl", true)
  .withEnumValue("log_level", "LogLevel", "INFO")
  .withTableProperties("server", JSON.stringify({
    host: "localhost",
    port: 8080,
    ssl:  true
  }))
  .withGroupArray("admins", JSON.stringify(["alice", "bob"]))
  .withGroupArray("enemies", JSON.stringify([
    { name: "Goblin", hp: 50 },
    { name: "Orc",    hp: 100 }
  ]))
  .toDatabase();

console.log(db.getString("app_name")); // "MyGame"
console.log(db.getInt("server.port")); // 8080
db.free();
```

## Converting from JSON / TOML
```typescript
const db = MdixDatabase.fromJson(JSON.stringify({ port: 8080 }));
const db2 = MdixDatabase.fromToml("port = 8080\n");
```

## Exporting
```typescript
const json = db.toJson(true);  // indented
const toml = db.toToml();
const mdix = db.toMdix();
```

## Merging

AST-level merge with weighted-priority conflict resolution, per-source
labels, and a full conflict report — not a JSON round-trip deep-merge.
See **https://dixscript-docs.pages.dev** for the full strategy semantics.
```typescript
import { mergeSources, mergeSourcesWeighted, MdixDatabase } from "@dixscript/core";

// Sources are weighted in descending order: first gets weight 1.0.
const outcome = mergeSources([baseSource, overridesSource]);
// strategy: "weighted" (default) | "primary_wins" | "secondary_wins" | "throw_on_conflict"
// arrayStrategy: "concat_dedup" (default) | "replace" | "concat"
const outcome2 = mergeSources([baseSource, overridesSource], "primary_wins", "concat");

const db = outcome.database();          // consumes the outcome — call once
const conflicts = outcome.conflicts();  // [{path, winningSource, winningLabel}, ...]

// Explicit per-source weights instead of positional descending weights:
const outcome3 = mergeSourcesWeighted([
  [baseSource, 1.0],
  [overridesSource, 0.8],
]);

// Merge two already-loaded databases directly:
const merged = dbA.mergeWith(dbB, "weighted", "concat_dedup").database();
```

## Schema validation

```typescript
import { MdixSchema, MdixDatabase } from "@dixscript/core";

const schema = new MdixSchema()
  .requireString("app_name")
  .requireInt("port")
  .requireLong("created_at_ms")
  .optionalBool("debug");

const report = db.validateSchema(schema);
if (!report.isValid) {
  console.log(report.toString());       // human-readable summary
  console.log(report.failedPaths());    // ["port", ...]
  console.log(report.errors());         // [{path, expected, actual, kind}, ...]
}
```
`MdixSchema` is not single-use — the same instance can validate multiple
databases. Custom validators (`require_with`/`optional_with` in the Rust
core) aren't exposed here; the named `require*`/`optional*` methods cover
the overwhelming majority of real schema use.

## Hot reload (content-hash watch)

Deliberately **not** filesystem-polling — wasm32 has no filesystem at all.
The host (Node's `fs.watch`/`chokidar`, or a browser polling its own
`fetch()`) always already knows *when* its own file changed; `MdixWatcher`
decides *whether* to re-parse, by content hash instead of a multi-KB
memcmp on every tick.
```typescript
import { MdixWatcher } from "@dixscript/core";

const watcher = new MdixWatcher();

// Node:
fs.watch("config.mdix", async () => {
  const text = await fs.promises.readFile("config.mdix", "utf8");
  const outcome = watcher.check(text);
  if (outcome.changed) applyNewConfig(outcome.database());
});

// Browser:
setInterval(async () => {
  const text = await (await fetch("/config.mdix")).text();
  const outcome = watcher.check(text);
  if (outcome.changed) applyNewConfig(outcome.database());
}, 5000);
```

## DLM (compress / encrypt / audit)

Compiles a source that declares an `@DLM(DCompressor..., DEncryptor...)`
section, applying compression/encryption entirely in memory (wasm32 can't
write `.mdix.enc`/`.mdix.key` to disk itself). If `source` has no `@DLM`
section, `compileWithDlm` still succeeds — `processedData` is just the
plain binary-packed AST, and `keyFileContent` is `undefined`.
```typescript
import { compileWithDlm, decompileWithDlm } from "@dixscript/core";

const source = `
  @DLM(DCompressor.xz, DEncryptor.aes256)
  @DATA(secret = "shh")
`;

const outcome = compileWithDlm(source, "my-config");
if (!outcome.isSuccess()) throw new Error(outcome.errors().join("; "));

const encryptedBytes = outcome.processedData();   // Uint8Array
const keyFileContent = outcome.keyFileContent();  // string | undefined

// ... store/send encryptedBytes + keyFileContent however you like ...

// Pass "" for keyFileContent when the original call returned undefined
// (source had no @DLM modules) — this unpacks directly instead of
// attempting decryption.
const db = decompileWithDlm(encryptedBytes, keyFileContent ?? "", "my-config");
db.getString("secret"); // "shh"
```

## Result pattern

All WASM methods throw on failure by default. Import `tryGet` to
get a `MdixResult<T>` instead:
```typescript
import { tryGet, unwrapOr } from "@dixscript/core";

const port = unwrapOr(tryGet(() => db.getInt("port")), 3000);
```

## Two-tier DATA rule

Flat properties must be added before table properties or group arrays.
Violating this throws immediately with a descriptive error:
```typescript
// WRONG — throws "cannot add flat property after table properties"
new MdixBuilder()
  .withTableProperties("server", JSON.stringify({ port: 8080 }))
  .withString("name", "MyApp"); // throws here

// CORRECT — flat first, then grouped
new MdixBuilder()
  .withString("name", "MyApp")
  .withTableProperties("server", JSON.stringify({ port: 8080 }));
```
