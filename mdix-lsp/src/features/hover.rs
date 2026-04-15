//! Hover provider.
//!
//! Shows rich IDE-style documentation for:
//! - Every @section keyword   → purpose + syntax example
//! - Every language keyword   → what it does + usage snippet
//! - QuickFunc names          → full signature
//! - DATA variables           → name + inferred type
//! - Enum access              → enum name, field name, numeric value
//! - Static method calls      → full signature + description + example
//! - ALL built-in static objects → method catalogue
//! - Date / Timestamp         → human-readable breakdown
//! - HexColor                 → RGBA channel values
//! - Regex r:(...)            → pattern validity
//! - Blob b:(...)             → decoded size + hex preview + MIME type

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Hover> {
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

        // ── Section keywords ───────────────────────────────────────────────
        TokenType::SectionConfig     => Some(section_hover("@CONFIG",
                                                           "Compiler settings and file metadata.",
                                                           "@CONFIG(\n  version -> \"1.0.0\"\n  author -> \"name\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",
                                                           "All entries use `key -> value` syntax (arrow, not equals).\n\n**Keys:** `version`, `author`, `created`, `encoding`, `debug_mode` (`off`/`regular`/`verbose`), `error_handling` (`halt`/`continue`/`recover`), `compatibility_mode` (`strict`/`best_effort`/`permissive`), `features` (`basic`/`advanced`/section list)."
        )),
        TokenType::SectionImports    => Some(section_hover("@IMPORTS",
                                                           "Import other `.mdix` files.",
                                                           "@IMPORTS(\n  Utils from \"common/utils.mdix\"\n  Base  from_cloud \"https://example.com/base.mdix\"\n)",
                                                           "The alias becomes a namespace. Call: `Utils.myFunc(x)`. Access enums: `Utils.Status.ACTIVE`.\n\nOptional: `verify \"hash\"` to check file integrity."
        )),
        TokenType::SectionDLM        => Some(section_hover("@DLM",
                                                           "Data Lifecycle Modules — applied at compile time.",
                                                           "@DLM(\n  DCompressor.gzip\n  DEncryptor.aes256\n)",
                                                           "**Compressors:** `DCompressor.gzip`, `.bzip2`, `.lzma`\n\n**Encryptors:** `DEncryptor.aes256`, `.aes128`, `.chacha20`, `.xor`\n\n**Auditor:** `DAuditor.diy`, `.enhanced`\n\nIf `DEncryptor` is present, `@SECURITY` is required."
        )),
        TokenType::SectionEnums      => Some(section_hover("@ENUMS",
                                                           "Named integer constants.",
                                                           "@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n  AIType     { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }\n)",
                                                           "Values auto-increment from 0 if omitted. Access with `EnumName.FIELD`. Annotate variables `<enum>` to enable enum access.\n\nAt runtime, enum values are resolved to their integer and stored as `Enum { enum_name, field_name, value }`."
        )),
        TokenType::SectionQuickFuncs => Some(section_hover("@QUICKFUNCS",
                                                           "Compile-time functions — zero runtime overhead.",
                                                           "@QUICKFUNCS(\n  ~weapon<object>(id, damage<int>) {\n    return {\n      id     = id\n      damage = damage\n      range  = damage * 2\n    }\n  }\n)",
                                                           "All computation happens at compile time. The binary contains only resolved data.\n\n**Syntax:** `~name<returnType>(params) { ... return expr }` \n\n**Supported statements:** `let`, `let mut`, `const`, `if:`, `elif:`, `else`, `chk:`, `return`, `log:`\n\n**Operators:** `+`, `-`, `*`, `/`, `%`, `**`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`/`and`, `||`/`or`, `!`/`not`, ternary `? :`"
        )),
        TokenType::SectionData       => Some(section_hover("@DATA",
                                                           "Data payload — the main output of the file.",
                                                           "@DATA(\n  // Flat properties (single =)\n  app_name = \"MyApp\"\n  port<int> = 8080\n\n  // Table property (single :)\n  server: host = \"localhost\", port = 8080\n\n  // Group array (double ::)\n  tags:: \"alpha\", \"beta\", \"v1\"\n)",
                                                           "**Two-tier ordering rule:** flat properties must come before any table/group entries.\n\n**Commas between entries are optional.** Commas inside function calls and object literals are required.\n\nType annotations `<int>` are optional; the compiler infers types automatically."
        )),
        TokenType::SectionSecurity   => Some(section_hover("@SECURITY",
                                                           "Encryption configuration.",
                                                           "@SECURITY(\n  encryption -> {\n    mode = \"keyfile\",\n    algorithm = \"aes256-gcm\"\n  }\n)",
                                                           "**Modes:** `\"password\"` (user-supplied at compile time), `\"keyfile\"` (auto-generated `.key` file)\n\n**Algorithms:** `\"aes256-gcm\"`, `\"aes128-gcm\"`, `\"chacha20-poly1305\"`\n\nCompile: `mdix compile secrets.mdix --password`\nLoad: `DixLoadOptions::with_key_file(\"path.key\")`"
        )),

        // ── Language keywords ──────────────────────────────────────────────
        TokenType::Keyword(kw) => hover_keyword(kw),

        // ── Boolean / null literals ────────────────────────────────────────
        TokenType::Bool(b) => Some(format!(
            "**`{}`** — boolean literal\n\nType: `<bool>`. Use in conditions and as property values.",
            if *b { "true" } else { "false" }
        )),

        // ── Enum access ────────────────────────────────────────────────────
        TokenType::EnumAccess { enum_name, value } => {
            hover_enum_access(doc, enum_name, value)
        }

        // ── Identifiers ────────────────────────────────────────────────────
        TokenType::Identifier(name) => hover_identifier(doc, name),

        // ── Date / Timestamp ───────────────────────────────────────────────
        TokenType::Date(d)        => hover_date(d),
        TokenType::Timestamp(ts)  => hover_timestamp(ts),

        // ── Static method calls ────────────────────────────────────────────
        TokenType::StaticFunction { class, method } => hover_static_method(class, method),

        // ── Regex constructor ──────────────────────────────────────────────
        TokenType::RegexConstructor(_) => Some(hover_regex(&doc.tokens, index)),

        // ── Blob constructor ───────────────────────────────────────────────
        TokenType::BlobConstructor(_) => Some(hover_blob(&doc.tokens, index)),

        // ── HexColor ──────────────────────────────────────────────────────
        TokenType::HexColor(hex) => hover_hex_color(hex),

        // ── Numeric literals ───────────────────────────────────────────────
        TokenType::Integer(i)           => Some(format!("**`{}`** — integer literal (`<int>`)", i)),
        TokenType::Float(f)             => Some(format!("**`{}f`** — 32-bit float literal (`<float>`)", f)),
        TokenType::Double(d)            => Some(format!("**`{}`** — 64-bit double literal (`<double>`)", d)),
        TokenType::ScientificNotation(d)=> Some(format!("**`{:e}`** — scientific notation (`<double>`)", d)),
        TokenType::HexLiteral(i)        => Some(format!("**`0x{:X}`** — hex integer literal (`<hex>`, value: {})", i, i)),

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
            "**Interpolated string** (`<string>`)\n\nUse `{{expr}}` to embed expressions.\n\n```mdix\n$\"{}\"\n```",
            s
        )),

        // ── Operators ──────────────────────────────────────────────────────
        TokenType::ArithmeticOp(op)       => hover_operator(op, "arithmetic"),
        TokenType::ArithmeticAssignOp(op) => hover_operator(op, "arithmetic assignment"),
        TokenType::ComparisonOp(op)       => hover_operator(op, "comparison"),
        TokenType::LogicalOp(op)          => hover_operator(op, "logical"),
        TokenType::BitwiseOp(op)          => hover_operator(op, "bitwise"),
        TokenType::DoubleColon            => Some("**`::`** — group array operator\n\nDefines a group array entry in `@DATA`.\n\n```mdix\ntags:: \"alpha\", \"beta\", \"v1\"\n```".to_string()),
        TokenType::Arrow                  => Some("**`->`** — association operator\n\nUsed in `@CONFIG` and `@SECURITY` to map a key to a block.\n\n```mdix\nencryption -> { mode = \"password\" }\n```".to_string()),
        TokenType::SwitchCase             => Some("**`->`** — switch case operator\n\nUsed inside `chk:` blocks.\n\n```mdix\nchk: x {\n  -> 1 { return \"one\" }\n  -> miss { return \"other\" }\n}\n```".to_string()),
        TokenType::FunctionPrefix         => Some("**`~`** — QuickFunc declaration prefix\n\nAll QuickFunc names start with `~`.\n\n```mdix\n~myFunc<int>(x<int>) { return x * 2 }\n```".to_string()),

        // ── Prefixed constructors ──────────────────────────────────────────
        TokenType::TupleConstructor(_) => Some(
            "**`t:(...)`** — tuple constructor\n\nMixed-type collection, maximum 6 elements.\n\n```mdix\ncoord = t:(128.5, 0.0, -64.3)\n```\n\nAccess with `.first()`, `.second()`, `.get(index)`, `.toArray()`.".to_string()
        ),

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

