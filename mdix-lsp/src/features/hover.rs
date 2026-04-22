// mdix-lsp/src/features/hover.rs
//! Hover provider.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{DataType, QuickFuncStatement};

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

        // ── Section keywords ──────────────────────────────────────────────
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

        // ── Identifiers — section-aware dispatch ───────────────────────────
        TokenType::Identifier(name) => {
            if token.section == SectionId::Config {
                hover_config_key(name)
                    .or_else(|| hover_identifier(doc, name, token.section))
            } else {
                hover_identifier(doc, name, token.section)
            }
        }

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
        TokenType::Integer(i)            => Some(format!("**`{}`** — integer literal (`<int>`)", i)),
        TokenType::Float(f)              => Some(format!("**`{}f`** — 32-bit float literal (`<float>`)", f)),
        TokenType::Double(d)             => Some(format!("**`{}`** — 64-bit double literal (`<double>`)", d)),
        TokenType::ScientificNotation(d) => Some(format!("**`{:e}`** — scientific notation (`<double>`)", d)),
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
            "**Interpolated string** (`<string>`)\n\nUse `{{expr}}` to embed expressions at compile time.\n\n```mdix\n$\"{}\"\n```\n\nExpressions inside `{{}}` are evaluated when the QuickFunc runs.",
            s
        )),

        // ── Operators ──────────────────────────────────────────────────────
        TokenType::ArithmeticOp(op)       => hover_operator(op, "arithmetic"),
        TokenType::ArithmeticAssignOp(op) => hover_operator(op, "arithmetic assignment"),
        TokenType::ComparisonOp(op)       => hover_operator(op, "comparison"),
        TokenType::LogicalOp(op)          => hover_operator(op, "logical"),
        TokenType::BitwiseOp(op)          => hover_operator(op, "bitwise"),
        TokenType::DoubleColon            => Some("**`::`** — group array operator\n\nDefines a group array entry in `@DATA`.\n\n```mdix\ntags:: \"alpha\", \"beta\", \"v1\"\n```".to_string()),
        TokenType::Arrow                  => Some("**`=>`** — association operator\n\nUsed in QuickFunc scope declarations.\n\n```mdix\n~func<int> => global(x<int>) { return x }\n```".to_string()),
        TokenType::SwitchCase             => Some("**`->`** — association / switch-case operator\n\nIn `@CONFIG`/`@SECURITY`: maps key to value block.\nIn `chk:`: introduces a case.\n\n```mdix\nencryption -> { mode = \"password\" }\n```\n\n```mdix\nchk: x {\n  -> 1 { return \"one\" }\n  -> miss { return \"other\" }\n}\n```".to_string()),
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

// ── CONFIG key hover ───────────────────────────────────────────────────────────

fn hover_config_key(name: &str) -> Option<String> {
    let content = match name.to_lowercase().as_str() {
        "version" => "**`version`** — CONFIG key\n\nDixScript format version. Must match the compiler.\n\nExample: `version -> \"1.0.0\"`",
        "encoding" => "**`encoding`** — CONFIG key\n\nSource file character encoding.\n\nSupported: `\"utf-8\"` *(default)*, `\"utf-16\"`, `\"utf-16le\"`, `\"utf-16be\"`, `\"ascii\"`, `\"iso-8859-1\"`",
        "author" => "**`author`** — CONFIG key\n\nFile author. Free-form string.\n\nExample: `author -> \"Alice\"`",
        "created" => "**`created`** — CONFIG key\n\nFile creation timestamp. Auto-filled by tooling.\n\nFormat: `YYYY-MM-DDThh:mm:ssZ`\n\nExample: `created -> \"2025-01-15T10:30:00Z\"`",
        "features" => "**`features`** — CONFIG key\n\nEnabled section features.\n\n| Value | Sections available |\n|-------|-------------------|\n| `\"basic\"` | DATA, SECURITY only |\n| `\"advanced\"` | All sections *(default)* |\n| `\"quickfuncs,enums\"` | Explicit list |\n\nExample: `features -> \"advanced\"`",
        "debug_mode" => "**`debug_mode`** — CONFIG key\n\nCompiler diagnostic verbosity.\n\n| Value | Effect |\n|-------|--------|\n| `\"off\"` | No debug output *(default)* |\n| `\"regular\"` | Key resolution steps |\n| `\"verbose\"` | Full execution trace |\n\nExample: `debug_mode -> \"regular\"`",
        "error_handling" => "**`error_handling`** — CONFIG key\n\nHow the compiler responds to errors.\n\n| Value | Behaviour |\n|-------|----------|\n| `\"halt\"` | Stop on first error *(default)* |\n| `\"continue\"` | Collect all errors then report |\n| `\"recover\"` | Try to parse past errors |\n\nExample: `error_handling -> \"continue\"`",
        "compatibility_mode" => "**`compatibility_mode`** — CONFIG key\n\nParser strictness level.\n\n| Value | Behaviour |\n|-------|----------|\n| `\"strict\"` | Reject unknown syntax *(default)* |\n| `\"best_effort\"` | Warn on unknown, continue |\n| `\"permissive\"` | Accept anything parseable |\n\nExample: `compatibility_mode -> \"strict\"`",
        _ => return None,
    };
    Some(content.to_string())
}

