# dixscript-go

Go bindings for the [DixScript](https://github.com/Mid-D-Man/DixScript-Rust) `.mdix` runtime.

[![Go Reference](https://pkg.go.dev/badge/github.com/Mid-D-Man/dixscript-go.svg)](https://pkg.go.dev/github.com/Mid-D-Man/dixscript-go)
[![CI](https://github.com/Mid-D-Man/DixScript-Rust/actions/workflows/go-ci.yml/badge.svg)](https://github.com/Mid-D-Man/DixScript-Rust/actions/workflows/go-ci.yml)

## Documentation Site https://dixscript-docs.pages.dev

---

## Requirements

| Requirement | Notes |
|---|---|
| Go 1.21+ | Query uses generics + the stdlib `cmp` package |
| CGO_ENABLED=1 | Default for native builds; does not work with CGO_ENABLED=0 |
| C compiler (gcc/clang) | Needed by cgo |
| `cargo build -p mdix-ffi` | Run once to generate the C header and native lib |

---

## Quick start

```go
import dixscript "github.com/Mid-D-Man/dixscript-go"

db, err := dixscript.Load("config.mdix")
if err != nil {
    log.Fatal(err)
}
defer db.Close()

port, _   := db.GetInt("server.port")
host, _   := db.GetString("server.host")
debug, _  := db.GetBool("debug")
```

---

## Setup after cloning

```bash
# 1. Build the Rust FFI layer — generates the C header and .so/.dylib
cargo build -p mdix-ffi --release

# 2. Copy the native lib into the Go package (Linux example)
mkdir -p mdix-go/internal/lib/linux-amd64
cp target/release/libmdix_ffi.so mdix-go/internal/lib/linux-amd64/

# macOS arm64:
# mkdir -p mdix-go/internal/lib/darwin-arm64
# cp target/release/libmdix_ffi.dylib mdix-go/internal/lib/darwin-arm64/

# 3. Build and test
cd mdix-go
go build ./...
go test ./...
```

---

## API overview

### Load

```go
db, err := dixscript.Load("config.mdix")           // from file — required for EnableHotReload
db, err := dixscript.LoadStr(src)                   // from string
db, err := dixscript.LoadEncrypted(enc, key)        // encrypted + key file
db, err := dixscript.LoadEncryptedPassword(enc, pw) // encrypted + password
db, err := dixscript.LoadJSON(jsonStr)              // from JSON
db, err := dixscript.LoadToml(tomlStr)              // from TOML
defer db.Close()
```

### Read

```go
s, err   := db.GetString("path")
i, err   := db.GetInt("path")     // 32-bit — mdix_get_int
i64, err := db.GetInt64("path")   // genuine 64-bit Long — mdix_get_long, not GetInt widened
f32, err := db.GetFloat32("path")
f64, err := db.GetFloat64("path")
b, err   := db.GetBool("path")

// Special types
color, err := db.GetHexColor("primary_color")  // → HexColor{R,G,B,A float32}
blob,  err := db.GetBlob("icon_data")           // → Blob; call .Bytes()
re,    err := db.GetRegex("validation_pattern") // → MdixRegex; call .Compile()
date,  err := db.GetDate("release_date")        // → MdixDate{Value time.Time}
ts,    err := db.GetTimestamp("created_at")     // → MdixTimestamp{Value time.Time}

// Enums
name,  err := db.GetEnumName("ai_type")    // → "AIType"
field, err := db.GetEnumField("ai_type")   // → "BOSS"
val,   err := db.GetEnumValue("ai_type")   // → 2 (resolved int)

// Introspection
typ := db.ValueTypeAt("path")   // → dixscript.TypeInt, TypeLong, TypeString, etc.
ok  := db.Exists("path")        // → bool
n,  err := db.ArrayLength("path")
keys, err := db.Keys("")        // top-level keys
```

> `GetInt` (32-bit) and `GetInt64`/Long are genuinely distinct at the FFI
> level — a value written as `9_000_000_000L` only reads back correctly
> through `GetInt64`. See `RequireInt`/`RequireLong` below for the same
> asymmetry in schema validation.

### Build

```go
b := dixscript.NewBuilder()
defer b.Close()

b.SetString("profile.name", "player1")
b.SetInt("profile.level", 42)       // 32-bit
b.SetInt64("profile.xp_total", 9_000_000_000) // genuine 64-bit Long
b.SetFloat64("profile.score", 9876.5)
b.SetBool("profile.active", true)
b.SetDate("profile.joined", time.Now())

err := b.SaveToFile("profile.mdix")

// Or get it as a string
src, err := b.ToString()

// Or load it directly
db, err := b.ToDatabase()
defer db.Close()
```

### Convert

```go
// Export
json, err := dixscript.Convert.ToJSON(db, true)
mdix, err := dixscript.Convert.ToMdix(db, dixscript.FormatPretty)
toml, err := dixscript.Convert.ToToml(db)

// Import
db, err := dixscript.Convert.FromJSON(jsonStr)
db, err := dixscript.Convert.FromToml(tomlStr)

// Format source text
formatted, err := dixscript.Convert.FormatSource(src, dixscript.FormatCompact)
minified,  err := dixscript.Convert.MinifySource(src)
```

### Query

DixScript's core `DixQuery` (`dixscript::Runtime::query`) takes Rust
closures, which can't cross the cgo boundary — so, like the Python and C#
bindings, this fetches the target array natively and queries it with Go's
own idioms (generics + closures) instead:

```go
type Enemy struct {
    Name string `json:"name"`
    HP   int    `json:"hp"`
}

q, err := dixscript.LoadQuery[Enemy](db, "enemies")
heavies := q.Where(func(e Enemy) bool { return e.HP > 500 })
names   := dixscript.Select(heavies, func(e Enemy) string { return e.Name })
sorted  := dixscript.OrderByDesc(heavies, func(e Enemy) int { return e.HP })
groups  := dixscript.GroupBy(q, func(e Enemy) string { return e.Name })
total   := dixscript.SumInt(q, func(e Enemy) int64 { return int64(e.HP) })

// Sibling paths sharing shape, wildcarding one segment:
statuses, err := dixscript.QueryMany[string](db, "servers.*.status")
```

Also available on `Query[T]`: `Skip`, `Take`, `Any`, `All`, `Count`,
`IsEmpty`, `First`, `FirstOr`, `Last`, `Nth`, `ToSlice`. Also as free
functions (need a 2nd type parameter, so can't be methods): `Distinct`,
`OrderBy`, `MinByKey`, `MaxByKey`, `SumFloat`, `AvgFloat`.

### Merge

Wraps the real AST-level merger (`mdix_merge_sources`) — full DixScript
type fidelity, not a JSON round-trip:

```go
db, conflicts, err := dixscript.MergeSources(
    []string{baseSrc, overrideSrc},
    dixscript.PrimaryWins,
    dixscript.ArrayConcat,
)
defer db.Close()

for _, c := range conflicts {
    fmt.Printf("%s: source %d won\n", c.Path, c.WinningSource)
}

// Or with explicit per-source weights:
db, conflicts, err := dixscript.MergeSourcesWeighted(sources, weights, dixscript.WeightedPriority, dixscript.ArrayReplace)
```

### Schema

`mdix-ffi` has no schema-validation C ABI, so — same as C#'s
`MdixSchemaBuilder` — this validates client-side, purely with
`Exists`/`ValueTypeAt`:

```go
report := dixscript.NewSchema().
    RequireString("app_name").
    RequireInt("port").
    OptionalBool("debug").
    Validate(db)

if !report.IsValid() {
    for _, e := range report.Errors {
        fmt.Println(e) // `"port": missing required field (expected Int)`
    }
}
```

A `SchemaBuilder` is reusable — `Validate` only reads from the `Database`
you pass it.

### Hot reload

Polls the source file's mtime rather than using a filesystem watcher —
this package has zero runtime dependencies beyond cgo by design, and
`fsnotify` would be the one thing that broke that:

```go
db, err := dixscript.Load("config.mdix") // must be Load, not LoadStr
db.OnReloaded(func(db *dixscript.Database) {
    fmt.Println("config changed, new port:", must(db.GetInt("port")))
})
db.OnReloadFailed(func(err error) {
    log.Println("reload failed, still serving last-good config:", err)
})
if err := db.EnableHotReload(500 * time.Millisecond); err != nil {
    log.Fatal(err)
}
defer db.Close() // also disables hot reload
```

On a successful reload the *same* `*Database` has its internal handle
swapped in place — anything already holding it sees fresh data with no
re-fetch needed.

---

## Error handling

All functions return idiomatic Go `(value, error)` pairs. The error type is `*MdixError`:

```go
port, err := db.GetInt("server.port")
if err != nil {
    var me *dixscript.MdixError
    if errors.As(err, &me) {
        switch me.Kind {
        case dixscript.ErrNotFound:
            // path doesn't exist
        case dixscript.ErrTypeMismatch:
            // wrong type at path
        case dixscript.ErrClosed:
            // database was already closed
        }
    }
}
```

---

## Directory layout

```
mdix-go/
├── dixscript.go            # Load*, NewBuilder, Version — top-level facade
├── database.go             # Database type — all typed getters, SourcePath
├── builder.go               # Builder type — Set*, Save, ToString, ToDatabase
├── converter.go             # Convert.ToJSON / FromJSON / ToToml / Format / Minify
├── query.go                  # Query[T] — Where/Select/OrderBy/GroupBy/..., QueryMany
├── merge.go                  # MergeSources / MergeSourcesWeighted
├── schema.go                 # SchemaBuilder — client-side Require*/Optional* validation
├── watch.go                   # EnableHotReload / DisableHotReload — mtime polling
├── types.go                 # HexColor, Blob, MdixRegex, MdixDate, MdixTimestamp,
│                             # ValueType, MergeStrategy, ArrayMergeStrategy, FormatMode
├── errors.go                # MdixError, ErrorKind constants
├── *_test.go                 # one per file above — go test -v ./...
├── internal/
│   ├── ffi.go              # All cgo declarations (the only file with unsafe)
│   ├── include/
│   │   └── mdix_ffi.h      # Generated by cbindgen — do not edit
│   └── lib/                # Native libs — populated by CI or local cargo build
│       ├── linux-amd64/
│       ├── linux-arm64/
│       ├── darwin-amd64/
│       ├── darwin-arm64/
│       └── windows-amd64/
└── examples/
    └── basic/main.go
```

---

## vs C# package

| Concept | C# | Go |
|---|---|---|
| Load | `Dix.Load(path)` | `dixscript.Load(path)` |
| Read | `db.GetInt("path").OrThrow()` | `db.GetInt("path")` → `(int, error)` |
| Build | `MdixBuilder.Create()` | `dixscript.NewBuilder()` |
| Query | `db.QueryWhere<T>(path, pred)` | `dixscript.LoadQuery[T](db, path)` + `.Where(pred)` |
| Merge | `MdixMerger.Merge(sources, strategy)` | `dixscript.MergeSources(sources, strategy, arrayStrategy)` |
| Schema | `MdixSchemaBuilder` (client-side) | `dixscript.NewSchema()` (client-side, same reason) |
| Hot reload | `db.EnableHotReload()` (FileSystemWatcher) | `db.EnableHotReload(interval)` (mtime polling) |
| Close | `using var db = ...` (IDisposable) | `defer db.Close()` (io.Closer) |
| Errors | `MdixResult<T>` with `.IsSuccess` | `(T, error)` idiomatic Go |
| FFI glue | csbindgen auto-generates `MdixNative.cs` | cgo with hand-written `internal/ffi.go` |
| Header | not needed | `mdix_ffi.h` (cbindgen generated) |
