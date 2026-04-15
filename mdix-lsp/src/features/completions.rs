//! Completion provider.
//!
//! Triggered by: '@', '.', '<', '~'
//! Covers: section snippets, ALL keywords, ALL built-in static objects and their
//! methods, ALL instance methods per type, enum values, QuickFunc names,
//! type annotations, DLM modules, CONFIG keys, SECURITY keys.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Position,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::AST::DixScript;
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc: Option<&Document>,
    pos: Position,
    trigger: Option<&str>,
) -> Option<CompletionResponse> {
    let items = match doc {
        None => section_snippet_completions(),
        Some(d) => {
            let trigger_ch: char = trigger
                .and_then(|t| t.chars().next())
                .unwrap_or_else(|| trigger_char(&d.source, pos));

            match trigger_ch {
                '@' => section_snippet_completions(),
                '<' => type_annotation_completions(),
                '~' => quickfunc_declaration_snippets(),
                '.' => dot_completions(d, pos),
                _   => {
                    // Check if we're inside an @ word (e.g. user typed "@conf")
                    let word = word_before_cursor(&d.source, pos);
                    if word.starts_with('@') {
                        section_snippet_completions()
                    } else {
                        general_completions(d, pos)
                    }
                }
            }
        }
    };

    if items.is_empty() { None } else { Some(CompletionResponse::Array(items)) }
}

/// Extract the word (including leading @) that the cursor is in the middle of.
fn word_before_cursor(source: &str, pos: Position) -> String {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to: String = line.chars().take(pos.character as usize).collect();
    // Walk backward until we hit a non-word, non-@ char
    let start = up_to
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '@')
        .map(|i| i + 1)
        .unwrap_or(0);
    up_to[start..].to_string()
}

// ── Section snippets ──────────────────────────────────────────────────────────

fn section_snippet_completions() -> Vec<CompletionItem> {
    // (label, insertText WITHOUT leading @, documentation)
    let sections: &[(&str, &str, &str)] = &[
        (
            "@CONFIG",
            "CONFIG(\n  version -> \"1.0.0\"\n  author -> \"${1:name}\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",
            "Compiler settings and metadata.\n\n**Keys:** `version`, `author`, `created`, `encoding`, `debug_mode` (`off`/`regular`/`verbose`), `error_handling` (`halt`/`continue`/`recover`), `compatibility_mode` (`strict`/`best_effort`/`permissive`), `features`.\n\n```mdix\n@CONFIG(\n  version -> \"1.0.0\"\n  debug_mode -> \"regular\"\n)\n```",
        ),
        (
            "@IMPORTS",
            "IMPORTS(\n  ${1:Alias} from \"${2:path/to/file.mdix}\"\n)",
            "Import other `.mdix` files. The alias becomes a namespace.\n\n```mdix\n@IMPORTS(\n  Utils from \"common/utils.mdix\"\n  Base  from \"../base.mdix\"\n)\n```\n\nCall imported functions: `Utils.myFunc(x)`. Access imported enums: `Utils.Status.ACTIVE`.",
        ),
        (
            "@DLM",
            "DLM(\n  DCompressor.${1|gzip,bzip2,lzma|}\n  DEncryptor.${2|aes256,aes128,chacha20|}\n)",
            "Data Lifecycle Modules — compression and encryption applied at compile time.\n\n**Compressors:** `DCompressor.gzip`, `DCompressor.bzip2`, `DCompressor.lzma`\n**Encryptors:** `DEncryptor.aes256`, `DEncryptor.aes128`, `DEncryptor.chacha20`, `DEncryptor.xor`\n**Auditor:** `DAuditor.diy`, `DAuditor.enhanced`\n\n```mdix\n@DLM(DCompressor.gzip, DEncryptor.aes256)\n```",
        ),
        (
            "@ENUMS",
            "ENUMS(\n  ${1:EnumName} { ${2:VALUE_A} = 0, ${3:VALUE_B} = 1 }\n)",
            "Named integer constants. Auto-increments from 0 if values omitted.\n\n```mdix\n@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n  AIType     { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }\n)\n```\n\nAccess: `Difficulty.HARD`, use type annotation `<enum>`.",
        ),
        (
            "@QUICKFUNCS",
            "QUICKFUNCS(\n  ~${1:funcName}<${2:object}>(${3:param1}, ${4:param2}) {\n    return {\n      ${5:key} = ${6:param1}\n    }\n  }\n)",
            "Compile-time functions. All computation happens at compile time — zero runtime overhead.\n\n```mdix\n@QUICKFUNCS(\n  ~weapon<object>(id, damage<int>) {\n    return { id = id, damage = damage, range = damage * 2 }\n  }\n)\n```\n\nCall in `@DATA`: `weapon(\"AK47\", 35)`",
        ),
        (
            "@DATA",
            "DATA(\n  ${1:key} = ${2:value}\n\n  ${3:table}: ${4:field} = ${5:value}\n\n  ${6:array}::\n    ${7:item1},\n    ${8:item2}\n)",
            "Data payload. Three entry types:\n\n- **Flat:** `name = value` (primitives)\n- **Table:** `section: field = value, field2 = value2`\n- **Group array:** `items:: value1, value2`\n\nCommas between entries are optional.\n\n```mdix\n@DATA(\n  app_name = \"MyApp\"\n  server: host = \"localhost\", port = 8080\n  tags:: \"alpha\", \"beta\"\n)\n```",
        ),
        (
            "@SECURITY",
            "SECURITY(\n  encryption -> {\n    mode = \"${1|password,keyfile|}\",\n    algorithm = \"${2|aes256-gcm,aes128-gcm,chacha20-poly1305|}\"\n  }\n)",
            "Encryption configuration. Required when `@DLM` includes a `DEncryptor`.\n\n**Modes:** `\"password\"` (user-supplied), `\"keyfile\"` (generates `.key` file)\n\n```mdix\n@SECURITY(\n  encryption -> { mode = \"keyfile\", algorithm = \"aes256-gcm\" }\n)\n```\n\nCompile: `mdix compile secrets.mdix --password`",
        ),
    ];

    sections.iter().map(|(label, snippet, doc)| {
        // label is "@CONFIG", strip @ for filter so matching works regardless of
        // whether the user typed the @ already.  The insert text never includes @
        // because @ is already the trigger character in the document.
        let filter = label.trim_start_matches('@').to_lowercase();
        CompletionItem {
            label:              label.to_string(),
            kind:               Some(CompletionItemKind::MODULE),
            detail:             Some("DixScript section".to_string()),
            filter_text:        Some(filter),
            documentation:      Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc.to_string(),
            })),
            insert_text:        Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text:          Some(format!("0_{}", label)), // sections sort first
            ..Default::default()
        }
    }).collect()
}

