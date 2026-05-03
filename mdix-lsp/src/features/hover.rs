// mdix-lsp/src/features/hover.rs
//! Hover provider.

use std::panic;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{DataType, QuickFuncStatement};

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, pos)
    }));
    match result {
        Ok(hover) => hover,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Hover panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let doc = doc?;
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;
    let content = hover_content_for(token, index, doc)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

// ── Token dispatch ─────────────────────────────────────────────────────────────

fn hover_content_for(token: &Token, index: usize, doc: &Document) -> Option<String> {
    match &token.token_type {

        // ── Section keywords ──────────────────────────────────────────────
        TokenType::SectionConfig => Some(section_hover("@CONFIG",
            "Compiler settings and file metadata.",
            "@CONFIG(\n  version -> \"1.0.0\"\n  author -> \"name\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",
            "All entries use `key -> value` syntax.\n\n**Keys:** `version`, `author`, `created`, `encoding`, `debug_mode` (`off`/`regular`/`verbose`), `error_handling` (`halt`/`continue`/`recover`), `compatibility_mode` (`strict`/`best_effort`/`permissive`), `features` (`basic`/`advanced`)."
        )),
        TokenType::SectionImports => Some(section_hover("@IMPORTS",
            "Import other `.mdix` files.",
            "@IMPORTS(\n  Utils from \"common/utils.mdix\"\n  Base  from_cloud \"https://example.com/base.mdix\"\n)",
            "The alias becomes a namespace. Call: `Utils.myFunc(x)`. Access enums: `Utils.Status.ACTIVE`.\n\nOptional: `verify \"hash\"` to check file integrity."
        )),
        TokenType::SectionDLM => Some(section_hover("@DLM",
            "Data Lifecycle Modules — applied at compile time.",
            "@DLM(\n  DCompressor.gzip\n  DEncryptor.aes256\n)",
            "**Compressors:** `DCompressor.gzip`, `.bzip2`, `.lzma`\n\n**Encryptors:** `DEncryptor.aes256`, `.aes128`, `.chacha20`, `.xor`\n\n**Auditor:** `DAuditor.diy`, `.enhanced`\n\nIf `DEncryptor` is present, `@SECURITY` is required."
        )),
        TokenType::SectionEnums => Some(section_hover("@ENUMS",
            "Named integer constants.",
            "@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n  AIType     { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }\n)",
            "Values auto-increment from 0 if omitted. Access with `EnumName.FIELD`. Annotate variables `<enum>` to enable enum access."
        )),
        TokenType::SectionQuickFuncs => Some(section_hover("@QUICKFUNCS",
            "Compile-time functions — zero runtime overhead.",
            "@QUICKFUNCS(\n  ~weapon<object>(id, damage<int>) {\n    return {\n      id     = id\n      damage = damage\n      range  = damage * 2\n    }\n  }\n)",
            "All computation happens at compile time.\n\n**Syntax:** `~name<returnType>(params) { ... return expr }`\n\n**Statements:** `let`, `let mut`, `const`, `if:`, `elif:`, `else`, `chk:`, `return`, `log:`"
        )),
        TokenType::SectionData => Some(section_hover("@DATA",
            "Data payload — the main output of the file.",
            "@DATA(\n  // Flat properties (single =)\n  app_name = \"MyApp\"\n  port<int> = 8080\n\n  // Table property (single :)\n  server: host = \"localhost\", port = 8080\n\n  // Group array (double ::)\n  tags:: \"alpha\", \"beta\", \"v1\"\n)",
            "**Two-tier ordering rule:** flat properties must come before any table/group entries.\n\nCommas between entries are optional."
        )),
        TokenType::SectionSecurity => Some(section_hover("@SECURITY",
            "Encryption configuration.",
            "@SECURITY(\n  encryption -> {\n    mode = \"keyfile\",\n    algorithm = \"aes256-gcm\"\n  }\n)",
            "**Modes:** `\"password\"` (user-supplied at compile time), `\"keyfile\"` (auto-generated `.key` file)\n\n**Algorithms:** `\"aes256-gcm\"`, `\"aes128-gcm\"`, `\"chacha20-poly1305\"`\n\nCompile: `mdix compile secrets.mdix --password`"
        )),

        // ── Language keywords ──────────────────────────────────────────────
        TokenType::Keyword(kw) => hover_keyword(kw),

        // ── Boolean / null literals ────────────────────────────────────────
        TokenType::Bool(b) => Some(format!(
            "**`{}`** — boolean literal\n\nType: `<bool>`.",
            if *b { "true" } else { "false" }
        )),

        // ── Enum access ────────────────────────────────────────────────────
        TokenType::EnumAccess { enum_name, value } => {
            hover_enum_access(doc, enum_name, value)
        }

        // ── Identifiers ────────────────────────────────────────────────────
        TokenType::Identifier(name) => {
            if token.section == SectionId::Config {
                hover_config_key(name)
                    .or_else(|| hover_identifier(doc, name, token.section))
            } else {
                hover_identifier(doc, name, token.section)
            }
        }

        // ── Date / Timestamp ───────────────────────────────────────────────
        TokenType::Date(d)       => hover_date(d),
        TokenType::Timestamp(ts) => hover_timestamp(ts),

        // ── Static method calls ────────────────────────────────────────────
        TokenType::StaticFunction { class, method } => hover_static_method(class, method),

        // ── Prefixed constructors ──────────────────────────────────────────
        TokenType::RegexConstructor(_)  => Some(hover_regex(&doc.tokens, index)),
        TokenType::BlobConstructor(_)   => Some(hover_blob(&doc.tokens, index)),
        TokenType::TupleConstructor(_)  => Some(concat!(
            "**`t:(...)`** — tuple constructor\n\n",
            "Mixed-type collection, maximum 6 elements.\n\n",
            "```mdix\ncoord = t:(128.5, 0.0, -64.3)\n```\n\n",
            "Methods: `.first()`, `.second()`, `.get(index)`, `.toArray()`."
        ).to_string()),

        // ── HexColor ──────────────────────────────────────────────────────
        TokenType::HexColor(hex) => hover_hex_color(hex),

        // ── Numeric literals ───────────────────────────────────────────────
        TokenType::Integer(i)            => Some(format!("**`{}`** — integer literal (`<int>`)", i)),
        TokenType::Float(f)              => Some(format!("**`{}f`** — 32-bit float literal (`<float>`)", f)),
        TokenType::Double(d)             => Some(format!("**`{}`** — 64-bit double literal (`<double>`)\n\nStored as IEEE 754 `f64` — full precision.", d)),
        TokenType::ScientificNotation(d) => Some(format!("**`{:e}`** — scientific notation (`<double>`)\n\nStored as IEEE 754 `f64` — full precision.", d)),
        TokenType::HexLiteral(i)         => Some(format!("**`0x{:X}`** — hex integer literal (`<hex>`, value: {})", i, i)),

        // ── String literals ────────────────────────────────────────────────
        TokenType::String(s) => Some(format!(
            "**String literal** (`<string>`)\n\nLength: {} characters\n\n```mdix\n\"{}\"\n```",
            s.len(), s
        )),
        TokenType::StringSingle(s) => Some(format!(
            "**String literal** (`<string>`, single-quoted)\n\nLength: {} characters\n\n```mdix\n'{}'\n```",
            s.len(), s
        )),
        TokenType::InterpolatedString(s) => Some(format!(
            "**Interpolated string** (`<string>`)\n\nUse `{{expr}}` to embed expressions at compile time.\n\n```mdix\n$\"{}\"\n```",
            s
        )),

        // ── Operators ──────────────────────────────────────────────────────
        TokenType::ArithmeticOp(op)       => hover_operator(op, "arithmetic"),
        TokenType::ArithmeticAssignOp(op) => hover_operator(op, "arithmetic assignment"),
        TokenType::ComparisonOp(op)       => hover_operator(op, "comparison"),
        TokenType::LogicalOp(op)          => hover_operator(op, "logical"),
        TokenType::BitwiseOp(op)          => hover_operator(op, "bitwise"),
        TokenType::DoubleColon            => Some("**`::`** — group array operator\n\nDefines a group array entry in `@DATA`.\n\n```mdix\ntags:: \"alpha\", \"beta\", \"v1\"\n```".to_string()),
        TokenType::Arrow                  => Some("**`=>`** — association operator\n\nUsed in QuickFunc scope declarations.".to_string()),
        TokenType::SwitchCase             => Some("**`->`** — association / switch-case operator\n\nIn `@CONFIG`/`@SECURITY`: maps key to value block.\nIn `chk:`: introduces a case.\n\n```mdix\nencryption -> { mode = \"password\" }\n```".to_string()),
        TokenType::Symbol('~') => Some("**`~`** — QuickFunc declaration prefix\n\n```mdix\n~myFunc<int>(x<int>) { return x * 2 }\n```".to_string()),
        // ── DataType annotations ───────────────────────────────────────────
        TokenType::DataType(dt) => hover_data_type(dt),

        _ => None,
    }
}

// ── Section hover helper ───────────────────────────────────────────────────────

fn section_hover(name: &str, description: &str, example: &str, notes: &str) -> String {
    format!(
        "**`{}`** — DixScript section\n\n{}\n\n```mdix\n{}\n```\n\n{}",
        name, description, example, notes
    )
}

// ── DLM module hover ───────────────────────────────────────────────────────────

fn hover_dlm_module(name: &str) -> Option<String> {
    match name {
        "DCompressor" => Some(concat!(
            "**`DCompressor`** — DLM compression module\n\n",
            "| Subtype | Algorithm | Notes |\n",
            "|---------|-----------|-------|\n",
            "| `gzip`  | DEFLATE   | Best compatibility; available on all targets |\n",
            "| `bzip2` | BWT+Huffman | ~15% better than gzip; not on wasm32 |\n",
            "| `lzma`  | LZMA      | Best ratio; slowest; not on wasm32 |\n\n",
            "```mdix\n@DLM(\n  DCompressor.gzip\n)\n```"
        ).to_string()),
        "DEncryptor" => Some(concat!(
            "**`DEncryptor`** — DLM encryption module\n\n",
            "Requires an `@SECURITY` section.\n\n",
            "| Subtype    | Algorithm         | Key size |\n",
            "|------------|-------------------|----------|\n",
            "| `aes256`   | AES-256-GCM       | 256-bit  |\n",
            "| `aes128`   | AES-128-GCM       | 128-bit  |\n",
            "| `chacha20` | ChaCha20-Poly1305 | 256-bit  |\n",
            "| `xor`      | XOR (⚠️ weak)     | varies   |\n\n",
            "```mdix\n@DLM(DCompressor.gzip, DEncryptor.aes256)\n```"
        ).to_string()),
        "DAuditor" => Some(concat!(
            "**`DAuditor`** — DLM audit module\n\n",
            "| Subtype    | Behaviour |\n",
            "|------------|-----------|\n",
            "| `diy`      | Calls a user-registered audit hook |\n",
            "| `enhanced` | Built-in checksum + metadata audit |\n\n",
            "```mdix\n@DLM(\n  DAuditor.enhanced\n)\n```"
        ).to_string()),
        _ => None,
    }
}

fn hover_dlm_subtype(name: &str) -> Option<String> {
    match name {
        "gzip"     => Some("**`gzip`** — DEFLATE compression\n\nAvailable on all targets including `wasm32`.\n\nUsage: `DCompressor.gzip`".to_string()),
        "bzip2"    => Some("**`bzip2`** — BWT + Huffman compression\n\nBetter ratio than gzip, slower. Not on `wasm32`.\n\nUsage: `DCompressor.bzip2`".to_string()),
        "lzma"     => Some("**`lzma`** — LZMA compression\n\nBest ratio, slowest. Not on `wasm32`.\n\nUsage: `DCompressor.lzma`".to_string()),
        "aes256"   => Some("**`aes256`** — AES-256-GCM encryption\n\nRecommended default. 256-bit key.\n\nUsage: `DEncryptor.aes256`".to_string()),
        "aes128"   => Some("**`aes128`** — AES-128-GCM encryption\n\n128-bit key. Faster on targets without hardware AES.\n\nUsage: `DEncryptor.aes128`".to_string()),
        "chacha20" => Some("**`chacha20`** — ChaCha20-Poly1305 encryption\n\n256-bit key. Preferred on mobile CPUs without hardware AES.\n\nUsage: `DEncryptor.chacha20`".to_string()),
        "xor"      => Some("**`xor`** — XOR obfuscation\n\n⚠️ **Not real encryption.** For obfuscation only.\n\nUsage: `DEncryptor.xor`".to_string()),
        "diy"      => Some("**`diy`** — DIY audit hook\n\nCalls your registered `DAuditor` callback.\n\nUsage: `DAuditor.diy`".to_string()),
        "enhanced" => Some("**`enhanced`** — Enhanced built-in audit\n\nChecksum + metadata integrity check.\n\nUsage: `DAuditor.enhanced`".to_string()),
        _ => None,
    }
}

// ── CONFIG key hover ───────────────────────────────────────────────────────────

fn hover_config_key(name: &str) -> Option<String> {
    let content = match name.to_lowercase().as_str() {
        "version"            => "**`version`** — CONFIG key\n\nDixScript format version. Must match the compiler.\n\nExample: `version -> \"1.0.0\"`",
        "encoding"           => "**`encoding`** — CONFIG key\n\nSource file character encoding.\n\nSupported: `\"utf-8\"` *(default)*, `\"utf-16\"`, `\"ascii\"`, `\"iso-8859-1\"`",
        "author"             => "**`author`** — CONFIG key\n\nFile author. Free-form string.",
        "created"            => "**`created`** — CONFIG key\n\nFile creation timestamp. Format: `YYYY-MM-DDThh:mm:ssZ`",
        "features"           => "**`features`** — CONFIG key\n\n| Value | Sections |\n|-------|----------|\n| `\"basic\"` | DATA, SECURITY only |\n| `\"advanced\"` | All sections *(default)* |",
        "debug_mode"         => "**`debug_mode`** — CONFIG key\n\n| Value | Effect |\n|-------|--------|\n| `\"off\"` | No output *(default)* |\n| `\"regular\"` | Key resolution |\n| `\"verbose\"` | Full trace |",
        "error_handling"     => "**`error_handling`** — CONFIG key\n\n| Value | Behaviour |\n|-------|----------|\n| `\"halt\"` | Stop on first error *(default)* |\n| `\"continue\"` | Collect all |\n| `\"recover\"` | Try to parse past |",
        "compatibility_mode" => "**`compatibility_mode`** — CONFIG key\n\n| Value | Behaviour |\n|-------|----------|\n| `\"strict\"` | Reject unknown *(default)* |\n| `\"best_effort\"` | Warn, continue |\n| `\"permissive\"` | Accept anything |",
        _ => return None,
    };
    Some(content.to_string())
}

// ── Keyword hover ──────────────────────────────────────────────────────────────

fn hover_keyword(kw: &str) -> Option<String> {
    // Data type keywords are handled by hover_data_type.
    match kw {
        "int" | "float" | "double" | "string" | "bool"
        | "array" | "tuple" | "object" | "hex" | "blob"
        | "regex" | "date" | "timestamp" | "enum" | "any" => {
            return hover_data_type(kw);
        }
        _ => {}
    }

    let content: &str = match kw {
        "if" | "if:"    => "**`if:`** — conditional\n\nNote: DixScript uses `if:` (colon required).\n\n```mdix\nif: x > 0 {\n  return x\n} else {\n  return -1\n}\n```",
        "elif" | "elif:"=> "**`elif:`** — else-if branch\n\n```mdix\nelif: difficulty == Difficulty.HARD {\n  multiplier = 2.0\n}\n```",
        "else"          => "**`else`** — fallback branch\n\nNo colon.",
        "chk" | "chk:" => "**`chk:`** — switch/match\n\n```mdix\nchk: aiType {\n  -> AIType.PASSIVE { return 0 }\n  -> miss           { return 5 }\n}\n```",
        "miss"          => "**`miss`** — default case in `chk:`\n\nMust be the last case.",
        "return"        => "**`return`** — return a value from a QuickFunc\n\n```mdix\nreturn { id = id, damage = damage }\n```",
        "log" | "log:"  => "**`log:`** — compile-time log\n\nLogs during compilation. No runtime effect.\n\n```mdix\nlog: \"Processing \" + name\n```",
        "let"           => "**`let`** — immutable local variable\n\n```mdix\nlet result = x + y\nlet name<string> = \"Alice\"\n```\n\nUse `let mut` for mutable.",
        "mut"           => "**`mut`** — mutable modifier\n\n```mdix\nlet mut counter<int> = 0\ncounter += 1\n```",
        "const"         => "**`const`** — compile-time constant\n\n```mdix\nconst MAX_HEALTH = 100\n```",
        "and"           => "**`and`** — logical AND\n\nEquivalent to `&&`.",
        "or"            => "**`or`** — logical OR\n\nEquivalent to `||`.",
        "not"           => "**`not`** — logical NOT\n\nEquivalent to `!`.",
        "true"          => "**`true`** — boolean literal (`<bool>`)",
        "false"         => "**`false`** — boolean literal (`<bool>`)",
        "null"          => "**`null`** — null literal\n\nRepresents an absent value.",
        "from"          => "**`from`** — import keyword\n\n```mdix\nUtils from \"common/utils.mdix\"\n```",
        "from_cloud"    => "**`from_cloud`** — remote import\n\n```mdix\nBase from_cloud \"https://example.com/base.mdix\"\n```",
        "verify"        => "**`verify`** — hash verification\n\n```mdix\nUtils from \"utils.mdix\" verify \"sha256:abc...\"\n```",
        "global"        => "**`global`** — global scope modifier",
        "then"          => "**`then`** — optional clause in extended conditional forms.",
        _               => return None,
    };
    Some(content.to_string())
}

// ── Data type annotation hover ─────────────────────────────────────────────────

fn hover_data_type(dt: &str) -> Option<String> {
    let content = match dt {
        "int"       => "**`<int>`** — 32-bit signed integer\n\nRange: −2,147,483,648 to 2,147,483,647\n\n```mdix\nport<int> = 8080\n```",
        "float"     => "**`<float>`** — 32-bit float\n\nRequires `f` suffix on literals.\n\n```mdix\nspeed<float> = 3.14f\n```",
        "double"    => "**`<double>`** — 64-bit float (IEEE 754 `f64`)\n\nDefault for decimal literals without `f` suffix. Full precision.\n\n```mdix\nprecision<double> = 3.14159265358979\n```",
        "string"    => "**`<string>`** — UTF-8 text\n\n```mdix\napp_name<string> = \"DixScript\"\n```",
        "bool"      => "**`<bool>`** — boolean\n\n```mdix\nenabled<bool> = true\n```",
        "array"     => "**`<array>`** — ordered collection\n\n```mdix\ntags:: \"alpha\", \"beta\"\n```\n\nAccess: `data.get(\"tags[0]\")`",
        "tuple"     => "**`<tuple>`** — mixed-type collection (max 6 elements)\n\n```mdix\ncoord = t:(128.5, 0.0, -64.3)\n```",
        "object"    => "**`<object>`** — key-value map\n\n```mdix\n~enemy<object>(name, health<int>) {\n  return { name = name, health = health }\n}\n```",
        "hex"       => "**`<hex>`** — hex color or integer\n\n```mdix\nprimary_color<hex> = #FF5733\nmask<hex>          = 0xFF00FF\n```",
        "blob"      => "**`<blob>`** — base64-encoded binary\n\n```mdix\navatar<blob> = b:(\"SGVsbG8gV29ybGQ=\")\n```",
        "regex"     => "**`<regex>`** — compiled regular expression\n\n```mdix\nemail<regex> = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```",
        "date"      => "**`<date>`** — ISO 8601 date\n\nFormat: `YYYY-MM-DD`\n\n```mdix\nrelease_date<date> = 2025-12-31\n```",
        "timestamp" => "**`<timestamp>`** — ISO 8601 date-time\n\nFormat: `YYYY-MM-DDThh:mm:ssZ`\n\n```mdix\ncreated_at<timestamp> = 2025-01-15T10:30:00Z\n```",
        "enum"      => "**`<enum>`** — enum value from `@ENUMS`\n\n```mdix\nlevel<enum> = Difficulty.HARD\n```\n\nAt runtime stored as `{ enum_name, field_name, value: int }`.",
        "any"       => "**`<any>`** — accepts any type\n\n```mdix\n~identity<any>(value) { return value }\n```",
        _           => return None,
    };
    Some(content.to_string())
}

// ── Operator hover ─────────────────────────────────────────────────────────────

fn hover_operator(op: &str, category: &str) -> Option<String> {
    let desc = match op {
        "+"   => "Addition or string concatenation",
        "-"   => "Subtraction",
        "*"   => "Multiplication",
        "/"   => "Division",
        "%"   => "Modulo (remainder)",
        "**"  => "Exponentiation: `2 ** 3` = 8",
        "++"  => "Increment shorthand",
        "--"  => "Decrement shorthand",
        "+="  => "Add and assign",
        "-="  => "Subtract and assign",
        "*="  => "Multiply and assign",
        "/="  => "Divide and assign",
        "%="  => "Modulo and assign",
        "**=" => "Exponentiate and assign",
        "=="  => "Equality",
        "!="  => "Inequality",
        "<"   => "Less than",
        ">"   => "Greater than",
        "<="  => "Less than or equal",
        ">="  => "Greater than or equal",
        "&&"  => "Logical AND (also: `and`)",
        "||"  => "Logical OR (also: `or`)",
        "&"   => "Bitwise AND",
        "|"   => "Bitwise OR",
        "^"   => "Bitwise XOR",
        "~"   => "Bitwise NOT / QuickFunc prefix",
        "<<"  => "Left bit shift",
        ">>"  => "Right bit shift",
        _     => return None,
    };
    Some(format!("**`{}`** — {} operator\n\n{}", op, category, desc))
}

// ── Enum access hover ──────────────────────────────────────────────────────────

fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
    let st    = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let value = st.try_get_enum_field_value(enum_name, field)?;

    Some(format!(
        "**`{}.{}`** — enum field\n\n```\n(enum) {} = {}\n```\n\nType: `<enum>`\n\nGet name at runtime: `Enum.getName(\"{}\", {})`",
        enum_name, field, field, value, enum_name, value
    ))
}

