//! Centralized path builder for DATA section variable paths
//!
//! Ensures consistent path construction across parsers, analyzers, and runtime
//!
//! ## Rules:
//! - All paths start with "DATA"
//! - Properties use dot notation: DATA.property
//! - Table paths use dots: DATA.table.subtable.property
//! - Array indices have NO dot prefix: DATA.array[0] NOT DATA.array.[0]
//! - Array item properties: DATA.array[0].property
//!
//! ## Examples:
//!
//! DATA.simple_host
//! DATA.server.config.host
//! DATA.inventory.items[0]
//! DATA.inventory.items[0].name
//! 

const ROOT: &str = "DATA";

/// PathBuilder - static utility for building DATA section paths
pub struct PathBuilder;

impl PathBuilder {
    // ==================== BASIC PATH BUILDING ====================
    
    /// Build a DATA path from segments
    /// Automatically handles array notation (segments starting with '[')
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::build(&["server", "host"]);
    /// // Returns: "DATA.server.host"
    ///
    /// let path = PathBuilder::build(&["items", "[0]", "name"]);
    /// // Returns: "DATA.items[0].name"
    /// 
    pub fn build(segments: &[&str]) -> String {
        if segments.is_empty() {
            return ROOT.to_string();
        }
        
        let mut result = String::from(ROOT);
        
        for segment in segments {
            if segment.is_empty() {
                continue;
            }
            
            // Array index - no dot prefix
            if segment.starts_with('[') && segment.ends_with(']') {
                result.push_str(segment);
            } else {
                // Regular property - add dot
                result.push('.');
                result.push_str(segment);
            }
        }
        
        result
    }
    
    /// Build path from a base path and additional segments
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::build_from("DATA.server", &["config", "host"]);
    /// // Returns: "DATA.server.config.host"
    /// 
    pub fn build_from(base_path: &str, segments: &[&str]) -> String {
        if base_path.is_empty() {
            return Self::build(segments);
        }
        
        if segments.is_empty() {
            return base_path.to_string();
        }
        
        let mut result = String::from(base_path);
        
        for segment in segments {
            if segment.is_empty() {
                continue;
            }
            
            if segment.starts_with('[') && segment.ends_with(']') {
                result.push_str(segment);
            } else {
                result.push('.');
                result.push_str(segment);
            }
        }
        
        result
    }
    
    /// Build path from current scope tracker state
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::build_from_scope("DATA", "server");
    /// // Returns: "DATA.server"
    ///
    /// let path = PathBuilder::build_from_scope("DATA.server", "host");
    /// // Returns: "DATA.server.host"
    /// 
    pub fn build_from_scope(current_path: &str, property: &str) -> String {
        if current_path == ROOT {
            Self::build(&[property])
        } else {
            Self::build_from(current_path, &[property])
        }
    }
    
    // ==================== ARRAY PATH BUILDING ====================
    
    /// Build array item path
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::build_array_item("DATA.inventory.items", 0);
    /// // Returns: "DATA.inventory.items[0]"
    /// 
    pub fn build_array_item(array_path: &str, index: usize) -> String {
        format!("{}[{}]", array_path, index)
    }
    
    /// Build array item property path
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::build_array_item_property("DATA.inventory.items", 0, "name");
    /// // Returns: "DATA.inventory.items[0].name"
    /// 
    pub fn build_array_item_property(array_path: &str, index: usize, property: &str) -> String {
        Self::build_from(&Self::build_array_item(array_path, index), &[property])
    }
    
    // ==================== PATH MANIPULATION ====================
    
    /// Remove "DATA." prefix if present
    ///
    /// # Examples
    /// 
    /// let stripped = PathBuilder::strip_root("DATA.server.host");
    /// // Returns: "server.host"
    ///
    /// let stripped = PathBuilder::strip_root("DATA");
    /// // Returns: ""
    /// 
    pub fn strip_root(path: &str) -> String {
        if path.starts_with(ROOT) {
            if path.len() == ROOT.len() {
                String::new()
            } else if path.chars().nth(ROOT.len()) == Some('.') {
                path[ROOT.len() + 1..].to_string()
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        }
    }
    
    /// Ensure path has "DATA." prefix
    ///
    /// # Examples
    /// 
    /// let path = PathBuilder::ensure_root("server.host");
    /// // Returns: "DATA.server.host"
    ///
    /// let path = PathBuilder::ensure_root("DATA.server.host");
    /// // Returns: "DATA.server.host" (unchanged)
    /// 
    pub fn ensure_root(path: &str) -> String {
        if path.is_empty() {
            ROOT.to_string()
        } else if path.starts_with(ROOT) {
            path.to_string()
        } else {
            Self::build(&[path])
        }
    }
    
    /// Get the last segment of a path
    ///
    /// # Examples
    /// 
    /// let segment = PathBuilder::get_last_segment("DATA.server.config.host");
    /// // Returns: "host"
    ///
    /// let segment = PathBuilder::get_last_segment("DATA.items[0]");
    /// // Returns: "[0]"
    /// 
    pub fn get_last_segment(path: &str) -> String {
        if path.is_empty() {
            return String::new();
        }
        
        // Handle array notation: DATA.items[0] → "[0]"
        if let Some(last_bracket) = path.rfind('[') {
            if path.ends_with(']') {
                return path[last_bracket..].to_string();
            }
        }
        
        // Handle dot notation
        if let Some(last_dot) = path.rfind('.') {
            path[last_dot + 1..].to_string()
        } else {
            path.to_string()
        }
    }
    
    /// Get parent path by removing last segment
    ///
    /// # Examples
    /// 
    /// let parent = PathBuilder::get_parent("DATA.server.config.host");
    /// // Returns: "DATA.server.config"
    ///
    /// let parent = PathBuilder::get_parent("DATA.items[0].name");
    /// // Returns: "DATA.items[0]"
    ///
    /// let parent = PathBuilder::get_parent("DATA");
    /// // Returns: "DATA"
    /// 
    pub fn get_parent(path: &str) -> String {
        if path.is_empty() || path == ROOT {
            return ROOT.to_string();
        }
        
        // Handle array notation: DATA.items[0].name → DATA.items[0]
        if let Some(last_dot) = path.rfind('.') {
            if last_dot <= ROOT.len() {
                ROOT.to_string()
            } else {
                path[..last_dot].to_string()
            }
        } else {
            ROOT.to_string()
        }
    }
    
    // ==================== PATH PARSING ====================
    
    /// Split path into segments (excluding ROOT)
    ///
    /// # Examples
    /// 
    /// let segments = PathBuilder::get_segments("DATA.server.config.host");
    /// // Returns: ["server", "config", "host"]
    ///
    /// let segments = PathBuilder::get_segments("DATA.items[0].name");
    /// // Returns: ["items", "[0]", "name"]
    /// 
    pub fn get_segments(path: &str) -> Vec<String> {
        let stripped = Self::strip_root(path);
        
        if stripped.is_empty() {
            return Vec::new();
        }
        
        let mut segments = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = stripped.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let c = chars[i];
            
            if c == '.' {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
            } else if c == '[' {
                // Add accumulated segment before array index
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
                
                // Find matching closing bracket
                let mut bracket_content = String::from("[");
                i += 1;
                while i < chars.len() && chars[i] != ']' {
                    bracket_content.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    bracket_content.push(']');
                }
                segments.push(bracket_content);
            } else {
                current.push(c);
            }
            
            i += 1;
        }
        
        if !current.is_empty() {
            segments.push(current);
        }
        
        segments
    }
    