// ── Keyword hover ──────────────────────────────────────────────────────────────

fn hover_keyword(kw: &str) -> Option<String> {
    let binding_int = hover_data_type("int").unwrap_or_default();
    let binding_float = hover_data_type("float").unwrap_or_default();
    let binding_double = hover_data_type("double").unwrap_or_default();
    let binding_string = hover_data_type("string").unwrap_or_default();
    let binding_bool = hover_data_type("bool").unwrap_or_default();
    let binding_array = hover_data_type("array").unwrap_or_default();
    let binding_tuple = hover_data_type("tuple").unwrap_or_default();
    let binding_object = hover_data_type("object").unwrap_or_default();
    let binding_hex = hover_data_type("hex").unwrap_or_default();
    let binding_blob = hover_data_type("blob").unwrap_or_default();
    let binding_regex = hover_data_type("regex").unwrap_or_default();
    let binding_date = hover_data_type("date").unwrap_or_default();
    let binding_timestamp = hover_data_type("timestamp").unwrap_or_default();
    let binding_enum = hover_data_type("enum").unwrap_or_default();
    let binding_any = hover_data_type("any").unwrap_or_default();
    let content = match kw {
        "if" | "if:" => "**`if:`** — conditional statement\n\nNote: DixScript uses `if:` (with colon).\n\n```mdix\nif: x > 0 {\n  return x\n} elif: x == 0 {\n  return 0\n} else {\n  return -1\n}\n```",
        "elif" | "elif:" => "**`elif:`** — else-if branch\n\nChained after `if:` or another `elif:`. Uses colon syntax.\n\n```mdix\nelif: difficulty == Difficulty.HARD {\n  multiplier = 2.0\n}\n```",
        "else"  => "**`else`** — fallback branch\n\nNo colon. Executes when all preceding `if:`/`elif:` conditions are false.",
        "chk" | "chk:"  => "**`chk:`** — switch/match statement\n\nEach case uses `->`. The default case is `-> miss { ... }`.\n\n```mdix\nchk: aiType {\n  -> AIType.PASSIVE    { return 0 }\n  -> AIType.AGGRESSIVE { return 10 }\n  -> miss              { return 5 }\n}\n```",
        "miss"  => "**`miss`** — default case in `chk:`\n\nMust be the last case.\n\n```mdix\n-> miss { return defaultValue }\n```",
        "return"=> "**`return`** — return a value from a QuickFunc\n\nEvery QuickFunc must end with `return`.\n\n```mdix\nreturn {\n  id     = id\n  damage = damage\n}\n```",
        "log" | "log:" => "**`log:`** — compile-time log statement\n\nLogs the expression value during compilation. No runtime effect.\n\n```mdix\nlog: \"Processing \" + name\nlog: someVariable\n```",
        "let"   => "**`let`** — declare an immutable local variable\n\nOptional type annotation.\n\n```mdix\nlet result = x + y\nlet name<string> = \"Alice\"\n```\n\nUse `let mut` for a mutable variable.",
        "mut"   => "**`mut`** — mutable modifier for `let`\n\n```mdix\nlet mut counter<int> = 0\ncounter += 1\n```",
        "const" => "**`const`** — compile-time constant\n\nValue cannot change. Usually all-caps by convention.\n\n```mdix\nconst MAX_HEALTH = 100\n```",
        "and"   => "**`and`** — logical AND (word form)\n\nEquivalent to `&&`. Use whichever reads clearer.\n\n```mdix\nif: x > 0 and y > 0 { ... }\n```",
        "or"    => "**`or`** — logical OR (word form)\n\nEquivalent to `||`.\n\n```mdix\nif: isAdmin or isModerator { ... }\n```",
        "not"   => "**`not`** — logical NOT (word form)\n\nEquivalent to `!`.\n\n```mdix\nif: not isEmpty { ... }\n```",
        "true"  => "**`true`** — boolean literal\n\nType: `<bool>`.",
        "false" => "**`false`** — boolean literal\n\nType: `<bool>`.",
        "null"  => "**`null`** — null literal\n\nRepresents an absent or unset value. Every type can be null.",
        "from"  => "**`from`** — import keyword\n\nImport a local `.mdix` file:\n\n```mdix\n@IMPORTS(\n  Utils from \"common/utils.mdix\"\n)\n```",
        "from_cloud" => "**`from_cloud`** — remote import keyword\n\nImport a `.mdix` file over HTTPS:\n\n```mdix\n@IMPORTS(\n  Base from_cloud \"https://example.com/base.mdix\"\n)\n```",
        "verify"=> "**`verify`** — hash verification for imports\n\nEnsures the imported file matches a known hash:\n\n```mdix\nUtils from \"utils.mdix\" verify \"sha256:abc123...\"\n```",
        "global"=> "**`global`** — scope modifier for QuickFunc scope lists\n\nMarks a QuickFunc as available globally (to all sections).",
        "then"  => "**`then`** — optional clause\n\nUsed in some extended conditional forms.",
        "int"       =>  binding_int.as_str(),
        "float"     => binding_float.as_str(),
        "double"    => binding_double.as_str(),
        "string"    => binding_string.as_str(),
        "bool"      => binding_bool.as_str(),
        "array"     => binding_array.as_str(),
        "tuple"     => binding_tuple.as_str(),
        "object"    => binding_object.as_str(),
        "hex"       => binding_hex.as_str(),
        "blob"      => binding_blob.as_str(),
        "regex"     => binding_regex.as_str(),
        "date"      => binding_date.as_str(),
        "timestamp" => binding_timestamp.as_str(),
        "enum"      => binding_enum.as_str(),
        "any"       => binding_any.as_str(),
        _ => return None,
    };
    Some(content.to_string())
}