// ── QuickFunc local variable hover ────────────────────────────────────────────

fn hover_qf_local_var(doc: &Document, name: &str) -> Option<String> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;

    for func in &qf.functions {
        if let Some((dt, is_mutable)) = find_var_decl_in_stmts(&func.body, name) {
            let type_str   = dt.map(|t| format!("{}", t)).unwrap_or_else(|| "?".to_string());
            let mut_str    = if is_mutable { "mut " } else { "" };
            return Some(format!(
                "**`{}`** — local variable in `~{}`\n\nDeclared as: `let {}{}<{}>`\n\nType: `<{}>` *(from declaration)*",
                name, func.name, mut_str, name, type_str, type_str
            ));
        }
    }
    None
}

fn find_var_decl_in_stmts(
    stmts: &[QuickFuncStatement],
    name:  &str,
) -> Option<(Option<DataType>, bool)> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, data_type, is_mutable, .. } => {
                if variable_name.as_str() == name {
                    return Some((*data_type, *is_mutable));
                }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(r) = find_var_decl_in_stmts(then_branch, name) { return Some(r); }
                if let Some(eb) = else_branch {
                    if let Some(r) = find_var_decl_in_stmts(eb, name) { return Some(r); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(r) = find_var_decl_in_stmts(&case.statements, name) { return Some(r); }
                }
                if let Some(dc) = default_case {
                    if let Some(r) = find_var_decl_in_stmts(&dc.statements, name) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Table path prefix hover ────────────────────────────────────────────────────

fn hover_table_path_prefix(doc: &Document, name: &str) -> Option<String> {
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let prefix_with_dot = format!("DATA.{}.", name);

    let mut child_names: Vec<String> = Vec::new();
    let mut total_children: usize = 0;

    for path in st.data_variables.keys() {
        if path.starts_with(&prefix_with_dot) {
            total_children += 1;
            if let Some(rest) = path.strip_prefix(&prefix_with_dot) {
                let seg = rest
                    .split('.')
                    .next()
                    .and_then(|s| s.split('[').next())
                    .unwrap_or(rest)
                    .to_string();
                if !child_names.contains(&seg) {
                    child_names.push(seg);
                }
            }
        }
    }

    if total_children == 0 { return None; }

    child_names.sort();
    let shown: Vec<String> = child_names.iter().take(8).map(|s| format!("`{}`", s)).collect();
    let more = if child_names.len() > 8 {
        format!(" … and {} more", child_names.len() - 8)
    } else {
        String::new()
    };

    Some(format!(
        "**`{}`** — DATA table / group\n\n**Children:** {}{}\n\nRuntime access:\n```rust\nlet val = data.get(\"{}.property\")?;\n```",
        name, shown.join(", "), more, name
    ))
}

// ── Identifier hover (section-aware) ──────────────────────────────────────────

fn hover_identifier(doc: &Document, name: &str, section: SectionId) -> Option<String> {

    // ── 0. DLM module / subtype names ─────────────────────────────────────
    if let Some(dlm) = hover_dlm_module(name)  { return Some(dlm); }
    if let Some(dlm) = hover_dlm_subtype(name) { return Some(dlm); }

    // ── 1. QuickFuncs section: params and local vars ───────────────────────
    if section == SectionId::QuickFuncs {
        if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
            for func in &qf.functions {
                for param in &func.parameters {
                    if param.name != name { continue; }
                    let type_str = param.data_type
                        .map(|t| format!("{}", t))
                        .unwrap_or_else(|| "any".to_string());
                    let default_note = if param.default_value.is_some() {
                        "\n\n*(has a default value)*"
                    } else { "" };
                    return Some(format!(
                        "**`{}`** — parameter of `~{}`\n\nType: `<{}>`{}",
                        name, func.name, type_str, default_note
                    ));
                }
            }
        }
        if let Some(content) = hover_qf_local_var(doc, name) {
            return Some(content);
        }
    }

    // ── 2. QuickFunc declaration / call site ──────────────────────────────
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            if func.name != name { continue; }

            let params: Vec<String> = func.parameters.iter().map(|p| {
                let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                let d = if p.default_value.is_some() { " = …" } else { "" };
                format!("{}{}{}", p.name, t, d)
            }).collect();

            let ret    = func.return_type.map(|t| format!("{}", t)).unwrap_or_else(|| "?".to_string());
            let scopes = func.scope_list.as_ref()
                .map(|s| format!("\n\n**Scope:** `=> {}`", s.join(", ")))
                .unwrap_or_default();

            let doc_comment = extract_doc_comment_for_func(&doc.tokens, func.position.line)
                .map(|c| format!("{}\n\n---\n\n", c))
                .unwrap_or_default();

            let param_names: Vec<&str> = func.parameters.iter().map(|p| p.name.as_str()).collect();

            return Some(format!(
                "{}**`~{}<{}>({})` — QuickFunc**\n\nCompile-time function — zero runtime overhead.{}\n\n```mdix\n// Call in @DATA:\n{}({})\n```",
                doc_comment, name, ret, params.join(", "), scopes, name, param_names.join(", ")
            ));
        }
    }

    // ── 3. Enum type name ──────────────────────────────────────────────────
    if let Some(enums) = doc.ast.as_ref().and_then(|a| a.enums.as_ref()) {
        for decl in &enums.enums {
            if decl.name != name { continue; }
            let fields: Vec<String> = decl.fields.iter().map(|f| {
                let v = f.value.map(|n| format!(" = {}", n)).unwrap_or_default();
                format!("`{}{}`", f.name, v)
            }).collect();
            return Some(format!(
                "**`{}`** — enum type\n\n**Fields:** {}\n\nAccess: `{}.FIELD_NAME`",
                name, fields.join(", "), name
            ));
        }
    }

    // ── 4. Semantic symbol table ───────────────────────────────────────────
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {

        if let Some(var) = st.try_get_data_variable(name)
            .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))
        {
            return Some(format_data_var_hover(name, var));
        }

        let suffix = format!(".{}", name);
        let mut best: Option<(usize, String, bool, Option<DataType>)> = None;
        for (path, var) in &st.data_variables {
            if !path.ends_with(&suffix) { continue; }
            let spec = path.len();
            match &best {
                None => best = Some((spec, path.clone(), var.is_inferred, var.effective_type())),
                Some((bs, _, _, _)) if spec > *bs =>
                    best = Some((spec, path.clone(), var.is_inferred, var.effective_type())),
                _ => {}
            }
        }
        if let Some((_, path, is_inferred, eff_type)) = best {
            let type_str = eff_type.map(|t| format!("{}", t)).unwrap_or_else(|| "unknown".to_string());
            let inferred = if is_inferred { " *(inferred)*" } else { "" };
            let access   = path.strip_prefix("DATA.").unwrap_or(path.as_str());
            return Some(format!(
                "**`{}`** — DATA property\n\nFull path: `{}`\nType: `<{}>`{}\n\nRuntime access:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```",
                name, access, type_str, inferred, type_str, access
            ));
        }

        if st.is_builtin_static_object(name) {
            return hover_static_object(name);
        }

        if let Some(ns) = st.try_get_namespace(name) {
            let funcs: Vec<String> = ns.functions.keys().take(6).map(|f| format!("`{}`", f)).collect();
            let enums: Vec<String> = ns.enums.keys().take(4).map(|e| format!("`{}`", e)).collect();
            return Some(format!(
                "**`{}`** — imported namespace\n\nSource: `{}`\n\n**Functions ({}):** {}\n\n**Enums ({}):** {}\n\nCall: `{}.funcName(…)`",
                name, ns.file_path,
                ns.functions.len(), funcs.join(", "),
                ns.enums.len(), enums.join(", "),
                name
            ));
        }
    }

    // ── 5. Table path prefix in DATA ──────────────────────────────────────
    if section == SectionId::Data {
        if let Some(content) = hover_table_path_prefix(doc, name) {
            return Some(content);
        }
    }

    None
}