    // ==================== PATH QUERIES ====================
    
    /// Check if path represents an array index
    ///
    /// # Examples
    /// 
    /// assert!(PathBuilder::is_array_index("DATA.items[0]"));
    /// assert!(!PathBuilder::is_array_index("DATA.items"));
    /// 
    pub fn is_array_index(path: &str) -> bool {
        path.contains('[') && path.ends_with(']')
    }
    
    /// Extract table path from property path
    ///
    /// # Examples
    /// 
    /// let table = PathBuilder::get_table_path("DATA.server.config.host");
    /// // Returns: "DATA.server.config"
    ///
    /// let table = PathBuilder::get_table_path("DATA.simple");
    /// // Returns: "DATA"
    /// 
    pub fn get_table_path(property_path: &str) -> String {
        let segments = Self::get_segments(property_path);
        
        if segments.len() <= 1 {
            return ROOT.to_string();
        }
        
        // Remove last segment (property name)
        let table_segments: Vec<&str> = segments[..segments.len() - 1]
            .iter()
            .map(|s| s.as_str())
            .collect();
        
        Self::build(&table_segments)
    }
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_build_simple() {
        assert_eq!(PathBuilder::build(&["server", "host"]), "DATA.server.host");
    }
    
    #[test]
    fn test_build_with_array() {
        assert_eq!(PathBuilder::build(&["items", "[0]"]), "DATA.items[0]");
        assert_eq!(PathBuilder::build(&["items", "[0]", "name"]), "DATA.items[0].name");
    }
    
    #[test]
    fn test_build_from() {
        assert_eq!(
            PathBuilder::build_from("DATA.server", &["config", "host"]),
            "DATA.server.config.host"
        );
    }
    
    #[test]
    fn test_strip_root() {
        assert_eq!(PathBuilder::strip_root("DATA.server.host"), "server.host");
        assert_eq!(PathBuilder::strip_root("DATA"), "");
    }
    
    #[test]
    fn test_ensure_root() {
        assert_eq!(PathBuilder::ensure_root("server.host"), "DATA.server.host");
        assert_eq!(PathBuilder::ensure_root("DATA.server.host"), "DATA.server.host");
    }
    
    #[test]
    fn test_get_last_segment() {
        assert_eq!(PathBuilder::get_last_segment("DATA.server.config.host"), "host");
        assert_eq!(PathBuilder::get_last_segment("DATA.items[0]"), "[0]");
    }
    
    #[test]
    fn test_get_parent() {
        assert_eq!(PathBuilder::get_parent("DATA.server.config.host"), "DATA.server.config");
        assert_eq!(PathBuilder::get_parent("DATA.items[0].name"), "DATA.items[0]");
        assert_eq!(PathBuilder::get_parent("DATA"), "DATA");
    }
    
    #[test]
    fn test_get_segments() {
        assert_eq!(
            PathBuilder::get_segments("DATA.server.config.host"),
            vec!["server", "config", "host"]
        );
        assert_eq!(
            PathBuilder::get_segments("DATA.items[0].name"),
            vec!["items", "[0]", "name"]
        );
    }
    
    #[test]
    fn test_is_array_index() {
        assert!(PathBuilder::is_array_index("DATA.items[0]"));
        assert!(!PathBuilder::is_array_index("DATA.items"));
    }
    
    #[test]
    fn test_get_table_path() {
        assert_eq!(
            PathBuilder::get_table_path("DATA.server.config.host"),
            "DATA.server.config"
        );
        assert_eq!(
            PathBuilder::get_table_path("DATA.simple"),
            "DATA"
        );
    }
  }
