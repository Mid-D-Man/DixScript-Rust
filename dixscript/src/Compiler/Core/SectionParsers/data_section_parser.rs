//! Parser for the `@DATA(...)` section.
//!
//! Two-tier system (TOML-inspired): flat Tier-1 properties must precede all
//! Tier-2 grouped entries. Commas between entries are optional; commas inside
//! function arguments, array literals, and object literals are required.
//!
//! ```text,no_run
//! DataSection  ::= "@DATA(" DataContent ")"
//! DataContent  ::= SimpleProperty* GroupedEntry*
//! GroupedEntry ::= TableProperty | GroupArray
//! ```

use crate::Compiler::AST::{
    DataSection, DataEntry, TablePath, PropertyAssignment, Position,
    Value, ObjectProperty, Expression, DataType, ElemType,
};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Compiler::Utilities::{IdentifierPatternAnalyzer, IdentifierPatternType};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Utilities::{estimate_properties_count, estimate_array_items_count};


pub struct DataSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
    max_iterations: usize,
    has_seen_grouped_data: bool,
    current_object_nesting_depth: usize,
    current_function_call_depth: usize,
    pending_angle: bool,
    pending_equal: bool, // set when '=' was consumed as part of a '>>=' token
}

const MAX_OBJECT_NESTING_DEPTH: usize = 64;
const MAX_FUNCTION_CALL_DEPTH: usize = 10;
const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

impl<'a> DataSectionParser<'a> {
   pub fn new(
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
) -> Self {
   Self::new_with_error_manager(tokens,operational_settings,ErrorManager::get_shared_instance())
}
pub fn new_with_error_manager(
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
) -> Self {
    let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

    let dynamic_limit = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
    let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

    if debug_config.is_enabled {
        error_manager.log_debug(&format!(
            "DATA section parser: {} tokens, strategy: {:?}, max_iter: {}",
            tokens.len(),
            operational_settings.error_handling_strategy,
            max_iterations
        ));
    }

    DataSectionParser {
        tokens,
        operational_settings,
        error_manager,
        debug_config,
        position: 0,
        last_position: usize::MAX,
        stuck_count: 0,
        iteration_count: 0,
        max_iterations,
        has_seen_grouped_data: false,
        current_object_nesting_depth: 0,
        current_function_call_depth: 0,
        pending_angle: false,
        pending_equal: false,
    }
}
    pub fn parse_section(&mut self) -> Option<DataSection> {
        self.log_debug("Starting DATA section parse");

        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let estimated_entries = estimate_properties_count(self.tokens.len());
        let mut data_entries = Vec::with_capacity(estimated_entries);

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start DATA section",
                &current,
            );
            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.log_debug("Parser stuck in DATA section, attempting recovery");
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            match self.parse_data_entry() {
                Some(entry) => {
                    data_entries.push(entry);
                    self.log_verbose("Successfully parsed data entry");
                }
                None => {
                    if self.should_halt_section() {
                        self.log_debug("HALT detected - terminating DATA section parsing");
                        return self.handle_section_failure(section_start_pos);
                    }
                    if self.operational_settings.error_handling_strategy
                        == ErrorHandlingStrategy::Recover
                    {
                        if !self.attempt_recovery() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }

            if !self.handle_data_entry_comma_separation() {
                if self.should_halt_section() {
                    return self.handle_section_failure(section_start_pos);
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close DATA section",
                &current,
            );
            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "DATA section parsed successfully with {} entries",
                data_entries.len()
            ));
        }

        Some(DataSection::new(data_entries, section_start_pos))
    }
fn reset_parse_state(&mut self) {
    self.last_position = usize::MAX;
    self.stuck_count = 0;
    self.iteration_count = 0;
    self.has_seen_grouped_data = false;
    self.current_object_nesting_depth = 0;
    self.current_function_call_depth = 0;
    self.pending_angle = false;
    self.pending_equal = false;
    self.log_verbose("Parse state reset");
}

    fn track_progress(&mut self) {
        self.iteration_count += 1;
        if self.position == self.last_position {
            self.stuck_count += 1;
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "Position unchanged: {}, stuck count: {}",
                    self.position, self.stuck_count
                ));
            }
        } else {
            self.stuck_count = 0;
        }
        self.last_position = self.position;
    }

    #[inline]
    fn is_stuck(&self) -> bool {
        self.stuck_count >= MAX_STUCK_COUNT
    }

    fn should_terminate_loop(&self) -> bool {
    if self.iteration_count >= self.max_iterations {
        self.error_manager.log_error(&format!(
            "Maximum iterations ({}) exceeded — emergency loop termination \
             (token-based: {}, absolute cap: {})",
            self.max_iterations,
            self.tokens.len() * MAX_ITERATIONS_PER_TOKEN,
            ABSOLUTE_MAX_ITERATIONS
        ));
        return true;
    }
    false
}

    fn recover_from_stuck(&mut self) -> bool {
        if self.is_at_end() {
            self.log_debug("Cannot recover from stuck state — at end of tokens");
            return false;
        }
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Forcing advancement from stuck position {}",
                self.position
            ));
        }
        self.advance();
        self.stuck_count = 0;
        true
    }

    fn ensure_progress(&mut self) {
        if !self.is_at_end() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "Ensuring progress by advancing from position {}",
                    self.position
                ));
            }
            self.advance();
        } else {
            self.log_debug("Cannot ensure progress — at end of tokens");
        }
    }

    fn attempt_recovery(&mut self) -> bool {
        self.log_debug("Attempting recovery through synchronization");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol(',')
                || self.is_current_symbol(')')
                || self.is_next_data_entry()
            {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "Found recovery point at token: {}",
                        self.current().get_token_value()
                    ));
                }
                return true;
            }
            self.advance();
            recovery_attempts += 1;
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Recovery completed after {} attempts",
                recovery_attempts
            ));
        }
        !self.is_at_end()
    }

    fn parse_data_entry(&mut self) -> Option<DataEntry> {
        self.log_verbose("Parsing data entry");

        let entry_type = self.determine_data_entry_type();
        if self.debug_config.is_verbose {
            self.error_manager
                .log_info(&format!("Determined data entry type: {:?}", entry_type));
        }

        match entry_type {
            DataEntryType::SimpleProperty => self.parse_simple_property(),
            DataEntryType::TableProperty => {
                self.has_seen_grouped_data = true;
                self.parse_table_property()
            }
            DataEntryType::GroupArray => {
                self.has_seen_grouped_data = true;
                self.parse_group_array()
            }
            DataEntryType::ObjectProperty => self.parse_data_entry_object_property(),
            DataEntryType::Unknown => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Unable to determine data entry type from token: {:?}",
                        current.token_type
                    ),
                    &current,
                );
                None
            }
        }
    }

    fn determine_data_entry_type(&self) -> DataEntryType {
    self.log_verbose("Determining data entry type");

    if self.is_at_end() {
        return DataEntryType::Unknown;
    }

    let current_token = self.current();

    let identifier_value = match &current_token.token_type {
        TokenType::Identifier(id) => Some(id.as_str()),
        // kw is &&'static str; *kw gives &'static str which coerces to &str.
        TokenType::Keyword(kw) => Some(*kw),
        _ => None,
    };

    if let Some(_id) = identifier_value {
        let mut look_ahead = 1;

        // Skip optional type annotation <...>.
        // Use skip_annotation_lookahead so that fused tokens (>>, >>=, >=)
        // produced by the tokenizer when there is no space between the closing
        // '>' and the following '=' are handled correctly.
        if let Some(token) = self.peek_ahead(look_ahead) {
            if let TokenType::Symbol('<') = token.token_type {
                let (new_look_ahead, eq_fused) =
                    self.skip_annotation_lookahead(look_ahead);
                look_ahead = new_look_ahead;

                // When the closing '>' was fused with '=' (e.g. `prop<int>=value`
                // becomes  Identifier  '<'  Keyword  '>='  Identifier),
                // look_ahead is now pointing past the '=' already.
                // Determine whether this is a simple or object property directly.
                if eq_fused {
                    return self.determine_simple_or_object_property(look_ahead);
                }
            }
        }

        let next_token = self.peek_ahead(look_ahead);
        if next_token.is_none() {
            return DataEntryType::Unknown;
        }

        let next = next_token.unwrap();

        if matches!(next.token_type, TokenType::DoubleColon) {
            self.log_verbose("Detected group array via DoubleColon token");
            return DataEntryType::GroupArray;
        }

        if let TokenType::Symbol(sym) = next.token_type {
            return match sym {
                '=' => self.determine_simple_or_object_property(look_ahead + 1),
                '.' => self.determine_table_or_group_property(look_ahead + 1),
                ':' => {
                    if let Some(after_colon) = self.peek_ahead(look_ahead + 1) {
                        if let TokenType::Symbol(':') = after_colon.token_type {
                            DataEntryType::GroupArray
                        } else {
                            DataEntryType::TableProperty
                        }
                    } else {
                        DataEntryType::TableProperty
                    }
                }
                _ => DataEntryType::Unknown,
            };
        }
    }

    self.log_verbose("Could not determine entry type, defaulting to Unknown");
    DataEntryType::Unknown
}

    fn determine_simple_or_object_property(&self, value_position: usize) -> DataEntryType {
        if let Some(value_token) = self.peek_ahead(value_position) {
            if let TokenType::Symbol('{') = value_token.token_type {
                return DataEntryType::ObjectProperty;
            }
        }
        DataEntryType::SimpleProperty
    }

    fn determine_table_or_group_property(&self, start_pos: usize) -> DataEntryType {
        let mut pos = start_pos;
        while let Some(token) = self.peek_ahead(pos) {
            if let TokenType::Symbol(':') = token.token_type {
                if let Some(next) = self.peek_ahead(pos + 1) {
                    if let TokenType::Symbol(':') = next.token_type {
                        return DataEntryType::GroupArray;
                    }
                }
                return DataEntryType::TableProperty;
            }
            if matches!(token.token_type, TokenType::DoubleColon) {
                return DataEntryType::GroupArray;
            }
            pos += 1;
        }
        DataEntryType::Unknown
    }

    fn parse_simple_property(&mut self) -> Option<DataEntry> {
    self.log_verbose("Parsing simple property");

    let start_pos = Position::from_token(self.current());

    if self.has_seen_grouped_data {
        let attempted_property_name = match &self.current().token_type {
            TokenType::Identifier(id) => id.clone(),
            TokenType::Keyword(kw) => kw.to_string(),
            _ => "unknown".to_string(),
        };

        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::SectionSyntaxError,
            &format!(
                "TWO-TIER VIOLATION: Flat property '{}' cannot appear after grouped data.\n\
                \n\
                DixScript uses a two-tier system (inspired by TOML):\n\
                \n\
                TIER 1 (Flat Properties): property = value\n\
                TIER 2 (Grouped Data):    table.path: ... OR array.path:: ...\n\
                \n\
                Correct order:\n\
                   @DATA(\n\
                     flat1 = \"value\",     // Tier 1 first\n\
                     flat2 = 42,\n\
                     table.prop: x = 1   // Tier 2 follows\n\
                     array:: item1, item2\n\
                   )\n\
                \n\
                Fix: Move '{}' before any table properties or group arrays.",
                attempted_property_name, attempted_property_name
            ),
            &current,
        );

        if self.should_halt_section() {
            return None;
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Skipping illegal flat property '{}' after grouped data",
                attempted_property_name
            ));
        }
        self.advance();
        return None;
    }

    let property_name = self.parse_property_name()?;
    if self.debug_config.is_verbose {
        self.error_manager
            .log_info(&format!("Parsed simple property name: {}", property_name));
    }

    let data_type = self.parse_optional_type_annotation();

    // Use consume_equal so that a '=' that was part of a fused '>>=' token is handled.
    if !self.consume_equal() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            &format!("Expected '=' after property name '{}'", property_name),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
    }

    let value = match self.parse_property_value() {
        Some(v) => v,
        None => {
            if self.should_halt_section() {
                return None;
            }
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                &format!(
                    "Expected property value after '=' in property '{}'",
                    property_name
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::Null { position: start_pos }
        }
    };

    if self.debug_config.is_verbose {
        self.error_manager
            .log_info(&format!("Created simple property AST node: {}", property_name));
    }
    Some(DataEntry::SimpleProperty {
        name: property_name,
        data_type,
        value,
        position: start_pos,
    })
}