// ── Format DATA variable hover ────────────────────────────────────────────────

fn format_data_var_hover(
    name: &str,
    var:  &dixscript::Compiler::Utilities::VariableInfo,
) -> String {
    let type_str = var.effective_type()
        .map(|t| format!("{}", t))
        .unwrap_or_else(|| "unknown".to_string());
    let inferred = if var.is_inferred { " *(inferred)*" } else { "" };
    format!(
        "**`{}`** — DATA variable\n\nType: `<{}>`{}\n\nRuntime access:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```",
        name, type_str, inferred, type_str, name
    )
}

// ── Static object hover ────────────────────────────────────────────────────────

fn hover_static_object(name: &str) -> Option<String> {
    let (desc, methods) = match name {
        "Math"      => ("Mathematical functions.", vec!["sqrt(x)","pow(base,exp)","abs(x)","floor(x)","ceil(x)","round(x)","min(a,b)","max(a,b)","clamp(v,min,max)","sin(x)","cos(x)","tan(x)","log(x)","pi()","e()"]),
        "DateTime"  => ("Date and time utilities.", vec!["now()","today()","format(ts,pat)","year(d)","month(d)","day(d)","addDays(d,n)","subtract(a,b)","isLeapYear(y)"]),
        "Array"     => ("Array factory functions.", vec!["empty()","range(start,end)","fill(val,count)","sort(arr)","unique(arr)","flatten(arr)","sum(arr)","average(arr)"]),
        "Random"    => ("Pseudo-random generation.", vec!["range(min,max)","float()","double()","boolean()","choice(arr)","shuffle(arr)","alphanumeric(len)"]),
        "Guid"      => ("GUID / UUID v4 generation.", vec!["new()","parse(str)","validate(str)","empty()","format(guid,fmt)"]),
        "IpAddress" => ("IPv4 and IPv6 utilities.", vec!["parse(str)","validate(str)","isV4(str)","isV6(str)","isPrivate(str)","localhost()"]),
        "Enum"      => ("Runtime enum introspection.", vec!["getValues(name)","getName(name,val)","getValue(name,field)","count(name)","exists(name)","list()"]),
        "Dix"       => ("Logging and string utilities.", vec!["Log(msg)","LogInfo(msg)","LogWarning(msg)","LogError(msg)","Assert(cond,msg)","Format(tmpl,...args)","Join(sep,...vals)"]),
        _ => return None,
    };
    Some(format!(
        "**`{}`** — built-in static object\n\n{}\n\n**Methods:** {}\n\nType `.` after `{}` for completions.",
        name, desc,
        methods.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", "),
        name
    ))
}

