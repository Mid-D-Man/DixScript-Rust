# @dixscript/core

WebAssembly runtime for DixScript (`.mdix`) — works in the browser,
Node.js, and any bundler that supports WASM (Vite, webpack, Rollup).

## Installation
```bash
npm install @dixscript/core
```

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
