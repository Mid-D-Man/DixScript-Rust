# DixScript Core — Module & API Catalogue

This indexes the crate's module tree and public API surface for
contributors and integrators who need more than the narrative quick-start
in [`README.md`](./README.md). For version-to-version history, see
[`CHANGELOG.md`](./CHANGELOG.md) instead — this file is a snapshot of the
*current* API shape, not a log of changes to it. It's organized by
confidence:

- **Confirmed** sections were built directly from the module's own source
  (`mod.rs` `pub use` lists, or the file itself).
- **Partial / inferred** sections were built from how other parts of the
  workspace (`mdix-cli`, `mdix-lsp`) actually import and call these
  modules, not from reading the module source directly. Treat the listed
  shapes as "known to exist, signature approximate" rather than gospel.

This file is **hand-maintained, not generated**. Re-check it against
`cargo doc --no-deps` (or `cargo public-api` if available) before each
crates.io release — it will drift silently otherwise, especially the
"partial / inferred" section.

---

## Crate layout

```
dixscript/
├── lib.rs                    pub mod Utilities, ErrorManager, Builtins, Compiler, Runtime
│
├── Utilities/                 crate-wide helper utilities (re-exported at crate root)
├── ErrorManager/               unified error types + per-instance isolated error state
├── Builtins/                  built-in QuickFunc / static-object / instance-method registries
│
├── Compiler/
│   ├── AST/                    AST node definitions — the DixScript syntax tree
│   ├── Core/
│   │   ├── Tokenizer/           lexer (Approach B: tokenizer-first pipeline)
│   │   ├── Config/              @CONFIG section handling, OperationalSettings, DebugMode
│   │   ├── GeneralParser            token stream -> raw AST
│   │   ├── GeneralSemanticAnalyzer  symbol table, validation
│   │   ├── GeneralAstEnhancer       AST normalization/enhancement pass
│   │   ├── ValueResolution/         compile-time QuickFunc expression evaluator
│   │   └── BinarySerialization/     binary pack/unpack for .mdix.enc payloads
│   ├── DLM/                    Data Lifecycle Modules — compression/encryption/audit pipeline
│   │   ├── KeyManagement/        .mdix.key file read/write, Argon2id KDF params
│   │   └── Auditor/              DIY / Enhanced auditor subtypes
│   ├── Utilities/               SecurityUtilities and related helpers
│   └── VersionControl/          CompatibilityMode, CompatibilityResult
│
└── Runtime/                     ← the public runtime API: load, read, build, convert
```

---

## Runtime — public API *(confirmed — from `Runtime/mod.rs`)*

This is the primary surface most consumers touch. Every item below is a
real `pub use` re-export from `Runtime::mod.rs`.

| File | Exports | What it's for |
|---|---|---|
| `loader.rs` | `DixLoader` | Compile/load `.mdix` from disk, string, or encrypted bytes — full pipeline (tokenize → parse → semantic → enhance → value-resolve → array-homogenize) |
| `load_options.rs` | `DixLoadOptions` | Password / key-file / key-content / key-URL / search-path loading config |
| `dix_data.rs` | `DixData` | O(1) flattened dotted-path data store; `to_hashmap`, `to_structural_hashmap`, `get`/`get_value`/`exists`/`get_keys`/`select_many` |
| `dix_value.rs` | `DixValue` | Runtime value enum (15 variants) covering every DixScript type, plus the shared internal `Value -> DixValue` conversion used by both `DixData` and `DixConverter` |
| `converter.rs` | `DixConverter` | DixScript ⇄ JSON/TOML/`HashMap<String, DixValue>`. Key methods: `from_dix_data`, `from_hashmap`, `from_json`, `from_toml`, `to_mdix`, `to_json`, `to_toml`, `to_hashmap` |
| `format_options.rs` | `DixFormatOptions` | Indent size/tabs, minify, sort-keys, section-inclusion options consumed by `to_mdix` |
| `compactor.rs` | `DixCompactor` | `minify` / `compact` / `remove_comments` source-text transforms (token-based, not regex) |
| `data_builder.rs` | `DixDataBuilder`, `ConfigBuilder`, `EnumsBuilder`, `DataBuilder`, `TablePropertiesBuilder`, `GroupArrayBuilder` | Fluent builder for constructing `DixData` (or writing `.mdix` directly via `build_and_save`) without a template file |
| `dix_deserialize.rs` | `DixDeserialize` trait, `dix_get`, `dix_get_or`, `dix_nested`, `dix_array_of`, `dix_path` | Read Rust structs from a `DixData` — implement once, call `data.deserialize_at("prefix")` |
| `dix_serialize.rs` | `DixSerialize` trait, `dix_set_str`/`int`/`long`/`float`/`double`/`bool`, `dix_set_nested`, `dix_set_array_of` | Write Rust structs into a `DataBuilder` — implement once, call `builder.serialize_at("prefix", &val)` |
| `schema.rs` | `SchemaBuilder`, `ExpectedValueType`, `ValidationReport`, `ValidationError`, `ValidationErrorKind` | Fluent runtime schema validation against a loaded `DixData`; never panics, collects every violation |
| `merge.rs` | `MdixMerger`, `MdixMergeInput`, `MdixMergeResult`, `MdixMergeStrategy`, `ArrayMergeStrategy`, `MergeConflict` | AST-level merge of two or more DixScript databases with weight-based or strict conflict resolution |
| `key_resolver.rs` | `KeyFileResolver`, `KeyResolver`, `KeySource`, `ResolvedKey`, `KeyFileResolution`, `KeyFileSource` | Locates/reads `.mdix.key` files and derives the actual AES/ChaCha20 key bytes (keyfile or password+Argon2id mode) |
| `array_homogenizer.rs` | `homogenize_data_section` | Post-resolution pass that promotes mixed-numeric-literal arrays (e.g. `[12.3, 4, 4.9]`) to a single consistent element type |
| `query.rs` | `DixQuery` | LINQ-style chaining over an array field's elements — `query(path)` for a plain `Array`/`GroupArray`, `query_many(pattern)` across sibling paths sharing shape via a wildcarded segment. `where_`/`order_by_desc`/`select` and friends |
| `hot_reload.rs` | `HotReloadWatcher` | Poll-based file-change watcher for Rust consumers (`check_and_reload()` in a game loop/tick). Each language binding implements its own native FS-event mechanism instead (inotify/FSEvents/ReadDirectoryChangesW) — this one's Rust-only |

