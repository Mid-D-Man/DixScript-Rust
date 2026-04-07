
/// Utilities for compacting and minifying DixScript files
/// 
/// Provides three levels of compression:
/// - `minify()` - Remove ALL unnecessary whitespace (smallest output)
/// - `compact()` - Remove extra whitespace but keep readability
/// - `remove_comments()` - Strip comments only
pub struct DixCompactor;

impl DixCompactor {
    /// Minify DixScript content - remove all unnecessary whitespace
    /// 
    /// Preserves:
    /// - String contents (including whitespace in strings)
    /// - Necessary spaces between identifiers/keywords
    /// 
    /// # Examples
    ///! ```
    ///  let input = "@CONFIG(\n  version -> \"1.0.0\"\n)";
    /// let output = DixCompactor::minify(input);
    /// // "@CONFIG(version->\"1.0.0\")"
    /// ```
    pub fn minify(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        
        let mut in_string = false;
        let mut in_interpolation = false;
        let mut string_char = '\0';
        
        while i < chars.len() {
            let c = chars[i];
            let next = if i + 1 < chars.len() { chars[i + 1] } else { '\0' };
            let prev = if i > 0 { chars[i - 1] } else { '\0' };
            
            // Handle string state
            if (c == '"' || c == '\'') && prev != '\\' {
                if !in_string {
                    in_string = true;
                    string_char = c;
                    result.push(c);
                } else if c == string_char {
                    in_string = false;
                    string_char = '\0';
                    result.push(c);
                } else {
                    result.push(c);
                }
                i += 1;
                continue;
            }
            
            // Handle interpolated strings
            if c == '$' && next == '"' {
                in_interpolation = true;
                result.push(c);
                i += 1;
                continue;
            }
            
            // Inside string or interpolation - preserve everything
            if in_string || in_interpolation {
                result.push(c);
                if in_interpolation && c == '"' && prev != '\\' {
                    in_interpolation = false;
                }
                i += 1;
                continue;
            }
            
            // Handle single-line comments
            if c == '/' && next == '/' {
                // Skip until end of line
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            
            // Handle multi-line comments
            if c == '/' && next == '*' {
                i += 2;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            
            // Handle whitespace
            if c.is_whitespace() {
                let last_char = result.chars().last().unwrap_or('\0');
                
                // Keep space between alphanumeric characters
                if last_char.is_alphanumeric() && next.is_alphanumeric() {
                    result.push(' ');
                }
                
                i += 1;
                continue;
            }
            
            // Keep all other characters
            result.push(c);
            i += 1;
        }
        
        result
    }
    
    /// Compact DixScript - remove extra whitespace but keep readability
    /// 
    /// - Removes trailing whitespace
    /// - Collapses multiple blank lines to single blank line
    /// - Preserves overall structure
    pub fn compact(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len());
        let mut consecutive_blank_lines = 0;
        
        for line in lines {
            let trimmed = line.trim_end();
            
            if trimmed.is_empty() {
                consecutive_blank_lines += 1;
                if consecutive_blank_lines <= 1 {
                    result.push('\n');
                }
            } else {
                consecutive_blank_lines = 0;
                result.push_str(trimmed);
                result.push('\n');
            }
        }
        
        result
    }
    
    /// Remove comments from DixScript
    /// 
    /// Preserves:
    /// - String contents
    /// - All code structure
    /// 
    /// Removes:
    /// - Single-line comments (`//`)
    /// - Multi-line comments (`/* */`)
    pub fn remove_comments(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        
        let mut in_string = false;
        let mut string_char = '\0';
        
        while i < chars.len() {
            let c = chars[i];
            let next = if i + 1 < chars.len() { chars[i + 1] } else { '\0' };
            let prev = if i > 0 { chars[i - 1] } else { '\0' };
            
            // Handle string state
            if (c == '"' || c == '\'') && prev != '\\' {
                if !in_string {
                    in_string = true;
                    string_char = c;
                } else if c == string_char {
                    in_string = false;
                }
                result.push(c);
                i += 1;
                continue;
            }
            
            // Inside string - preserve everything
            if in_string {
                result.push(c);
                i += 1;
                continue;
            }
            
            // Handle single-line comments
            if c == '/' && next == '/' {
                // Skip until end of line
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            
            // Handle multi-line comments
            if c == '/' && next == '*' {
                i += 2;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            
            // Keep all other characters
            result.push(c);
            i += 1;
        }
        
        result
    }
    
    /// Calculate compression ratio
    /// 
    /// Returns value between 0.0 and 1.0:
    /// - 0.0 = no compression
    /// - 1.0 = 100% compression
    pub fn get_compression_ratio(original: &str, compressed: &str) -> f64 {
        if original.is_empty() {
            return 0.0;
        }
        
        1.0 - (compressed.len() as f64 / original.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_minify_basic() {
        let input = "@CONFIG(\n  version -> \"1.0.0\"\n)";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "@CONFIG(version->\"1.0.0\")");
    }
    
    #[test]
    fn test_minify_preserves_strings() {
        let input = "name = \"Hello   World\"";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "name=\"Hello   World\"");
    }
    
    #[test]
    fn test_minify_preserves_necessary_spaces() {
        let input = "let x = 5";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "let x=5");
    }
    
    #[test]
    fn test_remove_comments_single_line() {
        let input = "x = 5 // comment\ny = 10";
        let output = DixCompactor::remove_comments(input);
        assert_eq!(output, "x = 5 \ny = 10");
    }
    
    #[test]
    fn test_remove_comments_multi_line() {
        let input = "x = 5 /* comment */ y = 10";
        let output = DixCompactor::remove_comments(input);
        assert_eq!(output, "x = 5  y = 10");
    }
    
    #[test]
    fn test_remove_comments_preserves_strings() {
        let input = "url = \"http://example.com\" // not a comment in string";
        let output = DixCompactor::remove_comments(input);
        assert_eq!(output, "url = \"http://example.com\" ");
    }
    
    #[test]
    fn test_compact_removes_extra_blank_lines() {
        let input = "line1\n\n\n\nline2";
        let output = DixCompactor::compact(input);
        assert_eq!(output, "line1\n\nline2\n");
    }
    
    #[test]
    fn test_compact_removes_trailing_whitespace() {
        let input = "line1   \nline2\t\t";
        let output = DixCompactor::compact(input);
        assert_eq!(output, "line1\nline2\n");
    }
    
    #[test]
    fn test_compression_ratio() {
        let original = "hello world";
        let compressed = "hello";
        let ratio = DixCompactor::get_compression_ratio(original, compressed);
        assert!((ratio - 0.545).abs() < 0.01); // ~54.5% compression
    }
  }