// ── Type annotation completions (<int>, <string>, …) ─────────────────────────

fn type_annotation_completions() -> Vec<CompletionItem> {
    let types: &[(&str, &str, &str)] = &[
        ("int",       "32-bit signed integer",             "42, -7, 0"),
        ("float",     "32-bit float (requires f suffix)",  "3.14f, -0.5f"),
        ("double",    "64-bit float",                      "3.14159, -2.718"),
        ("string",    "UTF-8 string",                      "\"hello\""),
        ("bool",      "Boolean",                           "true, false"),
        ("array",     "Ordered collection (::)",            "\"a\", \"b\", \"c\""),
        ("tuple",     "Mixed-type, max 6 elements",         "t:(1, \"a\", true)"),
        ("object",    "Key-value map  { }",                "{ x = 1, y = 2 }"),
        ("hex",       "Hex colour or integer",             "#FF5733, 0xFF"),
        ("blob",      "Base64-encoded binary",             "b:(\"SGVsbG8=\")"),
        ("regex",     "Regular expression pattern",        "r:(\"^[a-z]+$\")"),
        ("date",      "ISO 8601 date",                     "2025-12-31"),
        ("timestamp", "ISO 8601 timestamp",                "2025-12-31T10:30:00Z"),
        ("enum",      "Enum value from @ENUMS",            "MyEnum.VALUE"),
        ("any",       "Any type (no restriction)",         "anything"),
    ];

    types.iter().map(|(name, detail, example)| CompletionItem {
        label:              format!("<{}>", name),
        kind:               Some(CompletionItemKind::TYPE_PARAMETER),
        detail:             Some(detail.to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`<{}>`** — {}\n\nExample: `{}`", name, detail, example),
        })),
        insert_text:        Some(name.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }).collect()
}

// ── QuickFunc declaration snippets (triggered by ~) ───────────────────────────

fn quickfunc_declaration_snippets() -> Vec<CompletionItem> {
    let templates: &[(&str, &str, &str)] = &[
        (
            "~funcName<object>",
            "~${1:funcName}<object>(${2:param1}, ${3:param2}) {\n    return {\n      ${4:key} = ${5:param1}\n    }\n  }",
            "Object-returning QuickFunc (most common — for createEnemy, weapon, etc.)",
        ),
        (
            "~funcName<int>",
            "~${1:funcName}<int>(${2:x}<int>, ${3:y}<int>) {\n    return ${4:x + y}\n  }",
            "Integer-returning QuickFunc",
        ),
        (
            "~funcName<string>",
            "~${1:funcName}<string>(${2:value}) {\n    return ${3:value}\n  }",
            "String-returning QuickFunc",
        ),
        (
            "~funcName<bool>",
            "~${1:funcName}<bool>(${2:condition}) {\n    return ${3:condition}\n  }",
            "Boolean-returning QuickFunc",
        ),
        (
            "~funcName<array>",
            "~${1:funcName}<array>(${2:items}) {\n    return ${3:items}\n  }",
            "Array-returning QuickFunc",
        ),
    ];

    templates.iter().map(|(label, snippet, doc)| CompletionItem {
        label:              label.to_string(),
        kind:               Some(CompletionItemKind::FUNCTION),
        detail:             Some("QuickFunc declaration".to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: doc.to_string(),
        })),
        insert_text:        Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }).collect()
}

// ── Dot completions (EnumName. → values, StaticObj. → methods) ───────────────

