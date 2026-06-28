use crate::Compiler::AST::*;

/// Debug printer for DixScript AST.
/// Shows exact node types and structure for debugging.
pub struct AstDebugPrinter {
    output:         String,
    indent_level:   usize,
    show_positions: bool,
    show_types:     bool,
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
        self.writeln(&format!(
            "Generated: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));
        self.writeln("");

        self.visit_dixscript(ast);

        self.output.clone()
    }

    /// Print AST to file in project directory
    pub fn print_to_file(
        &mut self,
        ast: &DixScript,
        filename: &str,
    ) -> Result<String, String> {
        let content = self.print(ast);

        let mut project_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

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
            let from_keyword = if import.is_cloud_import {
                "from_cloud"
            } else {
                "from"
            };
            let verify = import
                .verify_hash
                .as_ref()
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
            let subtype = module
                .subtype
                .map(|s| format!(".{}", s))
                .unwrap_or_default();
            self.writeln(&format!("{}{}", module.module_type, subtype));
        }
    }

    fn visit_enums_section(&mut self, section: &EnumsSection) {
        for enum_decl in &section.enums {
            self.writeln(&format!(
                "enum {} {{{}",
                enum_decl.name,
                self.format_position(&enum_decl.position)
            ));
            self.indent();
            for field in &enum_decl.fields {
                let value = field
                    .value
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

    fn visit_quickfuncs_section(&mut self, section: &QuickFuncsSection) {
        self.writeln(&format!(
            "{} function(s){}",
            section.functions.len(),
            self.format_node_type("QuickFuncsSection")
        ));

        for func in &section.functions {
            let return_type = func
                .return_type
                .map(|rt| format!("<{}>", rt))
                .unwrap_or_default();
            let scope = func
                .scope_list
                .as_ref()
                .map(|s| format!(" => {}", s.join(",")))
                .unwrap_or_default();
            let pos = self.format_position(&func.position);

            self.writeln(&format!(
                "~{}{}{}{} {}",
                func.name,
                return_type,
                scope,
                pos,
                self.format_node_type("QuickFunction")
            ));

            self.indent();

            // Parameters
            if func.parameters.is_empty() {
                self.writeln("params: (none)");
            } else {
                self.writeln(&format!("params: ({} total)", func.parameters.len()));
                self.indent();
                for param in &func.parameters {
                    let type_str = param
                        .data_type
                        .map(|dt| format!("<{}>", dt))
                        .unwrap_or_default();
                    let default_str = if param.default_value.is_some() {
                        " = <default>".to_string()
                    } else {
                        String::new()
                    };
                    let param_pos = self.format_position(&param.position);
                    self.writeln(&format!(
                        "{}{}{}{}",
                        param.name, type_str, default_str, param_pos
                    ));
                }
                self.unindent();
            }

            // Body
            self.writeln(&format!("body: {} statement(s)", func.body.len()));
            for stmt in &func.body {
                self.indent();
                self.writeln(&format!("{}", stmt));
                self.unindent();
            }

            self.unindent();
        }
    }

    fn visit_data_section(&mut self, section: &DataSection) {
        self.writeln(&format!(
            "{} entry/entries{}",
            section.entries.len(),
            self.format_node_type("DataSection")
        ));

        for entry in &section.entries {
            match entry {
                DataEntry::SimpleProperty { name, data_type, value, position } => {
                    let type_str = data_type
                        .map(|dt| format!("<{}>", dt))
                        .unwrap_or_default();
                    let pos = self.format_position(position);
                    self.writeln(&format!(
                        "SimpleProperty{}: {}{} = {}{}",
                        self.format_node_type("SimpleProperty"),
                        name,
                        type_str,
                        value,
                        pos
                    ));
                }

                DataEntry::TableProperty { path, properties, position } => {
                    let pos = self.format_position(position);
                    self.writeln(&format!(
                        "TableProperty{}: {}{}",
                        self.format_node_type("TableProperty"),
                        path,
                        pos
                    ));
                    self.indent();
                    for prop in properties {
                        let type_str = prop
                            .data_type
                            .map(|dt| format!("<{}>", dt))
                            .unwrap_or_default();
                        let prop_pos = self.format_position(&prop.position);
                        self.writeln(&format!(
                            "{}{} = {}{}",
                            prop.name, type_str, prop.value, prop_pos
                        ));
                    }
                    self.unindent();
                }

                DataEntry::GroupArray { path, items, position } => {
                    let pos = self.format_position(position);
                    self.writeln(&format!(
                        "GroupArray{}: {}:: ({} item(s)){}",
                        self.format_node_type("GroupArray"),
                        path,
                        items.len(),
                        pos
                    ));
                    self.indent();
                    for item in items {
                        self.writeln(&format!("- {}", item));
                    }
                    self.unindent();
                }

                DataEntry::ObjectProperty { name, data_type, object, position } => {
                    let type_str = data_type
                        .map(|dt| format!("<{}>", dt))
                        .unwrap_or_default();
                    let pos = self.format_position(position);
                    self.writeln(&format!(
                        "ObjectProperty{}: {}{} = {}{}",
                        self.format_node_type("ObjectProperty"),
                        name,
                        type_str,
                        object,
                        pos
                    ));
                }
            }
        }
    }

    fn visit_security_section(&mut self, section: &SecuritySection) {
        self.writeln(&format!(
            "{} entry/entries{}",
            section.entries.len(),
            self.format_node_type("SecuritySection")
        ));

        for entry in &section.entries {
            let pos = self.format_position(&entry.position);
            self.writeln(&format!(
                "{} ->{}{}",
                entry.block_key,
                pos,
                self.format_node_type("SecurityEntry")
            ));

            self.indent();
            for field in &entry.fields {
                let field_pos = self.format_position(&field.position);
                self.writeln(&format!(
                    "{} = {}{}",
                    field.key, field.value, field_pos
                ));
            }
            self.unindent();
        }
    }

    // ==================== VALUE FORMATTING ====================

    fn format_config_value(&self, value: &ConfigValue) -> String {
        match value {
            ConfigValue::String(s)          => format!("\"{}\"", s),
            ConfigValue::Integer(i)         => i.to_string(),
            ConfigValue::Float(f)           => f.to_string(),
            ConfigValue::Boolean(b)         => b.to_string().to_lowercase(),
            ConfigValue::Date(d)            => d.clone(),
            ConfigValue::Timestamp(t)       => t.clone(),
            ConfigValue::Features(features) => format!("[{}]", features.join(", ")),
            ConfigValue::ErrorHandling(eh)  => format!("{:?}", eh),
            ConfigValue::Compatibility(c)   => format!("{:?}", c),
            ConfigValue::Debug(d)           => format!("{:?}", d),
        }
    }
}

impl Default for AstDebugPrinter {
    fn default() -> Self {
        Self::new(true, true)
    }
        }