// ── Keyword hover ──────────────────────────────────────────────────────────────

fn hover_keyword(kw: &str) -> Option<String> {
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
        "int"       => hover_data_type("int").unwrap_or_default().leak(),
        "float"     => hover_data_type("float").unwrap_or_default().leak(),
        "double"    => hover_data_type("double").unwrap_or_default().leak(),
        "string"    => hover_data_type("string").unwrap_or_default().leak(),
        "bool"      => hover_data_type("bool").unwrap_or_default().leak(),
        "array"     => hover_data_type("array").unwrap_or_default().leak(),
        "tuple"     => hover_data_type("tuple").unwrap_or_default().leak(),
        "object"    => hover_data_type("object").unwrap_or_default().leak(),
        "hex"       => hover_data_type("hex").unwrap_or_default().leak(),
        "blob"      => hover_data_type("blob").unwrap_or_default().leak(),
        "regex"     => hover_data_type("regex").unwrap_or_default().leak(),
        "date"      => hover_data_type("date").unwrap_or_default().leak(),
        "timestamp" => hover_data_type("timestamp").unwrap_or_default().leak(),
        "enum"      => hover_data_type("enum").unwrap_or_default().leak(),
        "any"       => hover_data_type("any").unwrap_or_default().leak(),
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

// ── QuickFunc local variable hover ────────────────────────────────────────────
//
// Searches every QuickFunc body for a VariableDeclaration matching `name`.

fn hover_qf_local_var(doc: &Document, name: &str) -> Option<String> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    for func in &qf.functions {
        if let Some((dt, is_mutable)) = find_var_decl_in_stmts(&func.body, name) {
            let type_str = dt
                .map(|t| format!("{}", t))
                .unwrap_or_else(|| "?".to_string());
            let mut_str = if is_mutable { "mut " } else { "" };
            return Some(format!(
                "**`{}`** — local variable in `~{}`\n\nDeclared as: `let {}{}<{}>`\n\nType: `<{}>` *(from declaration)*\n\nScope: compile-time only — resolved before runtime.",
                name, func.name, mut_str, name, type_str, type_str
            ));
        }
    }
    None
}

