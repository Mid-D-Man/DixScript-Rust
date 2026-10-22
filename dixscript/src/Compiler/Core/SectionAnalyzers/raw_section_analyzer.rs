// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionAnalyzers/raw_section_analyzer.rs"
// ============================================================================
//! Semantic validation of `@RAW` blocks.
//!
//! Takes a slice of blocks, not a single section: the cross-block
//! checks (unique `meta_data.id`, unique delimiter tag) need every block in
//! the file visible at once. `meta_data` requires `id`/`format`, type-checks
//! `checksum`/`size` when present. `using` is intentionally not validated
//! against a fixed key set — different `module` decoders need different
//! hint keys.
//!
//! The checks run over the arena form of the blocks:
//! [`analyze_doc`](RawSectionAnalyzer::analyze_doc) reads
//! [`DocRawBlock`]s and the [`Arena`] their values live in, and is what
//! `GeneralSemanticAnalyzer` calls when it has a `Document`.
//! [`analyze`](RawSectionAnalyzer::analyze) keeps the owned-type signature for callers
//! that only have a `DixScript`: it copies the blocks into a scratch arena and runs the
//! same checks, so the two entry points cannot disagree.

use crate::Compiler::AST::arena::{Arena, DocRawBlock, DocRawField, ValueNode};
use crate::Compiler::AST::{RawBlock, Position};
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, SemanticErrorType, DebugConfig};
use crate::Utilities::RustcHash::FxHashMap;

use super::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

const ERROR_MISSING_ID:        &str = "MISSING_ID";
const ERROR_MISSING_FORMAT:    &str = "MISSING_FORMAT";
const ERROR_MISSING_CONTENT:   &str = "MISSING_CONTENT";
const ERROR_DUPLICATE_ID:      &str = "DUPLICATE_ID";
const ERROR_DUPLICATE_TAG:     &str = "DUPLICATE_TAG";
const ERROR_WRONG_TYPE:        &str = "WRONG_FIELD_TYPE";

const WARN_EMPTY_PAYLOAD:      &str = "RAW_WARN001";
const WARN_UNRECOGNIZED_FIELD: &str = "RAW_WARN002";

