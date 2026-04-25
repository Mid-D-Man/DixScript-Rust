# @RAW Section — Foreign Payload Container

**DixScript Language Specification — Deferred Feature**

> **Status:** Specified, not yet implemented.
> Targeted for a post-v1.0.0 release.
> This document is the normative reference for the `@RAW` section grammar,
> semantics, and engine integration contract.

---

## Motivation

DixScript `.mdix` files are designed to be the single source of truth for
everything a game or application needs at startup: configuration, type
definitions, encrypted secrets, and now optionally — raw binary or text
payloads that require specialised handling by engine subsystems.

Without `@RAW`, large game projects accumulate a parallel directory tree of
disjointed asset files (shader WGSL, custom-format images, compiled binaries)
that have no formal relationship to the configuration that references them.
The `@RAW` section solves this by embedding the payload alongside its
handling instructions inside the same `.mdix` package.

### What it is not

`@RAW` is not a general-purpose binary bundler or a replacement for asset
pipeline tools.  It is a **foreign payload container** — a well-typed envelope
that tells the DixScript runtime *"hand this chunk to the module named X;
here is everything X needs to know before it touches a single byte"*.

---

## Section grammar

```ebnf
RawSection ::= "@RAW(" RawBlock ")"

RawBlock ::=
    "meta_data" "->" "{" MetaDataFields "}"
    ","?
    "using"     "->" "{" UsingFields "}"
    ","?
    "content"   "->" "{" ContentBlock "}"

MetaDataFields ::= MetaDataField (","? MetaDataField)*
MetaDataField  ::=
    | "id"       "=" StringLiteral
    | "format"   "=" StringLiteral
    | "checksum" "=" StringLiteral   (* optional *)
    | "size"     "=" Integer         (* optional; bytes of decoded payload *)
    | Identifier "=" Value           (* extension fields; engine-defined *)

UsingFields ::= UsingField (","? UsingField)*
UsingField  ::=
    | "filter"      "=" StringLiteral   (* pre-processing pass name *)
    | "compression" "=" StringLiteral   (* decompression algorithm *)
    | "threads"     "=" Integer         (* decoder parallelism hint *)
    | "module"      "=" StringLiteral   (* explicit Rust module / FFI plugin *)
    | Identifier    "=" Value           (* extension fields; engine-defined *)

ContentBlock ::= ContentDelimiter AnyBytes ContentDelimiter

(* The delimiter is the string "---" followed by an engine-chosen unique tag.
   The tag MUST appear on its own line and MUST NOT appear inside the payload.
   Recommended: derive the tag from meta_data.id or meta_data.checksum. *)
ContentDelimiter ::= "---" [UniqueTag] LineTerminator

(* AnyBytes: verbatim content, ignored by the DixScript lexer.
   May be raw binary, base64-encoded binary, or plain text. *)
AnyBytes ::= (Character | BinaryOctet)*
```

### Comma rules

All three sub-blocks (`meta_data`, `using`, `content`) follow the standard
DixScript comma-optional rule: commas between blocks are optional at the
top level.  Commas between fields inside `meta_data -> { }` and
`using -> { }` are also optional.

---

## Concrete syntax example

```dixscript
@RAW(
  meta_data -> {
    id       = "tex_atlas_001"
    format   = "MPX"
    checksum = "0xFA22B1"
    size     = 131072
  }

  using -> {
    filter      = "Paeth"
    compression = "MBFA"
    threads     = 4
    module      = "engine::texture::mpx_decoder"
  }

  content -> {
    ---tex_atlas_001_FA22B1---
    [ verbatim binary or text payload — ignored by the DixScript lexer ]
    ---tex_atlas_001_FA22B1---
  }
)
```

### WGSL shader example

```dixscript
@RAW(
  meta_data -> {
    id     = "shader_pbr_frag"
    format = "WGSL"
  }

  using -> {
    module  = "engine::renderer::shader_compiler"
    threads = 1
  }

  content -> {
    ---shader_pbr_frag---
    @fragment
    fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
        return vec4<f32>(in.color, 1.0);
    }
    ---shader_pbr_frag---
  }
)
```

---

## Delimiter contract

The content delimiter is a line that begins with exactly `---` followed
immediately by a unique tag string and a line terminator.  The DixScript
lexer MUST skip every byte between the opening and closing delimiter without
parsing or validating it.

### Tag uniqueness requirement