// ── Static method hover ────────────────────────────────────────────────────────

fn hover_static_method(class: &str, method: &str) -> Option<String> {
    let entry = STATIC_SIGS.iter().find(|(c, m, _, _, _)| *c == class && *m == method)?;
    Some(format!(
        "**`{}.{}`** — built-in static method\n\n```\n{}\n```\n\n{}\n\n```mdix\n// Example:\n{}\n```",
        class, method, entry.2, entry.3, entry.4
    ))
}

// ── HexColor hover ─────────────────────────────────────────────────────────────

fn hover_hex_color(hex: &str) -> Option<String> {
    // HexColor tokens are stored WITHOUT '#' by the lexer.
    // We accept both forms here for safety.
    let digits = hex.trim_start_matches('#');

    let (r, g, b, a, has_alpha_channel): (u8, u8, u8, u8, bool) = match digits.len() {
        3 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n)
            };
            (expand(&digits[0..1])?, expand(&digits[1..2])?, expand(&digits[2..3])?, 255, false)
        }
        4 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n)
            };
            (
                expand(&digits[0..1])?,
                expand(&digits[1..2])?,
                expand(&digits[2..3])?,
                expand(&digits[3..4])?,
                true,
            )
        }
        6 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            255,
            false,  // 6-digit hex has NO alpha channel — fully opaque by definition
        ),
        8 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            u8::from_str_radix(&digits[6..8], 16).ok()?,
            true,
        ),
        _ => return None,
    };

    let alpha_line = if has_alpha_channel {
        let pct = (a as f32 / 255.0 * 100.0).round() as u32;
        format!("Alpha (from color) | {} | `{:02X}` | {}% opacity", a, a, pct)
    } else {
        "Alpha | — | — | No alpha channel (6-digit hex is always fully opaque)".to_string()
    };

    Some(format!(
        "**HexColor** `#{}`\n\n\
         | Channel | Dec | Hex |\n\
         |---------|-----|-----|\n\
         | Red   | {} | `{:02X}` |\n\
         | Green | {} | `{:02X}` |\n\
         | Blue  | {} | `{:02X}` |\n\
         | {} |\n\n\
         Type: `<hex>`\n\n\
         > **Tip:** Use `#RRGGBBAA` (8 digits) to include an alpha channel, e.g. `#FF573380` = 50% opacity.",
        digits.to_uppercase(), r, r, g, g, b, b, alpha_line
    ))
}