fn parse_table_property(&mut self) -> Option<DataEntry> {
    self.log_verbose("Parsing table property");

    let start_pos = Position::from_token(self.current());
    let table_path = self.parse_table_path()?;

    if !self.match_and_consume_symbol(':') {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            &format!("Expected ':' after table path '{}'", table_path),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
    }

    let estimated_props = estimate_properties_count(self.tokens.len());
    let mut properties = Vec::with_capacity(estimated_props);

    while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
        self.track_progress();
        if self.is_stuck() {
            if !self.recover_from_stuck() {
                break;
            }
            continue;
        }

        if self.is_start_of_new_data_entry() {
            self.log_verbose("Detected start of new data entry — ending table property");
            break;
        }

        let assignment = self.parse_property_assignment();
        if assignment.is_none() && self.should_halt_section() {
            return None;
        }

        if let Some(assign) = assignment {
            properties.push(assign);
        }

        if self.is_current_symbol(',') {
            self.advance();
            self.log_verbose("Consumed optional comma within table property");
        } else if self.is_current_symbol(')') || self.is_start_of_new_data_entry() {
            self.log_verbose("Ending table property parsing");
            break;
        } else if !self.is_at_end() {
            let next_token = self.current();
            if matches!(
                next_token.token_type,
                TokenType::Identifier(_) | TokenType::Keyword(_)
            ) {
                // Look ahead past an optional type annotation to find the '='.
                // Uses skip_annotation_lookahead so that fused tokens produced by
                // the tokenizer when there is no space between the closing '>' and
                // the following '=' (e.g. `key<int>=value` → '>=') are handled.
                let mut look_ahead = 1;
                let mut eq_fused   = false;

                if let Some(token) = self.peek_ahead(look_ahead) {
                    if let TokenType::Symbol('<') = token.token_type {
                        let (new_look_ahead, fused) =
                            self.skip_annotation_lookahead(look_ahead);
                        look_ahead = new_look_ahead;
                        eq_fused   = fused;
                    }
                }

                // A property follows when either:
                //   - the '=' was fused into the closing angle token, OR
                //   - the token at look_ahead is a plain '='
                let is_next_property = eq_fused
                    || matches!(
                        self.peek_ahead(look_ahead).map(|t| &t.token_type),
                        Some(TokenType::Symbol('='))
                    );

                if is_next_property {
                    self.log_verbose("Next property detected without comma — continuing");
                    continue;
                }

                self.log_verbose("Token after identifier is not '=' — ending table property");
                break;
            }
            self.log_verbose("No more properties detected — ending table property");
            break;
        }
    }

    Some(DataEntry::TableProperty {
        path: table_path,
        properties,
        position: start_pos,
    })
}

    fn parse_group_array(&mut self) -> Option<DataEntry> {
        self.log_verbose("Parsing group array");

        let start_pos = Position::from_token(self.current());
        let table_path = self.parse_table_path()?;

        if self.debug_config.is_verbose {
            self.error_manager
                .log_info(&format!("Parsed group array path: {}", table_path));
        }

        if !self.consume_double_colon() {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected '::' after table path '{}' for group array",
                    table_path
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        let estimated_items = estimate_array_items_count(self.tokens.len());
        let mut items = Vec::with_capacity(estimated_items);

        while !self.is_at_end() && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.log_debug("Parser stuck in group array items, attempting recovery");
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            if self.is_current_symbol(')') {
                self.log_verbose("Found closing ')' — ending group array parsing");
                break;
            }

            if self.is_start_of_new_grouped_data_entry() {
                self.log_verbose("Detected next grouped data entry — ending group array");
                break;
            }

            let item = self.parse_array_item();

            if item.is_none() && self.should_halt_section() {
                return None;
            }

            if let Some(value) = item {
                items.push(value);
                self.log_verbose("Parsed array item");
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Failed to parse item in group array. Expected value or object, found: {}",
                        current.get_token_value()
                    ),
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }

                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                    && !self.is_start_of_new_grouped_data_entry()
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
                self.log_verbose("Consumed optional comma within group array");

                if self.is_current_symbol(')') {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::SectionSyntaxError,
                        "TRAILING COMMA: Found comma before ')' in group array. Remove the trailing comma.",
                        &current,
                    );
                    if self.should_halt_section() {
                        return None;
                    }
                    break;
                }

                if self.is_start_of_new_grouped_data_entry() {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::SectionSyntaxError,
                        "TRAILING COMMA: Found comma before next data entry. Remove the trailing comma after the last array item.",
                        &current,
                    );
                    if self.should_halt_section() {
                        return None;
                    }
                    break;
                }
            } else if self.is_current_symbol(')') || self.is_start_of_new_grouped_data_entry() {
                self.log_verbose("No comma after item — group array items complete");
                break;
            } else if !self.is_at_end() {
                let next_token = self.current();

                if matches!(
                    next_token.token_type,
                    TokenType::Integer(_)
                    | TokenType::Long(_)
                        | TokenType::Float(_)
                        | TokenType::Double(_)
                        | TokenType::String(_)
                        | TokenType::StringSingle(_)
                        | TokenType::Bool(_)
                        | TokenType::HexColor(_)
                        | TokenType::Date(_)
                        | TokenType::Timestamp(_)
                ) {
                    self.log_verbose("Next primitive value detected without comma — continuing");
                    continue;
                }

                if self.is_current_symbol('{') || self.is_current_symbol('[') {
                    self.log_verbose("Next object/array literal detected without comma — continuing");
                    continue;
                }

                if matches!(
                    next_token.token_type,
                    TokenType::BlobConstructor(_)
                        | TokenType::TupleConstructor(_)
                        | TokenType::RegexConstructor(_)
                ) {
                    self.log_verbose("Next prefixed constructor detected without comma — continuing");
                    continue;
                }

                if matches!(next_token.token_type, TokenType::Identifier(_)) {
                    self.log_verbose("Next identifier detected without comma — continuing");
                    continue;
                }

                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected array item, ',', or ')' in group array, found: {}",
                        current.get_token_value()
                    ),
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }

                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                    && !self.is_start_of_new_grouped_data_entry()
                {
                    self.advance();
                }
            }
        }

        let item_count = items.len();
        let group_array = DataEntry::GroupArray {
            path: table_path,
            items,
            position: start_pos,
        };

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "Created group array AST node with {} items",
                item_count
            ));
        }
        Some(group_array)
    }

    fn parse_data_entry_object_property(&mut self) -> Option<DataEntry> {
    self.log_verbose("Parsing object property");

    let start_pos = Position::from_token(self.current());
    let property_name = self.parse_property_name()?;

    if self.debug_config.is_verbose {
        self.error_manager
            .log_info(&format!("Parsed object property name: {}", property_name));
    }

    let data_type = self.parse_optional_type_annotation();

    // Use consume_equal so that a '=' that was part of a fused '>>=' token is handled.
    if !self.consume_equal() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            &format!(
                "Expected '=' after object property name '{}'",
                property_name
            ),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
    }

    let object_literal = match self.parse_object_literal() {
        Some(obj) => obj,
        None => {
            if self.should_halt_section() {
                self.log_debug("HALT detected during object literal parsing");
                return None;
            }
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                &format!(
                    "Expected object literal after '=' in object property '{}'",
                    property_name
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::Object {
                properties: Vec::new(),
                position: start_pos,
            }
        }
    };

    if self.debug_config.is_verbose {
        self.error_manager
            .log_info(&format!("Created object property AST node: {}", property_name));
    }
    Some(DataEntry::ObjectProperty {
        name: property_name,
        data_type,
        object: Box::new(object_literal),
        position: start_pos,
    })
}

    fn parse_property_value(&mut self) -> Option<Value> {
        self.log_verbose("Parsing property value");

        let current_token = self.current();
        let value_pos = Position::from_token(current_token);

        let result = match &current_token.token_type {
            TokenType::Integer(i) => {
                let val = *i;
                self.advance();
                Some(Value::Integer { value: val, position: value_pos })
            }
            TokenType::Long(l) => {
                let val = *l;
                self.advance();
                Some(Value::Long { value: val, position: value_pos })
            }
            TokenType::Float(f) => {
                let val = *f;
                self.advance();
                Some(Value::Float { value: val, position: value_pos })
            }
            TokenType::Double(d) => {
                let val = *d;
                self.advance();
                Some(Value::Double { value: val, position: value_pos })
            }
            TokenType::ScientificNotation(sn) => {
                let val = *sn;
                self.advance();
                Some(Value::ScientificNotation { value: val, position: value_pos })
            }
            TokenType::String(s) => {
                let val = s.clone();
                self.advance();
                Some(Value::String { value: val, position: value_pos })
            }
            TokenType::StringSingle(ss) => {
                let val = ss.clone();
                self.advance();
                Some(Value::String { value: val, position: value_pos })
            }
            TokenType::Bool(b) => {
                let val = *b;
                self.advance();
                Some(Value::Boolean { value: val, position: value_pos })
            }
            TokenType::HexColor(hc) => {
                let val = hc.clone();
                self.advance();
                Some(Value::HexColor { value: val, position: value_pos })
            }
            TokenType::Date(d) => {
                let val = d.clone();
                self.advance();
                Some(Value::Date { value: val, position: value_pos })
            }
            TokenType::Timestamp(ts) => {
                let val = ts.clone();
                self.advance();
                Some(Value::Timestamp { value: val, position: value_pos })
            }
            _ => {
                let token_clone = current_token.clone();
                self.parse_complex_property_value(&token_clone, value_pos)
            }
        };

        result
    }

    fn parse_complex_property_value(&mut self, current_token: &Token, pos: Position) -> Option<Value> {
        // Keyword literals: null, true, false.
        // kw is &&'static str here; *kw gives &'static str for direct comparison.
        if let TokenType::Keyword(kw) = &current_token.token_type {
            if *kw == "null" {
                self.advance();
                return Some(Value::Null { position: pos });
            }
            if *kw == "true" || *kw == "false" {
                let val = *kw == "true";
                self.advance();
                return Some(Value::Boolean { value: val, position: pos });
            }
        }

        if let TokenType::BlobConstructor(_) = current_token.token_type {
            self.advance();
            return self.parse_blob_constructor(pos);
        }
        if let TokenType::TupleConstructor(_) = current_token.token_type {
            self.advance();
            return self.parse_tuple_constructor(pos);
        }
        if let TokenType::RegexConstructor(_) = current_token.token_type {
            self.advance();
            return self.parse_regex_constructor(pos);
        }

        if self.is_current_symbol('[') {
            let array_literal = self.parse_array_literal();
            if array_literal.is_none() && self.should_halt_section() {
                return None;
            }
            return array_literal;
        }

        if self.is_current_symbol('{') {
            let obj_literal = self.parse_object_literal();
            if obj_literal.is_none() && self.should_halt_section() {
                return None;
            }
            return obj_literal;
        }

        if let TokenType::Identifier(id) = &current_token.token_type {
            let identifier_name = id.clone();
            return self.parse_identifier_value(&identifier_name, pos);
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Unknown property value type: {:?}",
                current_token.token_type
            ));
        }
        None
    }

    fn parse_identifier_value(&mut self, identifier: &str, pos: Position) -> Option<Value> {
        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "ParseIdentifierValue: '{}' at position {}",
                identifier, self.position
            ));
        }

        let pattern = IdentifierPatternAnalyzer::analyze_data_pattern(
            identifier,
            pos,
            self.tokens,
            self.position,
            Some(&self.error_manager),
        );

        if self.debug_config.is_verbose {
            self.error_manager
                .log_info(&format!("Pattern detected: {:?}", pattern.pattern_type));
        }

        self.advance();

        match pattern.pattern_type {
            IdentifierPatternType::LocalFunctionCall => {
                if self.debug_config.is_verbose {
                    self.error_manager
                        .log_info(&format!("Detected local function call: {}()", identifier));
                }
                self.parse_quick_func_call(identifier, pos, false)
            }
            IdentifierPatternType::LocalEnumAccess => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Detected local enum access: {}.{}",
                        identifier,
                        pattern.second_part.as_deref().unwrap_or("?")
                    ));
                }
                self.advance();
                self.advance();
                Some(Value::EnumValue {
                    enum_name: identifier.to_string(),
                    value: pattern.second_part.unwrap(),
                    position: pos,
                })
            }
            IdentifierPatternType::ImportedFunctionCall => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Detected imported function call: {}.{}()",
                        identifier,
                        pattern.second_part.as_deref().unwrap_or("?")
                    ));
                }
                self.advance();
                let func_name = pattern.second_part.unwrap();
                self.advance();
                self.parse_imported_function_call(identifier, &func_name, pos)
            }
            IdentifierPatternType::ImportedEnumAccess => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Detected imported enum access: {}.{}.{}",
                        identifier,
                        pattern.second_part.as_deref().unwrap_or("?"),
                        pattern.third_part.as_deref().unwrap_or("?")
                    ));
                }
                self.advance();
                self.advance();
                self.advance();
                let enum_value = pattern.third_part.unwrap();
                self.advance();
                Some(Value::EnumValue {
                    enum_name: format!("{}.{}", identifier, pattern.second_part.unwrap()),
                    value: enum_value,
                    position: pos,
                })
            }
            IdentifierPatternType::TableOrGroupSyntax => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::SectionSyntaxError,
                    &format!(
                        "Table/group array syntax '{}:' or '{}::' is not valid as a value",
                        identifier, identifier
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                Some(Value::Error {
                    message: "Invalid table syntax in value context".to_string(),
                    position: pos,
                })
            }
            _ => {
                self.log_verbose("Simple identifier reference");
                Some(Value::Identifier {
                    value: identifier.to_string(),
                    position: pos,
                })
            }
        }
    }

    fn parse_quick_func_call(
        &mut self,
        function_name: &str,
        pos: Position,
        _is_accumulative: bool,
    ) -> Option<Value> {
        self.current_function_call_depth += 1;

        if self.current_function_call_depth > MAX_FUNCTION_CALL_DEPTH {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::SectionSyntaxError,
                &format!(
                    "Maximum function call nesting depth ({}) exceeded. Function: {}",
                    MAX_FUNCTION_CALL_DEPTH, function_name
                ),
                &current,
            );
            self.current_function_call_depth -= 1;
            return None;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "Parsing QuickFunc call: {} (depth {}/{})",
                function_name, self.current_function_call_depth, MAX_FUNCTION_CALL_DEPTH
            ));
        }

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected '(' after function name '{}'", function_name),
                &current,
            );
            self.current_function_call_depth -= 1;
            if self.should_halt_section() {
                return None;
            }
            return None;
        }

        let estimated_args = estimate_array_items_count(self.tokens.len());
        let mut arguments = Vec::with_capacity(estimated_args);

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "Parser stuck in function arguments for '{}'",
                        function_name
                    ));
                }
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let argument = self.parse_argument_expression();

            if argument.is_none() && self.should_halt_section() {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "HALT detected while parsing QuickFunc call '{}'",
                        function_name
                    ));
                }
                self.current_function_call_depth -= 1;
                return None;
            }

            if let Some(arg) = argument {
                arguments.push(arg);
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Failed to parse argument in QuickFunc call '{}'",
                        function_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    self.current_function_call_depth -= 1;
                    return None;
                }
                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or ')' in function arguments for '{}', found {}",
                        function_name,
                        current.get_token_value()
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    self.current_function_call_depth -= 1;
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected ')' to close function call '{}'", function_name),
                &current,
            );
            self.current_function_call_depth -= 1;
            if self.should_halt_section() {
                return None;
            }
        }

        self.current_function_call_depth -= 1;

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "Parsed QuickFunc call: {} with {} arguments",
                function_name,
                arguments.len()
            ));
        }

        Some(Value::QuickFuncCall {
            function_name: function_name.to_string(),
            arguments,
            position: pos,
        })
    }

    fn parse_imported_function_call(
        &mut self,
        namespace_name: &str,
        function_name: &str,
        pos: Position,
    ) -> Option<Value> {
        let qualified_name = format!("{}.{}", namespace_name, function_name);

        if self.debug_config.is_verbose {
            self.error_manager
                .log_info(&format!("Parsing imported function call: {}()", qualified_name));
        }

        self.current_function_call_depth += 1;

        if self.current_function_call_depth > MAX_FUNCTION_CALL_DEPTH {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::SectionSyntaxError,
                &format!(
                    "Maximum function call nesting depth ({}) exceeded. Function: {}",
                    MAX_FUNCTION_CALL_DEPTH, qualified_name
                ),
                &current,
            );
            self.current_function_call_depth -= 1;
            return None;
        }

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected '(' after imported function name '{}'",
                    qualified_name
                ),
                &current,
            );
            self.current_function_call_depth -= 1;
            if self.should_halt_section() {
                return None;
            }
            return None;
        }

        let estimated_args = estimate_array_items_count(self.tokens.len());
        let mut arguments = Vec::with_capacity(estimated_args);

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "Parser stuck in imported function arguments for '{}'",
                        qualified_name
                    ));
                }
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let argument = self.parse_argument_expression();

            if argument.is_none() && self.should_halt_section() {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug(&format!(
                        "HALT detected while parsing imported function '{}'",
                        qualified_name
                    ));
                }
                self.current_function_call_depth -= 1;
                return None;
            }

            if let Some(arg) = argument {
                arguments.push(arg);
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Failed to parse argument in imported function call '{}'",
                        qualified_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    self.current_function_call_depth -= 1;
                    return None;
                }
                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or ')' in function arguments for '{}'",
                        qualified_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    self.current_function_call_depth -= 1;
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected ')' to close imported function call '{}'",
                    qualified_name
                ),
                &current,
            );
            self.current_function_call_depth -= 1;
            if self.should_halt_section() {
                return None;
            }
        }

        self.current_function_call_depth -= 1;

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "Parsed imported function call: {} with {} arguments",
                qualified_name,
                arguments.len()
            ));
        }

        Some(Value::QuickFuncCall {
            function_name: qualified_name,
            arguments,
            position: pos,
        })
    }

    fn parse_argument_expression(&mut self) -> Option<Expression> {
        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "ParseArgumentExpression at position {}",
                self.position
            ));
        }

        let current_token = self.current();
        let expr_pos = Position::from_token(current_token);

        if let TokenType::Integer(i) = current_token.token_type {
            let val = i;
            self.advance();
            return Some(Expression::Value {
                value: Value::Integer { value: val, position: expr_pos },
                position: expr_pos,
            });
        }
        if let TokenType::Long(i) = current_token.token_type {
            let val = i;
            self.advance();
            return Some(Expression::Value {
                value: Value::Long { value: val, position: expr_pos },
                position: expr_pos,
            });
        }
        if let TokenType::Float(f) = current_token.token_type {
            let val = f;
            self.advance();
            return Some(Expression::Value {
                value: Value::Float { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::Double(d) = current_token.token_type {
            let val = d;
            self.advance();
            return Some(Expression::Value {
                value: Value::Double { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::String(s) = &current_token.token_type {
            let val = s.clone();
            self.advance();
            return Some(Expression::Value {
                value: Value::String { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::Bool(b) = current_token.token_type {
            let val = b;
            self.advance();
            return Some(Expression::Value {
                value: Value::Boolean { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        // kw is &&'static str; *kw gives &'static str for comparison.
        if let TokenType::Keyword(kw) = &current_token.token_type {
            if *kw == "true" || *kw == "false" {
                let val = *kw == "true";
                self.advance();
                return Some(Expression::Value {
                    value: Value::Boolean { value: val, position: expr_pos },
                    position: expr_pos,
                });
            }
            if *kw == "null" {
                self.advance();
                return Some(Expression::Value {
                    value: Value::Null { position: expr_pos },
                    position: expr_pos,
                });
            }
        }

        if let TokenType::Date(d) = &current_token.token_type {
            let val = d.clone();
            self.advance();
            return Some(Expression::Value {
                value: Value::Date { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::Timestamp(ts) = &current_token.token_type {
            let val = ts.clone();
            self.advance();
            return Some(Expression::Value {
                value: Value::Timestamp { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::HexColor(hc) = &current_token.token_type {
            let val = hc.clone();
            self.advance();
            return Some(Expression::Value {
                value: Value::HexColor { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if let TokenType::ScientificNotation(sn) = current_token.token_type {
            let val = sn;
            self.advance();
            return Some(Expression::Value {
                value: Value::ScientificNotation { value: val, position: expr_pos },
                position: expr_pos,
            });
        }

        if matches!(current_token.token_type, TokenType::BlobConstructor(_)) {
            self.advance();
            let blob_value = self.parse_blob_constructor(expr_pos);
            if blob_value.is_none() && self.should_halt_section() {
                return None;
            }
            let val = blob_value.unwrap_or(Value::Null { position: expr_pos });
            return Some(Expression::Value { value: val, position: expr_pos });
        }

        if matches!(current_token.token_type, TokenType::TupleConstructor(_)) {
            self.advance();
            let tuple_value = self.parse_tuple_constructor(expr_pos);
            if tuple_value.is_none() && self.should_halt_section() {
                return None;
            }
            let val = tuple_value.unwrap_or(Value::Null { position: expr_pos });
            return Some(Expression::Value { value: val, position: expr_pos });
        }

        if matches!(current_token.token_type, TokenType::RegexConstructor(_)) {
            self.advance();
            let regex_value = self.parse_regex_constructor(expr_pos);
            if regex_value.is_none() && self.should_halt_section() {
                return None;
            }
            let val = regex_value.unwrap_or(Value::Null { position: expr_pos });
            return Some(Expression::Value { value: val, position: expr_pos });
        }

        if self.is_current_symbol('[') {
            let array_literal = self.parse_array_literal();
            if array_literal.is_none() && self.should_halt_section() {
                return None;
            }
            let val = array_literal.unwrap_or(Value::Array {
                values: Vec::new(),
                position: expr_pos,
            });
            return Some(Expression::Value { value: val, position: expr_pos });
        }

        if self.is_current_symbol('{') {
            let obj_literal = self.parse_object_literal();
            if obj_literal.is_none() && self.should_halt_section() {
                return None;
            }
            let val = obj_literal.unwrap_or(Value::Object {
                properties: Vec::new(),
                position: expr_pos,
            });
            return Some(Expression::Value { value: val, position: expr_pos });
        }

        if let TokenType::Symbol('(') = current_token.token_type {
            self.advance();
            let inner_expr = self.parse_argument_expression();
            if inner_expr.is_none() && self.should_halt_section() {
                return None;
            }
            if !self.match_and_consume_symbol(')') {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::MissingToken,
                    "Expected ')' to close parenthesized expression",
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
            }
            return inner_expr;
        }

        if let TokenType::Identifier(id) = &current_token.token_type {
            let identifier_name = id.clone();

            let pattern = IdentifierPatternAnalyzer::analyze_data_pattern(
                &identifier_name,
                expr_pos,
                self.tokens,
                self.position,
                Some(&self.error_manager),
            );

            self.advance();

            return match pattern.pattern_type {
                IdentifierPatternType::LocalFunctionCall => {
                    self.parse_function_call_expression(&identifier_name, expr_pos)
                }
                IdentifierPatternType::LocalEnumAccess => {
                    self.advance();
                    self.advance();
                    Some(Expression::EnumAccess {
                        namespace_name: None,
                        enum_name: identifier_name,
                        value: pattern.second_part.unwrap(),
                        position: expr_pos,
                    })
                }
                IdentifierPatternType::ImportedFunctionCall => {
                    self.advance();
                    let func_name = pattern.second_part.unwrap();
                    self.advance();
                    self.parse_imported_function_call_expression(
                        &identifier_name,
                        &func_name,
                        expr_pos,
                    )
                }
                IdentifierPatternType::ImportedEnumAccess => {
                    self.advance();
                    let enum_name = pattern.second_part.unwrap();
                    self.advance();
                    self.advance();
                    let enum_value = pattern.third_part.unwrap();
                    self.advance();
                    Some(Expression::EnumAccess {
                        namespace_name: Some(identifier_name),
                        enum_name,
                        value: enum_value,
                        position: expr_pos,
                    })
                }
                _ => {
                    Some(Expression::Identifier {
                        name: identifier_name,
                        position: expr_pos,
                    })
                }
            }
        }

        // ArithmeticOp now holds &'static str; copy it before re-borrowing current_token.
        if let TokenType::ArithmeticOp(op) = &current_token.token_type {
            let op_str: &'static str = *op;
            let current_clone = current_token.clone();
            self.handle_parse_error(
                ParseErrorType::SectionSyntaxError,
                &format!(
                    "Arithmetic operations ('{}') are not allowed in DATA section",
                    op_str
                ),
                &current_clone,
            );
            if self.should_halt_section() {
                return None;
            }
            self.advance();
            return Some(Expression::Value {
                value: Value::Error {
                    message: format!("Illegal arithmetic operator: {}", op_str),
                    position: expr_pos,
                },
                position: expr_pos,
            });
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Unknown argument expression type: {:?}",
                current_token.token_type
            ));
        }
        None
    }

    fn parse_function_call_expression(
        &mut self,
        function_name: &str,
        pos: Position,
    ) -> Option<Expression> {
        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "ParseFunctionCallExpression: {}",
                function_name
            ));
        }

        if !self.is_current_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected '(' after function name '{}'", function_name),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            return Some(Expression::Identifier {
                name: function_name.to_string(),
                position: pos,
            });
        }

        self.advance();

        let estimated_args = estimate_array_items_count(self.tokens.len());
        let mut arguments = Vec::with_capacity(estimated_args);

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();
            if self.is_stuck() {
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let argument = self.parse_argument_expression();
            if argument.is_none() && self.should_halt_section() {
                return None;
            }
            if let Some(arg) = argument {
                arguments.push(arg);
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Failed to parse argument in function call '{}'",
                        function_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or ')' in function arguments for '{}'",
                        function_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.is_current_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected ')' to close function call '{}'", function_name),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        } else {
            self.advance();
        }

        Some(Expression::QuickFuncCall {
            name: function_name.to_string(),
            arguments,
            position: pos,
        })
    }

    fn parse_imported_function_call_expression(
        &mut self,
        namespace_name: &str,
        function_name: &str,
        pos: Position,
    ) -> Option<Expression> {
        let qualified_name = format!("{}.{}", namespace_name, function_name);

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "ParseImportedFunctionCallExpression: {}",
                qualified_name
            ));
        }

        if !self.is_current_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected '(' after imported function name '{}'",
                    qualified_name
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            return Some(Expression::Identifier {
                name: qualified_name,
                position: pos,
            });
        }

        self.advance();

        let estimated_args = estimate_array_items_count(self.tokens.len());
        let mut arguments = Vec::with_capacity(estimated_args);

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();
            if self.is_stuck() {
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let argument = self.parse_argument_expression();
            if argument.is_none() && self.should_halt_section() {
                return None;
            }
            if let Some(arg) = argument {
                arguments.push(arg);
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Failed to parse argument in imported function '{}'",
                        qualified_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(')')
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or ')' in arguments for '{}'",
                        qualified_name
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.is_current_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected ')' to close imported function '{}'",
                    qualified_name
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        } else {
            self.advance();
        }

        Some(Expression::ImportedFunctionCall {
            namespace_name: namespace_name.to_string(),
            function_name: function_name.to_string(),
            arguments,
            position: pos,
        })
    }

    fn parse_array_item(&mut self) -> Option<Value> {
        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "ParseArrayItem at position {}",
                self.position
            ));
        }

        let item_pos = Position::from_token(self.current());

        if let TokenType::Identifier(func_id) = &self.current().token_type {
            let function_name = func_id.clone();

            let pattern = IdentifierPatternAnalyzer::analyze_data_pattern(
                &function_name,
                item_pos,
                self.tokens,
                self.position,
                Some(&self.error_manager),
            );

            if pattern.pattern_type == IdentifierPatternType::LocalFunctionCall {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Local function call in array: {}()",
                        function_name
                    ));
                }
                self.advance();
                let func_call = self.parse_quick_func_call(&function_name, item_pos, false);
                if func_call.is_none() && self.should_halt_section() {
                    return None;
                }
                return func_call;
            }

            if pattern.pattern_type == IdentifierPatternType::ImportedFunctionCall {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Imported function call in array: {}.{}()",
                        function_name,
                        pattern.second_part.as_deref().unwrap_or("?")
                    ));
                }
                self.advance();
                self.advance();
                let func_name = pattern.second_part.unwrap();
                self.advance();
                let imported_call =
                    self.parse_imported_function_call(&function_name, &func_name, item_pos);
                if imported_call.is_none() && self.should_halt_section() {
                    return None;
                }
                return imported_call;
            }

            if pattern.pattern_type == IdentifierPatternType::SimpleIdentifier {
                self.advance();
                return Some(Value::Identifier {
                    value: function_name,
                    position: item_pos,
                });
            }

            if pattern.pattern_type == IdentifierPatternType::LocalEnumAccess {
                if self.debug_config.is_verbose {
                    self.error_manager.log_info(&format!(
                        "Enum access in array: {}.{}",
                        function_name,
                        pattern.second_part.as_deref().unwrap_or("?")
                    ));
                }
                self.advance();
                self.advance();
                let enum_value = pattern.second_part.unwrap();
                self.advance();
                return Some(Value::EnumValue {
                    enum_name: function_name,
                    value: enum_value,
                    position: item_pos,
                });
            }
        }

        if self.is_current_symbol('{') {
            let obj_literal = self.parse_object_literal();
            if obj_literal.is_none() && self.should_halt_section() {
                return None;
            }
            return obj_literal;
        }

        self.parse_property_value()
    }

    fn parse_object_literal(&mut self) -> Option<Value> {
        self.current_object_nesting_depth += 1;

        let obj_pos = Position::from_token(self.current());

        if self.current_object_nesting_depth > MAX_OBJECT_NESTING_DEPTH {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::SectionSyntaxError,
                &format!(
                    "Maximum object nesting depth ({}) exceeded. Consider flattening your data structure.",
                    MAX_OBJECT_NESTING_DEPTH
                ),
                &current,
            );
            self.current_object_nesting_depth -= 1;
            return None;
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_info(&format!(
                "Parsing object literal — depth {}/{}",
                self.current_object_nesting_depth, MAX_OBJECT_NESTING_DEPTH
            ));
        }

        if !self.match_and_consume_symbol('{') {
            self.current_object_nesting_depth -= 1;
            return None;
        }

        let estimated_props = estimate_properties_count(self.tokens.len());
        let mut object_properties = Vec::with_capacity(estimated_props);

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            self.track_progress();
            if self.is_stuck() {
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let object_property = self.parse_object_property();
            if object_property.is_none() && self.should_halt_section() {
                self.current_object_nesting_depth -= 1;
                return None;
            }

            if let Some(prop) = object_property {
                object_properties.push(prop);
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol('}') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected ',' or '}' in object literal",
                    &current,
                );
                if self.should_halt_section() {
                    self.current_object_nesting_depth -= 1;
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.match_and_consume_symbol('}') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '}' to close object literal",
                &current,
            );
            if self.should_halt_section() {
                self.current_object_nesting_depth -= 1;
                return None;
            }
        }

        self.current_object_nesting_depth -= 1;
        self.log_verbose("Successfully parsed object literal");
        Some(Value::Object {
            properties: object_properties,
            position: obj_pos,
        })
    }

 fn parse_object_property(&mut self) -> Option<ObjectProperty> {
    self.log_verbose("Parsing object property");

    let prop_pos = Position::from_token(self.current());
    let property_key = self.parse_property_name()?;

    // Parse and discard optional type annotation.
    // ObjectProperty has no data_type field; the annotation is consumed here so
    // that `key<type>=value` sequences (no space before `=`) do not confuse the
    // separator check below.  The `pending_equal` flag is set automatically by
    // `parse_optional_type_annotation` when the annotation ends with a fused
    // `>=` or `>>=` token.
    let _ = self.parse_optional_type_annotation();

    if matches!(self.current().token_type, TokenType::DoubleColon) {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::SectionSyntaxError,
            &format!(
                "NESTED GROUP ARRAY: Property '{}' uses '::' inside an object.\n\
                Group arrays can only appear at the top level of the DATA section.\n\
                \n\
                Wrong:   {{ {}:: item1, item2 }}\n\
                Correct: {{ {} = [item1, item2] }}\n\
                Or move to top level: path.{}:: item1, item2",
                property_key, property_key, property_key, property_key
            ),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
        while !self.is_at_end()
            && !self.is_current_symbol(',')
            && !self.is_current_symbol('}')
        {
            self.advance();
        }
        return None;
    }

    if self.is_current_symbol(':') {
        if let Some(next) = self.peek_ahead(1) {
            if !matches!(next.token_type, TokenType::Symbol(':')) {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::SectionSyntaxError,
                    &format!(
                        "WRONG SYNTAX: Property '{}' uses ':' but DATA section requires '='.\n\
                        Wrong:   {{ {}: value }}\n\
                        Correct: {{ {} = value }}",
                        property_key, property_key, property_key
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                return None;
            }
        }
    }

    // Use consume_equal() so that a '=' fused into a '>=', '>>=', or '>>='
    // token consumed during type annotation parsing is handled correctly via
    // the pending_equal flag rather than requiring a real '=' token.
    if !self.consume_equal() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            &format!(
                "Expected '=' after property key '{}' in object literal",
                property_key
            ),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
        return None;
    }

    let property_value = match self.parse_property_value() {
        Some(v) => v,
        None => {
            if self.should_halt_section() {
                return None;
            }
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                &format!(
                    "Expected property value after '=' for key '{}'",
                    property_key
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::Null { position: prop_pos }
        }
    };

    if self.debug_config.is_verbose {
        self.error_manager.log_info(&format!(
            "Parsed object property: {}",
            property_key
        ));
    }
    Some(ObjectProperty::new(property_key, property_value, prop_pos))
}

    fn parse_array_literal(&mut self) -> Option<Value> {
        self.log_verbose("Parsing array literal");

        let array_pos = Position::from_token(self.current());

        if !self.match_and_consume_symbol('[') {
            return None;
        }

        let estimated_items = estimate_array_items_count(self.tokens.len());
        let mut array_values = Vec::with_capacity(estimated_items);

        while !self.is_at_end() && !self.is_current_symbol(']') && !self.should_terminate_loop() {
            self.track_progress();
            if self.is_stuck() {
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            let array_value = self.parse_property_value();
            if array_value.is_none() && self.should_halt_section() {
                return None;
            }

            if let Some(val) = array_value {
                array_values.push(val);
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Failed to parse array value",
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                while !self.is_at_end()
                    && !self.is_current_symbol(',')
                    && !self.is_current_symbol(']')
                {
                    self.advance();
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(']') {
                break;
            } else if !self.is_at_end() {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected ',' or ']' in array literal",
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.match_and_consume_symbol(']') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ']' to close array literal",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        self.log_verbose("Successfully parsed array literal");
        Some(Value::Array {
            values: array_values,
            position: array_pos,
        })
    }

    fn parse_blob_constructor(&mut self, pos: Position) -> Option<Value> {
        self.log_verbose("Parsing blob constructor");

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' after 'b:'",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        let value = if let TokenType::String(s) = &self.current().token_type {
            let val = s.clone();
            let str_pos = Position::from_token(self.current());
            self.advance();
            Value::String { value: val, position: str_pos }
        } else {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                "Expected string value in blob constructor",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::String { value: String::new(), position: pos }
        };

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close blob constructor",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        Some(Value::PrefixedConstructor {
            prefix: "b".to_string(),
            arguments: vec![value],
            position: pos,
        })
    }

    fn parse_tuple_constructor(&mut self, pos: Position) -> Option<Value> {
    self.log_verbose("Parsing tuple constructor");

    if !self.match_and_consume_symbol('(') {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            "Expected '(' after 't:'",
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
    }

    let mut values = Vec::new();

    // No element-count limit here — MAX_TUPLE_ELEMENTS = 6 is enforced by semantic analysis
    while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
        self.track_progress();
        if self.is_stuck() {
            if !self.recover_from_stuck() {
                break;
            }
            continue;
        }

        let value = self.parse_property_value();
        if value.is_none() && self.should_halt_section() {
            return None;
        }
        if let Some(val) = value {
            values.push(val);
        }

        if self.is_current_symbol(',') {
            self.advance();
        } else if self.is_current_symbol(')') {
            break;
        } else if !self.is_at_end() {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                "Expected ',' or ')' in tuple constructor",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            self.ensure_progress();
        }
    }

    if !self.match_and_consume_symbol(')') {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            "Expected ')' to close tuple constructor",
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
    }

    Some(Value::PrefixedConstructor {
        prefix: "t".to_string(),
        arguments: values,
        position: pos,
    })
        }

    fn parse_regex_constructor(&mut self, pos: Position) -> Option<Value> {
        self.log_verbose("Parsing regex constructor");

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' after 'r:'",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        let pattern = if let TokenType::String(s) = &self.current().token_type {
            let val = s.clone();
            let str_pos = Position::from_token(self.current());
            self.advance();
            Value::String { value: val, position: str_pos }
        } else {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                "Expected string value in regex constructor",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::String { value: String::new(), position: pos }
        };

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close regex constructor",
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
        }

        Some(Value::PrefixedConstructor {
            prefix: "r".to_string(),
            arguments: vec![pattern],
            position: pos,
        })
    }

    fn parse_property_name(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            // kw is &&'static str; .to_string() derives an owned String via Deref.
            TokenType::Keyword(kw) => {
                let name = kw.to_string();
                self.advance();
                Some(name)
            }
            _ => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected property name identifier",
                    &current,
                );
                None
            }
        }
    }

    fn parse_optional_type_annotation(&mut self) -> Option<DataType> {
    if !self.is_current_symbol('<') {
        return None;
    }
    self.advance(); // consume outer '<'

    let data_type = self.parse_data_type();
    if data_type.is_none() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::UnexpectedToken,
            "Expected data type in type annotation",
            &current,
        );
    }

    // Use closing-angle helper so ">>" from nested <array<int>> is handled correctly
    if !self.match_and_consume_closing_angle() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            "Expected '>' to close type annotation",
            &current,
        );
    }

    data_type
}

  fn parse_data_type(&mut self) -> Option<DataType> {
    let kw_str: Option<&'static str> = match &self.current().token_type {
        TokenType::Keyword(kw) => Some(*kw),
        _ => None,
    };

    let kw = kw_str?;

    let base: Option<DataType> = match kw {
        "int"       => Some(DataType::Int),
        "long"      => Some(DataType::Long),
        "float"     => Some(DataType::Float),
        "double"    => Some(DataType::Double),
        "string"    => Some(DataType::String),
        "bool"      => Some(DataType::Bool),
        "array"     => Some(DataType::Array),
        "tuple"     => Some(DataType::Tuple),
        "hex"       => Some(DataType::Hex),
        "blob"      => Some(DataType::Blob),
        "regex"     => Some(DataType::Regex),
        "object"    => Some(DataType::Object),
        "timestamp" => Some(DataType::Timestamp),
        "date"      => Some(DataType::Date),
        "enum"      => Some(DataType::Enum),
        _           => None,
    };

    if base.is_none() { return None; }

    self.advance(); // consume the base type keyword

    // Typed-collection syntax: array<int>, tuple<int,bool>
    if (kw == "array" || kw == "tuple") && self.is_current_symbol('<') {
        return self.parse_typed_collection(kw);
    }

    base
                    }

    /// Parse the inner `<elemType>` or `<e1,e2,...>` after the base keyword is consumed.
