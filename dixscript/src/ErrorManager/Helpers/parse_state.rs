
/// Parser state tracking for error recovery
#[derive(Debug, Clone)]
pub struct ParseState {
    /// Current index in token stream
    pub index: usize,

    /// Number of consecutive errors encountered
    pub consecutive_errors: usize,

    /// Number of iterations performed
    pub iteration_count: usize,

    /// Last index processed (for stuck detection)
    last_index: usize,
}

impl ParseState {
    /// Create a new parse state
    pub fn new() -> Self {
        ParseState {
            index: 0,
            consecutive_errors: 0,
            iteration_count: 0,
            last_index: usize::MAX, // Use MAX instead of -1 for unsigned
        }
    }

    /// Check if parser is stuck (not making progress)
    pub fn is_stuck(&self) -> bool {
        self.index == self.last_index
    }

    /// Update the last index to current index
    pub fn update_last_index(&mut self) {
        self.last_index = self.index;
    }

    /// Reset error count
    pub fn reset(&mut self) {
        self.consecutive_errors = 0;
    }

    /// Increment error count
    pub fn increment_error(&mut self) {
        self.consecutive_errors += 1;
    }

    /// Increment iteration count
    pub fn increment_iteration(&mut self) {
        self.iteration_count += 1;
    }

    /// Check if parsing should terminate based on error/iteration limits
    pub fn should_terminate(&self, max_errors: usize, max_iterations: usize) -> bool {
        self.consecutive_errors >= max_errors || self.iteration_count >= max_iterations
    }

    /// Advance the index
    pub fn advance(&mut self) {
        self.index += 1;
    }

    /// Advance the index by n positions
    pub fn advance_by(&mut self, n: usize) {
        self.index += n;
    }

    /// Set the index to a specific position
    pub fn set_index(&mut self, index: usize) {
        self.index = index;
    }
}

impl Default for ParseState {
    fn default() -> Self {
        Self::new()
    }
}