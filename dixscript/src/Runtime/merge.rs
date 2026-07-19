//! AST-level merging of two or more DixScript databases.
//!
//! Works directly on `DixScript` AST nodes — no JSON round-trip, no information
//! loss through type flattening.  All DixScript types (Long, Float, Double,
//! ScientificNotation, EnumValue, etc.) survive the merge exactly as-is.
//!
//! ## Conflict resolution
//!
//! When two sources define the same key, the configured `MdixMergeStrategy`
//! picks the winner:
//!
//! - `WeightedPriority` (default) — each source carries a `weight` in `[0.0, 1.0]`;
//!   higher weight wins.  Equal-weight ties fall back to primary (lower index) wins.
//! - `PrimaryWins` — the source with the lower index always wins.
//! - `SecondaryWins` — the source with the higher index always wins.
//! - `ThrowOnConflict` — any conflict returns `Err`.
//!
//! ## Two-tier ordering guarantee
//!
//! The merged `@DATA` section always places:
//!   - Tier 1 (`SimpleProperty` + `ObjectProperty`) before
//!   - Tier 2 (`TableProperty` + `GroupArray`)
//!
//! This invariant is enforced by construction, regardless of input ordering.
//!
//! ## Deep merging
//!
//! `TableProperty` entries sharing the same path are deep-merged at the
//! property level (field-by-field conflict resolution).  `SecurityEntry` blocks
//! sharing the same block key are deep-merged at the field level.  `EnumDeclaration`
//! sharing the same name are deep-merged at the field level.
//!
//! ## Array deduplication
//!
//! `GroupArray` entries sharing the same path are combined according to
//! `ArrayMergeStrategy` (default: `ConcatDedup` — winner's items first,
//! exact-duplicate primitive values removed).

use std::collections::HashMap;

use crate::Compiler::AST::{
    ConfigEntry, ConfigSection,
    DataEntry, DataSection,
    DixScript,
    DLMModule, DLMModuleType, DLMSection,
    EnumDeclaration, EnumField, EnumsSection,
    ImportDeclaration, ImportsSection,
    Position,
    PropertyAssignment,
    QuickFunction, QuickFuncsSection,
    SecurityEntry, SecurityField, SecuritySection,
    TablePath,
    Value,
};

// ── Strategy enums ────────────────────────────────────────────────────────────

/// How to pick the winner when two sources define the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum MdixMergeStrategy {
    /// The source with the highest `weight` wins.  Equal-weight ties go to
    /// the source with the lower index (primary).
    #[default]
    WeightedPriority,
    /// The source with the lower index (earliest added) always wins.
    PrimaryWins,
    /// The source with the higher index (latest added) always wins.
    SecondaryWins,
    /// Any key present in more than one source produces an error.
    ThrowOnConflict,
}


/// How to combine two `GroupArray` (or array-valued) entries that share a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum ArrayMergeStrategy {
    /// Winner's array entirely replaces the loser's.
    Replace,
    /// Both arrays are concatenated — winner's items first.
    Concat,
    /// Both arrays are concatenated (winner first); exact-duplicate primitive
    /// values are removed.  Complex values (Object, Array) are never deduped.
    #[default]
    ConcatDedup,
}


// ── Input / output types ──────────────────────────────────────────────────────

/// A single AST to be merged, together with its priority weight.
///
/// Higher `weight` beats lower `weight` under `WeightedPriority`.
/// `weight` is ignored for `PrimaryWins` and `SecondaryWins`.
#[derive(Debug, Clone)]
pub struct MdixMergeInput {
    pub ast: DixScript,
    /// Priority in `[0.0, 1.0]`.  Values outside the range are clamped.
    pub weight: f64,
    /// Optional human-readable label used in conflict reports and error messages.
    pub label: Option<String>,
}

impl MdixMergeInput {
    /// Create a new input with default weight `0.5`.
    pub fn new(ast: DixScript) -> Self {
        MdixMergeInput { ast, weight: 0.5, label: None }
    }

    /// Set the priority weight (clamped to `[0.0, 1.0]`).
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Attach a human-readable label (used in conflict reports).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A recorded conflict and which source won.
#[derive(Debug, Clone)]
pub struct MergeConflict {
    /// Dotted path of the conflicting key, e.g. `"DATA.server.host"`.
    pub path: String,
    /// 0-based index of the winning source in the input slice.
    pub winning_source: usize,
    /// Label of the winning source (if one was provided).
    pub winning_label: Option<String>,
}

impl std::fmt::Display for MergeConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.winning_label {
            Some(label) => write!(
                f,
                "[Conflict] '{}' → source[{}] ('{}') won",
                self.path, self.winning_source, label
            ),
            None => write!(
                f,
                "[Conflict] '{}' → source[{}] won",
                self.path, self.winning_source
            ),
        }
    }
}

/// The result of a merge operation.
#[derive(Debug)]
pub struct MdixMergeResult {
    /// The merged AST.
    pub merged_ast: DixScript,
    /// Every conflict that was resolved during the merge.
    pub conflicts: Vec<MergeConflict>,
    /// `true` when no fatal errors occurred.
    pub is_success: bool,
    /// Fatal error messages (non-empty only under `ThrowOnConflict` or bad inputs).
    pub errors: Vec<String>,
}

impl MdixMergeResult {
    /// Unwrap the merged AST, panicking with all errors joined if the merge failed.
    pub fn unwrap(self) -> DixScript {
        if !self.is_success {
            panic!("MdixMerge failed: {}", self.errors.join("; "));
        }
        self.merged_ast
    }

    /// Convert to `Result`, mapping errors to a joined string.
    pub fn into_result(self) -> Result<(DixScript, Vec<MergeConflict>), String> {
        if self.is_success {
            Ok((self.merged_ast, self.conflicts))
        } else {
            Err(self.errors.join("; "))
        }
    }
}

// ── MdixMerger ────────────────────────────────────────────────────────────────

/// AST-level merger for DixScript databases.
///
/// ```rust,ignore
/// use dixscript::Runtime::merge::{MdixMerger, MdixMergeInput, MdixMergeStrategy};
///
/// let result = MdixMerger::new()
///     .with_strategy(MdixMergeStrategy::WeightedPriority)
///     .merge_all(vec![
///         MdixMergeInput::new(ast_base).with_weight(1.0).with_label("base"),
///         MdixMergeInput::new(ast_patch).with_weight(0.8).with_label("patch"),
///     ]);
///
/// if result.is_success {
///     println!("Merged with {} conflict(s)", result.conflicts.len());
/// }
/// ```
pub struct MdixMerger {
    strategy:       MdixMergeStrategy,
    array_strategy: ArrayMergeStrategy,
}

impl MdixMerger {
    pub fn new() -> Self {
        MdixMerger {
            strategy:       MdixMergeStrategy::default(),
            array_strategy: ArrayMergeStrategy::default(),
        }
    }

