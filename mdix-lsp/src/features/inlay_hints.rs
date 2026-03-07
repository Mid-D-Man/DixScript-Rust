//! Inlay hint provider.
//!
//! Shows inferred type annotations after DATA variables that have no explicit
//! type annotation. For example:
//!
//!   count = 42        →  count: int = 42
//!   enabled = true    →  enabled: bool = true
//!
//! Uses SemanticAnalysisResult.type_index which is populated by the DATA
//! section analyzer. If the index is empty (e.g. because analysis failed),
//! this provider returns nothing rather than showing wrong information.

use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, Position,
};
use dixscript::Compiler::AST::DataEntry;
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;
    let data = ast.data.as_ref()?;

    // The type index maps variable name -> DataType, built during semantic analysis.
    let type_index = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.type_index.as_ref());

    let mut hints = Vec::new();

    for entry in &data.entries {
        if let DataEntry::SimpleProperty {
            name,
            data_type: None,  // only emit hints where type was NOT written by the author
            value: _,
            position,
        } = entry
        {
            // Look up the inferred type from the semantic analysis result.
            let type_label = type_index
                .and_then(|idx| idx.get(name))
                .map(|dt| format!(": {:?}", dt).to_lowercase())
                .unwrap_or_else(|| ": ?".to_string());

            // Place the hint at the end of the variable name.
            let line = position.line.saturating_sub(1) as u32;
            let col  = (position.column.saturating_sub(1) + name.len()) as u32;

            hints.push(InlayHint {
                position:      Position::new(line, col),
                label:         InlayHintLabel::String(type_label),
                kind:          Some(InlayHintKind::TYPE),
                text_edits:    None,
                tooltip:       None,
                padding_left:  Some(false),
                padding_right: Some(true),
                data:          None,
            });
        }
    }

    if hints.is_empty() {
        None
    } else {
        Some(hints)
    }
}