fn dot_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let word_before = word_before_dot(&doc.source, pos);
    if word_before.is_empty() {
        return items;
    }

    // Check enum names from current document
    if let Some(ast) = &doc.ast {
        items.extend(enum_value_completions(ast, &word_before));
    }

    // Check built-in static objects
    items.extend(static_method_completions(&word_before));

    // Check imported namespaces
    if let Some(sr) = doc.semantic_result.as_ref() {
        if let Some(st) = &sr.symbol_table {
            if let Some(ns) = st.try_get_namespace(&word_before) {
                for func_name in ns.functions.keys() {
                    items.push(CompletionItem {
                        label:  func_name.clone(),
                        kind:   Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("imported from {}", ns.alias)),
                        ..Default::default()
                    });
                }
                for enum_name in ns.enums.keys() {
                    items.push(CompletionItem {
                        label:  enum_name.clone(),
                        kind:   Some(CompletionItemKind::ENUM),
                        detail: Some(format!("enum from {}", ns.alias)),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Instance method completions: if the word looks like a type keyword,
    // offer that type's instance methods
    items.extend(instance_method_completions(&word_before));

    items
}

fn enum_value_completions(ast: &DixScript, enum_name: &str) -> Vec<CompletionItem> {
    let enums = match &ast.enums { Some(e) => e, None => return vec![] };
    for decl in &enums.enums {
        if decl.name == enum_name {
            return decl.fields.iter().map(|f| {
                let detail = f.value.map(|v| format!("= {}", v)).unwrap_or_default();
                CompletionItem {
                    label:    f.name.clone(),
                    kind:     Some(CompletionItemKind::ENUM_MEMBER),
                    detail:   Some(detail),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!("**{}.{}** — enum member\n\nUsage: `{}.{}`",
                                       enum_name, f.name, enum_name, f.name),
                    })),
                    ..Default::default()
                }
            }).collect();
        }
    }
    vec![]
}

// ── ALL built-in static object methods ────────────────────────────────────────

fn static_method_completions(object_name: &str) -> Vec<CompletionItem> {
    // (method_name, signature, description)
    let catalogue: &[(&str, &[(&str, &str, &str)])] = &[
        ("Math", &[
            ("abs",       "Math.abs(x) → double",                      "Absolute value of x"),
            ("sqrt",      "Math.sqrt(x) → double",                     "Square root (x must be ≥ 0)"),
            ("pow",       "Math.pow(base, exp) → double",               "base raised to the power of exp"),
            ("floor",     "Math.floor(x) → int",                       "Largest integer ≤ x"),
            ("ceil",      "Math.ceil(x) → int",                        "Smallest integer ≥ x"),
            ("round",     "Math.round(x) → int",                       "Round to nearest integer"),
            ("min",       "Math.min(a, b) → double",                   "Minimum of two numbers"),
            ("max",       "Math.max(a, b) → double",                   "Maximum of two numbers"),
            ("clamp",     "Math.clamp(v, min, max) → double",          "Clamp v between min and max"),
            ("sign",      "Math.sign(x) → int",                        "Sign of x: -1, 0, or 1"),
            ("truncate",  "Math.truncate(x) → int",                    "Integer part (truncate toward zero)"),
            ("remainder", "Math.remainder(dividend, divisor) → double", "Remainder after division"),
            ("sin",       "Math.sin(x) → double",                      "Sine of x (radians)"),
            ("cos",       "Math.cos(x) → double",                      "Cosine of x (radians)"),
            ("tan",       "Math.tan(x) → double",                      "Tangent of x (radians)"),
            ("log",       "Math.log(x) → double",                      "Natural logarithm (x > 0)"),
            ("log10",     "Math.log10(x) → double",                    "Base-10 logarithm (x > 0)"),
            ("exp",       "Math.exp(x) → double",                      "e raised to the power x"),
            ("radians",   "Math.radians(degrees) → double",            "Convert degrees to radians"),
            ("degrees",   "Math.degrees(radians) → double",            "Convert radians to degrees"),
            ("pi",        "Math.pi() → double",                        "π ≈ 3.14159265358979"),
            ("e",         "Math.e() → double",                         "e ≈ 2.71828182845905"),
        ]),
        ("DateTime", &[
            ("now",         "DateTime.now() → timestamp",                      "Current UTC date and time"),
            ("today",       "DateTime.today() → date",                         "Today's date at midnight UTC"),
            ("utcNow",      "DateTime.utcNow() → timestamp",                   "Alias for now()"),
            ("parse",       "DateTime.parse(str) → timestamp",                 "Parse an ISO date/time string"),
            ("parseExact",  "DateTime.parseExact(str, format) → timestamp",    "Parse with explicit format string"),
            ("create",      "DateTime.create(year, month, day) → date",        "Construct a date from components"),
            ("createTime",  "DateTime.createTime(y,m,d,h,min,s) → timestamp",  "Construct a full timestamp"),
            ("fromUnixTime","DateTime.fromUnixTime(secs) → timestamp",         "From Unix epoch seconds"),
            ("toUnixTime",  "DateTime.toUnixTime(ts) → double",                "To Unix epoch seconds"),
            ("format",      "DateTime.format(ts, pattern) → string",           "Format using strftime-style pattern"),
            ("year",        "DateTime.year(d) → int",                          "Year component (e.g. 2025)"),
            ("month",       "DateTime.month(d) → int",                         "Month component 1–12"),
            ("day",         "DateTime.day(d) → int",                           "Day component 1–31"),
            ("hour",        "DateTime.hour(ts) → int",                         "Hour component 0–23"),
            ("minute",      "DateTime.minute(ts) → int",                       "Minute component 0–59"),
            ("second",      "DateTime.second(ts) → int",                       "Second component 0–59"),
            ("millisecond", "DateTime.millisecond(ts) → int",                  "Millisecond component 0–999"),
            ("dayOfWeek",   "DateTime.dayOfWeek(d) → int",                     "0=Sunday … 6=Saturday"),
            ("dayOfYear",   "DateTime.dayOfYear(d) → int",                     "Day of year 1–366"),
            ("isLeapYear",  "DateTime.isLeapYear(year) → bool",                "True if year is a leap year"),
            ("daysInMonth", "DateTime.daysInMonth(year, month) → int",         "Number of days in the given month"),
            ("compare",     "DateTime.compare(a, b) → int",                    "-1 / 0 / 1 (a < b / equal / a > b)"),
            ("addDays",     "DateTime.addDays(d, n) → date|timestamp",         "Add n days (fractional ok)"),
            ("addMonths",   "DateTime.addMonths(d, n) → date|timestamp",       "Add n months"),
            ("addYears",    "DateTime.addYears(d, n) → date|timestamp",        "Add n years"),
            ("addHours",    "DateTime.addHours(ts, n) → timestamp",            "Add n hours"),
            ("addMinutes",  "DateTime.addMinutes(ts, n) → timestamp",          "Add n minutes"),
            ("addSeconds",  "DateTime.addSeconds(ts, n) → timestamp",          "Add n seconds"),
            ("subtract",    "DateTime.subtract(a, b) → double",                "Difference in days"),
        ]),
        ("Array", &[
            ("empty",       "Array.empty() → array",                         "Create an empty array"),
            ("range",       "Array.range(start, end) → array",               "Array of ints from start to end inclusive"),
            ("fill",        "Array.fill(value, count) → array",              "Array filled with value repeated count times"),
            ("of",          "Array.of(v1, v2, ...) → array",                 "Create array from arguments (variadic)"),
            ("concat",      "Array.concat(arr1, arr2, ...) → array",         "Concatenate multiple arrays"),
            ("repeat",      "Array.repeat(arr, times) → array",              "Repeat array content n times"),
            ("fromString",  "Array.fromString(str, sep) → array",            "Split string into array by separator"),
            ("reverse",     "Array.reverse(arr) → array",                    "Create reversed copy"),
            ("sort",        "Array.sort(arr) → array",                       "Create sorted copy (lexicographic)"),
            ("unique",      "Array.unique(arr) → array",                     "Remove duplicate values"),
            ("slice",       "Array.slice(arr, start, end) → array",          "Extract sub-array (supports negatives)"),
            ("filter",      "Array.filter(arr, value) → array",              "Keep only elements equal to value"),
            ("contains",    "Array.contains(arr, value) → bool",             "True if value is in array"),
            ("indexOf",     "Array.indexOf(arr, value) → int",               "First index of value, -1 if absent"),
            ("lastIndexOf", "Array.lastIndexOf(arr, value) → int",           "Last index of value, -1 if absent"),
            ("flatten",     "Array.flatten(arr) → array",                    "Recursively flatten nested arrays"),
            ("sum",         "Array.sum(arr) → double",                       "Sum of numeric elements"),
            ("average",     "Array.average(arr) → double",                   "Average of numeric elements"),
            ("min",         "Array.min(arr) → double",                       "Minimum numeric value"),
            ("max",         "Array.max(arr) → double",                       "Maximum numeric value"),
        ]),
        ("Random", &[
            ("range",       "Random.range(min, max) → int",                  "Random int in [min, max] inclusive"),
            ("float",       "Random.float() → float",                        "Random float in [0.0, 1.0)"),
            ("double",      "Random.double() → double",                      "Random double in [0.0, 1.0)"),
            ("boolean",     "Random.boolean() → bool",                       "Random true or false"),
            ("floatRange",  "Random.floatRange(min, max) → float",           "Random float in [min, max]"),
            ("doubleRange", "Random.doubleRange(min, max) → double",         "Random double in [min, max]"),
            ("choice",      "Random.choice(arr) → any",                      "Pick a random element from an array"),
            ("choices",     "Random.choices(arr, count) → array",            "Pick count elements with replacement"),
            ("sample",      "Random.sample(arr, count) → array",             "Pick count elements without replacement"),
            ("shuffle",     "Random.shuffle(arr) → array",                   "Fisher-Yates shuffle (returns copy)"),
            ("bytes",       "Random.bytes(count) → array",                   "Array of count random byte values 0–255"),
            ("string",      "Random.string(len, charset) → string",          "Random string from given character set"),
            ("alphanumeric","Random.alphanumeric(len) → string",             "Random A-Za-z0-9 string of given length"),
            ("weighted",    "Random.weighted(values, weights) → any",        "Weighted random selection"),
        ]),
        ("Guid", &[
            ("new",       "Guid.new() → string",                             "Generate a new UUID v4"),
            ("parse",     "Guid.parse(str) → string",                        "Parse and validate (throws on invalid)"),
            ("tryParse",  "Guid.tryParse(str) → string|null",               "Parse, returns null if invalid"),
            ("validate",  "Guid.validate(str) → bool",                      "True if str is a valid GUID format"),
            ("empty",     "Guid.empty() → string",                          "00000000-0000-0000-0000-000000000000"),
            ("format",    "Guid.format(guid, fmt) → string",                "Format: N (no dashes), D (dashes), B (braces), P (parens), X (hex)"),
            ("toBytes",   "Guid.toBytes(guid) → array",                     "16-byte array from GUID"),
            ("fromBytes", "Guid.fromBytes(arr) → string",                   "GUID from 16-byte array"),
        ]),
        ("IpAddress", &[
            ("parse",     "IpAddress.parse(str) → string",                  "Parse IP address (throws on invalid)"),
            ("tryParse",  "IpAddress.tryParse(str) → string|null",         "Parse, returns null if invalid"),
            ("validate",  "IpAddress.validate(str) → bool",                 "True if valid IPv4 or IPv6"),
            ("isV4",      "IpAddress.isV4(str) → bool",                     "True if IPv4 address"),
            ("isV6",      "IpAddress.isV6(str) → bool",                     "True if IPv6 address"),
            ("isPrivate", "IpAddress.isPrivate(str) → bool",                "True if in private range (10.x, 172.16-31.x, 192.168.x, fc00::/7)"),
            ("isLoopback","IpAddress.isLoopback(str) → bool",               "True if 127.0.0.1 or ::1"),
            ("isPublic",  "IpAddress.isPublic(str) → bool",                 "True if publicly routable"),
            ("toBytes",   "IpAddress.toBytes(str) → array",                 "4 bytes (IPv4) or 16 bytes (IPv6)"),
            ("fromBytes", "IpAddress.fromBytes(arr) → string",              "IP from 4 or 16-byte array"),
            ("inRange",   "IpAddress.inRange(ip, start, end) → bool",       "True if ip is within [start, end]"),
            ("localhost", "IpAddress.localhost() → string",                  "Returns \"127.0.0.1\""),
            ("any",       "IpAddress.any() → string",                       "Returns \"0.0.0.0\""),
            ("broadcast", "IpAddress.broadcast() → string",                 "Returns \"255.255.255.255\""),
        ]),
        ("Enum", &[
            ("getValues", "Enum.getValues(enumName) → array",               "All value names of an @ENUMS enum"),
            ("getName",   "Enum.getName(enumName, value) → string",         "Name for a numeric enum value"),
            ("getValue",  "Enum.getValue(enumName, name) → int",            "Numeric value for an enum name"),
            ("hasValue",  "Enum.hasValue(enumName, name) → bool",           "True if enum has that name"),
            ("contains",  "Enum.contains(enumName, value) → bool",          "True if enum has that numeric value"),
            ("count",     "Enum.count(enumName) → int",                     "Number of fields in the enum"),
            ("exists",    "Enum.exists(enumName) → bool",                   "True if enum is registered"),
            ("list",      "Enum.list() → array",                            "All registered enum names"),
            ("min",       "Enum.min(enumName) → int",                       "Minimum numeric value in enum"),
            ("max",       "Enum.max(enumName) → int",                       "Maximum numeric value in enum"),
            ("random",    "Enum.random(enumName) → string",                 "Random enum value name"),
            ("toArray",   "Enum.toArray(enumName) → array",                 "Array of { name, value } objects"),
        ]),
        ("Dix", &[
            ("Log",        "Dix.Log(message) → void",                       "Log at INFO level"),
            ("LogInfo",    "Dix.LogInfo(message) → void",                   "Log at INFO level (explicit)"),
            ("LogWarning", "Dix.LogWarning(message) → void",               "Log at WARNING level"),
            ("LogError",   "Dix.LogError(message) → void",                  "Log at ERROR level"),
            ("LogDebug",   "Dix.LogDebug(message) → void",                  "Log at DEBUG level (requires debug_mode)"),
            ("LogVerbose", "Dix.LogVerbose(message) → void",               "Log at VERBOSE level"),
            ("Assert",     "Dix.Assert(condition, message) → void",         "Throw if condition is false"),
            ("Trace",      "Dix.Trace(message, context) → void",            "Trace log with optional context tag"),
            ("Print",      "Dix.Print(message) → void",                     "Print directly to stdout"),
            ("PrintLine",  "Dix.PrintLine(message) → void",                 "Print with newline to stdout"),
            ("Format",     "Dix.Format(template, ...args) → string",        "Format string with {0}, {1} placeholders"),
            ("Join",       "Dix.Join(sep, ...values) → string",             "Join values with separator"),
        ]),
    ];

    for (obj, methods) in catalogue {
        if *obj == object_name {
            return methods.iter().map(|(method, sig, desc)| CompletionItem {
                label:       method.to_string(),
                kind:        Some(CompletionItemKind::METHOD),
                detail:      Some(sig.to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind:  MarkupKind::Markdown,
                    value: format!("**`{}`**\n\n{}\n\n```mdix\n{} = {}\n```",
                                   sig, desc, method, example_call(obj, method)),
                })),
                insert_text:        Some(format!("{}(", method)),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            }).collect();
        }
    }
    vec![]
}