// ── Date / Timestamp hover ─────────────────────────────────────────────────────

fn hover_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 { return None; }
    let year:  u32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day:   u32 = parts[2].parse().ok()?;
    let mname = month_name(month)?;
    let suf   = ordinal_suffix(day);
    Some(format!(
        "**Date**: `{}`\n\n{} {}{}, {}\n\nType: `<date>`\n\nCreate: `DateTime.create({}, {}, {})`",
        date_str, mname, day, suf, year, year, month, day
    ))
}

fn hover_timestamp(ts: &str) -> Option<String> {
    let tz = if ts.ends_with('Z') { "UTC" }
    else if ts.contains('+') || (ts.len() > 20 && ts.chars().nth(19) == Some('-')) {
        "with UTC offset"
    } else { "local time" };
    Some(format!(
        "**Timestamp**: `{}`\n\n*{}*\n\nType: `<timestamp>`\n\nComponents: `DateTime.year(ts)`, `DateTime.hour(ts)`, …",
        ts, tz
    ))
}

// ── Regex hover ────────────────────────────────────────────────────────────────

fn hover_regex(tokens: &[Token], constructor_index: usize) -> String {
    let pattern = find_adjacent_string(tokens, constructor_index);
    match pattern {
        None => concat!(
            "**`r:(...)`** — regex constructor\n\n",
            "```mdix\nemail = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```\n\n",
            "Methods: `.test(str)`, `.match(str)`, `.matchAll(str)`, `.replace(str,repl)`, `.split(str)`"
        ).to_string(),
        Some(pat) => {
            match regex::Regex::new(&pat) {
                Ok(re) => {
                    let groups = re.captures_len().saturating_sub(1);
                    format!(
                        "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n✅ Valid — {} capture group{}\n\nType: `<regex>`",
                        pat, groups, if groups == 1 { "" } else { "s" }
                    )
                }
                Err(e) => format!(
                    "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n❌ Invalid: {}\n\nType: `<regex>`",
                    pat, e.to_string().lines().next().unwrap_or("parse error")
                ),
            }
        }
    }
}