// ── Data type annotation hover ─────────────────────────────────────────────────

fn hover_data_type(dt: &str) -> Option<String> {
    let content = match dt {
        "int"       => "**`<int>`** — 32-bit signed integer\n\nRange: −2,147,483,648 to 2,147,483,647\n\n```mdix\nport<int>       = 8080\nmax_players<int>= 100\n```",
        "float"     => "**`<float>`** — 32-bit floating point\n\nRequires `f` suffix on literals.\n\n```mdix\nspeed<float>    = 3.14f\ndamage<float>   = -0.5f\n```",
        "double"    => "**`<double>`** — 64-bit floating point\n\nDefault for decimal literals without `f`.\n\n```mdix\nprecision<double> = 3.14159265358979\n```",
        "string"    => "**`<string>`** — UTF-8 text\n\nDouble or single quotes. Interpolated: `$\"Hello {name}\"`\n\n```mdix\napp_name<string> = \"DixScript\"\npath<string>     = 'relative/path'\n```",
        "bool"      => "**`<bool>`** — boolean\n\nLiterals: `true`, `false`\n\n```mdix\nenabled<bool> = true\ndebug<bool>   = false\n```",
        "array"     => "**`<array>`** — ordered collection\n\nDeclare with `::` in `@DATA`. Elements separated by commas.\n\n```mdix\ntags:: \"alpha\", \"beta\"\nscores:: 10, 20, 30\n```\n\nAccess: `data.get(\"tags[0]\")`",
        "tuple"     => "**`<tuple>`** — mixed-type collection (max 6 elements)\n\nUse `t:(...)` constructor.\n\n```mdix\ncoord = t:(128.5, 0.0, -64.3)\ncolor = t:(255, 128, 0, 255)\n```\n\nAccess: `.first()`, `.second()`, `.get(index)`",
        "object"    => "**`<object>`** — key-value map\n\nUsed as QuickFunc return type. Use `{ key = value }` syntax.\n\n```mdix\n~enemy<object>(name, health<int>) {\n  return { name = name, health = health }\n}\n```",
        "hex"       => "**`<hex>`** — hex color or integer\n\nColour: `#RRGGBB` or `#RRGGBBAA`. Integer: `0xDEAD`\n\n```mdix\nprimary_color<hex> = #FF5733\nmask<hex>          = 0xFF00FF\n```",
        "blob"      => "**`<blob>`** — base64-encoded binary data\n\nUse `b:(\"base64string\")` constructor.\n\n```mdix\navatar<blob> = b:(\"SGVsbG8gV29ybGQ=\")\n```\n\nInstance methods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.slice(start, end)`",
        "regex"     => "**`<regex>`** — compiled regular expression\n\nUse `r:(\"pattern\")` constructor. Pattern is validated at compile time.\n\n```mdix\nemail_pattern<regex> = r:(\"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\")\n```\n\nInstance methods: `.test(str)`, `.match(str)`, `.matchAll(str)`, `.replace(str, repl)`, `.split(str)`",
        "date"      => "**`<date>`** — ISO 8601 date\n\nFormat: `YYYY-MM-DD`\n\n```mdix\nrelease_date<date> = 2025-12-31\nbirth_date<date>   = 1990-06-15\n```\n\nCreate: `DateTime.create(year, month, day)`",
        "timestamp" => "**`<timestamp>`** — ISO 8601 date-time\n\nFormat: `YYYY-MM-DDThh:mm:ssZ` (UTC) or with offset `±HH:MM`\n\n```mdix\ncreated_at<timestamp> = 2025-01-15T10:30:00Z\nexpires_at<timestamp> = 2025-12-31T23:59:59+05:30\n```\n\nCreate: `DateTime.now()`, `DateTime.createTime(y,m,d,h,min,s)`",
        "enum"      => "**`<enum>`** — enum value from `@ENUMS`\n\nAnnotate with `<enum>` to allow enum access syntax.\n\n```mdix\n@ENUMS( Difficulty { EASY, NORMAL, HARD } )\n\n@DATA(\n  level<enum> = Difficulty.HARD\n)\n```\n\nAt runtime stored as `{ enum_name, field_name, value: int }`.",
        "any"       => "**`<any>`** — accepts any type\n\nNo type restriction. Useful for generic QuickFunc parameters.\n\n```mdix\n~identity<any>(value) { return value }\n```",
        _           => return None,
    };
    Some(content.to_string())
}