// ── Instance method completions (myArray., myString., etc.) ───────────────────

fn instance_method_completions(word: &str) -> Vec<CompletionItem> {
    // This fires when the word before '.' is a known type keyword or common name
    // suggesting an instance of that type.
    let lower = word.to_lowercase();
    match lower.as_str() {
        "string" | "str" | "text" | "name" | "label" | "message" => string_instance_methods(),
        "array"  | "arr" | "list" | "items" | "values" | "elements" => array_instance_methods(),
        "int"    | "integer" | "count" | "index" | "num" => int_instance_methods(),
        "float"  | "ratio" | "rate" => float_instance_methods(),
        "double" | "value" | "amount" | "price" => double_instance_methods(),
        "blob"   | "data" | "bytes" | "binary" => blob_instance_methods(),
        "regex"  | "pattern" => regex_instance_methods(),
        "tuple"  => tuple_instance_methods(),
        _ => vec![], // No hints for arbitrary words
    }
}

fn make_instance_item(method: &str, sig: &str, desc: &str) -> CompletionItem {
    CompletionItem {
        label:       method.to_string(),
        kind:        Some(CompletionItemKind::METHOD),
        detail:      Some(sig.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}`** — instance method\n\n{}", sig, desc),
        })),
        insert_text:        Some(format!("{}(", method)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }
}

fn string_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("toUpper",     "string.toUpper() → string",              "Convert to UPPERCASE"),
        ("toLower",     "string.toLower() → string",              "Convert to lowercase"),
        ("trim",        "string.trim() → string",                 "Remove leading/trailing whitespace"),
        ("length",      "string.length() → int",                  "Number of characters"),
        ("isEmpty",     "string.isEmpty() → bool",                "True if empty string"),
        ("isBlank",     "string.isBlank() → bool",                "True if empty or only whitespace"),
        ("contains",    "string.contains(sub) → bool",            "True if sub is found anywhere"),
        ("startsWith",  "string.startsWith(prefix) → bool",       "True if starts with prefix"),
        ("endsWith",    "string.endsWith(suffix) → bool",         "True if ends with suffix"),
        ("indexOf",     "string.indexOf(sub) → int",              "First index of sub, -1 if absent"),
        ("lastIndexOf", "string.lastIndexOf(sub) → int",          "Last index of sub, -1 if absent"),
        ("replace",     "string.replace(old, new) → string",      "Replace all occurrences of old with new"),
        ("split",       "string.split(sep) → array",              "Split into array by separator"),
        ("substring",   "string.substring(start, len) → string",  "Extract substring at start of given length"),
        ("charAt",      "string.charAt(index) → string",          "Character at index (0-based)"),
        ("padLeft",     "string.padLeft(width, char) → string",   "Pad left to total width with char"),
        ("padRight",    "string.padRight(width, char) → string",  "Pad right to total width with char"),
        // Universal methods
        ("toString",    "string.toString() → string",             "Identity — returns self"),
        ("type",        "string.type() → string",                 "Returns \"string\""),
        ("isNull",      "string.isNull() → bool",                 "True if null"),
        ("equals",      "string.equals(other) → bool",            "Equality comparison"),
        ("json",        "string.json() → string",                 "JSON representation: \"...\""),
        ("clone",       "string.clone() → string",                "Deep copy"),
        ("hashCode",    "string.hashCode() → int",                "Hash code of value"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn array_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("length",      "array.length() → int",                   "Number of elements"),
        ("isEmpty",     "array.isEmpty() → bool",                 "True if no elements"),
        ("contains",    "array.contains(value) → bool",           "True if value is present"),
        ("indexOf",     "array.indexOf(value) → int",             "First index of value, -1 if absent"),
        ("lastIndexOf", "array.lastIndexOf(value) → int",         "Last index of value, -1 if absent"),
        ("get",         "array.get(index) → any",                 "Element at index"),
        ("set",         "array.set(index, value) → array",        "Return copy with element replaced"),
        ("push",        "array.push(value) → array",              "Return copy with value appended"),
        ("pop",         "array.pop() → array",                    "Return copy with last element removed"),
        ("shift",       "array.shift() → array",                  "Return copy with first element removed"),
        ("unshift",     "array.unshift(value) → array",           "Return copy with value prepended"),
        ("slice",       "array.slice(start, end) → array",        "Sub-array from start to end (exclusive)"),
        ("join",        "array.join(sep) → string",               "Join elements with separator"),
        ("reverse",     "array.reverse() → array",                "Return reversed copy"),
        ("sort",        "array.sort() → array",                   "Return sorted copy (lexicographic)"),
        ("concat",      "array.concat(other) → array",            "Concatenate with another array"),
        ("filter",      "array.filter(value) → array",            "Remove elements equal to value"),
        ("flatten",     "array.flatten() → array",                "Flatten one level of nesting"),
        ("distinct",    "array.distinct() → array",               "Remove duplicate values"),
        ("count",       "array.count(value) → int",               "Count occurrences of value"),
        ("first",       "array.first() → any",                    "First element"),
        ("last",        "array.last() → any",                     "Last element"),
        ("sum",         "array.sum() → double",                   "Sum of numeric elements"),
        ("average",     "array.average() → double",               "Average of numeric elements"),
        ("min",         "array.min() → double",                   "Minimum numeric value"),
        ("max",         "array.max() → double",                   "Maximum numeric value"),
        // Universal
        ("toString",    "array.toString() → string",              "String representation"),
        ("type",        "array.type() → string",                  "Returns \"array\""),
        ("isNull",      "array.isNull() → bool",                  "True if null"),
        ("json",        "array.json() → string",                  "JSON array representation"),
        ("clone",       "array.clone() → array",                  "Deep copy"),
        ("size",        "array.size() → int",                     "Estimated memory size in bytes"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn int_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("abs",        "int.abs() → int",        "Absolute value"),
        ("sign",       "int.sign() → int",        "-1, 0, or 1"),
        ("isEven",     "int.isEven() → bool",     "True if divisible by 2"),
        ("isOdd",      "int.isOdd() → bool",      "True if not divisible by 2"),
        ("isPositive", "int.isPositive() → bool", "True if > 0"),
        ("isNegative", "int.isNegative() → bool", "True if < 0"),
        ("toString",   "int.toString() → string", "Decimal string representation"),
        ("toFloat",    "int.toFloat() → float",   "Convert to 32-bit float"),
        ("toDouble",   "int.toDouble() → double", "Convert to 64-bit double"),
        ("type",       "int.type() → string",     "Returns \"int\""),
        ("equals",     "int.equals(other) → bool","Equality comparison"),
        ("hashCode",   "int.hashCode() → int",    "Hash code"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn float_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("abs",        "float.abs() → float",                  "Absolute value"),
        ("sign",       "float.sign() → int",                   "-1, 0, or 1"),
        ("floor",      "float.floor() → int",                  "Floor to int"),
        ("ceil",       "float.ceil() → int",                   "Ceiling to int"),
        ("round",      "float.round(places) → float",          "Round to decimal places"),
        ("isNaN",      "float.isNaN() → bool",                 "True if NaN"),
        ("isInfinity", "float.isInfinity() → bool",            "True if ±Infinity"),
        ("isFinite",   "float.isFinite() → bool",              "True if not NaN or Infinity"),
        ("toString",   "float.toString() → string",            "String representation"),
        ("toInt",      "float.toInt() → int",                  "Truncate to int"),
        ("toDouble",   "float.toDouble() → double",            "Widen to double"),
        ("type",       "float.type() → string",                "Returns \"float\""),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn double_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("abs",        "double.abs() → double",                "Absolute value"),
        ("sign",       "double.sign() → int",                  "-1, 0, or 1"),
        ("floor",      "double.floor() → int",                 "Floor to int"),
        ("ceil",       "double.ceil() → int",                  "Ceiling to int"),
        ("round",      "double.round(places) → double",        "Round to decimal places"),
        ("isNaN",      "double.isNaN() → bool",                "True if NaN"),
        ("isInfinity", "double.isInfinity() → bool",           "True if ±Infinity"),
        ("isFinite",   "double.isFinite() → bool",             "True if not NaN or Infinity"),
        ("toString",   "double.toString() → string",           "String representation"),
        ("toInt",      "double.toInt() → int",                 "Truncate to int"),
        ("toFloat",    "double.toFloat() → float",             "Narrow to float"),
        ("toDouble",   "double.toDouble() → double",           "Identity"),
        ("type",       "double.type() → string",               "Returns \"double\""),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn blob_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("size",     "blob.size() → int",                  "Byte count of decoded data"),
        ("mimeType", "blob.mimeType() → string",           "MIME type from magic bytes (e.g. \"image/png\")"),
        ("toHex",    "blob.toHex() → string",              "Hex string of decoded bytes"),
        ("toBytes",  "blob.toBytes() → array",             "Array of byte values 0–255"),
        ("isValid",  "blob.isValid() → bool",              "True if valid base64 encoding"),
        ("slice",    "blob.slice(start, end) → blob",      "Extract byte range as new blob"),
        ("type",     "blob.type() → string",               "Returns \"blob\""),
        ("toString", "blob.toString() → string",           "Base64 string"),
        ("json",     "blob.json() → string",               "JSON representation"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn regex_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("test",     "regex.test(str) → bool",              "True if pattern matches anywhere in str"),
        ("match",    "regex.match(str) → array",            "First match + capture groups, or empty array"),
        ("matchAll", "regex.matchAll(str) → array",         "All matches as array of capture-group arrays"),
        ("replace",  "regex.replace(str, replacement) → string", "Replace all matches with replacement"),
        ("split",    "regex.split(str) → array",            "Split str by pattern"),
        ("isValid",  "regex.isValid() → bool",              "True if pattern compiled without errors"),
        ("type",     "regex.type() → string",               "Returns \"regex\""),
        ("toString", "regex.toString() → string",           "Pattern string"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

fn tuple_instance_methods() -> Vec<CompletionItem> {
    let methods: &[(&str, &str, &str)] = &[
        ("length",  "tuple.length() → int",          "Number of elements (max 6)"),
        ("get",     "tuple.get(index) → any",        "Element at 0-based index"),
        ("first",   "tuple.first() → any",           "Element at index 0"),
        ("second",  "tuple.second() → any",          "Element at index 1"),
        ("third",   "tuple.third() → any",           "Element at index 2"),
        ("fourth",  "tuple.fourth() → any",          "Element at index 3"),
        ("fifth",   "tuple.fifth() → any",           "Element at index 4"),
        ("sixth",   "tuple.sixth() → any",           "Element at index 5"),
        ("contains","tuple.contains(value) → bool",  "True if value is present"),
        ("toArray", "tuple.toArray() → array",       "Convert to array"),
        ("reverse", "tuple.reverse() → tuple",       "Return reversed copy"),
        ("swap",    "tuple.swap(i, j) → tuple",      "Return copy with elements i and j swapped"),
        ("type",    "tuple.type() → string",         "Returns \"tuple\""),
        ("toString","tuple.toString() → string",     "String representation"),
    ];
    methods.iter().map(|(m, s, d)| make_instance_item(m, s, d)).collect()
}

// ── General completions (no special trigger) ──────────────────────────────────

fn general_completions(doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // ── QuickFunc names from the current document ──────────────────────────
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                let params: Vec<String> = func.parameters.iter()
                    .map(|p| {
                        let t = p.data_type.as_ref()
                            .map(|dt| format!("<{:?}>", dt).to_lowercase())
                            .unwrap_or_default();
                        format!("{}{}", p.name, t)
                    }).collect();
                let ret = func.return_type.as_ref()
                    .map(|t| format!("{:?}", t).to_lowercase())
                    .unwrap_or_else(|| "?".to_string());

                items.push(CompletionItem {
                    label:   func.name.clone(),
                    kind:    Some(CompletionItemKind::FUNCTION),
                    detail:  Some(format!("~{}<{}>({}) — QuickFunc", func.name, ret, params.join(", "))),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!(
                            "**Compile-time function** defined in this file.\n\n```mdix\n~{}<{}>({})\n```",
                            func.name, ret, params.join(", ")
                        ),
                    })),
                    insert_text:        Some(format!("{}(", func.name)),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
            }
        }

        // ── Enum names from current document ──────────────────────────────
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                items.push(CompletionItem {
                    label:   decl.name.clone(),
                    kind:    Some(CompletionItemKind::ENUM),
                    detail:  Some(format!("{} fields: {}", decl.fields.len(),
                                          decl.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", "))),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!("**Enum `{}`**\n\nAccess: `{}.FIELD_NAME`\n\nFields: {}",
                                       decl.name, decl.name,
                                       decl.fields.iter().map(|f| {
                                           let v = f.value.map(|n| format!(" = {}", n)).unwrap_or_default();
                                           format!("`{}{}`", f.name, v)
                                       }).collect::<Vec<_>>().join(", ")
                        ),
                    })),
                    ..Default::default()
                });
            }
        }
    }

    // ── All language keywords ─────────────────────────────────────────────
    items.extend(keyword_completions());

    // ── Built-in static object names ─────────────────────────────────────
    let static_objects: &[(&str, &str)] = &[
        ("Math",      "Built-in math functions: sqrt, pow, sin, cos, clamp, …"),
        ("DateTime",  "Date/time functions: now, today, format, addDays, …"),
        ("Array",     "Array factory functions: range, fill, sort, flatten, …"),
        ("Random",    "Random generation: range, choice, shuffle, alphanumeric, …"),
        ("Guid",      "GUID/UUID generation and validation"),
        ("IpAddress", "IP address parsing and validation (IPv4 & IPv6)"),
        ("Enum",      "Runtime enum introspection: getValues, getName, exists, …"),
        ("Dix",       "Logging and utilities: Log, Assert, Format, Join, …"),
    ];

    for (name, desc) in static_objects {
        items.push(CompletionItem {
            label:   name.to_string(),
            kind:    Some(CompletionItemKind::CLASS),
            detail:  Some("built-in static object".to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: format!("**`{}`** — built-in static object\n\n{}\n\nType `.` after this name to see all methods.", name, desc),
            })),
            ..Default::default()
        });
    }

    // ── DLM module names ─────────────────────────────────────────────────
    for (name, desc) in &[
        ("DCompressor", "Compression: .gzip, .bzip2, .lzma"),
        ("DEncryptor",  "Encryption: .aes256, .aes128, .chacha20, .xor"),
        ("DAuditor",    "Auditing: .diy, .enhanced"),
    ] {
        items.push(CompletionItem {
            label:  name.to_string(),
            kind:   Some(CompletionItemKind::MODULE),
            detail: Some(desc.to_string()),
            ..Default::default()
        });
    }

    items
}

// ── All keyword completions ───────────────────────────────────────────────────

fn keyword_completions() -> Vec<CompletionItem> {
    let keywords: &[(&str, &str, &str)] = &[
        // Control flow
        ("if:",     "if: condition { ... }",            "If branch. DixScript uses `if:` with a colon.\n\n```mdix\nif: x > 0 {\n  return x\n}\n```"),
        ("elif:",   "elif: condition { ... }",          "Else-if branch.\n\n```mdix\nelif: x == 0 {\n  return 0\n}\n```"),
        ("else",    "else { ... }",                     "Else branch.\n\n```mdix\nelse {\n  return -1\n}\n```"),
        ("chk:",    "chk: expr { -> val { } }",         "Switch/match statement.\n\n```mdix\nchk: difficulty {\n  -> Difficulty.EASY   { return 1 }\n  -> Difficulty.HARD   { return 3 }\n  -> miss              { return 2 }\n}\n```"),
        ("miss",    "-> miss { ... }",                  "Default case in a `chk:` switch."),
        ("return",  "return expr",                      "Return a value from a QuickFunc.\n\n```mdix\nreturn { key = value }\n```"),
        ("log:",    "log: expr",                        "Log an expression at DEBUG level during compilation."),
        // Variable declaration
        ("let",     "let name = expr",                  "Declare an immutable local variable.\n\n```mdix\nlet result = x + y\nlet name<string> = \"Alice\"\n```"),
        ("let mut", "let mut name = expr",              "Declare a mutable local variable.\n\n```mdix\nlet mut total<int> = 0\ntotal += 1\n```"),
        ("const",   "const name = expr",                "Declare a compile-time constant."),
        // Logical operators
        ("and",     "a and b",                          "Logical AND (word form, equivalent to `&&`)."),
        ("or",      "a or b",                           "Logical OR (word form, equivalent to `||`)."),
        ("not",     "not expr",                         "Logical NOT (word form, equivalent to `!`)."),
        // Literals
        ("true",    "true",                             "Boolean true literal."),
        ("false",   "false",                            "Boolean false literal."),
        ("null",    "null",                             "Null literal — absent or unset value."),
        // Import keywords
        ("from",         "Alias from \"path\"",         "Import a local `.mdix` file."),
        ("from_cloud",   "Alias from_cloud \"url\"",    "Import a remote `.mdix` file over HTTPS."),
        ("verify",       "verify \"hash\"",             "Verify import file hash."),
        // Scope
        ("global",  "global",                          "Mark a QuickFunc variable as globally scoped."),
    ];

    keywords.iter().map(|(label, detail, doc)| CompletionItem {
        label:       label.to_string(),
        kind:        Some(CompletionItemKind::KEYWORD),
        detail:      Some(detail.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: doc.to_string(),
        })),
        insert_text:        Some(label.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }).collect()
}

