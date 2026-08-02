# MidManStudio.Mdix

.NET runtime for DixScript (`.mdix`) — MidManStudio. Thin, safe P/Invoke bindings over
the native `dixscript` Rust core (via `mdix-ffi`), plus a reflection-based POCO
serializer, LINQ-style querying, AST-level merging, and native file-watch hot reload
on top.

Targets `netstandard2.1` — works from .NET Framework 4.7.2+, .NET Core 3.0+, .NET 5+,
and Unity (2021 LTS+, IL2CPP and Mono).

## Installation
```bash
dotnet add package MidManStudio.Mdix
```

## Quick start
```csharp
using MidManStudio.Mdix.Core;

// Load and read
var dbResult = Dix.LoadStr("@DATA( port = 8080, host = \"localhost\" )");
if (dbResult.IsFailure)
{
    Console.WriteLine(dbResult.Error.Message);
    return;
}

using var db = dbResult.SuccessResult;
var port = db.GetInt("port").SuccessResult;      // 8080
var host = db.GetString("host").SuccessResult;   // "localhost"

// Or from disk
using var fromDisk = Dix.Load("config.mdix").SuccessResult;
```

`MdixResult<T>` never throws for expected failure paths (missing key, type mismatch,
bad file) — check `IsSuccess`/`IsFailure` and read `SuccessResult`/`Error` explicitly.
Exceptions are reserved for programmer errors (null handle after `Dispose`, cyclic
object graphs in the serializer, and similar).

## Builder — construct a database programmatically
```csharp
using var builder = MdixBuilder.Create()
    .Config(c => c.WithVersion("1.0.0"))
    .Enums(e => e.WithEnum("LogLevel", "DEBUG", "INFO", "WARN", "ERROR"))
    .Data(d => d
        .WithString("app_name", "MyGame")
        .WithInt("port", 8080)
        .WithEnum("log_level", "LogLevel", "INFO")
        .WithTableProperties("server", t => t
            .WithString("host", "localhost")
            .WithInt("port", 8080)));

using var db = builder.ToDatabase().SuccessResult;
```
The two-tier ordering DixScript's grammar requires (flat properties before any
grouped/table/array entries) is enforced by the builder itself — an out-of-order call
fails fast rather than producing `.mdix` text that won't parse.

## POCO serialization — value types and reference types
```csharp
public enum Environment { DEV = 1, STAGING = 2, PROD = 3 }

public class ServerConfig
{
    public string Host { get; set; } = "";
    public int Port { get; set; }
    public long RequestId { get; set; }       // 64-bit, no truncation
    public int? MaxConnections { get; set; }  // nullable value types
    public Environment Env { get; set; }      // enum properties
    public MdixHexColor AccentColor { get; set; }
}

var config = db.Deserialize<ServerConfig>("server").SuccessResult;

using var builder = MdixBuilder.Create();
builder.Serialize(config, "server");        // MdixResult<Unit>
var mdixText = builder.Serialize().SuccessResult;  // MdixResult<string>
```
Enum-typed properties round-trip as real DixScript enum references (`Environment.PROD`,
not a bare int or string) as long as the C# enum's type name matches the DixScript
`@ENUMS` declaration name it corresponds to — see **Enum code generation** below for a
way to guarantee that by construction. `long`, `float`, `Nullable<T>` of any supported
type, and the five DixScript leaf value types (`MdixHexColor`, `MdixBlob`, `MdixRegex`,
`MdixDate`, `MdixTimestamp`) are all supported on both records (reference types) and
structs (value types), nested arbitrarily deep.

## Enum code generation
Generate a real, type-safe C# `enum` for every DixScript enum declared in a file's
`@ENUMS` section, instead of hand-writing one and hoping it stays in sync:
```csharp
var generated = MdixEnumCodeGenerator.GenerateFromFile(
    "config.mdix",
    @namespace: "MyGame.Config",
    accessModifier: "public");

File.WriteAllText("Environment.g.cs", generated.SuccessResult);
```
Field values without an explicit `= N` in the source are left unassigned in the
generated code on purpose — C#'s own enum auto-numbering rule (previous value + 1,
starting at 0) is identical to DixScript's, so the C# compiler reproduces the same
numbers DixScript would without this tool duplicating that arithmetic itself. The
generated type name matches the DixScript enum name, which is exactly what
`MdixSerializer` expects when writing an enum-typed property back out (see above).

## MdixQuery — LINQ-style querying
```csharp
var highPriority = db.QueryWhere<TaskItem>("tasks", t => t.Priority == 3)
                      .SuccessResult
                      .OrderByDescending(t => t.Priority);

var first = db.QueryFirst<TaskItem>("tasks", t => t.Priority == 3);
var count = db.QueryCount<TaskItem>("tasks");
```
`QueryFirst`, `QueryLast`, `QuerySingle`, `QueryWhere`, `QuerySelect`, `QueryCount`,
`QueryAny`, `QueryAll`, `QueryOrderBy`/`QueryOrderByDescending`, `QueryDistinct`,
`QueryTake`/`QuerySkip` all deserialize to a `List<T>` and hand you back a normal
`System.Linq`-composable result — anything `System.Linq` itself offers (`GroupBy`,
`Sum`, `Average`, ...) is already available on what these return.

## Merge — full AST-level merging, not text concatenation
```csharp
using var merged = MdixMerge.MergeSources(
    new[] { baseSource, overrideSource },
    strategy: MdixMergeStrategy.PrimaryWins,
    arrayStrategy: MdixArrayMergeStrategy.Concat).SuccessResult;

foreach (var conflict in merged.Conflicts)
    Console.WriteLine(conflict);  // "[Conflict] 'server.port' -> source[1] won"
```
Every value type (including the five DixScript leaf types and enums) survives a merge
with full fidelity — this delegates straight into the native Rust merger rather than
reimplementing conflict resolution in C#.

## Hot reload
```csharp
using var db = Dix.Load("config.mdix").SuccessResult;
db.OnReloaded     += newDb  => ApplyConfig(newDb);
db.OnReloadFailed += error  => Log.Warn(error.Message);
db.EnableHotReload();
```
Backed by `System.IO.FileSystemWatcher` (OS-level change notifications, not polling),
with debounced reload handling for the double-fire behavior `FileSystemWatcher` is
known for on most platforms. Call `DisableHotReload()` to stop watching.

## Cross-platform native library
The native `mdix_ffi` library is bundled per-RID under `runtimes/{rid}/native/` and
resolved automatically on .NET Core 3.0+/.NET 5+; a `build/`-injected `.targets` file
handles copy-to-output on .NET Framework and other older hosts. Check the specific
package version's supported RID list if you're targeting something other than
`win-x64`, `linux-x64`, or `osx-arm64` — not every RID is necessarily built for every
release.

## Requirements
- `netstandard2.1`-compatible target (.NET Framework 4.7.2+, .NET Core 3.0+, .NET 5+,
  Unity 2021 LTS+)
- No Rust toolchain required — native binaries are pre-built and bundled