pub struct RawSectionAnalyzer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl<'a> RawSectionAnalyzer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(operational_settings, ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(
        operational_settings: &'a OperationalSettings,
        error_manager: ErrorManager,
    ) -> Self {
        RawSectionAnalyzer {
            error_manager,
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            operational_settings,
        }
    }

    /// Analyzes every `@RAW` block in the file together — see this file's
    /// top doc comment for why cross-block checks have to happen here
    /// rather than per-block.
    ///
    /// This is the entry point for callers that hold the owned [`RawBlock`] type. It copies
    /// the blocks into a scratch arena and calls [`analyze_doc`](Self::analyze_doc), so the
    /// diagnostics are the ones `analyze_doc` gives for the same blocks.
    ///
    /// # Panics
    ///
    /// Panics if the blocks need more nodes or text than an arena can address (more than
    /// `u32::MAX` of either).
    pub fn analyze(
        &mut self,
        blocks: &[RawBlock],
        symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut arena = Arena::new();
        let doc_blocks: Vec<DocRawBlock> = blocks
            .iter()
            .map(|block| DocRawBlock::from_owned(&mut arena, block))
            .collect();
        self.analyze_doc(&doc_blocks, &arena, symbol_table)
    }

    /// Analyzes every `@RAW` block of a `Document` together.
    ///
    /// `blocks` and `arena` must come from the same `Document` (its `raw` field and its
    /// `arena` field): the blocks hold ids into the arena and have no meaning without it.
    ///
    /// # Examples
    ///
    /// ```
    /// use dixscript::Compiler::AST::arena::{Arena, DocRawBlock};
    /// use dixscript::Compiler::AST::{Position, RawBlock, RawContent, RawField, Value};
    /// use dixscript::Compiler::Core::Config::OperationalSettings;
    /// use dixscript::Compiler::Core::SectionAnalyzers::RawSectionAnalyzer;
    /// use dixscript::Compiler::Utilities::SymbolTable;
    ///
    /// let pos = Position::new(1, 1);
    /// let field = |key: &str, text: &str| RawField::new(
    ///     key.to_string(),
    ///     Value::String { value: text.to_string(), position: pos },
    ///     pos,
    /// );
    /// // Two blocks that share the id "atlas".
    /// let block = |tag: &str| RawBlock::new(
    ///     vec![field("id", "atlas"), field("format", "MPX")],
    ///     vec![],
    ///     Some(RawContent::new(tag.to_string(), 0, 4, pos)),
    ///     pos,
    /// );
    ///
    /// let mut arena = Arena::new();
    /// let blocks = vec![
    ///     DocRawBlock::from_owned(&mut arena, &block("a")),
    ///     DocRawBlock::from_owned(&mut arena, &block("b")),
    /// ];
    ///
    /// let settings = OperationalSettings::default();
    /// let mut analyzer = RawSectionAnalyzer::new(&settings);
    /// let result = analyzer.analyze_doc(&blocks, &arena, &mut SymbolTable::new());
    ///
    /// assert!(!result.is_success);
    /// assert_eq!(result.errors[0].error_id, "RAW007");
    /// ```
    pub fn analyze_doc(
        &mut self,
        blocks: &[DocRawBlock],
        arena: &Arena,
        _symbol_table: &mut SymbolTable,
    ) -> SectionAnalysisResult {
        let mut result = SectionAnalysisResult::new("RAW");

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analyzing RAW section: {} block(s)", blocks.len()
            ));
        }

        // Pass 1: per-block structural checks (required fields, types).
        for block in blocks {
            self.validate_block(block, arena, &mut result);
            if self.should_halt(&result) {
                return result;
            }
        }

        // Pass 2: cross-block uniqueness — needs every block's id/tag
        // already collected, so this only makes sense after pass 1.
        self.check_duplicate_ids(blocks, arena, &mut result);
        self.check_duplicate_tags(blocks, &mut result);

        result.is_success = result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "RAW analysis complete: {} — blocks: {}, errors: {}, warnings: {}",
                if result.is_success { "SUCCESS" } else { "FAILURE" },
                blocks.len(), result.errors.len(), result.warnings.len(),
            ));
        }

        result
    }

    // ═════════════════════════════════════════════════════════════════════
    // Per-block validation
    // ═════════════════════════════════════════════════════════════════════

    fn validate_block(
        &mut self,
        block: &DocRawBlock,
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        match block.id(arena) {
            Some(id) if !id.trim().is_empty() => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!("RAW block '{}': validating", id));
                }
            }
            _ => {
                self.add_error(
                    result, "RAW001", ERROR_MISSING_ID,
                    "@RAW block is missing a required 'meta_data.id' string",
                    "Add id = \"<unique-name>\" to this block's meta_data",
                    Some(block.position),
                );
            }
        }

        if block.format(arena).map(|f| f.trim().is_empty()).unwrap_or(true) {
            self.add_error(
                result, "RAW002", ERROR_MISSING_FORMAT,
                "@RAW block is missing a required 'meta_data.format' string",
                "Add format = \"<engine-defined-format>\" to this block's meta_data — this is the hint the host application dispatches on",
                Some(block.position),
            );
        }

        match &block.content {
            Some(content) => {
                if content.byte_len() == 0 {
                    self.add_warning(
                        result, WARN_EMPTY_PAYLOAD,
                        &format!("@RAW block content (tag '{}') is empty", content.tag),
                        Some(content.position),
                    );
                }
            }
            None => {
                self.add_error(
                    result, "RAW003", ERROR_MISSING_CONTENT,
                    "@RAW block is missing its required 'content' block",
                    "Add content -> { with ---<tag> on its own line, the payload, then ---<tag> on its own line, then }",
                    Some(block.position),
                );
            }
        }

        self.validate_meta_data_types(&block.meta_data, arena, result);
        self.validate_using_types(&block.using, arena, result);
    }

    /// `size` should be numeric, `checksum` should be a string — checked
    /// only when present, since only `id`/`format` are actually required.
    fn validate_meta_data_types(
        &mut self,
        meta_data: &[DocRawField],
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        for field in meta_data {
            let value = arena.value(field.value);
            match field.key.as_str() {
                "size" if !matches!(value, ValueNode::Integer { .. } | ValueNode::Long { .. }) => {
                    self.add_error(
                        result, "RAW004", ERROR_WRONG_TYPE,
                        "'meta_data.size' should be an integer (byte count)",
                        "Use a plain integer, e.g. size = 131072",
                        Some(field.position),
                    );
                }
                "checksum" if !matches!(value, ValueNode::String { .. }) => {
                    self.add_error(
                        result, "RAW005", ERROR_WRONG_TYPE,
                        "'meta_data.checksum' should be a string",
                        "Use a quoted hex string, e.g. checksum = \"0xFA22B1\"",
                        Some(field.position),
                    );
                }
                _ => {}
            }
        }
    }

    /// `using` is intentionally open-ended (different `module` decoders
    /// need different hint keys) — only the well-known keys get a type
    /// check when present; anything else is accepted silently.
    fn validate_using_types(
        &mut self,
        using: &[DocRawField],
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        for field in using {
            let value = arena.value(field.value);
            match field.key.as_str() {
                "threads" if !matches!(value, ValueNode::Integer { .. } | ValueNode::Long { .. }) => {
                    self.add_error(
                        result, "RAW006", ERROR_WRONG_TYPE,
                        "'using.threads' should be an integer",
                        "Use a plain integer, e.g. threads = 4",
                        Some(field.position),
                    );
                }
                "filter" | "compression" | "module"
                    if !matches!(value, ValueNode::String { .. }) =>
                {
                    self.add_warning(
                        result, WARN_UNRECOGNIZED_FIELD,
                        &format!("'using.{}' is usually a string — double-check this value", field.key),
                        Some(field.position),
                    );
                }
                _ => {}
            }
        }
    }

    // ═════════════════════════════════════════════════════════════════════
    // Cross-block uniqueness
    // ═════════════════════════════════════════════════════════════════════

    fn check_duplicate_ids(
        &mut self,
        blocks: &[DocRawBlock],
        arena: &Arena,
        result: &mut SectionAnalysisResult,
    ) {
        let mut seen: FxHashMap<&str, Position> = FxHashMap::default();
        for block in blocks {
            let Some(id) = block.id(arena) else { continue }; // already reported by validate_block
            if let Some(&first_pos) = seen.get(id) {
                self.add_error(
                    result, "RAW007", ERROR_DUPLICATE_ID,
                    &format!(
                        "@RAW meta_data.id '{}' is used by more than one block (first seen at line {})",
                        id, first_pos.line
                    ),
                    "Every @RAW block in a file must have a unique meta_data.id",
                    Some(block.position),
                );
            } else {
                seen.insert(id, block.position);
            }
        }
    }

    fn check_duplicate_tags(&mut self, blocks: &[DocRawBlock], result: &mut SectionAnalysisResult) {
        let mut seen: FxHashMap<&str, Position> = FxHashMap::default();
        for block in blocks {
            let Some(content) = &block.content else { continue }; // already reported by validate_block
            if let Some(&first_pos) = seen.get(content.tag.as_str()) {
                self.add_error(
                    result, "RAW008", ERROR_DUPLICATE_TAG,
                    &format!(
                        "@RAW content delimiter tag '{}' is used by more than one block (first seen at line {})",
                        content.tag, first_pos.line
                    ),
                    "Every @RAW block's content delimiter tag must be unique across the whole file",
                    Some(content.position),
                );
            } else {
                seen.insert(content.tag.as_str(), content.position);
            }
        }
    }

    // ═════════════════════════════════════════════════════════════════════
    // Helpers
    // ═════════════════════════════════════════════════════════════════════

    #[inline]
    fn should_halt(&self, result: &SectionAnalysisResult) -> bool {
        !result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn add_error(
        &mut self,
        result: &mut SectionAnalysisResult,
        error_id: &str,
        error_type: &str,
        message: &str,
        suggestion: &str,
        position: Option<Position>,
    ) {
        result.errors.push(SemanticErrorInfo {
            error_id:     error_id.to_string(),
            error_type:   error_type.to_string(),
            message:      message.to_string(),
            section_name: "RAW".to_string(),
            suggestion:   suggestion.to_string(),
            position,
        });

        let (line, col) = position.map(|p| (p.line as i32, p.column as i32)).unwrap_or((0, 0));
        self.error_manager.add_semantic_error(
            SemanticErrorType::DuplicateDefinition,
            message.to_string(),
            line, col,
            Some("RAW".to_string()),
            Some(suggestion.to_string()),
        );
    }

    fn add_warning(
        &mut self,
        result: &mut SectionAnalysisResult,
        warning_id: &str,
        message: &str,
        position: Option<Position>,
    ) {
        result.warnings.push(SemanticWarningInfo {
            warning_id:   warning_id.to_string(),
            message:      message.to_string(),
            section_name: "RAW".to_string(),
            position,
        });
        if self.debug_config.is_enabled {
            self.error_manager.log_warning(message);
        }
    }
}
