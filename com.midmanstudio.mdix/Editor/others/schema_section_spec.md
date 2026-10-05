# `@SCHEMA` section specification

`@SCHEMA` declares constraints on the file's `@DATA` section. It is
**compile-time only**: descriptors are checked while the file compiles, and
nothing from `@SCHEMA` is serialized to the binary format or carried into the
runtime `DixData`.

## Grammar

```
SchemaSection ::= "@SCHEMA" "(" SchemaField* ")"
SchemaField   ::= FieldPath "=" "Schema" "." TypeName "(" ArgList? ")" ","?
FieldPath     ::= Segment ( "." Segment )*
Segment       ::= Identifier | Keyword
TypeName      ::= "Int" | "Long" | "Float" | "Double" | "String" | "Bool"
                | "Array" | "Enum" | "Object" | "Tuple" | "Date"
                | "Timestamp" | "Hex" | "Blob" | "Regex"
ArgList       ::= Literal ( "," Literal )*
Literal       ::= Boolean | Integer | Long | Float | Double
                | ScientificNotation | StringLiteral
```

* Commas between fields are optional, as between `@DATA` entries.
* **Type names are capitalised.** The lowercase spellings (`int`, `string`,
  `bool`, ...) are lexer keywords, and only a plain identifier is accepted
  after `.`; `Schema.int(...)` is rejected with a hint naming the corrected
  spelling.
* `@SCHEMA` is a singleton: one per file, describing that file's `@DATA`.
* Feature name for `@CONFIG`'s `features` list: `schema`. Default `advanced`
  mode unlocks it.

## Paths

A field path uses the same dotted addressing `@DATA` does. The last segment is
the property name; earlier segments name the table or object it lives in.

| `@DATA` entry                              | Schema path that reaches it            |
|--------------------------------------------|----------------------------------------|
| `port = 8080`                              | `port`                                 |
| `database.primary: host = "x", port = 1`   | `database.primary.host`, `.port`       |
| `meta = { owner = "me" }`                  | `meta`, `meta.owner`                   |
| `origins:: "a", "b"`                       | `origins` (an array of 2 items)        |

A path that names a table *prefix* (`database`, for `database.primary: ...`)
resolves as a present **object**, so `Schema.Object(true)` is satisfiable.

## Descriptor types

`required` (a bool) is always the first argument.

| Method                                  | Constraints (in argument order)   | Accepts                              |
|-----------------------------------------|-----------------------------------|--------------------------------------|
| `Int(required, min?, max?)`             | numeric `min`, `max`              | integer                              |
| `Long(required, min?, max?)`            | numeric `min`, `max`              | integer, long                        |
| `Float(required, min?, max?)`           | numeric `min`, `max`              | any numeric                          |
| `Double(required, min?, max?)`          | numeric `min`, `max`              | any numeric                          |
| `String(required, minLength?, maxLength?)` | int `minLength`, `maxLength`   | string                               |
| `Bool(required)`                        | —                                 | bool                                 |
| `Array(required, minItems?, maxItems?)` | int `minItems`, `maxItems`        | array, `path::` group array          |
| `Enum(required, enumName)`              | enum name (mandatory)             | a value of that enum                 |
| `Object(required)`                      | —                                 | object literal, table                |
| `Tuple(required)`                       | —                                 | tuple                                |
| `Date(required)`                        | —                                 | date                                 |
| `Timestamp(required)`                   | —                                 | timestamp                            |
| `Hex(required)`                         | —                                 | hex colour                           |
| `Blob(required)`                        | —                                 | blob                                 |
| `Regex(required)`                       | —                                 | regex                                |

Numeric compatibility **widens, never narrows**: `Float` and `Double` accept an
integer literal, `Int` never accepts a fractional value. Bounds are inclusive.
An explicit `<type>` annotation on the `@DATA` property takes precedence over
the literal's own kind, since it is what the runtime stores.

## Validation

1. **Descriptor validation.** Each field is evaluated through the `Schema`
   builtin (`Builtins/Static/schema_object.rs`), which is the single source of
   truth for method names and argument rules. Unknown methods, a non-bool
   `required`, or badly-typed constraints are `SCH001`. A path declared twice
   is `SCH002`.
2. **Data validation.** Each descriptor is checked against `@DATA`:

| Id     | Meaning                                                    |
|--------|------------------------------------------------------------|
| SCH003 | required field missing (or null)                           |
| SCH004 | type mismatch                                              |
| SCH005 | numeric value outside `min`/`max`                          |
| SCH006 | string length or array item count outside its bounds       |
| SCH007 | enum value belongs to a different enum than `enumName`     |

An optional field (`required = false`) may be absent. A `null` value counts as
absent.

## When validation runs

Twice, deliberately:

* In the **semantic phase**, against `@DATA` as parsed. Plain literals are fully
  checked. Values not knowable yet — QuickFunc calls, references, expressions —
  are skipped, not failed.
* After **value resolution**, in `DixLoader` (Stage 9), against the resolved
  `@DATA`, where computed values are concrete. This is the only place a schema
  can constrain a QuickFunc-produced value.

## Not covered

* Nested shape validation: `Schema.Object` checks presence and kind only.
* Wildcard or array-element paths.
* Validation of data supplied after compilation.
* Merging `@SCHEMA` across sources in `mdix merge`.

## Example

```
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1, WARN = 2 }
)

@DATA(
  app_name = "Demo"
  port = 8080
  log_level<enum> = LogLevel.INFO
  database.primary: host = "db.local", port = 5432
  allowed_origins:: "https://a.example.com", "https://b.example.com"
)

@SCHEMA(
  app_name              = Schema.String(true, 1, 32)
  port                  = Schema.Int(true, 1, 65535)
  log_level             = Schema.Enum(true, "LogLevel")
  database.primary.host = Schema.String(true)
  database.primary.port = Schema.Int(true, 1, 65535)
  allowed_origins       = Schema.Array(true, 1, 5)
  timeout_ms            = Schema.Int(false, 0, 60000)
)
```