/// Handles the inner `<` and inner `>` (including `>>` split for `<array<int>>`).
fn parse_typed_collection(&mut self, base_kw: &str) -> Option<DataType> {
    self.advance(); // consume inner '<'

    if base_kw == "array" {
        let elem = self.parse_elem_type_keyword();

        if !self.match_and_consume_closing_angle() {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '>' to close array element type annotation (e.g. <array<int>>)",
                &current,
            );
            if self.should_halt_section() { return None; }
        }

        return Some(match elem {
            Some(e) => DataType::TypedArray(e),
            None    => DataType::Array, // fallback to untyped on parse error
        });
    }

    // tuple: parse up to 6 comma-separated element types
    let mut elems: [Option<ElemType>; 6] = [None; 6];
    let mut count = 0usize;

    loop {
        if count >= 6 {
            // Skip the rest; semantic analysis will report TUPLE_TOO_LARGE
            while !self.is_at_end() && !self.is_closing_angle() {
                self.advance();
            }
            break;
        }

        let elem = self.parse_elem_type_keyword();
        elems[count] = elem;
        count += 1;

        if self.is_current_symbol(',') {
            self.advance();
        } else {
            break;
        }
    }

    if !self.match_and_consume_closing_angle() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            "Expected '>' to close tuple element types annotation (e.g. <tuple<int,bool>>)",
            &current,
        );
        if self.should_halt_section() { return None; }
    }

    Some(if count == 0 {
        DataType::Tuple // empty inner list — treat as untyped
    } else {
        DataType::TypedTuple(elems)
    })
    }

    fn parse_elem_type_keyword(&mut self) -> Option<ElemType> {
    let lower: Option<String> = match &self.current().token_type {
        TokenType::Keyword(kw)    => Some(kw.to_lowercase()),
        TokenType::Identifier(id) => Some(id.to_lowercase()),
        _ => None,
    };

    match lower {
        Some(ref s) => {
            let elem = ElemType::from_keyword(s.as_str());
            if elem.is_some() {
                self.advance(); // consume the type keyword
                // If the element type is itself a typed collection
                // (e.g. tuple<int,bool> nested inside <array<tuple<int,bool>>>),
                // skip the inner <...> to stay in sync.
                // ElemType is intentionally flat and cannot represent nested
                // collection types, so we discard the inner annotation after
                // consuming it — semantic analysis enforces limits separately.
                if self.is_current_symbol('<') {
                    self.skip_nested_angle_content();
                }
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Unknown element type '{}' in typed collection annotation; \
                         valid types: int, long, float, double, string, bool, \
                         hex, blob, regex, object, date, timestamp, enum, any, array, tuple",
                        s
                    ),
                    &current,
                );
                if !self.should_halt_section() {
                    self.advance(); // skip invalid token, continue
                }
            }
            elem
        }
        None => {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                "Expected element type keyword (int, string, bool, …) in typed collection annotation",
                &current,
            );
            None
        }
    }
                }

    fn parse_table_path(&mut self) -> Option<TablePath> {
        let mut segments = Vec::new();
        let first_segment = self.parse_property_name()?;
        segments.push(first_segment);

        while self.is_current_symbol('.') {
            self.advance();
            let segment = match self.parse_property_name() {
                Some(s) => s,
                None => {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        "Expected identifier after '.' in table path",
                        &current,
                    );
                    break;
                }
            };
            segments.push(segment);
        }

        Some(TablePath::new(segments))
    }

    fn parse_property_assignment(&mut self) -> Option<PropertyAssignment> {
    if self.debug_config.is_verbose {
        self.error_manager
            .log_info("ParsePropertyAssignment START");
    }

    let assign_pos = Position::from_token(self.current());
    let assignment_name = self.parse_property_name()?;

    if self.debug_config.is_verbose {
        self.error_manager.log_info(&format!(
            "Parsed assignment name: {}",
            assignment_name
        ));
    }

    let data_type = self.parse_optional_type_annotation();

    // Use consume_equal so that a '=' that was part of a fused '>>=' token is handled.
    if !self.consume_equal() {
        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::MissingToken,
            &format!("Expected '=' after assignment name '{}'", assignment_name),
            &current,
        );
        if self.should_halt_section() {
            return None;
        }
        return None;
    }

    let value = match self.parse_property_value() {
        Some(v) => v,
        None => {
            if self.should_halt_section() {
                return None;
            }
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::UnexpectedToken,
                &format!(
                    "Expected value after '=' in assignment '{}'",
                    assignment_name
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            Value::Null { position: assign_pos }
        }
    };

    if self.debug_config.is_verbose {
        self.error_manager.log_info(&format!(
            "ParsePropertyAssignment END: {} = {}",
            assignment_name, value
        ));
    }
    Some(PropertyAssignment::new(assignment_name, data_type, value, assign_pos))
}

    fn handle_data_entry_comma_separation(&mut self) -> bool {
        self.log_verbose("Handling data entry separation");

        if self.is_current_symbol(',') {
            self.advance();
            self.log_verbose("Consumed optional comma between data entries");
            return true;
        }

        if self.is_current_symbol(')') {
            self.log_verbose("Found closing parenthesis, ending DATA entries");
            return true;
        }

        if self.is_next_data_entry() {
            self.log_verbose("No comma — moving to next data entry (comma optional)");
            return true;
        }

        if self.is_at_end() {
            return true;
        }

        let current = self.current().clone();
        self.handle_parse_error(
            ParseErrorType::UnexpectedToken,
            &format!(
                "Unexpected token in DATA section: {:?}",
                current.token_type
            ),
            &current,
        );

        if self.should_halt_section() {
            return false;
        }

        self.ensure_progress();
        true
    }

    fn is_next_data_entry(&self) -> bool {
        let current_token = self.current();
        if matches!(current_token.token_type, TokenType::Identifier(_)) {
            return true;
        }
        if let TokenType::Keyword(k) = &current_token.token_type {
            if !k.starts_with('@') {
                return true;
            }
        }
        false
    }

    fn is_start_of_new_data_entry(&self) -> bool {
        if !matches!(self.current().token_type, TokenType::Identifier(_)) {
            return false;
        }
        if let Some(next) = self.peek_ahead(1) {
            match &next.token_type {
                TokenType::Symbol(sym) if *sym == '.' || *sym == ':' => return true,
                TokenType::DoubleColon => return true,
                _ => {}
            }
        }
        false
    }

    fn is_start_of_new_grouped_data_entry(&self) -> bool {
        if !matches!(self.current().token_type, TokenType::Identifier(_)) {
            return false;
        }
        if let Some(next) = self.peek_ahead(1) {
            if matches!(next.token_type, TokenType::DoubleColon) {
                return true;
            }
        }

        let mut look_ahead = 1;
        while let Some(token) = self.peek_ahead(look_ahead) {
            if let TokenType::Symbol('(') = token.token_type {
                return false;
            }
            if let TokenType::Symbol(':') = token.token_type {
                return true;
            }
            if matches!(token.token_type, TokenType::DoubleColon) {
                return true;
            }
            if let TokenType::Symbol('.') = token.token_type {
                look_ahead += 1;
                if let Some(next_token) = self.peek_ahead(look_ahead) {
                    if matches!(next_token.token_type, TokenType::Identifier(_)) {
                        look_ahead += 1;
                        continue;
                    }
                }
                return false;
            }
            return false;
        }
        false
    }

    fn consume_double_colon(&mut self) -> bool {
        if matches!(self.current().token_type, TokenType::DoubleColon) {
            self.advance();
            return true;
        }
        if self.is_current_symbol(':') {
            if let Some(next) = self.peek_ahead(1) {
                if let TokenType::Symbol(':') = next.token_type {
                    self.advance();
                    self.advance();
                    return true;
                }
            }
        }
        false
    }

    #[inline]
    fn peek_ahead(&self, offset: usize) -> Option<&Token> {
        let target = self.position.checked_add(offset)?;
        self.tokens.get(target)
    }

    #[inline]
    fn current(&self) -> &Token {
        static EOF_TOKEN: Token = Token {
            token_type: TokenType::EndOfFile,
            line: 1,
            column: 1,
            section: SectionId::None,
        };
        self.tokens.get(self.position).unwrap_or(&EOF_TOKEN)
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
            || matches!(self.current().token_type, TokenType::EndOfFile)
    }

    #[inline]
    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    #[inline]
    fn is_current_symbol(&self, symbol: char) -> bool {
        matches!(&self.current().token_type, TokenType::Symbol(s) if *s == symbol)
    }

    #[inline]
    fn match_and_consume_symbol(&mut self, symbol: char) -> bool {
        if self.is_current_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }
    /// True if the current token is `>` or the first `>` of `>>` / `>>=` / `>=`.