// ── Blob hover ─────────────────────────────────────────────────────────────────

fn hover_blob(tokens: &[Token], constructor_index: usize) -> String {
    let data = find_adjacent_string(tokens, constructor_index);
    match data {
        None => concat!(
            "**`b:(...)`** — blob constructor\n\nBase64-encoded binary data.\n\n",
            "```mdix\navatar = b:(\"SGVsbG8gV29ybGQ=\")\n```\n\n",
            "Methods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.isValid()`, `.slice(start,end)`"
        ).to_string(),
        Some(b64) => {
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(&b64) {
                Ok(bytes) => {
                    let mime = detect_mime(&bytes);
                    let hex_preview: Vec<String> = bytes.iter().take(12)
                        .map(|b| format!("{:02X}", b)).collect();
                    let ellipsis = if bytes.len() > 12 { " …" } else { "" };
                    format!(
                        "**`b:(...)`** — blob\n\n📦 **{}** bytes · {} base64 chars\n\n🗂 MIME: `{}`\n\nFirst bytes: `{}{}`\n\nType: `<blob>`",
                        bytes.len(), b64.len(), mime, hex_preview.join(" "), ellipsis
                    )
                }
                Err(_) => format!(
                    "**`b:(...)`** — blob\n\n⚠️ {} chars — **invalid base64**\n\nType: `<blob>`",
                    b64.len()
                ),
            }
        }
    }
}