### Not re-exported at `Runtime::` top level, but reachable

`DixLoader::compile_to_resolved_ast(path) -> Result<DixScript, String>` —
runs the full pipeline and hands back the raw `DixScript` AST without
running any DLM modules. This is the entry point `mdix-lsp` and parts of
`mdix-cli` (`mdix merge`) use when they need the AST itself rather than a
flattened `DixData` — most useful for direct `DixConverter::to_json` /
`to_toml` / `to_mdix` without an intermediate hashmap round trip.

---

## Compiler::AST — public API *(confirmed — from `Compiler/AST/mod.rs`)*

The AST node types returned by `compile_to_resolved_ast` and consumed by
`DixConverter`. Re-exported flat from `Compiler::AST::*`.

| Type | Role |
|---|---|
| `Position` | Source line/column, `Copy`, used on every node |
| `DataType`, `ElemType` | Type annotations incl. `TypedArray<T>` / `TypedTuple<[T; 6]>` |
| `ErrorHandlingStrategy`, `CompatibilityMode`, `DebugMode` | `@CONFIG` enum values |
| `DLMModuleType`, `DLMModuleSubtype` | `@DLM` module/subtype tags |
| `DeclarationType` | `let` / `const` in QuickFuncs |
| `DixScript` | Root AST — `config`, `imports`, `dlm`, `enums`, `quick_functions`, `data`, `security`, all `Option<...Section>` |
| `ConfigSection`, `ConfigEntry`, `ConfigValue` | `@CONFIG` |
| `ImportsSection`, `ImportDeclaration` | `@IMPORTS` (local + `from_cloud`) |
| `DLMSection`, `DLMModule` | `@DLM` |
| `EnumsSection`, `EnumDeclaration`, `EnumField` | `@ENUMS` |
| `SecuritySection`, `SecurityEntry`, `SecurityField` | `@SECURITY` |
| `DataSection`, `DataEntry`, `TablePath`, `PropertyAssignment` | `@DATA` — `DataEntry` is `SimpleProperty \| TableProperty \| GroupArray \| ObjectProperty` |
| `Value`, `ObjectProperty` | Literal/expression value tree (Integer, Long, Float, Double, ScientificNotation, String, Boolean, InterpolatedString, HexColor, Date, Timestamp, Null, Array, NestedArray, Object, PrefixedConstructor, EnumValue, Identifier, QuickFuncCall, Expression, Range, Lambda, ParseError, Error, Unknown) |
| `Expression` | QuickFunc expression tree (function calls, operators, access expressions, conditionals, type casts, ...) |
| `QuickFuncStatement`, `SwitchCase` | QuickFunc statement tree (`if:`/`elif:`, `chk:`/`miss`, assignments, declarations, `log:`) |
| `QuickFuncsSection`, `QuickFunction`, `QuickFuncParam` | `@QUICKFUNCS` |
| Helper functions (`helpers.rs`) | `create_*` constructors for building AST nodes by hand (mainly used in tests) |