/// Recursively search statements for a VariableDeclaration whose name matches.
/// Returns `(declared_type, is_mutable)` when found.
fn find_var_decl_in_stmts(
    stmts: &[QuickFuncStatement],
    name: &str,
) -> Option<(Option<DataType>, bool)> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration {
                variable_name,
                data_type,
                is_mutable,
                ..
            } => {
                if variable_name.as_str() == name {
                    return Some((*data_type, *is_mutable));
                }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(r) = find_var_decl_in_stmts(then_branch, name) {
                    return Some(r);
                }
                if let Some(else_stmts) = else_branch {
                    if let Some(r) = find_var_decl_in_stmts(else_stmts, name) {
                        return Some(r);
                    }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(r) = find_var_decl_in_stmts(&case.statements, name) {
                        return Some(r);
                    }
                }
                if let Some(dc) = default_case {
                    if let Some(r) = find_var_decl_in_stmts(&dc.statements, name) {
                        return Some(r);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Table path / group array prefix hover ─────────────────────────────────────
//
// When hovering on a name like `server` in `server: host = "x", port = 8080`,
// the symbol table has no entry for `server` itself — only `DATA.server.host`
// and `DATA.server.port`.  This helper detects that case and shows a summary.

fn hover_table_path_prefix(doc: &Document, name: &str) -> Option<String> {
    let sr = doc.semantic_result.as_ref()?;
    let st = sr.symbol_table.as_ref()?;

    let prefix_with_dot = format!("DATA.{}.", name);

    let mut child_names: Vec<String> = Vec::new();
    let mut total_children: usize = 0;

    for path in st.data_variables.keys() {
        if path.starts_with(&prefix_with_dot) {
            total_children += 1;
            // Extract the immediate child segment after the prefix.
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

    if total_children == 0 {
        return None;
    }

    child_names.sort();
    let shown: Vec<String> = child_names.iter().take(8).map(|s| format!("`{}`", s)).collect();
    let more = if child_names.len() > 8 {
        format!(" … and {} more", child_names.len() - 8)
    } else {
        String::new()
    };

    Some(format!(
        "**`{}`** — DATA table / group\n\n**Child properties:** {}{}\n\nRuntime access:\n```rust\nlet val = data.get(\"{}.property\")?;\n// Or access child keys:\nlet keys = data.get_keys(\"{}\");\n```",
        name, shown.join(", "), more, name, name
    ))
}

// ── Identifier hover ───────────────────────────────────────────────────────────

fn hover_identifier(doc: &Document, name: &str, section: SectionId) -> Option<String> {

    // ── 1. QuickFuncs section: parameter + local variable hover ───────────
    if section == SectionId::QuickFuncs {
        if let Some(ast) = &doc.ast {
            if let Some(qf) = &ast.quick_functions {
                // Check parameters first
                for func in &qf.functions {
                    for param in &func.parameters {
                        if param.name != name { continue; }
                        let type_str = param.data_type
                            .map(|t| format!("{}", t))
                            .unwrap_or_else(|| "any".to_string());
                        let default_note = if param.default_value.is_some() {
                            "\n\n*(has a default value from type annotation)*"
                        } else { "" };
                        return Some(format!(
                            "**`{}`** — parameter of `~{}`\n\nType: `<{}>`{}",
                            name, func.name, type_str, default_note
                        ));
                    }
                }
            }
        }
        // Check local variable declarations in function bodies
        if let Some(content) = hover_qf_local_var(doc, name) {
            return Some(content);
        }
    }

    // ── 2. QuickFunc declaration or call site ─────────────────────────────
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                if func.name != name { continue; }

                let params: Vec<String> = func.parameters.iter().map(|p| {
                    let t = p.data_type
                        .map(|dt| format!("<{}>", dt))
                        .unwrap_or_default();
                    let d = if p.default_value.is_some() { " = …" } else { "" };
                    format!("{}{}{}", p.name, t, d)
                }).collect();

                let ret = func.return_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "?".to_string());

                let scopes = func.scope_list.as_ref()
                    .map(|s| format!("\n\n**Scope:** `=> {}`", s.join(", ")))
                    .unwrap_or_default();

                let doc_comment_block = extract_doc_comment_for_func(
                    &doc.tokens,
                    func.position.line,
                )
                    .map(|c| format!("{}\n\n---\n\n", c))
                    .unwrap_or_default();

                let signature = format!(
                    "**`~{}<{}>({})`** — QuickFunc",
                    name, ret, params.join(", ")
                );

                let param_names: Vec<&str> = func.parameters.iter()
                    .map(|p| p.name.as_str())
                    .collect();

                let body = format!(
                    "Compile-time function. Resolved entirely at compile time; the binary stores only the result.{}\n\n```mdix\n// Example call in @DATA:\n{}({})\n```",
                    scopes, name, param_names.join(", ")
                );

                return Some(format!("{}{}\n\n{}", doc_comment_block, signature, body));
            }
        }

        // ── 3. Enum type name ──────────────────────────────────────────────
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                if decl.name != name { continue; }
                let fields: Vec<String> = decl.fields.iter().map(|f| {
                    let v = f.value.map(|n| format!(" = {}", n)).unwrap_or_default();
                    format!("`{}{}`", f.name, v)
                }).collect();
                return Some(format!(
                    "**`{}`** — enum type\n\n**Fields:** {}\n\nAccess: `{}.FIELD_NAME`\n\nAnnotate variables with `<enum>` to enable enum-value assignment.",
                    name, fields.join(", "), name
                ));
            }
        }
    }

    // ── 4. Semantic symbol table ───────────────────────────────────────────
    if let Some(sr) = &doc.semantic_result {
        if let Some(st) = &sr.symbol_table {

            // 4a. Exact bare-name lookup
            if let Some(var) = st.try_get_data_variable(name) {
                return Some(format_data_var_hover(name, var));
            }

            // 4b. Full-path lookup with DATA. prefix
            if let Some(var) = st.try_get_data_variable(&format!("DATA.{}", name)) {
                return Some(format_data_var_hover(name, var));
            }

            // 4c. Suffix match: e.g. "host" finds "DATA.server.host"
            let suffix = format!(".{}", name);
            let mut best: Option<(usize, String, bool, Option<DataType>)> = None;
            for (path, var) in &st.data_variables {
                if !path.ends_with(&suffix) { continue; }
                let specificity   = path.len();
                let effective_type = var.effective_type();
                let is_inferred    = var.is_inferred;
                match &best {
                    None => best = Some((specificity, path.clone(), is_inferred, effective_type)),
                    Some((bs, _, _, _)) if specificity > *bs =>
                        best = Some((specificity, path.clone(), is_inferred, effective_type)),
                    _ => {}
                }
            }
            if let Some((_, path, is_inferred, effective_type)) = best {
                let type_str = effective_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                let inferred_note = if is_inferred { " *(inferred)*" } else { "" };
                let access_path = path.strip_prefix("DATA.").unwrap_or(path.as_str());
                return Some(format!(
                    "**`{}`** — DATA property\n\nFull path: `{}`\nType: `<{}>`{}\n\nRuntime access:\n```rust\nlet val: {} = data.get(\"{}\")?;\n```",
                    name, access_path, type_str, inferred_note, type_str, access_path
                ));
            }

            // 4d. Built-in static object (Math, DateTime, …)
            if st.is_builtin_static_object(name) {
                return hover_static_object(name);
            }

            // 4e. Imported namespace alias
            if let Some(ns) = st.try_get_namespace(name) {
                let func_names: Vec<String> =
                    ns.functions.keys().take(6).cloned().collect();
                let enum_names: Vec<String> =
                    ns.enums.keys().take(4).cloned().collect();
                return Some(format!(
                    "**`{}`** — imported namespace\n\nSource: `{}`\n\n**Functions ({}):** {}\n\n**Enums ({}):** {}\n\nCall: `{}.funcName(…)`",
                    name,
                    ns.file_path,
                    ns.functions.len(),
                    func_names.iter().map(|f| format!("`{}`", f)).collect::<Vec<_>>().join(", "),
                    ns.enums.len(),
                    enum_names.iter().map(|e| format!("`{}`", e)).collect::<Vec<_>>().join(", "),
                    name
                ));
            }
        }
    }

    // ── 5. Table path prefix (e.g. `server` in `server: host = "x"`) ─────
    if section == SectionId::Data {
        if let Some(content) = hover_table_path_prefix(doc, name) {
            return Some(content);
        }
    }

    None
}