fn detect_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() < 4 { return "application/octet-stream"; }
    match bytes {
        b if b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF => "image/jpeg",
        b if b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47 => "image/png",
        b if b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46 => "image/gif",
        b if b.len() >= 12
            && b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46
            && b[8] == 0x57 && b[9] == 0x45 && b[10] == 0x42 && b[11] == 0x50 => "image/webp",
        b if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46 => "application/pdf",
        b if b[0] == 0x50 && b[1] == 0x4B => "application/zip",
        _ => "application/octet-stream",
    }
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

/// Find the token covering `pos` and return it with its index.
/// Used by this module and by goto_definition.rs.
pub fn token_and_index_at(tokens: &[Token], pos: Position) -> Option<(&Token, usize)> {
    let target_line = pos.line as usize + 1;  // AST is 1-based
    let target_col  = pos.character as usize + 1;
    let mut best: Option<(&Token, usize)> = None;

    for (i, token) in tokens.iter().enumerate() {
        if token.line < target_line { continue; }
        if token.line > target_line { break; }
        if token.column > target_col { break; }
        let len = token_value_len(token);
        if target_col <= token.column + len {
            best = Some((token, i));
        }
    }
    best
}

fn token_value_len(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,
        TokenType::HexColor(h)           => h.len() + 1, // stored without '#', source has '#'
        TokenType::Comment(c)            => c.len() + 2,
        TokenType::Bool(b)               => if *b { 4 } else { 5 },
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),
        TokenType::SectionConfig         =>  7,
        TokenType::SectionImports        =>  8,
        TokenType::SectionDLM            =>  4,
        TokenType::SectionEnums          =>  6,
        TokenType::SectionQuickFuncs     => 11,
        TokenType::SectionData           =>  5,
        TokenType::SectionSecurity       =>  9,
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}