    /// Set the conflict resolution strategy.
    pub fn with_strategy(mut self, strategy: MdixMergeStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the array merge strategy (applies to `GroupArray` entries and
    /// array-valued `SimpleProperty` entries that share a key).
    pub fn with_array_strategy(mut self, strategy: ArrayMergeStrategy) -> Self {
        self.array_strategy = strategy;
        self
    }

    // ── Public entry points ───────────────────────────────────────────────────

    /// Merge exactly two ASTs.
    pub fn merge(&self, primary: MdixMergeInput, secondary: MdixMergeInput) -> MdixMergeResult {
        self.merge_all(vec![primary, secondary])
    }

    /// Merge two or more ASTs.
    ///
    /// Sources are processed in order; index 0 is always "primary" for
    /// tie-breaking when weights are equal under `WeightedPriority`, and
    /// is always the winner under `PrimaryWins`.
    pub fn merge_all(&self, sources: Vec<MdixMergeInput>) -> MdixMergeResult {
        if sources.is_empty() {
            return MdixMergeResult {
                merged_ast: DixScript::new(),
                conflicts:  vec![],
                is_success: false,
                errors:     vec!["merge_all: no sources provided".to_string()],
            };
        }
        if sources.len() == 1 {
            let s = sources.into_iter().next().unwrap();
            return MdixMergeResult {
                merged_ast: s.ast,
                conflicts:  vec![],
                is_success: true,
                errors:     vec![],
            };
        }

        let mut conflicts = Vec::new();
        let mut errors    = Vec::new();

        let merged_ast = self.do_merge(&sources, &mut conflicts, &mut errors);

        let is_success = errors.is_empty();
        MdixMergeResult { merged_ast, conflicts, is_success, errors }
    }

    // ── Core orchestrator ─────────────────────────────────────────────────────

    fn do_merge(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> DixScript {
        DixScript {
            config:          self.merge_config(sources, conflicts, errors),
            imports:         self.merge_imports(sources, conflicts, errors),
            dlm:             self.merge_dlm(sources),
            enums:           self.merge_enums(sources, conflicts, errors),
            quick_functions: self.merge_quickfuncs(sources, conflicts, errors),
            data:            self.merge_data(sources, conflicts, errors),
            security:        self.merge_security(sources, conflicts, errors),
        }
    }

    // ── @CONFIG ───────────────────────────────────────────────────────────────

    fn merge_config(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<ConfigSection> {
        let present: Vec<(usize, &ConfigSection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.config.as_ref().map(|c| (i, c)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // key → (winning_source_idx, ConfigEntry)
        let mut key_map: HashMap<String, (usize, ConfigEntry)> = HashMap::new();
        // Preserve insertion order so output matches primary source order.
        let mut key_order: Vec<String> = Vec::new();

        for (src_idx, section) in &present {
            for entry in &section.entries {
                let key = entry.key.clone();
                if let Some((existing_idx, existing_entry)) = key_map.get(&key) {
                    // Identical value on both sides is not a real conflict — every
                    // parsed file gets the same auto-populated minimal-config
                    // defaults (version, debug_mode, ...) when the user never wrote
                    // an explicit @CONFIG block, so this key collides on *every*
                    // multi-source merge regardless of @DATA content. Without this
                    // guard, ThrowOnConflict raised unconditionally on every merge.
                    if existing_entry.value == entry.value {
                        continue;
                    }
                    let existing_idx = *existing_idx;
                    if let Some(winner) = self.resolve_conflict(
                        format!("CONFIG.{}", key),
                        *src_idx, existing_idx, sources, conflicts, errors,
                    ) {
                        if winner == *src_idx {
                            key_map.insert(key, (*src_idx, entry.clone()));
                        }
                    }
                } else {
                    key_order.push(key.clone());
                    key_map.insert(key, (*src_idx, entry.clone()));
                }
            }
        }

        if !errors.is_empty() {
            return None;
        }

        let mut entries = Vec::with_capacity(key_order.len());
        for key in &key_order {
            if let Some((_, entry)) = key_map.get(key) {
                entries.push(entry.clone());
            }
        }

        Some(ConfigSection { entries, position: Position::UNKNOWN })
    }

    // ── @IMPORTS ──────────────────────────────────────────────────────────────

    fn merge_imports(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<ImportsSection> {
        let present: Vec<(usize, &ImportsSection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.imports.as_ref().map(|imp| (i, imp)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // alias → (source_idx, ImportDeclaration)
        let mut alias_map: HashMap<String, (usize, ImportDeclaration)> = HashMap::new();
        let mut alias_order: Vec<String> = Vec::new();

        for (src_idx, section) in &present {
            for import in &section.imports {
                let alias = import.alias.clone();
                if let Some((existing_idx, existing_import)) = alias_map.get(&alias) {
                    // Identical alias + identical path + same cloud flag → harmless dup, skip.
                    if existing_import.path == import.path
                        && existing_import.is_cloud_import == import.is_cloud_import
                    {
                        continue;
                    }
                    // Different target — real conflict.
                    let existing_idx = *existing_idx;
                    if let Some(winner) = self.resolve_conflict(
                        format!("IMPORTS.{}", alias),
                        *src_idx, existing_idx, sources, conflicts, errors,
                    ) {
                        if winner == *src_idx {
                            alias_map.insert(alias, (*src_idx, import.clone()));
                        }
                    }
                } else {
                    alias_order.push(alias.clone());
                    alias_map.insert(alias, (*src_idx, import.clone()));
                }
            }
        }

        if !errors.is_empty() || alias_map.is_empty() {
            return if errors.is_empty() { None } else { None };
        }

        let mut imports = Vec::with_capacity(alias_order.len());
        for alias in &alias_order {
            if let Some((_, imp)) = alias_map.get(alias) {
                imports.push(imp.clone());
            }
        }

        Some(ImportsSection { imports, position: Position::UNKNOWN })
    }

    // ── @DLM ─────────────────────────────────────────────────────────────────

    fn merge_dlm(&self, sources: &[MdixMergeInput]) -> Option<DLMSection> {
        let present: Vec<&DLMSection> = sources
            .iter()
            .filter_map(|s| s.ast.dlm.as_ref())
            .collect();

        if present.is_empty() {
            return None;
        }

        // One module per module-type category; first occurrence (primary) wins.
        let mut type_map: HashMap<u8, DLMModule> = HashMap::new();

        for section in &present {
            for module in &section.modules {
                let key = dlm_module_type_key(module.module_type);
                type_map.entry(key).or_insert_with(|| module.clone());
            }
        }

        if type_map.is_empty() {
            return None;
        }

        // Canonical pipeline order: compress → audit → encrypt.
        let mut modules: Vec<DLMModule> = type_map.into_values().collect();
        modules.sort_by_key(|m| match m.module_type {
            DLMModuleType::DCompressor => 0u8,
            DLMModuleType::DAuditor    => 1,
            DLMModuleType::DEncryptor  => 2,
            DLMModuleType::ParseError  => 3,
        });

        Some(DLMSection { modules, position: Position::UNKNOWN })
    }

    // ── @ENUMS ────────────────────────────────────────────────────────────────

    fn merge_enums(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<EnumsSection> {
        let present: Vec<(usize, &EnumsSection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.enums.as_ref().map(|e| (i, e)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // name → (winning_source_idx, EnumDeclaration)
        let mut enum_map: HashMap<String, (usize, EnumDeclaration)> = HashMap::new();
        let mut enum_order: Vec<String> = Vec::new();

        for (src_idx, section) in &present {
            for decl in &section.enums {
                if enum_map.contains_key(&decl.name) {
                    let existing_src_idx = enum_map[&decl.name].0;
                    let existing_fields  = enum_map[&decl.name].1.fields.clone();

                    // Same enum name, and every field already carries the same
                    // value on both sides — not a real conflict. Compare only
                    // `.value` (like merge_enum_fields already does below), not
                    // the whole EnumField, since each field's `position` will
                    // always differ across two independently-parsed sources.
                    let fields_identical = existing_fields.len() == decl.fields.len()
                        && existing_fields.iter().all(|ef| {
                            decl.fields.iter().any(|df| df.name == ef.name && df.value == ef.value)
                        });

                    if fields_identical {
                        continue;
                    }

                    // Deep-merge: same enum name, merge fields.
                    let Some(winner) = self.resolve_conflict(
                        format!("ENUMS.{}", decl.name),
                        *src_idx, existing_src_idx, sources, conflicts, errors,
                    ) else { continue; };

                    let merged_fields = self.merge_enum_fields(
                        &existing_fields, existing_src_idx,
                        &decl.fields, *src_idx,
                        winner, sources, conflicts, errors,
                        &decl.name,
                    );

                    let entry = enum_map.get_mut(&decl.name).unwrap();
                    entry.0 = winner;
                    entry.1.fields = merged_fields;
                } else {
                    enum_order.push(decl.name.clone());
                    enum_map.insert(decl.name.clone(), (*src_idx, decl.clone()));
                }
            }
        }

        if !errors.is_empty() || enum_map.is_empty() {
            return None;
        }

        let mut enums = Vec::with_capacity(enum_order.len());
        for name in &enum_order {
            if let Some((_, decl)) = enum_map.get(name) {
                enums.push(decl.clone());
            }
        }

        Some(EnumsSection { enums, position: Position::UNKNOWN })
    }

    fn merge_enum_fields(
        &self,
        primary_fields:   &[EnumField],
        primary_src:      usize,
        secondary_fields: &[EnumField],
        secondary_src:    usize,
        winner:           usize,
        sources:          &[MdixMergeInput],
        conflicts:        &mut Vec<MergeConflict>,
        errors:           &mut Vec<String>,
        enum_name:        &str,
    ) -> Vec<EnumField> {
        // name → (source_idx, EnumField)
        let mut field_map: HashMap<String, (usize, EnumField)> = HashMap::new();
        let mut field_order: Vec<String> = Vec::new();

        for field in primary_fields {
            field_order.push(field.name.clone());
            field_map.insert(field.name.clone(), (primary_src, field.clone()));
        }

        for field in secondary_fields {
            if let Some((existing_src, existing_field)) = field_map.get(&field.name) {
                // Only a conflict if the integer values differ.
                if existing_field.value != field.value {
                    let existing_src = *existing_src;
                    if let Some(field_winner) = self.resolve_conflict(
                        format!("ENUMS.{}.{}", enum_name, field.name),
                        secondary_src, existing_src, sources, conflicts, errors,
                    ) {
                        if field_winner == secondary_src {
                            field_map.insert(field.name.clone(), (secondary_src, field.clone()));
                        }
                    }
                }
                // Identical value → silent dedup, no conflict recorded.
            } else {
                // New field only in secondary → always add.
                field_order.push(field.name.clone());
                field_map.insert(field.name.clone(), (secondary_src, field.clone()));
            }
        }

        let mut result = Vec::with_capacity(field_order.len());
        for name in &field_order {
            if let Some((_, f)) = field_map.get(name) {
                result.push(f.clone());
            }
        }
        result
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────

    fn merge_quickfuncs(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<QuickFuncsSection> {
        let present: Vec<(usize, &QuickFuncsSection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.quick_functions.as_ref().map(|q| (i, q)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // name → (winning_source_idx, QuickFunction)
        let mut func_map: HashMap<String, (usize, QuickFunction)> = HashMap::new();
        let mut func_order: Vec<String> = Vec::new();

        for (src_idx, section) in &present {
            for func in &section.functions {
                if let Some((existing_idx, existing_func)) = func_map.get(&func.name) {
                    // Identical signature + body on both sides is not a real
                    // conflict. NOTE: unlike CONFIG/ENUMS above, this comparison
                    // includes each QuickFuncStatement's own `position`, so in
                    // practice this only dedups trivially-identical (e.g. empty
                    // body) functions parsed from otherwise-different source
                    // text — a real identical function body from two distinct
                    // source files will still (correctly) be flagged, since we
                    // can't cheaply prove text-identity without positions here.
                    let identical = existing_func.return_type == func.return_type
                        && existing_func.scope_list == func.scope_list
                        && existing_func.parameters == func.parameters
                        && existing_func.body == func.body;
                    if identical {
                        continue;
                    }
                    match self.resolve_conflict(
                        format!("QUICKFUNCS.{}", func.name),
                        *src_idx, *existing_idx, sources, conflicts, errors,
                    ) {
                        Some(winner) if winner == *src_idx => {
                            func_map.insert(func.name.clone(), (*src_idx, func.clone()));
                        }
                        _ => {}
                    }
                } else {
                    func_order.push(func.name.clone());
                    func_map.insert(func.name.clone(), (*src_idx, func.clone()));
                }
            }
        }

        if !errors.is_empty() || func_map.is_empty() {
            return None;
        }

        let mut functions = Vec::with_capacity(func_order.len());
        for name in &func_order {
            if let Some((_, func)) = func_map.get(name) {
                functions.push(func.clone());
            }
        }

        Some(QuickFuncsSection { functions, position: Position::UNKNOWN })
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────

    fn merge_data(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<DataSection> {
        let present: Vec<(usize, &DataSection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.data.as_ref().map(|d| (i, d)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // ── Per-tier accumulators ─────────────────────────────────────────────
        // Tier 1: flat (SimpleProperty, ObjectProperty)
        let mut simple_map:   HashMap<String, (usize, DataEntry)> = HashMap::new();
        let mut simple_order: Vec<String>                          = Vec::new();
        let mut object_map:   HashMap<String, (usize, DataEntry)> = HashMap::new();
        let mut object_order: Vec<String>                          = Vec::new();

        // Tier 2: grouped (TableProperty, GroupArray)
        let mut table_map:   HashMap<String, (usize, Vec<PropertyAssignment>)> = HashMap::new();
        let mut table_order: Vec<String>                                        = Vec::new();
        let mut array_map:   HashMap<String, (usize, Vec<Value>)>              = HashMap::new();
        let mut array_order: Vec<String>                                        = Vec::new();

        for (src_idx, section) in &present {
            for entry in &section.entries {
                match entry {
                    DataEntry::SimpleProperty { name, .. } => {
                        self.upsert_unique_entry(
                            name.clone(), *src_idx, entry.clone(),
                            &mut simple_map, &mut simple_order,
                            "DATA", conflicts, errors, sources,
                        );
                    }

                    DataEntry::ObjectProperty { name, .. } => {
                        self.upsert_unique_entry(
                            name.clone(), *src_idx, entry.clone(),
                            &mut object_map, &mut object_order,
                            "DATA", conflicts, errors, sources,
                        );
                    }

                    DataEntry::TableProperty { path, properties, .. } => {
                        let key = path.to_string();
                        if table_map.contains_key(&key) {
                            let existing_src = table_map[&key].0;
                            let existing_props = table_map[&key].1.clone();
                            // Same key existing on both sides isn't a conflict
                            // by itself -- only differing content is. Without
                            // this, re-diffing/re-merging identical content
                            // (e.g. `mdix diff file.mdix file.mdix`) reported
                            // every TableProperty as conflicting purely
                            // because the key was present twice.
                            if table_props_are_equal(&existing_props, properties) {
                                continue;
                            }
                            let Some(winner) = self.resolve_conflict(
                                format!("DATA.{}", key),
                                *src_idx, existing_src, sources, conflicts, errors,
                            ) else { continue; };
                            let merged = self.merge_table_props(
                                &existing_props, existing_src,
                                properties, *src_idx,
                                winner, sources, conflicts, errors,
                                &format!("DATA.{}", key),
                            );
                            let slot = table_map.get_mut(&key).unwrap();
                            slot.0 = winner;
                            slot.1 = merged;
                        } else {
                            table_order.push(key.clone());
                            table_map.insert(key, (*src_idx, properties.clone()));
                        }
                    }

                    DataEntry::GroupArray { path, items, .. } => {
                        let key = path.to_string();
                        if array_map.contains_key(&key) {
                            let existing_src = array_map[&key].0;
                            let existing_items = array_map[&key].1.clone();
                            // Same guard as TableProperty above -- identical
                            // items on both sides isn't a real conflict.
                            if array_items_are_equal(&existing_items, items) {
                                continue;
                            }
                            let Some(winner) = self.resolve_conflict(
                                format!("DATA.{}", key),
                                *src_idx, existing_src, sources, conflicts, errors,
                            ) else { continue; };
                            let merged_items = self.merge_array_items(
                                &existing_items, existing_src,
                                items, *src_idx,
                                winner,
                            );
                            let slot = array_map.get_mut(&key).unwrap();
                            slot.0 = winner;
                            slot.1 = merged_items;
                        } else {
                            array_order.push(key.clone());
                            array_map.insert(key, (*src_idx, items.clone()));
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return None;
        }

        // ── Assemble with two-tier ordering enforced ──────────────────────────
        let mut entries = Vec::new();

        // Tier 1a — SimpleProperty
        for key in &simple_order {
            if let Some((_, entry)) = simple_map.get(key) {
                entries.push(entry.clone());
            }
        }
        // Tier 1b — ObjectProperty
        for key in &object_order {
            if let Some((_, entry)) = object_map.get(key) {
                entries.push(entry.clone());
            }
        }
        // Tier 2a — TableProperty
        for key in &table_order {
            if let Some((_, props)) = table_map.get(key) {
                let segments = key.split('.').map(String::from).collect();
                entries.push(DataEntry::TableProperty {
                    path:       TablePath { segments },
                    properties: props.clone(),
                    position:   Position::UNKNOWN,
                });
            }
        }
        // Tier 2b — GroupArray
        for key in &array_order {
            if let Some((_, items)) = array_map.get(key) {
                let segments = key.split('.').map(String::from).collect();
                entries.push(DataEntry::GroupArray {
                    path:     TablePath { segments },
                    items:    items.clone(),
                    position: Position::UNKNOWN,
                });
            }
        }

        if entries.is_empty() { None } else { Some(DataSection { entries, position: Position::UNKNOWN }) }
    }

    /// Insert or update a single-key entry (SimpleProperty / ObjectProperty).
    fn upsert_unique_entry(
        &self,
        key:       String,
        src_idx:   usize,
        entry:     DataEntry,
        map:       &mut HashMap<String, (usize, DataEntry)>,
        order:     &mut Vec<String>,
        section:   &str,
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
        sources:   &[MdixMergeInput],
    ) {
        if let Some((existing_idx, existing_entry)) = map.get(&key) {
            // Identical value on both sides is not a real conflict. Only
            // SimpleProperty is compared in full (via values_are_equal,
            // Position-blind) — TableProperty/GroupArray/ObjectProperty fall
            // through to "always a conflict" the same way values_are_equal's
            // own catch-all treats complex types, since a full position-blind
            // structural walk of those isn't worth the complexity here.
            if data_entries_are_equal(existing_entry, &entry) {
                return;
            }
            let existing_idx = *existing_idx;
            if let Some(winner) = self.resolve_conflict(
                format!("{}.{}", section, key),
                src_idx, existing_idx, sources, conflicts, errors,
            ) {
                if winner == src_idx {
                    map.insert(key, (src_idx, entry));
                    // key is already in `order`
                }
            }
        } else {
            order.push(key.clone());
            map.insert(key, (src_idx, entry));
        }
    }

    /// Deep-merge two TableProperty property lists (field-by-field conflict resolution).
    fn merge_table_props(
        &self,
        primary_props:   &[PropertyAssignment],
        primary_src:     usize,
        secondary_props: &[PropertyAssignment],
        secondary_src:   usize,
        winner:          usize,
        sources:         &[MdixMergeInput],
        conflicts:       &mut Vec<MergeConflict>,
        errors:          &mut Vec<String>,
        path_label:      &str,
    ) -> Vec<PropertyAssignment> {
        let mut prop_map:   HashMap<String, (usize, PropertyAssignment)> = HashMap::new();
        let mut prop_order: Vec<String>                                   = Vec::new();

        for prop in primary_props {
            prop_order.push(prop.name.clone());
            prop_map.insert(prop.name.clone(), (primary_src, prop.clone()));
        }

        for prop in secondary_props {
            if let Some((existing_src, existing_prop)) = prop_map.get(&prop.name) {
                // Identical value on both sides is not a real conflict. Uses
                // values_are_equal (already defined below for merge_array_items'
                // ConcatDedup strategy) rather than raw `==`, since Value's
                // compound variants (Array/Object) embed their own per-source
                // Position and would never derive-equal even when the content
                // matches; values_are_equal already treats those conservatively.
                if values_are_equal(&existing_prop.value, &prop.value) {
                    continue;
                }
                let existing_src = *existing_src;
                if let Some(prop_winner) = self.resolve_conflict(
                    format!("{}.{}", path_label, prop.name),
                    secondary_src, existing_src, sources, conflicts, errors,
                ) {
                    if prop_winner == secondary_src {
                        prop_map.insert(prop.name.clone(), (secondary_src, prop.clone()));
                    }
                }
            } else {
                prop_order.push(prop.name.clone());
                prop_map.insert(prop.name.clone(), (secondary_src, prop.clone()));
            }
        }

        let mut result = Vec::with_capacity(prop_order.len());
        for name in &prop_order {
            if let Some((_, prop)) = prop_map.get(name) {
                result.push(prop.clone());
            }
        }
        result
    }

    /// Combine two GroupArray item lists per the configured `ArrayMergeStrategy`.
    fn merge_array_items(
        &self,
        primary_items:  &[Value],
        primary_src:    usize,
        secondary_items: &[Value],
        secondary_src:  usize,
        winner:         usize,
    ) -> Vec<Value> {
        let (winner_items, loser_items) = if winner == primary_src {
            (primary_items, secondary_items)
        } else {
            (secondary_items, primary_items)
        };

        match self.array_strategy {
            ArrayMergeStrategy::Replace => winner_items.to_vec(),

            ArrayMergeStrategy::Concat => {
                let mut result = winner_items.to_vec();
                result.extend_from_slice(loser_items);
                result
            }

            ArrayMergeStrategy::ConcatDedup => {
                let mut result = winner_items.to_vec();
                for item in loser_items {
                    if !result.iter().any(|existing| values_are_equal(existing, item)) {
                        result.push(item.clone());
                    }
                }
                result
            }
        }
    }

    // ── @SECURITY ─────────────────────────────────────────────────────────────

    fn merge_security(
        &self,
        sources:   &[MdixMergeInput],
        conflicts: &mut Vec<MergeConflict>,
        errors:    &mut Vec<String>,
    ) -> Option<SecuritySection> {
        let present: Vec<(usize, &SecuritySection)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.ast.security.as_ref().map(|sec| (i, sec)))
            .collect();

        if present.is_empty() {
            return None;
        }

        // block_key → (winning_source_idx, SecurityEntry)
        let mut entry_map:   HashMap<String, (usize, SecurityEntry)> = HashMap::new();
        let mut entry_order: Vec<String>                              = Vec::new();

        for (src_idx, section) in &present {
            for entry in &section.entries {
                let key = entry.block_key.clone();
                if entry_map.contains_key(&key) {
                    let existing_src    = entry_map[&key].0;
                    let existing_fields = entry_map[&key].1.fields.clone();

                    // Same block key, and every field already carries the same
                    // value on both sides (via values_are_equal, see
                    // merge_table_props above for why not raw `==`) — not a
                    // real conflict.
                    let fields_identical = existing_fields.len() == entry.fields.len()
                        && existing_fields.iter().all(|ef| {
                            entry.fields.iter().any(|df| {
                                df.key == ef.key && values_are_equal(&df.value, &ef.value)
                            })
                        });
                    if fields_identical {
                        continue;
                    }

                    let Some(winner) = self.resolve_conflict(
                        format!("SECURITY.{}", key),
                        *src_idx, existing_src, sources, conflicts, errors,
                    ) else { continue; };
                    let merged_fields = self.merge_security_fields(
                        &existing_fields, existing_src,
                        &entry.fields, *src_idx,
                        winner, sources, conflicts, errors,
                        &format!("SECURITY.{}", key),
                    );
                    let slot = entry_map.get_mut(&key).unwrap();
                    slot.0 = winner;
                    slot.1.fields = merged_fields;
                } else {
                    entry_order.push(key.clone());
                    entry_map.insert(key, (*src_idx, entry.clone()));
                }
            }
        }

        if !errors.is_empty() || entry_map.is_empty() {
            return None;
        }

        let mut entries = Vec::with_capacity(entry_order.len());
        for key in &entry_order {
            if let Some((_, entry)) = entry_map.get(key) {
                entries.push(entry.clone());
            }
        }

        Some(SecuritySection { entries, position: Position::UNKNOWN })
    }

    fn merge_security_fields(
        &self,
        primary_fields:   &[SecurityField],
        primary_src:      usize,
        secondary_fields: &[SecurityField],
        secondary_src:    usize,
        winner:           usize,
        sources:          &[MdixMergeInput],
        conflicts:        &mut Vec<MergeConflict>,
        errors:           &mut Vec<String>,
        block_label:      &str,
    ) -> Vec<SecurityField> {
        let mut field_map:   HashMap<String, (usize, SecurityField)> = HashMap::new();
        let mut field_order: Vec<String>                              = Vec::new();

        for field in primary_fields {
            field_order.push(field.key.clone());
            field_map.insert(field.key.clone(), (primary_src, field.clone()));
        }

        for field in secondary_fields {
            if let Some((existing_src, existing_field)) = field_map.get(&field.key) {
                // Identical value on both sides is not a real conflict.
                if values_are_equal(&existing_field.value, &field.value) {
                    continue;
                }
                let existing_src = *existing_src;
                if let Some(field_winner) = self.resolve_conflict(
                    format!("{}.{}", block_label, field.key),
                    secondary_src, existing_src, sources, conflicts, errors,
                ) {
                    if field_winner == secondary_src {
                        field_map.insert(field.key.clone(), (secondary_src, field.clone()));
                    }
                }
            } else {
                field_order.push(field.key.clone());
                field_map.insert(field.key.clone(), (secondary_src, field.clone()));
            }
        }

        let mut result = Vec::with_capacity(field_order.len());
        for key in &field_order {
            if let Some((_, f)) = field_map.get(key) {
                result.push(f.clone());
            }
        }
        result
    }

    // ── Conflict resolution ───────────────────────────────────────────────────

    /// Resolve a conflict between `challenger` (new) and `existing` (already
    /// stored) for a given reporting `path`, and — unlike calling
    /// `pick_winner` directly — always record a `MergeConflict` for it,
    /// regardless of whether a winner could actually be picked.
    ///
    /// Every call site used to push to `conflicts` only from inside
    /// `pick_winner`'s `Ok` arm, which `ThrowOnConflict` never returns (it's
    /// `Err` unconditionally, by design — see `pick_winner`). That meant
    /// `result.conflicts` came back empty under `ThrowOnConflict` even when
    /// real, detected disagreements existed, which silently broke `mdix
    /// diff` (`mdix-cli/src/services/diff_service.rs`): it runs merges
    /// under `ThrowOnConflict` specifically to *list* every disagreement
    /// without picking a winner, so an always-empty `conflicts` made every
    /// diff report "no conflicts" no matter what.
    ///
    /// Under `ThrowOnConflict` there's no real winner to apply to the
    /// merged output — `winning_source`/`winning_label` on that conflict
    /// describe `challenger` purely as a reference point for display, the
    /// same way `SecondaryWins` would report it. Nothing is actually
    /// applied to the merged AST for it either way, since the caller's
    /// `errors` (pushed here too, exactly as `pick_winner` already did) is
    /// non-empty afterward, which already makes the overall merge
    /// `is_success: false` — callers that only care about a successful
    /// merge (e.g. `mdix merge`) bail out on that before ever looking at
    /// `conflicts`, so this doesn't change their behavior.
    ///
    /// Returns `Some(winner)` when a winner was actually selected and
    /// should be applied to the merged output, or `None` when the strategy
    /// refused (the error has already been pushed to `errors`).
    fn resolve_conflict(
        &self,
        path:       String,
        challenger: usize,
        existing:   usize,
        sources:    &[MdixMergeInput],
        conflicts:  &mut Vec<MergeConflict>,
        errors:     &mut Vec<String>,
    ) -> Option<usize> {
        match self.pick_winner(challenger, existing, sources) {
            Ok(winner) => {
                conflicts.push(MergeConflict {
                    path,
                    winning_source: winner,
                    winning_label: sources[winner].label.clone(),
                });
                Some(winner)
            }
            Err(e) => {
                conflicts.push(MergeConflict {
                    path,
                    winning_source: challenger,
                    winning_label: sources.get(challenger).and_then(|s| s.label.clone()),
                });
                errors.push(e);
                None
            }
        }
    }

    /// Returns the index of the winner between `challenger` (new) and `existing`
    /// (already stored).  Returns `Err` under `ThrowOnConflict`.
    fn pick_winner(
        &self,
        challenger: usize,
        existing:   usize,
        sources:    &[MdixMergeInput],
    ) -> Result<usize, String> {
        match self.strategy {
            MdixMergeStrategy::ThrowOnConflict => Err(format!(
                "Conflict between source[{}]{} and source[{}]{} (ThrowOnConflict)",
                existing,
                sources.get(existing)
                    .and_then(|s| s.label.as_ref())
                    .map(|l| format!(" ('{}')", l))
                    .unwrap_or_default(),
                challenger,
                sources.get(challenger)
                    .and_then(|s| s.label.as_ref())
                    .map(|l| format!(" ('{}')", l))
                    .unwrap_or_default(),
            )),

            MdixMergeStrategy::PrimaryWins => {
                // Lower index = primary = wins.
                Ok(existing.min(challenger))
            }

            MdixMergeStrategy::SecondaryWins => {
                // Higher index = latest = wins.
                Ok(existing.max(challenger))
            }

            MdixMergeStrategy::WeightedPriority => {
                let w_existing    = sources.get(existing).map(|s| s.weight).unwrap_or(0.0);
                let w_challenger  = sources.get(challenger).map(|s| s.weight).unwrap_or(0.0);
                // Higher weight wins; tie → lower index (primary) wins.
                if w_challenger > w_existing {
                    Ok(challenger)
                } else if w_existing > w_challenger {
                    Ok(existing)
                } else {
                    Ok(existing.min(challenger))
                }
            }
        }
    }
}

// ── File-loading convenience ──────────────────────────────────────────────────

impl MdixMerger {
    /// Load, merge, and return a fully resolved `DixData` from a list of `.mdix`
    /// file paths.
    ///
    /// Files are assigned weights in descending order so the first path has the
    /// highest weight (`1.0`) and the last path has the lowest (approaching `0.0`).
    /// Use `merge_files_weighted` to supply explicit weights.
    ///
    /// Each file is compiled through the full pipeline (tokenize → parse →
    /// semantic → enhance → value-resolve) before merging, so QuickFunc
    /// calls are already inlined into @DATA values.
    pub fn merge_files(
        &self,
        file_paths: &[impl AsRef<str>],
    ) -> Result<super::dix_data::DixData, String> {
        let n = file_paths.len();
        if n == 0 {
            return Err("merge_files: no file paths provided".to_string());
        }

        let weights: Vec<f64> = (0..n)
            .map(|i| if n == 1 { 1.0 } else { 1.0 - (i as f64 / (n - 1) as f64) })
            .collect();

        let weighted: Vec<(&str, f64)> = file_paths
            .iter()
            .zip(weights.iter())
            .map(|(p, &w)| (p.as_ref(), w))
            .collect();

        self.merge_files_weighted(&weighted)
    }

    /// Load, merge, and return a `DixData` from `(path, weight)` pairs.
    ///
    /// Higher weight = higher priority under `WeightedPriority`.
    pub fn merge_files_weighted(
        &self,
        paths_and_weights: &[(&str, f64)],
    ) -> Result<super::dix_data::DixData, String> {
        if paths_and_weights.is_empty() {
            return Err("merge_files_weighted: no paths provided".to_string());
        }

        let loader = super::loader::DixLoader::new();

        let sources: Vec<MdixMergeInput> = paths_and_weights
            .iter()
            .map(|&(path, weight)| {
                loader
                    .compile_to_resolved_ast(path)
                    .map(|ast| {
                        MdixMergeInput::new(ast)
                            .with_weight(weight)
                            .with_label(path.to_string())
                    })
            })
            .collect::<Result<_, _>>()?;

        let result = self.merge_all(sources);

        if !result.is_success {
            return Err(result.errors.join("; "));
        }

        Ok(super::dix_data::DixData::from_ast(
            result.merged_ast,
            "1.0.0".to_string(),
            chrono::Utc::now(),
            false,
            false,
            vec![],
        ))
    }
}

// ── DixData extension ─────────────────────────────────────────────────────────

impl super::dix_data::DixData {
    /// Convenience: merge two DixData objects at the AST level by re-loading
    /// them from their source paths.
    ///
    /// If you already have `DixScript` ASTs, use `MdixMerger::merge` directly
    /// instead — it avoids the file I/O.
    pub fn merge_files(
        primary_path:   &str,
        secondary_path: &str,
        strategy:       MdixMergeStrategy,
    ) -> Result<super::dix_data::DixData, String> {
        MdixMerger::new()
            .with_strategy(strategy)
            .merge_files(&[primary_path, secondary_path])
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Numeric key for a `DLMModuleType` — used as the deduplication key in the
/// DLM section merge so only one module of each type survives.
#[inline]
fn dlm_module_type_key(t: DLMModuleType) -> u8 {
    match t {
        DLMModuleType::DCompressor => 0,
        DLMModuleType::DAuditor    => 1,
        DLMModuleType::DEncryptor  => 2,
        DLMModuleType::ParseError  => 255,
    }
}

/// Shallow structural equality for `Value` — used by `ConcatDedup`.
///
/// Only primitive / leaf variants are compared.  Collection types (`Array`,
/// `Object`, `NestedArray`, `PrefixedConstructor`) are always treated as
/// non-equal so they are never silently dropped from a merged group array.
/// Semantic equality for two top-level `@DATA` entries, ignoring Position.
/// See the comment at its call site in `upsert_unique_entry` for scope.
/// Whether two `TableProperty` property lists are equal enough that
/// there's no real conflict to report at all -- same length, and every
/// side-a property has a same-named, `values_are_equal` counterpart on
/// side b. Table properties aren't semantically ordered (`host = ...,
/// port = ...` means the same thing in any order), so this compares as an
/// unordered set of (name, value) pairs, not positionally.
///
/// Mirrors the per-property short-circuit `merge_table_props` already does
/// once it's inside a table conflict (`values_are_equal(&existing_prop.
/// value, &prop.value)`) -- this is that same check applied one level up,
/// so a `TableProperty` whose key merely repeats across sources with
/// identical contents (e.g. diffing a file against itself) never gets
/// treated as a conflict in the first place.
fn table_props_are_equal(a: &[PropertyAssignment], b: &[PropertyAssignment]) -> bool {
    a.len() == b.len()
        && a.iter().all(|pa| {
            b.iter().any(|pb| pa.name == pb.name && values_are_equal(&pa.value, &pb.value))
        })
}

/// Whether two `GroupArray` item lists are equal enough that there's no
/// real conflict to report. Unlike table properties, array items *are*
/// ordered, so this compares positionally rather than as a set.
fn array_items_are_equal(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| values_are_equal(x, y))
}

fn data_entries_are_equal(a: &DataEntry, b: &DataEntry) -> bool {
    match (a, b) {
        (DataEntry::SimpleProperty { value: v1, .. }, DataEntry::SimpleProperty { value: v2, .. }) => {
            values_are_equal(v1, v2)
        }
        _ => false,
    }
}

fn values_are_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null { .. },      Value::Null { .. })      => true,
        (Value::Boolean { value: v1, .. }, Value::Boolean { value: v2, .. }) => v1 == v2,
        (Value::Integer { value: v1, .. }, Value::Integer { value: v2, .. }) => v1 == v2,
        (Value::Long    { value: v1, .. }, Value::Long    { value: v2, .. }) => v1 == v2,
        // Float and Double use bit-equal comparison — NaN != NaN as per IEEE 754.
        (Value::Float   { value: v1, .. }, Value::Float   { value: v2, .. }) => v1.to_bits() == v2.to_bits(),
        (Value::Double  { value: v1, .. }, Value::Double  { value: v2, .. }) => v1.to_bits() == v2.to_bits(),
        (Value::ScientificNotation { value: v1, .. }, Value::ScientificNotation { value: v2, .. }) => {
            v1.to_bits() == v2.to_bits()
        }
        (Value::String    { value: v1, .. }, Value::String    { value: v2, .. }) => v1 == v2,
        (Value::HexColor  { value: v1, .. }, Value::HexColor  { value: v2, .. }) => v1 == v2,
        (Value::Date      { value: v1, .. }, Value::Date      { value: v2, .. }) => v1 == v2,
        (Value::Timestamp { value: v1, .. }, Value::Timestamp { value: v2, .. }) => v1 == v2,
        (Value::EnumValue { enum_name: e1, value: v1, .. },
         Value::EnumValue { enum_name: e2, value: v2, .. }) => e1 == e2 && v1 == v2,
        // Complex types — never equal for dedup purposes.
        _ => false,
    }
}

// ── Default ───────────────────────────────────────────────────────────────────

impl Default for MdixMerger {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn int_val(n: i32) -> Value {
        Value::Integer { value: n, position: Position::UNKNOWN }
    }
    fn str_val(s: &str) -> Value {
        Value::String { value: s.into(), position: Position::UNKNOWN }
    }
    fn bool_val(b: bool) -> Value {
        Value::Boolean { value: b, position: Position::UNKNOWN }
    }
    fn long_val(l: i64) -> Value {
        Value::Long { value: l, position: Position::UNKNOWN }
    }

    fn simple_prop(name: &str, value: Value) -> DataEntry {
        DataEntry::SimpleProperty {
            name: name.into(), data_type: None, value, position: Position::UNKNOWN,
        }
    }
    fn table_prop(path: &str, props: Vec<(&str, Value)>) -> DataEntry {
        let segments = path.split('.').map(String::from).collect();
        let properties = props.into_iter().map(|(k, v)| PropertyAssignment {
            name: k.into(), data_type: None, value: v, position: Position::UNKNOWN,
        }).collect();
        DataEntry::TableProperty { path: TablePath { segments }, properties, position: Position::UNKNOWN }
    }
    fn group_array(path: &str, items: Vec<Value>) -> DataEntry {
        let segments = path.split('.').map(String::from).collect();
        DataEntry::GroupArray { path: TablePath { segments }, items, position: Position::UNKNOWN }
    }
    fn config_entry(key: &str, val: &str) -> ConfigEntry {
        ConfigEntry { key: key.into(), value: ConfigValue::String(val.into()), position: Position::UNKNOWN }
    }

    fn ast_with_data(entries: Vec<DataEntry>) -> DixScript {
        let mut s = DixScript::new();
        s.data = Some(DataSection { entries, position: Position::UNKNOWN });
        s
    }
    fn ast_with_config(entries: Vec<ConfigEntry>) -> DixScript {
        let mut s = DixScript::new();
        s.config = Some(ConfigSection { entries, position: Position::UNKNOWN });
        s
    }

    // ── basic merge ───────────────────────────────────────────────────────────

    #[test]
    fn merge_disjoint_simple_properties() {
        let a = ast_with_data(vec![simple_prop("x", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("y", int_val(2))]);

        let result = MdixMerger::new().merge(
            MdixMergeInput::new(a),
            MdixMergeInput::new(b),
        );
        assert!(result.is_success);
        assert!(result.conflicts.is_empty());

        let entries = &result.merged_ast.data.unwrap().entries;
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn merge_conflicting_simple_property_primary_wins_by_default() {
        let a = ast_with_data(vec![simple_prop("x", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("x", int_val(99))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge(
                MdixMergeInput::new(a),
                MdixMergeInput::new(b),
            );

        assert!(result.is_success);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].winning_source, 0);

        let entries = &result.merged_ast.data.unwrap().entries;
        assert_eq!(entries.len(), 1);
        if let DataEntry::SimpleProperty { value: Value::Integer { value, .. }, .. } = &entries[0] {
            assert_eq!(*value, 1);
        } else {
            panic!("expected SimpleProperty(int)");
        }
    }

    #[test]
    fn merge_conflicting_simple_property_secondary_wins() {
        let a = ast_with_data(vec![simple_prop("x", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("x", int_val(99))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::SecondaryWins)
            .merge(
                MdixMergeInput::new(a),
                MdixMergeInput::new(b),
            );

        if let DataEntry::SimpleProperty { value: Value::Integer { value, .. }, .. } =
            &result.merged_ast.data.unwrap().entries[0]
        {
            assert_eq!(*value, 99);
        } else {
            panic!("expected 99");
        }
    }

    #[test]
    fn merge_conflicting_simple_property_higher_weight_wins() {
        let a = ast_with_data(vec![simple_prop("x", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("x", int_val(99))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::WeightedPriority)
            .merge(
                MdixMergeInput::new(a).with_weight(0.3),
                MdixMergeInput::new(b).with_weight(0.9),
            );

        if let DataEntry::SimpleProperty { value: Value::Integer { value, .. }, .. } =
            &result.merged_ast.data.unwrap().entries[0]
        {
            assert_eq!(*value, 99, "higher-weight source should win");
        } else {
            panic!("expected SimpleProperty");
        }
    }

    #[test]
    fn throw_on_conflict_produces_error() {
        let a = ast_with_data(vec![simple_prop("x", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("x", int_val(2))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::ThrowOnConflict)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(!result.is_success);
        assert!(!result.errors.is_empty());
    }

    // ── two-tier ordering ─────────────────────────────────────────────────────

    #[test]
    fn two_tier_ordering_maintained_after_merge() {
        // Source A: has a table property first, then a simple property
        let mut a = DixScript::new();
        a.data = Some(DataSection {
            entries: vec![
                // Deliberately put in wrong order — merge must fix it.
                table_prop("server", vec![("host", str_val("a.local"))]),
                simple_prop("version", str_val("1.0")),
            ],
            position: Position::UNKNOWN,
        });

        // Source B: just a simple property
        let b = ast_with_data(vec![simple_prop("debug", bool_val(true))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(result.is_success, "{:?}", result.errors);

        let entries = result.merged_ast.data.unwrap().entries;
        // Tier 1 (flat) must come before tier 2 (grouped).
        let flat_positions: Vec<usize> = entries.iter().enumerate()
            .filter(|(_, e)| matches!(e,
                DataEntry::SimpleProperty { .. } | DataEntry::ObjectProperty { .. }))
            .map(|(i, _)| i)
            .collect();
        let grouped_positions: Vec<usize> = entries.iter().enumerate()
            .filter(|(_, e)| matches!(e,
                DataEntry::TableProperty { .. } | DataEntry::GroupArray { .. }))
            .map(|(i, _)| i)
            .collect();

        if !flat_positions.is_empty() && !grouped_positions.is_empty() {
            let last_flat  = *flat_positions.last().unwrap();
            let first_grouped = *grouped_positions.first().unwrap();
            assert!(
                last_flat < first_grouped,
                "flat entries (last at {}) must precede grouped entries (first at {})",
                last_flat, first_grouped
            );
        }
    }

    // ── TableProperty deep-merge ──────────────────────────────────────────────

    #[test]
    fn table_property_same_path_deep_merged() {
        let a = ast_with_data(vec![
            table_prop("server", vec![("host", str_val("a.local")), ("port", int_val(8080))]),
        ]);
        let b = ast_with_data(vec![
            table_prop("server", vec![("port", int_val(9090)), ("ssl", bool_val(true))]),
        ]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(result.is_success);
        let entries = result.merged_ast.data.unwrap().entries;
        assert_eq!(entries.len(), 1, "should be one merged table entry");

        if let DataEntry::TableProperty { properties, .. } = &entries[0] {
            assert_eq!(properties.len(), 3, "host + port (primary wins) + ssl from secondary");
            let host = properties.iter().find(|p| p.name == "host").unwrap();
            assert!(matches!(&host.value, Value::String { value, .. } if *value == "a.local"));
            let port = properties.iter().find(|p| p.name == "port").unwrap();
            assert!(matches!(&port.value, Value::Integer { value: 8080, .. }));
            let ssl = properties.iter().find(|p| p.name == "ssl").unwrap();
            assert!(matches!(&ssl.value, Value::Boolean { value: true, .. }));
        } else {
            panic!("expected TableProperty");
        }
    }

    // ── GroupArray merge strategies ───────────────────────────────────────────

    #[test]
    fn group_array_concat_dedup_removes_duplicates() {
        let a = ast_with_data(vec![group_array("tags", vec![str_val("alpha"), str_val("beta")])]);
        let b = ast_with_data(vec![group_array("tags", vec![str_val("beta"), str_val("gamma")])]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .with_array_strategy(ArrayMergeStrategy::ConcatDedup)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(result.is_success);
        if let DataEntry::GroupArray { items, .. } =
            &result.merged_ast.data.unwrap().entries[0]
        {
            assert_eq!(items.len(), 3, "alpha, beta, gamma — beta deduped");
        }
    }

    #[test]
    fn group_array_replace_uses_winner_only() {
        let a = ast_with_data(vec![group_array("tags", vec![str_val("alpha")])]);
        let b = ast_with_data(vec![group_array("tags", vec![str_val("omega")])]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::SecondaryWins)
            .with_array_strategy(ArrayMergeStrategy::Replace)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        if let DataEntry::GroupArray { items, .. } =
            &result.merged_ast.data.unwrap().entries[0]
        {
            assert_eq!(items.len(), 1);
            assert!(matches!(&items[0], Value::String { value, .. } if *value == "omega"));
        }
    }

    // ── Config merge ──────────────────────────────────────────────────────────

    #[test]
    fn config_sections_merged_with_dedup() {
        let a = ast_with_config(vec![
            config_entry("version", "1.0.0"),
            config_entry("author", "Alice"),
        ]);
        let b = ast_with_config(vec![
            config_entry("version", "2.0.0"),
            config_entry("debug", "off"),
        ]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(result.is_success);
        let cfg = result.merged_ast.config.unwrap();
        assert_eq!(cfg.entries.len(), 3); // version(primary), author, debug

        let version = cfg.entries.iter().find(|e| e.key == "version").unwrap();
        assert!(matches!(&version.value, ConfigValue::String(s) if *s == "1.0.0"));
    }

    // ── Enum deep-merge ───────────────────────────────────────────────────────

    #[test]
    fn enum_same_name_fields_are_deep_merged() {
        let make_enum = |name: &str, fields: Vec<(&str, Option<i32>)>| {
            let mut s = DixScript::new();
            s.enums = Some(EnumsSection {
                enums: vec![EnumDeclaration {
                    name: name.into(),
                    fields: fields.into_iter().map(|(n, v)| EnumField {
                        name: n.into(), value: v, position: Position::UNKNOWN,
                    }).collect(),
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            });
            s
        };

        let a = make_enum("Status", vec![("ACTIVE", Some(0)), ("INACTIVE", Some(1))]);
        let b = make_enum("Status", vec![("INACTIVE", Some(99)), ("PENDING", Some(2))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge(MdixMergeInput::new(a), MdixMergeInput::new(b));

        assert!(result.is_success);
        let decl = &result.merged_ast.enums.unwrap().enums[0];
        assert_eq!(decl.fields.len(), 3); // ACTIVE, INACTIVE (primary=1), PENDING

        let inactive = decl.fields.iter().find(|f| f.name == "INACTIVE").unwrap();
        assert_eq!(inactive.value, Some(1), "primary wins on field conflict");

        assert!(decl.fields.iter().any(|f| f.name == "PENDING"), "new field from secondary added");
    }

    // ── Multi-source merge ────────────────────────────────────────────────────

    #[test]
    fn merge_all_three_sources() {
        let a = ast_with_data(vec![simple_prop("a", int_val(1))]);
        let b = ast_with_data(vec![simple_prop("b", int_val(2))]);
        let c = ast_with_data(vec![simple_prop("c", int_val(3)), simple_prop("a", int_val(99))]);

        let result = MdixMerger::new()
            .with_strategy(MdixMergeStrategy::PrimaryWins)
            .merge_all(vec![
                MdixMergeInput::new(a).with_label("A"),
                MdixMergeInput::new(b).with_label("B"),
                MdixMergeInput::new(c).with_label("C"),
            ]);

        assert!(result.is_success);
        assert_eq!(result.conflicts.len(), 1, "only 'a' conflicts");
        assert_eq!(result.conflicts[0].winning_source, 0); // source A wins

        let entries = result.merged_ast.data.unwrap().entries;
        assert_eq!(entries.len(), 3, "a, b, c");

        if let DataEntry::SimpleProperty { name, value: Value::Integer { value, .. }, .. } =
            entries.iter().find(|e| matches!(e, DataEntry::SimpleProperty { name, .. } if *name == "a")).unwrap()
        {
            assert_eq!(*value, 1);
        }
    }

    // ── Empty / single source ─────────────────────────────────────────────────

    #[test]
    fn single_source_returns_as_is() {
        let ast = ast_with_data(vec![simple_prop("x", long_val(42))]);
        let result = MdixMerger::new().merge_all(vec![MdixMergeInput::new(ast.clone())]);
        assert!(result.is_success);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn empty_sources_fails_gracefully() {
        let result = MdixMerger::new().merge_all(vec![]);
        assert!(!result.is_success);
        assert!(!result.errors.is_empty());
    }

    // ── Long / typed value survival ───────────────────────────────────────────

    #[test]
    fn long_values_survive_merge() {
        let a = ast_with_data(vec![simple_prop("big", long_val(9_000_000_000))]);
        let b = ast_with_data(vec![simple_prop("small", int_val(42))]);

        let result = MdixMerger::new().merge(
            MdixMergeInput::new(a),
            MdixMergeInput::new(b),
        );

        assert!(result.is_success);
        let entries = result.merged_ast.data.unwrap().entries;
        assert!(entries.iter().any(|e| matches!(e,
            DataEntry::SimpleProperty { value: Value::Long { value: 9_000_000_000, .. }, .. }
        )));
    }

    // ── values_are_equal ─────────────────────────────────────────────────────

    #[test]
    fn values_are_equal_primitives() {
        assert!(values_are_equal(&int_val(42), &int_val(42)));
        assert!(!values_are_equal(&int_val(42), &int_val(43)));
        assert!(values_are_equal(&str_val("x"), &str_val("x")));
        assert!(!values_are_equal(&str_val("x"), &str_val("y")));
        // Complex types are never equal
        let arr = Value::Array { values: vec![], position: Position::UNKNOWN };
        assert!(!values_are_equal(&arr, &arr));
    }
            }
