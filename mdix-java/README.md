# dixscript-java

Java and Kotlin bindings for DixScript (`.mdix`) — MidManStudio, via JNI directly
against the `dixscript` Rust runtime (not a wrapper over `mdix-ffi`'s C ABI).

Full language reference, `.mdix` syntax, and the DLM/schema/query semantics
that this binding surfaces: **https://dixscript-docs.pages.dev**

## Installation

```kotlin
// build.gradle.kts
dependencies {
    implementation("com.midmanstudio:dixscript-java:1.0.0")
}
```
```groovy
// build.gradle
dependencies {
    implementation 'com.midmanstudio:dixscript-java:1.0.0'
}
```
```xml
<!-- Maven -->
<dependency>
    <groupId>com.midmanstudio</groupId>
    <artifactId>dixscript-java</artifactId>
    <version>1.0.0</version>
</dependency>
```

Native libraries for `linux-x86_64`, `darwin-x86_64`, `darwin-aarch64`, and
`win32-x86-64` are bundled inside the jar and extracted automatically at
first use (see `internal/NativeLoader`) — no separate native install step.

## Quick start

```java
import com.midmanstudio.dixscript.*;

// Load and read
try (Database db = DixScript.loadStr("@DATA( port = 8080, host = \"localhost\" )")) {
    int port = db.getInt("port");
    String host = db.getString("host", "localhost"); // default if missing
    var keys = db.keys();
}

// Build
try (Builder b = new Builder()) {
    b.setString("app_name", "MyGame")
     .setInt("port", 8080)
     .setBool("ssl", true);
    b.saveToFile("out.mdix");
}

// Foreign format import / export
try (Database fromJson = DixScript.convert().fromJson("{\"port\": 8080}")) {
    String json = DixScript.convert().toJson(fromJson, true);
    String toml = DixScript.convert().toToml(fromJson);
    String mdix = DixScript.convert().toMdix(fromJson, Converter.FormatMode.PRETTY);
}
```

## MdixQuery — LINQ-style querying

```java
try (Database db = DixScript.loadStr("""
    @DATA(
      tasks::
        { name = "Backup", priority = 3 },
        { name = "Docs",   priority = 1 },
        { name = "Audit",  priority = 3 }
    )
""")) {
    List<String> highPriority = db.query("tasks")
        .where_(t -> t.field("priority").asLong() == 3)
        .orderByDescending(t -> t.field("priority").asLong())
        .select(t -> t.field("name").asString());
    // ["Backup", "Audit"]

    // Sibling paths sharing shape via a wildcarded segment
    MdixQuery statuses = db.queryMany("servers.*.status");
}
```
`db.query(path)` covers a plain array literal or a GroupArray's items alike
— see `MdixValue`'s class doc for the JSON-fidelity tradeoffs (`Int`/`Long`/
`Float`/`Double` all collapse to one numeric kind; `Date`/`Timestamp`/
`HexColor`/`Blob`/`Regex` all read back as `Kind.STRING`). Every predicate
is a plain `Predicate<MdixValue>` — `where_`, `whereFieldEquals`, `select`,
`selectField`, `orderBy`, `orderByDescending`, `groupBy`, `distinct`,
`skip`, `take`, `any`, `all`, `count`, `isEmpty`, `first`/`firstOr`/`last`/
`nth`, `sumInt`/`sumFloat`/`avgFloat`, `minByKey`/`maxByKey`, `toList`, and
a `stream()` escape hatch for anything not covered.

In Kotlin, `where_` also has a `filter` alias (`import ...filter`) that
reads more naturally without the trailing underscore.

## Merge — weighted AST-level merge

```java
Merge.Result result = Merge.sourcesWeighted(
    List.of(baseConfigSrc, overrideConfigSrc),
    new double[] { 1.0, 0.5 },
    Merge.Strategy.WEIGHTED_PRIORITY,
    Merge.ArrayStrategy.CONCAT_DEDUP);

try (Database merged = result.database) {
    if (result.hasConflicts()) {
        result.conflicts.forEach(System.out::println);
    }
    int port = merged.getInt("server.port");
}
```
Real AST-level merge (`dixscript::Runtime::MdixMerger`) — weighted-priority
conflict resolution, per-source conflict reporting, and full type fidelity
for every DixScript value type, not a shallow JSON-object merge.
`Merge.databases(Database... dbs)` merges already-loaded databases by
round-tripping each through `toMdix()` first.

## SchemaBuilder — field validation

```java
SchemaBuilder.Report report = new SchemaBuilder()
    .requireString("app_name")
    .requireInt("port")
    .requireWith("port", SchemaBuilder.ExpectedType.INT, data -> {
        int port = data.getInt("port");
        return (port >= 1025 && port <= 65535) ? null : "port out of range";
    })
    .optionalBool("debug")
    .validate(db);

if (!report.isValid()) {
    System.err.println(report);
    // Validation failed with 1 error(s):
    // [Missing] 'app_name': expected string (required), got missing
}
```
The type/required check runs natively (the same `SchemaBuilder` DixScript's
Rust runtime uses); custom validators (`requireWith`/`optionalWith`) run
afterward in pure Java against the loaded `Database`, since a Rust closure
can't cross the JNI boundary the way a type tag can.

## HotReload — poll-based file watching

```java
try (HotReload watcher = new HotReload("config.mdix")) {
    while (running) {
        watcher.checkAndReload().ifPresent(fresh -> {
            try (fresh) {
                applyNewConfig(fresh);
            }
        });
        tick();
    }
}
```
Poll-based, not OS-event-based — a single `stat()` call per check, cheap
enough to run every frame and consistent across every platform. The first
check always reports a change. Use `forceReload()` to reload
unconditionally, or `hasChanged()` to check without reloading.
**Encrypted `.mdix` files are not supported by hot reload** — this is a
limitation of the core Runtime feature itself.

In Kotlin, `HotReload.awaitChange(pollIntervalMs)` is a `suspend` function
that polls on an interval and suspends until the next successful reload,
and `watchMdix(path) { ... }` runs a block with the watcher, closing it
automatically afterward.

## Kotlin extras

Beyond `filter` and `awaitChange` above: `db["path"]` reified-generic
dotted-path access, `Builder`'s `builder["path"] = value` operator,
`buildMdix { ... }` / `buildSchema { ... }` DSL builders,
`mergeSources(...)` / `mergeDatabases(...)` with Kotlin default arguments,
`Result`-returning `safeLoad`/`safeLoadStr`, and `...Async` coroutine
wrappers (`loadAsync`, `queryAsync`, `mergeSourcesAsync`, `saveToFileAsync`,
...) on `Dispatchers.IO`. See `Extensions.kt`.

## Requirements

- Java 11+
- Kotlin 1.9+ (for the `Extensions.kt` sugar — the Java API itself has no
  Kotlin dependency)
- No Rust toolchain required (pre-built natives for `linux-x86_64`,
  `darwin-x86_64`, `darwin-aarch64`, `win32-x86-64` are bundled in the jar)

## Building from source

```bash
./gradlew build   # compiles the Rust native lib for the host platform, then Java/Kotlin
./gradlew test    # runs the full JUnit 5 + Kotlin test suite against it
```