// ── Helper: example call string ───────────────────────────────────────────────

fn example_call(obj: &str, method: &str) -> String {
    match (obj, method) {
        ("Math", "sqrt")       => "Math.sqrt(16)       // → 4.0".to_string(),
        ("Math", "clamp")      => "Math.clamp(15, 0, 10) // → 10.0".to_string(),
        ("Math", "pi")         => "Math.pi()           // → 3.14159…".to_string(),
        ("DateTime", "now")    => "DateTime.now()      // → 2025-01-15T10:30:00Z".to_string(),
        ("DateTime", "format") => "DateTime.format(DateTime.now(), \"%Y-%m-%d\")".to_string(),
        ("Array", "range")     => "Array.range(1, 5)   // → [1,2,3,4,5]".to_string(),
        ("Array", "fill")      => "Array.fill(0, 3)    // → [0,0,0]".to_string(),
        ("Random", "range")    => "Random.range(1, 100)".to_string(),
        ("Guid", "new")        => "Guid.new()  // → \"550e8400-e29b-41d4-…\"".to_string(),
        ("Dix", "Log")         => "Dix.Log(\"Hello from compile time!\")".to_string(),
        ("Dix", "Format")      => "Dix.Format(\"Value: {0}\", myVar)".to_string(),
        ("Enum", "getValues")  => "Enum.getValues(\"Difficulty\") // → [\"EASY\",…]".to_string(),
        _ => format!("{}.{}(…)", obj, method),
    }
}