The tag MUST NOT appear anywhere inside the payload bytes.  Recommended
derivation strategies (in order of preference):

1. `meta_data.id` + `_` + first 6 hex digits of `meta_data.checksum`
   → `tex_atlas_001_FA22B1`
2. A UUID derived from the file path + payload hash.
3. A random token generated at authoring time and stored alongside the file.

If the payload could contain the tag, the author MUST choose a different tag
or pre-process the payload (e.g. base64-encode it) to guarantee the tag does
not appear within it.

---

## Lexer handling (implementation note)

When the lexer encounters `@RAW`:

1. Parse `meta_data -> { ... }` and `using -> { ... }` normally — these
   blocks use standard DixScript syntax and are fully tokenised.

2. When `content -> {` is reached, scan forward for the **opening delimiter**
   (`---<tag>`) using `memchr::memmem::find`.  Record the byte position.

3. Scan forward again from that position for the **closing delimiter**.  All
   bytes between the two delimiters are the raw payload — store them as a
   single opaque `RawPayload(Vec<u8>)` token (or an offset range into the
   source).  Do not tokenise them.

4. Advance past the closing delimiter and resume normal tokenisation.

This approach is zero-copy if the lexer stores a byte-range reference into
the original source buffer rather than cloning the payload.

---

## Runtime / engine integration

### DixValue variant (future)

```rust
pub enum DixValue {
    // ... existing variants ...

    /// Opaque foreign payload.  The engine dispatches on `format` and `module`
    /// to locate the correct decoder.  `payload` is the verbatim bytes between
    /// the content delimiters.
    RawPayload {
        id:          String,
        format:      String,
        checksum:    Option<String>,
        using:       HashMap<String, DixValue>,
        payload:     Vec<u8>,   // or Arc<[u8]> for cheap clones
    },
}
```

### Engine dispatch contract

The engine MUST consult the `using.module` field to route the payload:

```rust
match raw.format.as_str() {
    "MPX"  => engine::texture::mpx_decoder::decode(&raw),
    "WGSL" => engine::renderer::shader_compiler::compile(&raw),
    "SPIR-V" => engine::renderer::spirv_loader::load(&raw),
    other  => {
        // Look up `using.module` as a Rust module path or FFI plugin name.
        engine::plugin_registry::dispatch(other, &raw)
    }
}
```

### Deferred / lazy loading

The primary value of `@RAW` for large assets is deferred loading.

The runtime MUST expose a lazy accessor:

```rust
impl DixData {
    /// Returns metadata for a raw section without decompressing the payload.
    pub fn get_raw_meta(&self, id: &str) -> Option<&RawMeta>;

    /// Decompresses and returns the payload on demand.
    /// May be called from a thread pool if `using.threads > 1`.
    pub fn get_raw_payload(&self, id: &str) -> Result<Vec<u8>, String>;
}
```

This allows the engine to register all `@RAW` assets at startup (cheap
— only reads `meta_data` and `using`) and decompress on first use.

---

## Interaction with @DLM

When `@DLM` specifies a `DEncryptor`, the entire `.mdix` file including all
`@RAW` content blocks is encrypted as one unit.  On load, the DixScript
runtime decrypts the full payload first, then the lexer processes the
decrypted bytes.

If a `@RAW` block specifies its own `using.compression`, the decompression
named there is applied **only to that payload** by the engine decoder — it
is a second-level decompression pass distinct from the `@DLM` pipeline.

Order of operations on load:
file bytes
→ DLM decrypt (DEncryptor)
→ DLM decompress (DCompressor)
→ DixScript lexer / parser
→ for each @RAW block:
→ hand payload + using map to engine module
→ engine applies using.filter + using.compression internally

---

## Known limitations and open questions

| Area | Status |
|------|--------|
| Binary payloads that contain the delimiter tag | Author must choose a unique tag or base64-encode |
| Maximum payload size | Unspecified; recommend ≤ 64 MB per block in v1 |
| Multiple `@RAW` blocks in one file | Allowed; each MUST have a unique `meta_data.id` |
| `@RAW` inside `@IMPORTS` targets | Not allowed in v1 |
| Streaming / memory-mapped payloads | Deferred post-v1 |
| Tag collision detection at compile time | Planned for `mdix validate` |

---

## Version

This feature is scheduled for the first minor release after v1.0.0.
The section keyword `@RAW` is reserved in v1.0.0 and will produce a
`FeatureNotEnabled` semantic error if encountered by a v1.0.0 compiler.