`Compiler::AST::Visitors` is also re-exported (`pub use Visitors::*`) but
not catalogued in detail here — check the module directly if you need a
visitor pattern over the tree.

---

## Compiler::Core::Tokenizer — public API *(confirmed — from `Tokenizer/mod.rs`)*

| Export | Role |
|---|---|
| `Tokenizer` | Lexer entry point — `Tokenizer::new(source, &settings).tokenize()` |
| `TokenizationResult`, `TokenizationMetadata` | Token stream + metadata returned by `tokenize()` |
| `PrefixedConstructorInfo`, `StaticCallInfo` | Lexer-level metadata for `b:(...)`/`t:(...)`/`r:(...)` and static calls |
| `Token`, `TokenType`, `TokenExtensions` | Individual token type, the full `TokenType` enum, and `could_be_static_object_name()` |
| `split_config_tokens`, `TokenSplitResult` | Splits `@CONFIG` tokens from the rest of the stream (Approach B pipeline) |

---

## ErrorManager — public API *(confirmed — from `ErrorManager/mod.rs`)*

Every `DixError` variant (see the AST table above) wraps a phase-specific
`XxxError` struct; `XxxErrorType` is that phase's enum of specific
sub-kinds. `ErrorManager::add_<phase>_error(...)` exists for every one of
these eleven phases, not just the two mentioned in the old, much shorter
version of this table (`add_runtime_error`, `add_dlm_error`) — that
omission is exactly the kind of drift this file's own maintenance
checklist warns about; re-checking it is what caught this.

**Core:**

| Export | Role |
|---|---|
| `ErrorManager` | Per-instance isolated error state. Key methods: `new_isolated()`, `new_isolated_silent()`, `get_shared_instance()`, `clear_errors()`, `has_errors()`, `update_settings(...)`, `force_strategy(...)`, `get_debug_mode()`, plus `add_<phase>_error(...)` for all eleven phases below, `add_runtime_error_with_severity(...)`, `log_info`/`log_debug`/`log_warning`, `get_runtime_errors()` |
| `DixError` | Enum over every error phase (see AST table) — each variant wraps that phase's `XxxError` struct |
| `DebugConfig` | Controls verbosity/format of `ErrorManager`'s own diagnostic output |
| `LogFormat` | Output format for `ErrorManager` logging (plain/structured) |
| `DiagnosticDumper` | Standalone diagnostic report generator — `generate_dump() -> String`, `dump_to_file(filename) -> Result<String, String>`; independent of a specific `ErrorManager` instance |

