
use crate::Compiler::AST::*;
use std::fmt::Write;

/// Debug printer for DixScript AST
/// Shows exact node types and structure for debugging
pub struct AstDebugPrinter {
    output: String,
    indent_level: usize,
    show_positions: bool,
    show_types: bool,
}

const INDENT: &str = "  ";

impl AstDebugPrinter {
    /// Create new debug printer
    pub fn new(show_positions: bool, show_types: bool) -> Self {
        AstDebugPrinter {
            output: String::new(),
            indent_level: 0,
            show_positions,
            show_types,
        }
    }

    /// Print AST to string
    pub fn print(&mut self, ast: &DixScript) -> String {
        self.output.clear();
        self.indent_level = 0;

        self.writeln("=== DIXSCRIPT AST DEBUG OUTPUT ===");
        self.writeln(&format!("Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        self.writeln("");

        self.visit_dixscript(ast);

        self.output.clone()
    }

    /// Print AST to file in project directory
    pub fn print_to_file(&mut self, ast: &DixScript, filename: &str) -> Result<String, String> {
        let content = self.print(ast);

        // Find project root
        let mut project_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Look for Cargo.toml
        loop {
            if project_dir.join("../../Cargo.toml").exists() {
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
        println!("[AstDebugPrinter] AST dumped to: {}", filepath_str);

        Ok(filepath_str)
    }

    // ==================== HELPER METHODS ====================

    fn writeln(&mut self, text: &str) {
        for _ in 0..(self.indent_level * INDENT.len()) {
            self.output.push(' ');
        }
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn unindent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn format_position(&self, pos: &Position) -> String {
        if !self.show_positions || pos.is_unknown() {
            String::new()
        } else {
            format!(" @L{}:C{}", pos.line, pos.column)
        }
    }

    fn format_node_type(&self, node_type: &str) -> String {
        if !self.show_types {
            String::new()
        } else {
            format!("[{}]", node_type)
        }
    }

    // ==================== VISITOR METHODS ====================

    fn visit_dixscript(&mut self, ast: &DixScript) {
        if let Some(ref config) = ast.config {
            self.writeln("@CONFIG");
            self.indent();
            self.visit_config_section(config);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref imports) = ast.imports {
            self.writeln("@IMPORTS");
            self.indent();
            self.visit_imports_section(imports);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref dlm) = ast.dlm {
            self.writeln("@DLM");
            self.indent();
            self.visit_dlm_section(dlm);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref enums) = ast.enums {
            self.writeln("@ENUMS");
            self.indent();
            self.visit_enums_section(enums);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref quick_funcs) = ast.quick_functions {
            self.writeln("@QUICKFUNCS");
            self.indent();
            self.visit_quickfuncs_section(quick_funcs);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref data) = ast.data {
            self.writeln("@DATA");
            self.indent();
            self.visit_data_section(data);
            self.unindent();
            self.writeln("");
        }

        if let Some(ref security) = ast.security {
            self.writeln("@SECURITY");
            self.indent();
            self.visit_security_section(security);
            self.unindent();
            self.writeln("");
        }
    }

    fn visit_config_section(&mut self, section: &ConfigSection) {
        for entry in &section.entries {
            self.writeln(&format!(
                "{} -> {}",
                entry.key,
                self.format_config_value(&entry.value)
            ));
        }
    }

    fn visit_imports_section(&mut self, section: &ImportsSection) {
        for import in &section.imports {
            let from_keyword = if import.is_cloud_import { "from_cloud" } else { "from" };
            let verify = import.verify_hash.as_ref()
                .map(|h| format!(" verify \"{}\"", h))
                .unwrap_or_default();

            self.writeln(&format!(
                "{} {} \"{}\"{}{}",
                import.alias,
                from_keyword,
                import.path,
                verify,
                self.format_position(&import.position)
            ));
        }
    }

    fn visit_dlm_section(&mut self, section: &DLMSection) {
        for module in &section.modules {
            let subtype = module.subtype
                .map(|s| format!(".{}", s))
                .unwrap_or_default();
            self.writeln(&format!("{}{}", module.module_type, subtype));
        }
    }

    fn visit_enums_section(&mut self, section: &EnumsSection) {
        for enum_decl in &section.enums {
            self.writeln(&format!("enum {} {{{}",
                                  enum_decl.name,
                                  self.format_position(&enum_decl.position)
            ));

            self.indent();
            for field in &enum_decl.fields {
                let value = field.value
                    .map(|v| format!(" = {}", v))
                    .unwrap_or_default();
                self.writeln(&format!(
                    "{}{}{}",
                    field.name,
                    value,
                    self.format_position(&field.position)
                ));
            }
            self.unindent();

            self.writeln("}");
        }
    }

    fn visit_quickfuncs_section(&mut self, _section: &QuickFuncsSection) {
        // TODO: Implement when QuickFuncs is ready
        self.writeln("[QuickFuncs section - not yet implemented]");
    }

    fn visit_data_section(&mut self, _section: &DataSection) {
        // TODO: Implement when Data section is ready
        self.writeln("[Data section - not yet implemented]");
    }

    fn visit_security_section(&mut self, _section: &SecuritySection) {
        // TODO: Implement when Security section is ready
        self.writeln("[Security section - not yet implemented]");
    }

    // ==================== VALUE FORMATTING ====================

    fn format_config_value(&self, value: &ConfigValue) -> String {
        match value {
            ConfigValue::String(s) => format!("\"{}\"", s),
            ConfigValue::Integer(i) => i.to_string(),
            ConfigValue::Float(f) => f.to_string(),
            ConfigValue::Boolean(b) => b.to_string().to_lowercase(),
            ConfigValue::Date(d) => d.clone(),
            ConfigValue::Timestamp(t) => t.clone(),
            ConfigValue::Features(features) => format!("[{}]", features.join(", ")),
            ConfigValue::ErrorHandling(eh) => format!("{:?}", eh),
            ConfigValue::Compatibility(c) => format!("{:?}", c),
            ConfigValue::Debug(d) => format!("{:?}", d),
        }
    }
}

impl Default for AstDebugPrinter {
    fn default() -> Self {
        Self::new(true, true)
    }
}