// ── Operator hover ─────────────────────────────────────────────────────────────

fn hover_operator(op: &str, category: &str) -> Option<String> {
    let desc = match op {
        "+"   => "Addition (numeric) or string concatenation",
        "-"   => "Subtraction",
        "*"   => "Multiplication",
        "/"   => "Division (always returns double)",
        "%"   => "Modulo (remainder)",
        "**"  => "Exponentiation: `2 ** 3` = 8",
        "++"  => "Increment shorthand",
        "--"  => "Decrement shorthand",
        "+="  => "Add and assign: `x += 1` ≡ `x = x + 1`",
        "-="  => "Subtract and assign",
        "*="  => "Multiply and assign",
        "/="  => "Divide and assign",
        "%="  => "Modulo and assign",
        "**=" => "Exponentiate and assign",
        "=="  => "Equality. Numeric types coerce (int == float with epsilon).",
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
        "&="  => "Bitwise AND and assign",
        "|="  => "Bitwise OR and assign",
        "^="  => "Bitwise XOR and assign",
        "<<=" => "Left shift and assign",
        ">>=" => "Right shift and assign",
        ">_<" => "Bitwise rotate (3-char operator)",
        "~?"  => "Bitwise test",
        "%%"  => "Extended modulo",
        "%&"  => "Modulo-AND combination",
        "&%"  => "AND-modulo combination",
        _     => return None,
    };
    Some(format!("**`{}`** — {} operator\n\n{}", op, category, desc))
}

// ── Enum access hover ──────────────────────────────────────────────────────────

fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
    let sr    = doc.semantic_result.as_ref()?;
    let st    = sr.symbol_table.as_ref()?;
    let value = st.try_get_enum_field_value(enum_name, field)?;

    Some(format!(
        "**`{}.{}`** — enum field\n\n```\n(enum) {} = {}\n```\n\nType: `<enum>`\n\nUse `Enum.getName(\"{}\", {})` to convert back to name at runtime.",
        enum_name, field, field, value, enum_name, value
    ))
}

// ── Identifier hover ───────────────────────────────────────────────────────────