// ── Source text helpers ───────────────────────────────────────────────────────

fn trigger_char(source: &str, pos: Position) -> char {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    if pos.character == 0 { return '\0'; }
    line.chars().nth((pos.character - 1) as usize).unwrap_or('\0')
}

fn word_before_dot(source: &str, pos: Position) -> String {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to: String = line
        .char_indices()
        .take_while(|(i, _)| *i < pos.character.saturating_sub(1) as usize)
        .map(|(_, c)| c)
        .collect();
    up_to
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .last()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{Position, Url};

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            source.to_string(), 0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn section_snippets_cover_all_sections() {
        let items  = section_snippet_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for s in &["@CONFIG","@IMPORTS","@DLM","@ENUMS","@QUICKFUNCS","@DATA","@SECURITY"] {
            assert!(labels.iter().any(|l| l.to_string() == s.to_string()), "missing: {}", s);
        }
    }

    #[test]
    fn type_annotations_complete() {
        let items = type_annotation_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for t in &["<int>","<float>","<double>","<string>","<bool>","<array>","<tuple>",
            "<object>","<hex>","<blob>","<regex>","<date>","<timestamp>","<enum>","<any>"] {
            assert!(labels.iter().any(|l| l == t), "missing type: {}", t);
        }
    }

    #[test]
    fn all_static_objects_have_completions() {
        for obj in &["Math","DateTime","Array","Random","Guid","IpAddress","Enum","Dix"] {
            let methods = static_method_completions(obj);
            assert!(!methods.is_empty(), "{} has no static method completions", obj);
        }
    }

    #[test]
    fn keyword_completions_non_empty() {
        let kws = keyword_completions();
        assert!(!kws.is_empty());
        let labels: Vec<String> = kws.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "return"), "return missing");
        assert!(labels.iter().any(|l| l == "if:"),    "if: missing");
        assert!(labels.iter().any(|l| l == "let"),    "let missing");
        assert!(labels.iter().any(|l| l == "null"),   "null missing");
    }

    #[test]
    fn quickfunc_names_appear_in_general_completions() {
        let src = "@QUICKFUNCS(\n  ~calc<int>(x) { return x }\n)\n@DATA(\n  y = 1\n)";
        let doc = test_doc(src);
        let items = general_completions(&doc, Position::new(3, 0));
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "calc"), "QuickFunc 'calc' missing; got: {:?}", labels);
    }

    #[test]
    fn instance_methods_blob() {
        let methods = blob_instance_methods();
        let labels: Vec<String> = methods.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "mimeType"), "mimeType missing from blob methods");
        assert!(labels.iter().any(|l| l == "size"),     "size missing from blob methods");
    }

    #[test]
    fn instance_methods_regex() {
        let methods = regex_instance_methods();
        let labels: Vec<String> = methods.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "test"),    "test missing from regex methods");
        assert!(labels.iter().any(|l| l == "replace"), "replace missing from regex methods");
    }
}