fn find_adjacent_string(tokens: &[Token], start_index: usize) -> Option<String> {
    for token in tokens.iter().skip(start_index + 1).take(5) {
        match &token.token_type {
            TokenType::String(s) | TokenType::StringSingle(s) => return Some(s.clone()),
            TokenType::SectionData | TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

fn extract_doc_comment_for_func(tokens: &[Token], func_def_line: usize) -> Option<String> {
    if func_def_line == 0 { return None; }
    let search_start = func_def_line.saturating_sub(60);

    let mut spans: Vec<(usize, usize, String)> = tokens.iter()
        .filter(|t| t.line >= search_start && t.line < func_def_line)
        .filter_map(|t| {
            if let TokenType::Comment(c) = &t.token_type {
                let newlines = c.chars().filter(|&ch| ch == '\n').count();
                let end_line = t.line + newlines;
                if end_line < func_def_line { return Some((t.line, end_line, c.clone())); }
            }
            None
        })
        .collect();

    if spans.is_empty() { return None; }
    spans.sort_by_key(|(s, _, _)| *s);

    let mut collected: Vec<String> = Vec::new();
    let mut expected_end = func_def_line.saturating_sub(1);

    for (start, end, content) in spans.iter().rev() {
        if *end == expected_end {
            collected.insert(0, content.clone());
            expected_end = start.saturating_sub(1);
        } else {
            break;
        }
    }

    if collected.is_empty() { return None; }
    let raw = collected.join("\n").trim().to_string();
    let cleaned: String = raw.lines()
        .map(|l| l.trim_start_matches('/').trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    Some(cleaned)
}

// ── Minimal static sig table ───────────────────────────────────────────────────

static STATIC_SIGS: &[(&str, &str, &str, &str, &str)] = &[
    ("Math","sqrt",  "Math.sqrt(x: double) → double",       "Square root. x must be ≥ 0.",     "Math.sqrt(16)       // → 4.0"),
    ("Math","abs",   "Math.abs(x: number) → double",         "Absolute value.",                 "Math.abs(-42)       // → 42.0"),
    ("Math","pow",   "Math.pow(base, exp: double) → double", "base raised to exp.",             "Math.pow(2, 10)     // → 1024.0"),
    ("Math","floor", "Math.floor(x: double) → int",          "Largest integer ≤ x.",            "Math.floor(3.9)     // → 3"),
    ("Math","ceil",  "Math.ceil(x: double) → int",           "Smallest integer ≥ x.",           "Math.ceil(3.1)      // → 4"),
    ("Math","round", "Math.round(x: double) → int",          "Round to nearest integer.",       "Math.round(3.5)     // → 4"),
    ("Math","clamp", "Math.clamp(v, min, max) → double",     "Clamp v so min ≤ result ≤ max.", "Math.clamp(15,0,10) // → 10.0"),
    ("Math","pi",    "Math.pi() → double",                   "π ≈ 3.14159265358979",            "Math.pi()           // → 3.14159…"),
    ("DateTime","now",    "DateTime.now() → timestamp",       "Current UTC date-time.",         "now = DateTime.now()"),
    ("DateTime","today",  "DateTime.today() → date",          "Today's date at midnight UTC.",  "today = DateTime.today()"),
    ("DateTime","format", "DateTime.format(ts, pat) → string","Format via strftime pattern.",   "DateTime.format(DateTime.now(), \"%Y-%m-%d\")"),
    ("Array","range",  "Array.range(start, end: int) → array","Integers from start to end.",   "Array.range(1, 5) // → [1,2,3,4,5]"),
    ("Random","range", "Random.range(min, max: int) → int",   "Random int in [min,max].",      "Random.range(1, 6)"),
    ("Guid","new",     "Guid.new() → string",                 "Generate a UUID v4 string.",    "id = Guid.new()"),
    ("Dix","Log",      "Dix.Log(message: any) → void",        "Log at INFO level.",            "Dix.Log(\"Building \" + name)"),
    ("Dix","Assert",   "Dix.Assert(cond, msg) → void",        "Abort if condition is false.",  "Dix.Assert(health > 0, \"positive\")"),
    ("Enum","getValues","Enum.getValues(name) → array",       "All field names of an enum.",   "Enum.getValues(\"Difficulty\")"),
];

// ── Calendar helpers ───────────────────────────────────────────────────────────

fn month_name(m: u32) -> Option<&'static str> {
    match m {
        1  => Some("January"),   2  => Some("February"), 3  => Some("March"),
        4  => Some("April"),     5  => Some("May"),       6  => Some("June"),
        7  => Some("July"),      8  => Some("August"),    9  => Some("September"),
        10 => Some("October"),   11 => Some("November"),  12 => Some("December"),
        _  => None,
    }
}

fn ordinal_suffix(d: u32) -> &'static str {
    match d {
        11 | 12 | 13         => "th",
        n if n % 10 == 1     => "st",
        n if n % 10 == 2     => "nd",
        n if n % 10 == 3     => "rd",
        _                    => "th",
    }
}