fn hover_identifier(doc: &Document, name: &str) -> Option<String> {
    // QuickFunc declaration or call
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                if func.name != name { continue; }

                let params: Vec<String> = func.parameters.iter().map(|p| {
                    let t = p.data_type.as_ref()
                        .map(|dt| format!("<{:?}>", dt).to_lowercase())
                        .unwrap_or_default();
                    let d = p.default_value.as_ref()
                        .map(|_| " = …".to_string())
                        .unwrap_or_default();
                    format!("{}{}{}", p.name, t, d)
                }).collect();

                let ret = func.return_type.as_ref()
                    .map(|t| format!("{:?}", t).to_lowercase())
                    .unwrap_or_else(|| "?".to_string());

                let scopes = func.scope_list.as_ref()
                    .map(|s| format!("\n\n**Scope:** `=> {}`", s.join(", ")))
                    .unwrap_or_default();

                return Some(format!(
                    "**`~{}<{}>({})`** — QuickFunc\n\nCompile-time function. All calls are resolved at compile time; the binary stores only the result.{}\n\n```mdix\n// Example call in @DATA:\n{}({})\n```",
                    name, ret, params.join(", "), scopes,
                    name,
                    params.iter().map(|p| p.split('<').next().unwrap_or("…")).collect::<Vec<_>>().join(", ")
                ));
            }
        }

        // Enum type name
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                if decl.name != name { continue; }
                let fields: Vec<String> = decl.fields.iter().map(|f| {
                    let v = f.value.map(|n| format!(" = {}", n)).unwrap_or_default();
                    format!("`{}{}`", f.name, v)
                }).collect();
                return Some(format!(
                    "**`{}`** — enum type\n\n**Fields:** {}\n\nAccess: `{}.FIELD_NAME`\n\nAnnotate variables `<enum>` to enable enum access.",
                    name, fields.join(", "), name
                ));
            }
        }
    }

    // DATA variable
    if let Some(sr) = &doc.semantic_result {
        if let Some(st) = &sr.symbol_table {
            if let Some(var) = st.try_get_data_variable(name) {
                let type_str = var.effective_type()
                    .map(|t| format!("{:?}", t).to_lowercase())
                    .unwrap_or_else(|| "unknown".to_string());
                let inferred = if var.is_inferred { " *(inferred)*" } else { "" };
                return Some(format!(
                    "**`{}`** — DATA variable\n\nType: `<{}>`{}\n\nAccess at runtime:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```",
                    name, type_str, inferred, type_str, name
                ));
            }

            // Static object name
            if st.is_builtin_static_object(name) {
                return hover_static_object(name);
            }
        }
    }

    None
}

// ── Static object hover (when hovering the object name itself) ─────────────────

fn hover_static_object(name: &str) -> Option<String> {
    let (desc, methods) = match name {
        "Math" => ("Mathematical functions — all return numeric types.", vec![
            "sqrt(x)", "pow(base,exp)", "abs(x)", "floor(x)", "ceil(x)",
            "round(x)", "min(a,b)", "max(a,b)", "clamp(v,min,max)", "sin(x)",
            "cos(x)", "tan(x)", "log(x)", "log10(x)", "exp(x)", "pi()", "e()",
            "radians(deg)", "degrees(rad)", "sign(x)", "truncate(x)", "remainder(a,b)",
        ]),
        "DateTime" => ("Date and time utilities.", vec![
            "now()", "today()", "utcNow()", "parse(str)", "parseExact(str,fmt)",
            "create(y,m,d)", "createTime(y,m,d,h,min,s)", "fromUnixTime(n)", "toUnixTime(ts)",
            "format(ts,pat)", "year(d)", "month(d)", "day(d)", "hour(ts)", "minute(ts)",
            "second(ts)", "millisecond(ts)", "dayOfWeek(d)", "dayOfYear(d)",
            "isLeapYear(y)", "daysInMonth(y,m)", "compare(a,b)", "subtract(a,b)",
            "addDays(d,n)", "addMonths(d,n)", "addYears(d,n)", "addHours(ts,n)",
            "addMinutes(ts,n)", "addSeconds(ts,n)",
        ]),
        "Array" => ("Array factory functions.", vec![
            "empty()", "range(start,end)", "fill(val,count)", "of(v1,v2,...)",
            "concat(arr1,arr2,...)", "repeat(arr,times)", "fromString(str,sep)",
            "reverse(arr)", "sort(arr)", "unique(arr)", "slice(arr,start,end)",
            "filter(arr,val)", "contains(arr,val)", "indexOf(arr,val)",
            "lastIndexOf(arr,val)", "flatten(arr)", "sum(arr)", "average(arr)",
            "min(arr)", "max(arr)",
        ]),
        "Random" => ("Pseudo-random generation.", vec![
            "range(min,max)", "float()", "double()", "boolean()", "floatRange(min,max)",
            "doubleRange(min,max)", "choice(arr)", "choices(arr,n)", "sample(arr,n)",
            "shuffle(arr)", "bytes(n)", "string(len,charset)", "alphanumeric(len)",
            "weighted(values,weights)",
        ]),
        "Guid" => ("GUID / UUID v4 generation and validation.", vec![
            "new()", "parse(str)", "tryParse(str)", "validate(str)", "empty()",
            "format(guid,fmt)", "toBytes(guid)", "fromBytes(arr)",
        ]),
        "IpAddress" => ("IPv4 and IPv6 address utilities.", vec![
            "parse(str)", "tryParse(str)", "validate(str)", "isV4(str)", "isV6(str)",
            "isPrivate(str)", "isLoopback(str)", "isPublic(str)", "toBytes(str)",
            "fromBytes(arr)", "inRange(ip,start,end)", "localhost()", "any()", "broadcast()",
        ]),
        "Enum" => ("Runtime enum introspection (for @ENUMS-defined enums).", vec![
            "getValues(name)", "getName(name,value)", "getValue(name,fieldName)",
            "hasValue(name,fieldName)", "contains(name,value)", "count(name)",
            "exists(name)", "list()", "min(name)", "max(name)", "random(name)",
            "toArray(name)",
        ]),
        "Dix" => ("Logging and string utilities.", vec![
            "Log(msg)", "LogInfo(msg)", "LogWarning(msg)", "LogError(msg)",
            "LogDebug(msg)", "LogVerbose(msg)", "Assert(cond,msg)", "Trace(msg,ctx)",
            "Print(msg)", "PrintLine(msg)", "Format(template,...args)", "Join(sep,...values)",
        ]),
        _ => return None,
    };

    Some(format!(
        "**`{}`** — built-in static object\n\n{}\n\n**Methods:** {}\n\nType `.` after `{}` to see all method completions.",
        name, desc,
        methods.iter().map(|m| format!("`{}`", m)).collect::<Vec<_>>().join(", "),
        name
    ))
}