**Per-phase error types** (`XxxError` is what the matching `DixError::Xxx`
variant wraps; `XxxErrorType` is that phase's enum of specific sub-kinds):

| Phase | Error struct | Error type enum |
|---|---|---|
| Lexical | `LexicalError` | `LexicalErrorType` |
| Parse | `ParseError` | `ParseErrorType` |
| Semantic | `SemanticError` | `SemanticErrorType` |
| Imports resolution | `ImportsResolutionError` | `ImportsResolutionErrorType` |
| AST enhancement | `AstEnhancementError` | `AstEnhancementErrorType` |
| Value resolution | `ValueResolutionError` | `ValueResolutionErrorType` |
| DLM | `DlmError` | `DlmErrorType` |
| Binary serialization | `BinarySerializationError` | `BinarySerializationErrorType` |
| Runtime | `RuntimeError` | `RuntimeErrorType` |
| Config | `ConfigError` | `ConfigErrorType` |
| General | `GeneralError` | `GeneralErrorType` |

**Shared enums:**

| Export | Role |
|---|---|
| `ErrorSeverity` | `Fatal`, `Error`, `Warning`, `Info` |
| `ErrorSource` | Where an error originated (tokenizer/parser/runtime/etc.) — distinct from `DixError`'s phase, which is *what kind* of error; this is *which subsystem* raised it |

**Exception / context helper types** (from `ErrorManager::Helpers`, used
internally by the phases above and by consumers building custom
diagnostics on top of a caught error):

| Export | Role |
|---|---|
| `TokenizationException`, `ParseException`, `SemanticsException`, `DLMPipelineException`, `ImportsResolutionException`, `BinarySerializationException`, `RuntimeException`, `AstEnhancementException`, `ValueResolutionException` | Phase-specific exception context wrappers, one per phase (a different set from the `XxxError`/`XxxErrorType` pairs above — these carry richer contextual state for propagation, not just the classified error kind) |
| `ParseState` | Parser state snapshot attached to parse-phase exceptions |
| `SourceLineExtensions`, `get_source_line_from_tokens` | Pulls the offending source line out of a token stream for error display (what powers the `line`/`column`-adjacent source snippets in CLI error output) |

---

## Compiler (other submodules) — partial / inferred

These are real, working modules (every CLI command and the LSP depend on
them), but this catalogue was built from call sites in `mdix-cli` /
`mdix-lsp` rather than from reading each module's source directly —
confirm exact signatures against source before depending on the shape
described here.

| Path | What it does | Known entry points |
|---|---|---|
| `Compiler::Core::GeneralParser` | Token stream → raw AST | `GeneralParser::new(tokens, &config_section, &settings)?.parse()` |
| `Compiler::Core::GeneralSemanticAnalyzer` | Symbol table construction + validation | `GeneralSemanticAnalyzer::new(&ast, &settings).analyze()` → `SemanticAnalysisResult { is_success, errors, warnings, symbol_table: Option<SymbolTable> }`; `symbol_table.namespaces` map exposes `.functions` for imported-namespace lookups |
| `Compiler::Core::GeneralAstEnhancer` | AST normalization pass (post-parse, pre-resolve) | `GeneralAstEnhancer::new(&settings).enhance(&ast, Some(&semantic_result))` → `EnhancementResult { is_success, errors, total_enhancements, enhanced_ast }` |
| `Compiler::Core::ValueResolution::ValueResolver` | Compile-time QuickFunc expression evaluator | `ValueResolver::new(ast, &symbol_table, debug_mode).resolve()` → `ResolutionResult { is_success, errors, function_calls_resolved, resolved_ast: Option<DixScript> }` |
| `Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker}` | Binary pack/unpack for `.mdix.enc` payloads | `BinaryPacker::new().pack(&ast)`, `BinaryUnpacker::new().unpack(&bytes)` |
| `Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings, DebugMode}` | `@CONFIG` token processing → runtime settings | `ConfigSectionHandler::new(None).process_config_tokens(&tokens)` → `operational_settings`, `config_section` |
| `Compiler::DLM::{DLMPipelineExecutor, DLMReverseExecutor}` | Forward (compile) and reverse (decrypt) DLM module execution | `DLMPipelineExecutor::new(source_path, output_dir, debug_mode).execute(&mut ast, binary_data)`; `DLMReverseExecutor::new(enc_path, key_path, password, debug_mode).execute()` |
| `Compiler::DLM::KeyManagement::{KeyFileManager, KeyFileData, EncryptionKeyData, KDFParameters, MdixKeyWriter, KeyFileDataBuilder}` | `.mdix.key` file read/write, Argon2id KDF parameters | `KeyFileManager::new(source_path, output_dir).read_key_file(path)`; `MdixKeyWriter::write(&data)`; `KeyFileDataBuilder::new()...build()` |
| `Compiler::DLM::Auditor::{IAuditor, DiyAuditor, EnhancedAuditor}` | `@DLM(DAuditor...)` subtypes | `IAuditor` trait with `start_audit(&ast, &[])` / `finalize_audit()`; `DiyAuditor::new(source, output_dir)`, `EnhancedAuditor::new(source, output_dir, ast)` |
| `Compiler::Utilities::SecurityUtilities` | `@SECURITY` section helpers | `ensure_valid_security_section(existing_security, dlm_section)` |
| `Compiler::VersionControl::{CompatibilityMode, CompatibilityResult}` | Version-mismatch handling | `CompatibilityMode` used by `DixLoadOptions::compatibility_mode`; `CompatibilityResult` appears on `Value::Unknown` |

---

## Builtins — not catalogued in detail

`Builtins::Resolver` (with `static_object_registry` and
`instance_method_registry` submodules) backs `Dix.*` calls, static-object
method calls, and instance-method calls inside QuickFuncs. This is
implementation detail behind the compile pipeline, not part of the
documented public contract — listed here only as a pointer for
contributors working on QuickFunc builtins, not for crate consumers.

---

## Utilities — top-level

`Utilities::*` is re-exported at the crate root (`pub use Utilities::*` in
`lib.rs`) alongside `ErrorManager::*`. Not catalogued item-by-item here —
check `src/Utilities/` directly; nothing in the workspace currently
depends on a specific named export from it outside the crate itself.

---

## Maintenance checklist (before each release)

- [ ] Diff this file's **Confirmed** sections against the actual `mod.rs`
      `pub use` lists — those should never drift since they're copy-paste
      from source, but verify after any `Runtime`/`AST`/`Tokenizer` change.
- [ ] Spot-check the **Partial / inferred** Compiler section against
      source if any of those modules changed since the last release.
- [ ] Run `cargo doc --no-deps -p dixscript` and skim for any newly
      `pub` item that isn't reflected here.
