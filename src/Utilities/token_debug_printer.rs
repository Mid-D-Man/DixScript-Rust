use crate::Compiler::Core::Tokenizer::{Token, TokenType, TokenizationResult};
use std::fmt::Write;

/// Debug printer for DixScript tokens
/// Shows exact token types and positions for debugging lexer output
pub struct TokenDebugPrinter {
    output: String,
    show_positions: bool,
    show_sections: bool,
    group_by_line: bool,
}

impl TokenDebugPrinter {
    /// Create new debug printer
    pub fn new(show_positions: bool, show_sections: bool, group_by_line: bool) -> Self {
        TokenDebugPrinter {
            output: String::new(),
            show_positions,
            show_sections,
            group_by_line,
        }
    }

    /// Print tokens to string
    pub fn print(&mut self, result: &TokenizationResult) -> String {
        self.output.clear();

        self.writeln("=== DIXSCRIPT TOKEN DEBUG OUTPUT ===");
        self.writeln(&format!("Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        self.writeln(&format!("Total Tokens: {}", result.tokens.len()));
        self.writeln(&format!("Total Lines: {}", result.metadata.total_lines));
        self.writeln("");

        if self.group_by_line {
            self.print_grouped_by_line(&result.tokens);
        } else {
            self.print_sequential(&result.tokens);
        }

        self.writeln("");
        self.writeln("=== METADATA ===");
        self.writeln(&format!("Version: {}", result.metadata.version));
        self.writeln(&format!("Sections Detected: {:?}", result.metadata.sections_detected));
        self.writeln(&format!("Prefixed Constructors: {}", result.metadata.prefixed_constructors_found));
        self.writeln(&format!("  - Blob: {}", result.metadata.blob_constructors));
        self.writeln(&format!("  - Tuple: {}", result.metadata.tuple_constructors));
        self.writeln(&format!("  - Regex: {}", result.metadata.regex_constructors));
        self.writeln(&format!("Static Calls Found: {}", result.metadata.static_calls_found));
        self.writeln(&format!("Potential Builtin Calls: {}", result.metadata.potential_builtin_calls));

        if !result.prefixed_constructors.is_empty() {
            self.writeln("");
            self.writeln("=== PREFIXED CONSTRUCTORS ===");
            for pc in &result.prefixed_constructors {
                self.writeln(&format!(
                    "{}:{} at L{}:C{} ({})",
                    pc.prefix,
                    pc.constructor_type,
                    pc.line,
                    pc.column,
                    pc.section.as_deref().unwrap_or("unknown")
                ));
            }
        }

        if !result.static_calls.is_empty() {
            self.writeln("");
            self.writeln("=== STATIC CALLS ===");
            for sc in &result.static_calls {
                self.writeln(&format!(
                    "{}.{} at L{}:C{} ({})",
                    sc.object_name,
                    sc.method_name,
                    sc.line,
                    sc.column,
                    sc.section.as_deref().unwrap_or("unknown")
                ));
            }
        }

        self.output.clone()
    }

    /// Print tokens to file in project directory
    pub fn print_to_file(&mut self, result: &TokenizationResult, filename: &str) -> Result<String, String> {
        let content = self.print(result);

        // Find project root
        let mut project_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Look for Cargo.toml
        loop {
            if project_dir.join("Cargo.toml").exists() {
                break;
            }

            match project_dir.parent() {
                Some(parent) => project_dir = parent.to_path_buf(),
                None => break,
            }
        }

        let filepath = project_dir.join(filename);
        std::fs::write(&filepath, &content)
            .map_err(|e| format!("Failed to write file: {}", e))?;

        let filepath_str = filepath.to_string_lossy().to_string();
        println!("[TokenDebugPrinter] Tokens dumped to: {}", filepath_str);

        Ok(filepath_str)
    }

    fn print_sequential(&mut self, tokens: &[Token]) {
        for (idx, token) in tokens.iter().enumerate() {
            self.write_token(idx, token);
        }
    }

    fn print_grouped_by_line(&mut self, tokens: &[Token]) {
        if tokens.is_empty() {
            return;
        }

        let mut current_line = 1;
        let mut line_tokens = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            if token.line != current_line {
                // Print accumulated line
                self.print_line(current_line, &line_tokens);
                line_tokens.clear();
                current_line = token.line;
            }

            line_tokens.push((idx, token));
        }

        // Print last line
        if !line_tokens.is_empty() {
            self.print_line(current_line, &line_tokens);
        }
    }

    fn print_line(&mut self, line_num: usize, tokens: &[(usize, &Token)]) {
        self.writeln(&format!("--- Line {} ---", line_num));
        for (idx, token) in tokens {
            self.write_token(*idx, token);
        }
        self.writeln("");
    }

    fn write_token(&mut self, idx: usize, token: &Token) {
        let mut parts = vec![format!("[{}]", idx)];

        // Token type
        parts.push(self.format_token_type(&token.token_type));

        // Position
        if self.show_positions {
            parts.push(format!("@L{}:C{}", token.line, token.column));
        }

        // Section
        if self.show_sections {
            if let Some(ref section) = token.section {
                parts.push(format!("({})", section));
            }
        }

        self.writeln(&parts.join(" "));
    }

    fn format_token_type(&self, token_type: &TokenType) -> String {
        match token_type {
            TokenType::Keyword(k) => format!("Keyword({})", k),
            TokenType::Identifier(i) => format!("Identifier({})", i),
            TokenType::Integer(i) => format!("Integer({})", i),
            TokenType::Float(f) => format!("Float({})", f),
            TokenType::Double(d) => format!("Double({})", d),
            TokenType::ScientificNotation(sn) => format!("ScientificNotation({})", sn),
            TokenType::String(s) => format!("String(\"{}\")", Self::escape_string(s)),
            TokenType::StringSingle(ss) => format!("StringSingle('{}')", Self::escape_string(ss)),
            TokenType::Bool(b) => format!("Bool({})", b),
            TokenType::InterpolatedString(ist) => format!("InterpolatedString($\"{}\")", Self::escape_string(ist)),
            TokenType::Symbol(s) => format!("Symbol({})", s),
            TokenType::MultiCharSymbol(ms) => format!("MultiCharSymbol({})", ms),
            TokenType::HexColor(hc) => format!("HexColor({})", hc),
            TokenType::HexLiteral(hl) => format!("HexLiteral(0x{:X})", hl),
            TokenType::Date(d) => format!("Date({})", d),
            TokenType::Timestamp(t) => format!("Timestamp({})", t),
            TokenType::TablePath(tp) => format!("TablePath({})", tp),
            TokenType::DoubleColon => "DoubleColon(::)".to_string(),
            TokenType::Arrow => "Arrow(=>)".to_string(),
            TokenType::SwitchCase => "SwitchCase(->)".to_string(),
            TokenType::FunctionPrefix => "FunctionPrefix(~)".to_string(),
            TokenType::ControlFlowColon => "ControlFlowColon(:)".to_string(),
            TokenType::PrefixedConstructor { prefix, value } => {
                format!("PrefixedConstructor({}:{})", prefix, value)
            }
            TokenType::BlobConstructor(bc) => format!("BlobConstructor(b:{})", bc),
            TokenType::TupleConstructor(tc) => format!("TupleConstructor(t:{})", tc),
            TokenType::RegexConstructor(rc) => format!("RegexConstructor(r:{})", rc),
            TokenType::ArithmeticOp(ao) => format!("ArithmeticOp({})", ao),
            TokenType::ArithmeticAssignOp(aao) => format!("ArithmeticAssignOp({})", aao),
            TokenType::ComparisonOp(co) => format!("ComparisonOp({})", co),
            TokenType::LogicalOp(lo) => format!("LogicalOp({})", lo),
            TokenType::BitwiseOp(bo) => format!("BitwiseOp({})", bo),
            TokenType::SectionConfig => "SectionConfig(@CONFIG)".to_string(),
            TokenType::SectionDLM => "SectionDLM(@DLM)".to_string(),
            TokenType::SectionEnums => "SectionEnums(@ENUMS)".to_string(),
            TokenType::SectionImports => "SectionImports(@IMPORTS)".to_string(),
            TokenType::SectionQuickFuncs => "SectionQuickFuncs(@QUICKFUNCS)".to_string(),
            TokenType::SectionData => "SectionData(@DATA)".to_string(),
            TokenType::SectionSecurity => "SectionSecurity(@SECURITY)".to_string(),
            TokenType::ConfigAccess(ca) => format!("ConfigAccess(config.{})", ca),
            TokenType::EnumAccess { enum_name, value } => {
                format!("EnumAccess({}.{})", enum_name, value)
            }
            TokenType::ObjectAccess(oa) => format!("ObjectAccess({})", oa.join(".")),
            TokenType::ScopeDeclaration(sd) => format!("ScopeDeclaration(=> {})", sd),
            TokenType::StaticFunction { class, method } => {
                format!("StaticFunction({}.{})", class, method)
            }
            TokenType::DixFunction(df) => format!("DixFunction(Dix.{})", df),
            TokenType::BuiltinMethod(bm) => format!("BuiltinMethod(.{})", bm),
            TokenType::DataType(dt) => format!("DataType(<{}>)", dt),
            TokenType::Comment(c) => format!("Comment({})", Self::truncate_string(c, 50)),
            TokenType::Error(e) => format!("Error({})", e),
            TokenType::EndOfFile => "EndOfFile".to_string(),
            TokenType::ParseContext(pc) => format!("ParseContext({})", pc),
        }
    }

    fn escape_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r")
            .replace('"', "\\\"")
    }

    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }

    fn writeln(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }
}

impl Default for TokenDebugPrinter {
    fn default() -> Self {
        Self::new(true, true, false)
    }
}