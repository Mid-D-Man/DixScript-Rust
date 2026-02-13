// src/Runtime/format_options.rs

/// Formatting options for DixScript conversion and export
/// 
/// Controls how DixData is serialized to MDIX format:
/// - Indentation (spaces/tabs)
/// - Minification
/// - Comments
/// - Key sorting
/// - Section inclusion
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DixFormatOptions {
    /// Indent output with spaces (default: true)
    pub indented: bool,
    
    /// Number of spaces for indentation (default: 2)
    pub indent_size: usize,
    
    /// Use tabs instead of spaces (default: false)
    pub use_tabs: bool,
    
    /// Minify output - remove all whitespace (default: false)
    pub minify: bool,
    
    /// Include comments in output (default: true)
    pub include_comments: bool,
    
    /// Sort keys alphabetically (default: false)
    pub sort_keys: bool,
    
    /// Include type annotations (default: false)
    pub include_type_annotations: bool,
    
    /// Escape unicode characters (default: false)
    pub escape_unicode: bool,
    
    /// Maximum line length before wrapping (0 = no limit)
    pub max_line_length: usize,
    
    /// Generate CONFIG section with metadata (default: true)
    pub include_config_section: bool,
    
    /// Generate version information (default: true)
    pub include_version: bool,
}

impl DixFormatOptions {
    /// Default formatting options (readable, indented)
    pub fn new() -> Self {
        DixFormatOptions {
            indented: true,
            indent_size: 2,
            use_tabs: false,
            minify: false,
            include_comments: true,
            sort_keys: false,
            include_type_annotations: false,
            escape_unicode: false,
            max_line_length: 0,
            include_config_section: true,
            include_version: true,
        }
    }
    
    /// Compact formatting (no minification, no comments)
    pub fn compact() -> Self {
        DixFormatOptions {
            indented: false,
            include_comments: false,
            include_config_section: false,
            ..Default::default()
        }
    }
    
    /// Minified formatting (smallest possible output)
    pub fn minified() -> Self {
        DixFormatOptions {
            minify: true,
            indented: false,
            include_comments: false,
            include_config_section: false,
            include_version: false,
            ..Default::default()
        }
    }
    
    /// Pretty formatting (readable, verbose, with annotations)
    pub fn pretty() -> Self {
        DixFormatOptions {
            indented: true,
            indent_size: 4,
            sort_keys: true,
            include_type_annotations: true,
            include_comments: true,
            ..Default::default()
        }
    }
    
    /// Get indentation string for given level
    /// 
    /// # Examples
    /// ``` rust,ignore
    /// let opts = DixFormatOptions::new();
    /// assert_eq!(opts.get_indentation(1), "  "); // 2 spaces
    /// assert_eq!(opts.get_indentation(2), "    "); // 4 spaces
    /// ```
    pub fn get_indentation(&self, level: usize) -> String {
        if !self.indented || self.minify {
            return String::new();
        }
        
        let unit = if self.use_tabs {
            "\t"
        } else {
            &" ".repeat(self.indent_size)
        };
        
        unit.repeat(level)
    }
    
    /// Get newline string (empty if minified)
    #[inline]
    pub fn get_newline(&self) -> &'static str {
        if self.minify { "" } else { "\n" }
    }
    
    /// Get space string (empty if minified)
    #[inline]
    pub fn get_space(&self) -> &'static str {
        if self.minify { "" } else { " " }
    }
}

impl Default for DixFormatOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_options() {
        let opts = DixFormatOptions::new();
        assert!(opts.indented);
        assert_eq!(opts.indent_size, 2);
        assert!(!opts.minify);
    }
    
    #[test]
    fn test_compact_options() {
        let opts = DixFormatOptions::compact();
        assert!(!opts.indented);
        assert!(!opts.include_comments);
        assert!(!opts.include_config_section);
    }
    
    #[test]
    fn test_minified_options() {
        let opts = DixFormatOptions::minified();
        assert!(opts.minify);
        assert!(!opts.indented);
        assert_eq!(opts.get_newline(), "");
        assert_eq!(opts.get_space(), "");
    }
    
    #[test]
    fn test_indentation() {
        let opts = DixFormatOptions::new();
        assert_eq!(opts.get_indentation(1), "  ");
        assert_eq!(opts.get_indentation(2), "    ");
        
        let tabs = DixFormatOptions {
            use_tabs: true,
            ..Default::default()
        };
        assert_eq!(tabs.get_indentation(1), "\t");
    }
}