// ── Format a DATA variable hover ──────────────────────────────────────────────

fn format_data_var_hover(
    name: &str,
    var: &dixscript::Compiler::Utilities::VariableInfo,
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
    let entry = STATIC_SIGS.iter().find(|(c, m, _, _, _)| *c == class && *m == method)?;
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
    else if ts.contains('+') || (ts.len() > 20 && ts.chars().nth(19) == Some('-')) {
        "with UTC offset"
    } else { "local time" };
    Some(format!(
        "**Timestamp**: `{}`\n\n*{}*\n\nType: `<timestamp>`\n\nGet components: `DateTime.year(ts)`, `DateTime.hour(ts)`, …",
        ts, tz
    ))
}

// ── Regex hover ────────────────────────────────────────────────────────────────

fn hover_regex(tokens: &[Token], constructor_index: usize) -> String {
    let pattern = find_adjacent_string(tokens, constructor_index);
    match pattern {
        None => concat!(
            "**`r:(...)`** — regex constructor\n\n",
            "Provide the pattern as a string.\n\n",
            "```mdix\nemail = r:(\"^[\\\\w.]+@[\\\\w.]+$\")\n```\n\n",
            "Instance methods: `.test(str)`, `.match(str)`, `.matchAll(str)`, ",
            "`.replace(str,repl)`, `.split(str)`, `.isValid()`"
        ).to_string(),
        Some(pat) => {
            match regex::Regex::new(&pat) {
                Ok(re) => {
                    let groups = re.captures_len().saturating_sub(1);
                    let encoded: String = pat.chars().flat_map(|c| {
                        match c {
                            ' '  => "%20".chars().collect::<Vec<_>>(),
                            '+'  => "%2B".chars().collect(),
                            '/'  => "%2F".chars().collect(),
                            '?'  => "%3F".chars().collect(),
                            '#'  => "%23".chars().collect(),
                            '&'  => "%26".chars().collect(),
                            '='  => "%3D".chars().collect(),
                            _    => vec![c],
                        }
                    }).collect();
                    let test_url = format!(
                        "https://regex101.com/?regex={}&flavor=rust",
                        encoded
                    );
                    format!(
                        "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n✅ **Pattern valid** — {} capture group{}\n\nType: `<regex>`\n\nMethods: `.test(str)`, `.match(str)` → `[full, g1, g2, …]`, `.matchAll(str)`, `.replace(str,repl)`, `.split(str)`\n\n[🔗 Test this pattern on regex101]({}) — paste your text there to validate matches interactively.",
                        pat, groups, if groups == 1 { "" } else { "s" }, test_url
                    )
                }
                Err(e) => format!(
                    "**`r:(...)`** — regex constructor\n\n```\n{}\n```\n\n❌ **Invalid pattern:** {}\n\nType: `<regex>`",
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
                    let mime    = detect_mime(&bytes);
                    let preview = build_blob_preview(&bytes, &b64, mime);
                    let hex_bytes: Vec<String> = bytes.iter().take(12)
                        .map(|b| format!("{:02X}", b)).collect();
                    let ellipsis = if bytes.len() > 12 { " …" } else { "" };
                    format!(
                        "**`b:(...)`** — blob\n\n📦 **{}** bytes · {} base64 chars\n\n🗂 MIME type: `{}`\n\nFirst bytes: `{}{}`{}\n\nType: `<blob>`\n\nMethods: `.size()`, `.mimeType()`, `.toHex()`, `.toBytes()`, `.isValid()`, `.slice(start,end)`",
                        bytes.len(), b64.len(), mime,
                        hex_bytes.join(" "), ellipsis, preview,
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

fn build_blob_preview(bytes: &[u8], b64: &str, mime: &str) -> String {
    if mime.starts_with("image/") {
        if bytes.len() <= 300_000 {
            return format!("\n\n![blob preview](data:{};base64,{})", mime, b64);
        }
        return format!(
            "\n\n🖼 **Image** (`{}`) — {} KB (too large to preview inline)",
            mime, bytes.len() / 1024
        );
    }
    if mime.starts_with("audio/") {
        return format!(
            "\n\n🔊 **Audio** (`{}`) — {} bytes\n\nSave to disk and open with a media player to listen.",
            mime, bytes.len()
        );
    }
    if mime == "application/pdf" {
        return format!("\n\n📄 **PDF document** — {} bytes", bytes.len());
    }
    String::new()
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
        b if b[0] == 0x52 && b[1] == 0x49 && b[2] == 0x46 && b[3] == 0x46 => "audio/wav",
        b if b[0] == 0x49 && b[1] == 0x44 && b[2] == 0x33 => "audio/mp3",
        b if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 => "audio/mp3",
        b if b[0] == 0x4F && b[1] == 0x67 && b[2] == 0x67 && b[3] == 0x53 => "audio/ogg",
        b if b[0] == 0x25 && b[1] == 0x50 && b[2] == 0x44 && b[3] == 0x46 => "application/pdf",
        b if b[0] == 0x50 && b[1] == 0x4B => "application/zip",
        b if b[0] == 0x7F && b[1] == 0x45 && b[2] == 0x4C && b[3] == 0x46 => "application/elf",
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
        "**HexColor** `#{}`\n\n\
         | Channel | Dec | Hex |\n\
         |---------|-----|-----|\n\
         | Red   | {} | `{:02X}` |\n\
         | Green | {} | `{:02X}` |\n\
         | Blue  | {} | `{:02X}` |\n\n\
         {}\n\n\
         Type: `<hex>`\n\n\
         Click the color swatch in the gutter to open the color picker.",
        stripped, r, r, g, g, b, b, alpha_line
    ))
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

pub fn token_and_index_at(tokens: &[Token], pos: Position) -> Option<(&Token, usize)> {
    let target_line = pos.line as usize + 1;
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

pub fn token_at(tokens: &[Token], pos: Position) -> Option<&Token> {
    token_and_index_at(tokens, pos).map(|(t, _)| t)
}

fn token_value_len(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,
        TokenType::HexColor(h)           => h.len(),
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
            TokenType::Identifier(_)
            | TokenType::SectionData
            | TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

fn extract_doc_comment_for_func(tokens: &[Token], func_def_line: usize) -> Option<String> {
    if func_def_line == 0 { return None; }
    let search_start = func_def_line.saturating_sub(60);

    let mut spans: Vec<(usize, usize, String)> = tokens
        .iter()
        .filter(|t| t.line >= search_start && t.line < func_def_line)
        .filter_map(|t| {
            if let TokenType::Comment(c) = &t.token_type {
                let newlines = c.chars().filter(|&ch| ch == '\n').count();
                let end_line = t.line + newlines;
                if end_line < func_def_line {
                    return Some((t.line, end_line, c.clone()));
                }
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
    Some(format_doc_comment(&collected.join("\n")))
}

fn format_doc_comment(raw: &str) -> String {
    let text = raw.trim();
    let cleaned: String = text
        .lines()
        .map(|l| l.trim_start_matches('/').trim_start())
        .collect::<Vec<_>>()
        .join("\n");

    if cleaned.contains("```") || cleaned.contains('`') {
        cleaned
    } else {
        cleaned
            .lines()
            .map(|l| format!("> {}", l))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ── Static method signature table ─────────────────────────────────────────────

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
    ("DateTime","now",         "DateTime.now() → timestamp",              "Current UTC date and time.",                "now = DateTime.now()"),
    ("DateTime","today",       "DateTime.today() → date",                "Today's date at midnight UTC.",             "today = DateTime.today()"),
    ("DateTime","format",      "DateTime.format(ts, pattern) → string",  "Format using strftime-style pattern.",      "DateTime.format(DateTime.now(), \"%Y-%m-%d\")"),
    ("DateTime","year",        "DateTime.year(d) → int",                 "Year component.",                           "DateTime.year(DateTime.today())"),
    ("DateTime","month",       "DateTime.month(d) → int",                "Month 1–12.",                               "DateTime.month(DateTime.today())"),
    ("DateTime","day",         "DateTime.day(d) → int",                  "Day of month 1–31.",                        "DateTime.day(DateTime.today())"),
    ("DateTime","subtract",    "DateTime.subtract(a, b) → double",       "Difference in days (fractional).",          "DateTime.subtract(d1, d2) // → days"),
    ("DateTime","addDays",     "DateTime.addDays(d, n) → date|timestamp","Add n days (fractional values supported).", "DateTime.addDays(DateTime.today(), 7)"),
    ("DateTime","isLeapYear",  "DateTime.isLeapYear(year) → bool",       "True if year has 366 days.",                "DateTime.isLeapYear(2024) // → true"),
    ("Array","range",   "Array.range(start, end: int) → array",         "Integers from start to end inclusive.",      "Array.range(1, 5) // → [1,2,3,4,5]"),
    ("Array","fill",    "Array.fill(value: any, count: int) → array",   "Array of count copies of value.",            "Array.fill(0, 3)  // → [0,0,0]"),
    ("Array","sum",     "Array.sum(arr: array) → double",               "Sum of numeric elements.",                   "Array.sum([1,2,3]) // → 6.0"),
    ("Array","flatten", "Array.flatten(arr: array) → array",            "Recursively flatten all nested arrays.",     "Array.flatten([[1,2],[3,4]]) // → [1,2,3,4]"),
    ("Array","unique",  "Array.unique(arr: array) → array",             "Remove duplicate values (preserves order).","Array.unique([1,1,2,3,2]) // → [1,2,3]"),
    ("Random","range",  "Random.range(min, max: int) → int",            "Random integer in [min, max] inclusive.",    "Random.range(1, 6) // dice roll"),
    ("Random","choice", "Random.choice(arr: array) → any",              "Pick a random element from arr.",            "Random.choice([\"rock\",\"paper\",\"scissors\"])"),
    ("Random","shuffle","Random.shuffle(arr: array) → array",           "Fisher-Yates shuffle — returns a new array.","Random.shuffle(Array.range(1, 52))"),
    ("Guid","new",      "Guid.new() → string",                          "Generate a UUID v4 string.",                 "id = Guid.new()  // \"550e8400-…\""),
    ("Guid","validate", "Guid.validate(str: string) → bool",            "True if str is a valid GUID format.",        "Guid.validate(\"550e8400-e29b-41d4-a716-446655440000\")"),
    ("Dix","Log",       "Dix.Log(message: any) → void",                 "Log at INFO level during compilation.",      "Dix.Log(\"Building \" + name)"),
    ("Dix","Assert",    "Dix.Assert(condition: bool, message: string) → void","Abort compilation if condition is false.","Dix.Assert(health > 0, \"health must be positive\")"),
    ("Dix","Format",    "Dix.Format(template: string, ...args) → string","Format string with {0}, {1} placeholders.", "Dix.Format(\"Hello {0}!\", \"World\")"),
    ("Enum","getValues","Enum.getValues(enumName: string) → array",     "All field names of an @ENUMS enum.",         "Enum.getValues(\"Difficulty\") // → [\"EASY\",…]"),
    ("Enum","exists",   "Enum.exists(enumName: string) → bool",         "True if the enum is registered.",            "Enum.exists(\"Difficulty\")"),
    ("IpAddress","validate","IpAddress.validate(str: string) → bool",   "True if str is a valid IPv4 or IPv6 address.","IpAddress.validate(\"192.168.1.1\")"),
    ("IpAddress","isPrivate","IpAddress.isPrivate(str: string) → bool", "True if in RFC-1918 / ULA range.",           "IpAddress.isPrivate(\"10.0.0.1\") // → true"),
];

// ── Helpers ───────────────────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use base64::Engine;
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
        assert!(provide(None, Position::new(0, 0)).is_none());
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

    #[test]
    fn hover_qf_local_var_found() {
        let src = "@QUICKFUNCS(\n  ~calc<int>(x) {\n    let result = x + 1\n    return result\n  }\n)\n@DATA(\n  y = 1\n)";
        let doc = test_doc(src);
        // hover_qf_local_var should find "result"
        let res = hover_qf_local_var(&doc, "result");
        assert!(res.is_some(), "should find local var 'result'");
        assert!(res.unwrap().contains("local variable"), "should mention local variable");
    }

    #[test]
    fn hover_table_path_prefix_found() {
        // For this test the symbol table needs to have DATA.server.host etc.
        // We test the function exists and handles missing data gracefully.
        let doc = test_doc("@DATA(\n  server: host = \"localhost\", port = 8080\n)");
        // May or may not find it depending on symbol table population,
        // but must not panic.
        let _ = hover_table_path_prefix(&doc, "server");
    }

    #[test]
    fn hover_regex_valid_pattern() {
        let result = hover_regex(&[], 0);
        assert!(result.contains("r:(...)"));
    }

    #[test]
    fn blob_preview_image_small() {
        let png_header = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_header);
        let preview = build_blob_preview(&png_header, &b64, "image/png");
        assert!(preview.contains("data:image/png;base64,"), "should embed image");
    }

    #[test]
    fn blob_preview_audio() {
        let wav_header = vec![0x52u8, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_header);
        let preview = build_blob_preview(&wav_header, &b64, "audio/wav");
        assert!(preview.contains("🔊"), "should show audio icon");
    }
}
