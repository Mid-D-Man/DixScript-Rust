# mdix-php

PHP bindings for DixScript (`.mdix`) — MidManStudio, via PHP's `ext-ffi` directly
against the pre-built `mdix_ffi` native library (the same C ABI `mdix-c` wraps).

Full language reference, `.mdix` syntax, and the DLM/schema/query semantics
that this binding surfaces: **https://dixscript-docs.pages.dev**

## Installation

```bash
composer require midmanstudio/mdix
```

Requires PHP 8.1+ with the `ffi` extension enabled (`extension=ffi` in
`php.ini`; on the CLI SAPI it's on by default, on `php-fpm` set
`ffi.enable=true`). Needs the platform native library — either drop a
prebuilt one at `mdix-php/lib/libmdix_ffi.{so,dylib}` / `mdix_ffi.dll`, or
point `MDIX_LIB_PATH` at it:

```bash
export MDIX_LIB_PATH=/path/to/libmdix_ffi.so
```

## Quick start

```php
<?php
use MidManStudio\Mdix\{MdixDatabase, MdixBuilder};

// Load and read
$db = MdixDatabase::loadStr('@DATA( port = 8080, host = "localhost" )');
try {
    $port = $db->getInt('port');
    $host = $db->getString('host', 'localhost'); // default if missing
    $keys = $db->keys();
} finally {
    $db->close();
}

// Build
$builder = new MdixBuilder();
try {
    $builder->setString('app_name', 'MyGame')
            ->setInt('port', 8080)
            ->setBool('ssl', true);
    $builder->saveToFile('out.mdix');
} finally {
    $builder->close();
}
```

Every handle-holding class (`MdixDatabase`, `MdixBuilder`, `MdixHotReload`)
releases native memory in `close()` and again automatically via `__destruct()`
as a safety net — call `close()` explicitly (or wrap in `try`/`finally`) for
anything long-lived rather than relying on GC timing.

## MdixQuery — filter/sort/group over decoded values

```php
$db = MdixDatabase::loadStr(<<<'MDIX'
@DATA(
  tasks::
    { name = "Backup", priority = 3 },
    { name = "Docs",   priority = 1 },
    { name = "Audit",  priority = 3 }
)
MDIX);

$highPriority = $db->query('tasks')
    ->where(fn($t) => $t['priority'] === 3)
    ->orderByDescending(fn($t) => $t['priority'])
    ->select(fn($t) => $t['name']);
// ["Backup", "Audit"]

// Sibling paths sharing shape via a wildcarded segment
$statuses = $db->queryMany('servers.*.status');
```

Built directly on plain PHP arrays (`json_decode()` under the hood) rather
than a custom value-wrapper type — PHP arrays are already the dynamic,
freely-indexable structure the Java/C++ bindings had to build from scratch
for the same job. `where`, `whereFieldEquals`, `select`, `selectField`,
`orderBy`, `orderByDescending`, `groupBy`, `distinct`, `skip`, `take`, `any`,
`all`, `count`, `isEmpty`, `first`/`firstOr`/`last`/`nth`,
`sumInt`/`sumFloat`/`avgFloat`, `minByKey`/`maxByKey`, and `toArray()` to
drop back to a plain array for anything not covered.

## MdixMerge — weighted AST-level merge

```php
use MidManStudio\Mdix\{MdixMerge, MergeStrategy, ArrayMergeStrategy};

$result = MdixMerge::sourcesWeighted(
    [$baseConfigSrc, $overrideConfigSrc],
    [1.0, 0.5],
    MergeStrategy::WeightedPriority,
    ArrayMergeStrategy::ConcatDedup,
);

try {
    if ($result->hasConflicts()) {
        foreach ($result->conflicts as $c) { echo $c, "\n"; }
    }
    $port = $result->database->getInt('server.port');
} finally {
    $result->database->close();
}
```

Real AST-level merge (`dixscript::Runtime::MdixMerger`) — weighted-priority
conflict resolution, per-source conflict reporting, and full type fidelity
for every DixScript value type, not a shallow JSON-object merge.
`MdixMerge::databases(...)` merges already-loaded databases by
round-tripping each through `toMdix()` first.

## MdixSchemaBuilder — field validation

```php
use MidManStudio\Mdix\{MdixSchemaBuilder, ExpectedType};

$report = (new MdixSchemaBuilder())
    ->requireString('app_name')
    ->requireInt('port')
    ->requireWith('port', ExpectedType::Int, function ($data) {
        $port = $data->getInt('port');
        return ($port >= 1025 && $port <= 65535) ? null : "port {$port} out of range";
    })
    ->optionalBool('debug')
    ->validate($db);

if (!$report->isValid()) {
    echo $report, "\n";
    // Validation failed with 1 error(s):
    // [Missing] 'app_name': expected string (required), got missing
}
```

The type/required check runs natively (the same `SchemaBuilder` DixScript's
Rust runtime uses); custom validators (`requireWith`/`optionalWith`) run
afterward in pure PHP against the loaded `MdixDatabase`, since a Rust
closure can't cross the FFI boundary the way a type tag can.

## MdixHotReload — poll-based file watching

```php
use MidManStudio\Mdix\MdixHotReload;

$watcher = new MdixHotReload('config.mdix');
try {
    while ($running) {
        $fresh = $watcher->checkAndReload();
        if ($fresh !== null) {
            try {
                applyNewConfig($fresh);
            } finally {
                $fresh->close();
            }
        }
        tick();
    }
} finally {
    $watcher->close();
}
```

Poll-based, not OS-event-based — a single `stat()` call per check, cheap
enough to run every request/tick and consistent across every platform. The
first check always reports a change. Use `forceReload()` to reload
unconditionally, or `hasChanged()` to check without reloading.
**Encrypted `.mdix` files are not supported by hot reload** — this is a
limitation of the core Runtime feature itself.

## Requirements

- PHP 8.1+ with `ext-ffi` enabled
- No Rust toolchain required — bring a prebuilt native library (see
  Installation above)

## Running tests

```bash
composer install
composer test
# or directly:
vendor/bin/phpunit
```

## A note on this release

Fixed two real bugs while extending this library — worth knowing if
you're upgrading from an earlier version:

- **`ValueType` / `mdix_get_type()` mismatch.** The `Long` case was
  missing from both `ValueType.php` and the FFI header's `MdixType`
  mirror, silently shifting every case from `Float` onward one below its
  real native value, and topping out one short of the real `Enum`
  discriminant — any DixScript enum-typed field crashed `valueTypeAt()`
  with an uncaught `\ValueError`. Fixed; see `ValueTypeTest.php` for
  regression coverage.
- **Native-error reporting crashed instead of reporting.** Several
  error paths (`getInt`/`getLong`/`getFloat`/`getDouble`/`getBool`,
  `MdixConverter`'s format helpers, and others) called `FFI::string()`
  on the result of `mdix_get_last_error()` — which PHP's FFI already
  auto-converts straight to a native PHP string (or `null`) because it's
  declared `const char*`, unlike a plain `char*` return which stays a
  `FFI\CData` object. The result: any *real* native error crashed with
  an unrelated `TypeError` instead of raising a useful `MdixError`.
  Fixed across every call site.