#[inline]
fn is_closing_angle(&self) -> bool {
    match &self.current().token_type {
        TokenType::Symbol('>') => true,
        TokenType::BitwiseOp(op) if *op == ">>" || *op == ">>=" => true,
        TokenType::ComparisonOp(op) if *op == ">=" => true,
        _ => false,
    }
}

/// Consume one closing `>`.
/// For plain `>`: advance normally.
/// For `>>` (emitted as `BitwiseOp(">>")`): advance and set `pending_angle = true`
/// so the next call returns `true` without advancing (acts as the second `>`).
/// For `>>=`: advance and set both `pending_angle` and `pending_equal`.
/// For `>=` (emitted as `ComparisonOp(">=")`): advance and set `pending_equal = true`
/// so the next `=` consumption succeeds without reading another token.
fn match_and_consume_closing_angle(&mut self) -> bool {
    if self.pending_angle {
        self.pending_angle = false;
        return true;
    }
    if self.is_current_symbol('>') {
        self.advance();
        return true;
    }
    if let TokenType::BitwiseOp(op) = &self.current().token_type {
        if *op == ">>" {
            self.advance();
            self.pending_angle = true;
            return true;
        }
        if *op == ">>=" {
            self.advance();
            self.pending_angle = true;
            self.pending_equal = true;
            return true;
        }
    }
    if let TokenType::ComparisonOp(op) = &self.current().token_type {
        if *op == ">=" {
            // Tokenizer fused '>' and '=' into one token (no space between them).
            // '>' closes this annotation level; '=' is left pending for consume_equal().
            self.advance();
            self.pending_equal = true;
            return true;
        }
    }
    false
}
    /// Consume a balanced `<…>` sequence starting at the current position