// ── Static method hover ────────────────────────────────────────────────────────

fn hover_static_method(class: &str, method: &str) -> Option<String> {
    let entry = STATIC_SIGS.iter().find(|(c, m, _, _,_)| *c == class && *m == method)?;
    Some(format!(
        "**`{}.{}`** — built-in static method\n\n```\n{}\n```\n\n{}\n\n```mdix\n// Example:\n{}\n```",
        class, method, entry.2, entry.3, entry.4
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
        "**Date**: `{}`\n\n{} {}{}, {}\n\nType: `<date>`\n\nCreate via `DateTime.create({}, {}, {})`",
        date_str, mname, day, suf, year, year, month, day
    ))
}

fn hover_timestamp(ts: &str) -> Option<String> {
    let tz = if ts.ends_with('Z') { "UTC" }
    else if ts.contains('+') || (ts.len() > 20 && ts.chars().nth(19) == Some('-')) { "with UTC offset" }
    else { "local time" };
    Some(format!(
        "**Timestamp**: `{}`\n\n*{}*\n\nType: `<timestamp>`\n\nGet components: `DateTime.year(ts)`, `DateTime.hour(ts)`, …",
        ts, tz
    ))
}

// ── Regex hover ────────────────────────────────────────────────────────────────

fn hover_regex(tokens: &[Token], constructor_index: usize) -> String {
    let pattern = find_adjacent_string(tokens, constructor_index);
    match pattern {
        None => "**`r:(...)`** — regex constructor\n\nProvide the pattern as a string.\n\n```mdix\nemail = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```\n\nInstance methods: `.test(str)`, `.match(str)`, `.matchAll(str)`, `.replace(str,repl)`, `.split(str)`, `.isValid()`".to_string(),
        Some(pat) => {
            match regex::Regex::new(&pat) {
                Ok(re) => {
                    let groups = re.captures_len().saturating_sub(1);
                    format!(
                        "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n✅ **Pattern valid** — {} capture group{}\n\nType: `<regex>`\n\nMethods: `.test(str)`, `.match(str)` → `[full, g1, g2, …]`, `.matchAll(str)`, `.replace(str,repl)`, `.split(str)`",
                        pat, groups, if groups == 1 { "" } else { "s" }
                    )
                }
                Err(e) => format!(
                    "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n❌ **Invalid pattern:** {}\n\nType: `<regex>`",
                    pat,
                    e.to_string().lines().next().unwrap_or("parse error")
                ),
            }
        }
    }
}

// ── Blob hover ─────────────────────────────────────────────────────────────────

fn hover_blob(tokens: &[Token], constructor_index: usize) -> String {
    let data = find_adjacent_string(tokens, constructor_index);
    match data {
        None => "**`b:(...)`** — blob constructor\n\nBase64-encoded binary data.\n\n```mdix\navatar = b:(\"SGVsbG8gV29ybGQ=\")\n```\n\nMethods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.isValid()`, `.slice(start,end)`".to_string(),
        Some(b64) => {
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(&b64) {
                Ok(bytes) => {
                    let mime = detect_mime(&bytes);
                    let preview: Vec<String> = bytes.iter().take(12)
                        .map(|b| format!("{:02X}", b)).collect();
                    let ellipsis = if bytes.len() > 12 { " …" } else { "" };
                    format!(
                        "**`b:(...)`** — blob\n\n📦 **{}** bytes · {} base64 chars\n\n🗂 MIME type: `{}`\n\nFirst bytes: `{}{}`\n\nType: `<blob>`\n\nMethods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.isValid()`, `.slice(start,end)`",
                        bytes.len(), b64.len(), mime,
                        preview.join(" "), ellipsis,
                    )
                }
                Err(_) => format!(
                    "**`b:(...)`** — blob\n\n⚠️ `{}` chars — **invalid base64 encoding**\n\nType: `<blob>`",
                    b64.len()
                ),
            }
        }
    }
}

fn detect_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() < 4 { return "application/octet-stream"; }
    match bytes {
        b if b[0]==0xFF && b[1]==0xD8 && b[2]==0xFF                             => "image/jpeg",
        b if b[0]==0x89 && b[1]==0x50 && b[2]==0x4E && b[3]==0x47              => "image/png",
        b if b[0]==0x47 && b[1]==0x49 && b[2]==0x46                            => "image/gif",
        b if b[0]==0x52 && b[1]==0x49 && b[2]==0x46 && b[3]==0x46              => "audio/wav",
        b if b[0]==0x49 && b[1]==0x44 && b[2]==0x33                            => "audio/mp3",
        b if b[0]==0x25 && b[1]==0x50 && b[2]==0x44 && b[3]==0x46              => "application/pdf",
        b if b[0]==0x50 && b[1]==0x4B                                           => "application/zip",
        b if b[0]==0x7F && b[1]==0x45 && b[2]==0x4C && b[3]==0x46              => "application/elf",
        _ => "application/octet-stream",
    }
}

// ── HexColor hover ─────────────────────────────────────────────────────────────

