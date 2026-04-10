// mdix-lsp/src/features/inlay_hints.rs

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use dixscript::Compiler::AST::DataEntry;
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<Vec<InlayHint>> {
    let doc  = doc?;
    let ast  = doc.ast.as_ref()?;
    let data = ast.data.as_ref()?;

    let type_index = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.type_index.as_ref());

    let mut hints = Vec::new();

    for entry in &data.entries {
        // Use explicit `ref` bindings so we never move out of the borrowed
        // DataEntry, which fixes E0382 ("value used after move") when match
        // ergonomics don't kick in automatically for this enum variant.
        if let DataEntry::SimpleProperty {
            ref name,
            data_type: None,
            value: _,
            ref position,
        } = *entry
        {
            let type_label = type_index
                .and_then(|idx| idx.get(name.as_str()))
                .map(|dt| format!(": {:?}", dt).to_lowercase())
                .unwrap_or_else(|| ": ?".to_string());

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

    if hints.is_empty() { None } else { Some(hints) }
}
