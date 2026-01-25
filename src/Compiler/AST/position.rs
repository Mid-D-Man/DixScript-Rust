/// Position tracking for AST nodes
///
/// Uses Copy trait since it's only 2 usizes (16 bytes on 64-bit)
/// - Zero-cost to pass by value
/// - No cloning overhead
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    /// Unknown position (when position info unavailable)
    pub const UNKNOWN: Self = Position { line: 0, column: 0 };

    /// Start of file position
    pub const START: Self = Position { line: 1, column: 1 };

    /// Create new position
    #[inline]
    pub const fn new(line: usize, column: usize) -> Self {
        Position { line, column }
    }

    /// Check if position is valid (not UNKNOWN)
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.line > 0 && self.column > 0
    }

    /// Check if position is unknown
    #[inline]
    pub const fn is_unknown(&self) -> bool {
        self.line == 0 && self.column == 0
    }

    /// Create position from token (will be used by parser)
    pub fn from_token(token: &crate::Compiler::Core::Tokenizer::Token) -> Self {
        Position {
            line: token.line,
            column: token.column,
        }
    }

    /// Human-readable position string
    pub fn to_short_string(&self) -> String {
        if self.is_unknown() {
            "??:??".to_string()
        } else {
            format!("{}:{}", self.line, self.column)
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Position::UNKNOWN
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_unknown() {
            write!(f, "Unknown Position")
        } else {
            write!(f, "Line {}, Column {}", self.line, self.column)
        }
    }
}