fn hover_hex_color(hex: &str) -> Option<String> {
    use crate::features::document_color::parse_hex_color;
    let color = parse_hex_color(hex)?;
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let stripped = hex.trim_start_matches('#').to_uppercase();
    let alpha_line = if a == 255 {
        "Alpha: 255 (fully opaque)".to_string()
    } else {
        format!("Alpha: {} ({:.0}% opacity)", a, a as f32 / 255.0 * 100.0)
    };

    Some(format!(
        "**HexColor** `#{}`\n\n| Channel | Dec | Hex |\n|---------|-----|-----|\n| Red   | {} | `{:02X}` |\n| Green | {} | `{:02X}` |\n| Blue  | {} | `{:02X}` |\n\n{}\n\nType: `<hex>`\n\nClick the color swatch to open the color picker.",
        stripped, r, r, g, g, b, b, alpha_line
    ))
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

pub fn token_and_index_at(tokens: &[Token], pos: Position) -> Option<(&Token, usize)> {
    let target_line = pos.line as usize + 1;
    let target_col  = pos.character as usize + 1;
    let mut best: Option<(&Token, usize)> = None;

    for (i, token) in tokens.iter().enumerate() {
        if token.line != target_line { continue; }
        if token.column > target_col { break; }
        let len = token_value_len(token);
        if target_col <= token.column + len {
            best = Some((token, i));
        }
    }
    best
}

pub fn token_at(tokens: &[Token], pos: Position) -> Option<&Token> {
    token_and_index_at(tokens, pos).map(|(t, _)| t)
}

fn token_value_len(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,
        TokenType::HexColor(h)           => h.len() + 1,
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
            TokenType::Identifier(_) | TokenType::SectionData | TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

// ── Lookup table (class, method, signature, description, example) ─────────────

static STATIC_SIGS: &[(&str, &str, &str, &str, &str)] = &[
    ("Math","abs",      "Math.abs(x: number) → double",                      "Absolute value of x.",                                     "Math.abs(-42)        // → 42.0"),
    ("Math","sqrt",     "Math.sqrt(x: double) → double",                     "Square root. x must be ≥ 0.",                              "Math.sqrt(16)        // → 4.0"),
    ("Math","pow",      "Math.pow(base: double, exp: double) → double",       "base raised to exp.",                                      "Math.pow(2, 10)      // → 1024.0"),
    ("Math","floor",    "Math.floor(x: double) → int",                       "Largest integer ≤ x.",                                     "Math.floor(3.9)      // → 3"),
    ("Math","ceil",     "Math.ceil(x: double) → int",                        "Smallest integer ≥ x.",                                    "Math.ceil(3.1)       // → 4"),
    ("Math","round",    "Math.round(x: double) → int",                       "Round to nearest integer.",                                "Math.round(3.5)      // → 4"),
    ("Math","min",      "Math.min(a: number, b: number) → double",           "Minimum of two numbers.",                                  "Math.min(10, 3)      // → 3.0"),
    ("Math","max",      "Math.max(a: number, b: number) → double",           "Maximum of two numbers.",                                  "Math.max(10, 3)      // → 10.0"),
    ("Math","clamp",    "Math.clamp(v, min, max: number) → double",          "Clamp v so min ≤ result ≤ max.",                           "Math.clamp(15, 0, 10) // → 10.0"),
    ("Math","sign",     "Math.sign(x: number) → int",                        "Returns -1, 0, or 1.",                                     "Math.sign(-7)        // → -1"),
    ("Math","truncate", "Math.truncate(x: double) → int",                    "Integer part (truncate toward zero).",                     "Math.truncate(3.99)  // → 3"),
    ("Math","remainder","Math.remainder(dividend, divisor: number) → double","Remainder after division.",                                 "Math.remainder(7, 3) // → 1.0"),
    ("Math","sin",      "Math.sin(x: double) → double",                      "Sine of x (radians).",                                     "Math.sin(Math.pi() / 2) // → 1.0"),
    ("Math","cos",      "Math.cos(x: double) → double",                      "Cosine of x (radians).",                                   "Math.cos(0)          // → 1.0"),
    ("Math","tan",      "Math.tan(x: double) → double",                      "Tangent of x (radians).",                                  "Math.tan(Math.pi() / 4) // → 1.0"),
    ("Math","log",      "Math.log(x: double) → double",                      "Natural logarithm. x must be > 0.",                        "Math.log(Math.e())   // → 1.0"),
    ("Math","log10",    "Math.log10(x: double) → double",                    "Base-10 logarithm. x must be > 0.",                        "Math.log10(100)      // → 2.0"),
    ("Math","exp",      "Math.exp(x: double) → double",                      "e raised to the power x.",                                 "Math.exp(1)          // → 2.718…"),
    ("Math","radians",  "Math.radians(degrees: double) → double",            "Convert degrees to radians.",                              "Math.radians(180)    // → π"),
    ("Math","degrees",  "Math.degrees(radians: double) → double",            "Convert radians to degrees.",                              "Math.degrees(Math.pi()) // → 180.0"),
    ("Math","pi",       "Math.pi() → double",                                "π ≈ 3.14159265358979",                                     "Math.pi()            // → 3.14159…"),
    ("Math","e",        "Math.e() → double",                                 "e ≈ 2.71828182845905",                                     "Math.e()             // → 2.71828…"),
    ("DateTime","now",         "DateTime.now() → timestamp",                              "Current UTC date and time.",                          "now = DateTime.now()"),
    ("DateTime","today",       "DateTime.today() → date",                                "Today's date at midnight UTC.",                       "today = DateTime.today()"),
    ("DateTime","format",      "DateTime.format(ts: timestamp, pattern: string) → string","Format using strftime-style pattern.",                "DateTime.format(DateTime.now(), \"%Y-%m-%d\")"),
    ("DateTime","year",        "DateTime.year(d: date|timestamp) → int",                 "Year component.",                                     "DateTime.year(DateTime.today()) // → 2025"),
    ("DateTime","month",       "DateTime.month(d: date|timestamp) → int",                "Month 1–12.",                                         "DateTime.month(DateTime.today())"),
    ("DateTime","day",         "DateTime.day(d: date|timestamp) → int",                  "Day of month 1–31.",                                  "DateTime.day(DateTime.today())"),
    ("DateTime","subtract",    "DateTime.subtract(a, b: date|timestamp) → double",       "Difference in days (fractional).",                    "DateTime.subtract(d1, d2) // → days"),
    ("DateTime","addDays",     "DateTime.addDays(d, n: double) → date|timestamp",        "Add n days (fractional values supported).",           "DateTime.addDays(DateTime.today(), 7)"),
    ("DateTime","isLeapYear",  "DateTime.isLeapYear(year: int) → bool",                  "True if year has 366 days.",                          "DateTime.isLeapYear(2024) // → true"),
    ("Array","range",   "Array.range(start, end: int) → array",             "Integers from start to end inclusive.",                     "Array.range(1, 5) // → [1,2,3,4,5]"),
    ("Array","fill",    "Array.fill(value: any, count: int) → array",       "Array of count copies of value.",                          "Array.fill(0, 3)  // → [0,0,0]"),
    ("Array","sum",     "Array.sum(arr: array) → double",                   "Sum of numeric elements.",                                 "Array.sum([1,2,3]) // → 6.0"),
    ("Array","flatten", "Array.flatten(arr: array) → array",                "Recursively flatten all nested arrays.",                    "Array.flatten([[1,2],[3,4]]) // → [1,2,3,4]"),
    ("Array","unique",  "Array.unique(arr: array) → array",                 "Remove duplicate values (preserves order).",               "Array.unique([1,1,2,3,2]) // → [1,2,3]"),
    ("Random","range",  "Random.range(min, max: int) → int",                "Random integer in [min, max] inclusive.",                  "Random.range(1, 6) // dice roll"),
    ("Random","choice", "Random.choice(arr: array) → any",                  "Pick a random element from arr.",                          "Random.choice([\"rock\",\"paper\",\"scissors\"])"),
    ("Random","shuffle","Random.shuffle(arr: array) → array",               "Fisher-Yates shuffle — returns a new array.",              "Random.shuffle(Array.range(1, 52))"),
    ("Guid","new",      "Guid.new() → string",                              "Generate a UUID v4 string.",                               "id = Guid.new()  // \"550e8400-…\""),
    ("Guid","validate", "Guid.validate(str: string) → bool",                "True if str is a valid GUID format.",                      "Guid.validate(\"550e8400-e29b-41d4-a716-446655440000\")"),
    ("Dix","Log",       "Dix.Log(message: any) → void",                     "Log at INFO level during compilation.",                    "Dix.Log(\"Building \" + name)"),
    ("Dix","Assert",    "Dix.Assert(condition: bool, message: string) → void","Abort compilation if condition is false.",               "Dix.Assert(health > 0, \"health must be positive\")"),
    ("Dix","Format",    "Dix.Format(template: string, ...args) → string",   "Format string with {0}, {1} placeholders.",               "Dix.Format(\"Hello {0}!\", \"World\")"),
    ("Enum","getValues","Enum.getValues(enumName: string) → array",         "All field names of an @ENUMS enum.",                      "Enum.getValues(\"Difficulty\") // → [\"EASY\",…]"),
    ("Enum","exists",   "Enum.exists(enumName: string) → bool",             "True if the enum is registered.",                         "Enum.exists(\"Difficulty\")"),
    ("IpAddress","validate","IpAddress.validate(str: string) → bool",       "True if str is a valid IPv4 or IPv6 address.",             "IpAddress.validate(\"192.168.1.1\")"),
    ("IpAddress","isPrivate","IpAddress.isPrivate(str: string) → bool",     "True if in RFC-1918 / ULA range.",                        "IpAddress.isPrivate(\"10.0.0.1\") // → true"),
];

// ── Helpers ───────────────────────────────────────────────────────────────────

fn month_name(m: u32) -> Option<&'static str> {
    match m {
        1=>"January",2=>"February",3=>"March",4=>"April",5=>"May",6=>"June",
        7=>"July",8=>"August",9=>"September",10=>"October",11=>"November",12=>"December",
        _=>return None,
    }.into()
}

fn ordinal_suffix(d: u32) -> &'static str {
    match d { 11|12|13=>"th", n if n%10==1=>"st", n if n%10==2=>"nd", n if n%10==3=>"rd", _=>"th" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{Position, Url};

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(Url::parse("file:///test.mdix").unwrap(), source.to_string(), 0);
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn hover_none_doc_returns_none() {
        assert!(provide(None, Position::new(0,0)).is_none());
    }

    #[test]
    fn hover_data_section_keyword() {
        let doc = test_doc("@DATA(\n  x = 1\n)");
        let res = provide(Some(&doc), Position::new(0, 1));
        assert!(res.is_some());
    }

    #[test]
    fn hover_hex_color_rgb() {
        let res = hover_hex_color("#FF5733");
        assert!(res.is_some());
        assert!(res.unwrap().contains("255"));
    }

    #[test]
    fn hover_keyword_return() {
        let res = hover_keyword("return");
        assert!(res.is_some());
    }

    #[test]
    fn hover_data_type_all_types() {
        for t in &["int","float","double","string","bool","array","tuple","object",
            "hex","blob","regex","date","timestamp","enum","any"] {
            assert!(hover_data_type(t).is_some(), "missing hover for type: {}", t);
        }
    }

    #[test]
    fn hover_all_keywords() {
        for kw in &["if:","elif:","else","chk:","miss","return","log:","let","const",
            "and","or","not","true","false","null","from","from_cloud","verify","global"] {
            assert!(hover_keyword(kw).is_some(), "missing hover for keyword: {}", kw);
        }
    }
}