/// (the opening `<` has NOT yet been consumed when this is called).
///
/// Tokenizer output to keep in mind:
///   `>`     → Symbol('>')           — one closing angle
///   `>=`    → ComparisonOp(">=")    — one closing angle + fused '='
///   `>>`    → BitwiseOp(">>")       — two closing angles (pending_angle mechanism)
///   `>>=`   → BitwiseOp(">>=")      — two closing angles + fused '='
///
/// When consuming `>>` would close our innermost level and leave one
/// spare `>` that belongs to the outer annotation, we advance past the
/// `>>` token and set `pending_angle = true`.  The outer parser's next
/// call to `match_and_consume_closing_angle` returns true immediately
/// without consuming another token, acting as that spare `>`.
fn skip_nested_angle_content(&mut self) {
    let mut depth = 0i32;

    while !self.is_at_end() {
        let tt = self.current().token_type.clone();

        match tt {
            TokenType::Symbol('<') => {
                depth += 1;
                self.advance();
            }

            TokenType::Symbol('>') => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                self.advance();
                if depth == 0 {
                    break;
                }
            }

            // ">=" — one closing angle plus '=' fused by the tokenizer (no space).
            TokenType::ComparisonOp(ref op) if *op == ">=" => {
                if depth == 0 {
                    // The '>' belongs outside our scope — do not consume.
                    break;
                }
                depth -= 1;
                self.advance();
                self.pending_equal = true;
                if depth == 0 {
                    break;
                }
                // depth > 0: the '=' was embedded inside nested content.
                // pending_equal is set; loop continues for the remaining depth.
            }

            // ">>" — two closing angles fused by the tokenizer.
            TokenType::BitwiseOp(ref op) if *op == ">>" => {
                match depth {
                    0 => {
                        // Both angles are outside our scope — do not consume.
                        break;
                    }
                    1 => {
                        // First '>' closes our level; second '>' belongs to the outer context.
                        self.advance();
                        self.pending_angle = true;
                        break;
                    }
                    _ => {
                        // depth >= 2: both angles are within our nested content.
                        depth -= 2;
                        self.advance();
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }

            // ">>=" — two closing angles plus assignment fused by the tokenizer.
            TokenType::BitwiseOp(ref op) if *op == ">>=" => {
                match depth {
                    0 => {
                        // All three chars are outside our scope — do not consume.
                        break;
                    }
                    1 => {
                        // First '>' closes our level, second '>' is pending outer, '=' is pending.
                        self.advance();
                        self.pending_angle = true;
                        self.pending_equal = true;
                        break;
                    }
                    _ => {
                        // depth >= 2: both '>'s consumed within nested content, '=' is pending.
                        depth -= 2;
                        self.advance();
                        self.pending_equal = true;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }

            TokenType::EndOfFile => break,

            _ => {
                self.advance();
            }
        }
    }
}

    /// Consume a `=` token, either the real current token or a virtual one left
/// behind when `>>=` was split by `match_and_consume_closing_angle`.
#[inline]
fn consume_equal(&mut self) -> bool {
    if self.pending_equal {
        self.pending_equal = false;
        return true;
    }
    self.match_and_consume_symbol('=')
}

/// Look-ahead helper: given `at` pointing at a `<` token, returns
/// `(new_position, eq_was_fused)`.
///
/// `new_position` is the lookahead offset of the token immediately after the
/// closing `>`.  `eq_was_fused` is `true` when the closing `>` was part of a
/// fused `>=` or `>>=` token, meaning the `=` that separates the annotation
/// from the value was already consumed by that token and must NOT be sought
/// again as a separate `Symbol('=')`.
///
/// Handles all fused-token forms produced by the tokenizer:
///   `>`     → Symbol('>')           — one closing angle
///   `>=`    → ComparisonOp(">=")   — one closing angle + fused '='
///   `>>`    → BitwiseOp(">>")      — two closing angles
///   `>>=`   → BitwiseOp(">>=")     — two closing angles + fused '='
fn skip_annotation_lookahead(&self, at: usize) -> (usize, bool) {
    // `at` points to the opening '<'; start scanning from the token after it.
    let mut pos   = at + 1;
    let mut depth = 1i32;

    while let Some(token) = self.peek_ahead(pos) {
        match &token.token_type {
            TokenType::Symbol('<') => {
                depth += 1;
                pos   += 1;
            }
            TokenType::Symbol('>') => {
                depth -= 1;
                pos   += 1;
                if depth == 0 { return (pos, false); }
            }
            // ">=" — one closing angle with a fused '='
            TokenType::ComparisonOp(op) if *op == ">=" => {
                depth -= 1;
                pos   += 1;
                if depth == 0 { return (pos, true); }
                // depth > 0: the '=' is embedded inside nested angle content;
                // keep scanning.
            }
            // ">>" — two closing angles in one token
            TokenType::BitwiseOp(op) if *op == ">>" => {
                depth -= 2;
                pos   += 1;
                if depth <= 0 { return (pos, false); }
            }
            // ">>=" — two closing angles + fused '='
            TokenType::BitwiseOp(op) if *op == ">>=" => {
                depth -= 2;
                pos   += 1;
                if depth <= 0 { return (pos, true); }
                // depth > 0: the '=' is inside nested content; keep scanning.
            }
            TokenType::EndOfFile => break,
            _ => { pos += 1; }
        }
    }

    (pos, false)
            }
    

    fn handle_parse_error(&mut self, error_type: ParseErrorType, message: &str, token: &Token) {
        let source_line = self.get_source_line(token);
        self.error_manager.add_parse_error(
            error_type,
            message.to_string(),
            token.line,
            token.column,
            None,
            source_line,
        );
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Error (strategy: {:?}): {}",
                self.operational_settings.error_handling_strategy, message
            ));
        }
    }

    #[inline]
    fn should_halt_section(&self) -> bool {
        self.error_manager.should_terminate_parsing()
    }

    fn handle_section_failure(&self, start_pos: Position) -> Option<DataSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            self.error_manager
                .log_error("DATA section parsing halted due to errors");
            None
        } else {
            self.error_manager.log_warning(
                "DATA section parsing completed with errors — returning empty section",
            );
            Some(DataSection::new(Vec::new(), start_pos))
        }
    }

    fn get_source_line(&self, token: &Token) -> Option<String> {
        let line_tokens: Vec<&Token> = self
            .tokens
            .iter()
            .filter(|t| t.line == token.line)
            .collect();

        if line_tokens.is_empty() {
            return None;
        }

        let mut source_line = String::new();
        let mut current_column = 0usize;

        for t in line_tokens {
            while current_column < t.column {
                source_line.push(' ');
                current_column += 1;
            }
            let token_value = t.get_token_value();
            source_line.push_str(&token_value);
            current_column += token_value.len();
        }

        Some(source_line)
    }

    // Helpers that gate the debug_config check internally.
    // Only pass static string literals here; for format!() calls, inline the guard.
    #[inline]
    fn log_debug(&self, message: &str) {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(message);
        }
    }

    #[inline]
    fn log_verbose(&self, message: &str) {
        if self.debug_config.is_verbose {
            self.error_manager.log_info(message);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataEntryType {
    Unknown,
    SimpleProperty,
    TableProperty,
    GroupArray,
    ObjectProperty,
}
