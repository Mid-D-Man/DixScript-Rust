use super::{CallGraph, FunctionCallCollector};
use crate::Compiler::AST::{QuickFuncsSection, Position};
use crate::Compiler::Core::OperationalSettings;
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use std::collections::HashSet;

/// Validates QuickFunction call graphs for circular dependencies
/// Integrates with QuickFuncsSectionAnalyzer to catch cycles at compile-time
///
/// Detection Strategy:
/// 1. Build call graph by scanning all function bodies
/// 2. Run DFS-based cycle detection (O(V+E) complexity)
/// 3. Report detailed errors with call paths if cycles found
/// 4. Validate that all called functions are defined
pub struct CycleDetectionValidator {
    error_manager: ErrorManager,
    operational_settings: OperationalSettings,
}

impl CycleDetectionValidator {
    pub fn new(
        error_manager: ErrorManager,
        operational_settings: OperationalSettings,
    ) -> Self {
        CycleDetectionValidator {
            error_manager,
            operational_settings,
        }
    }
    /// Main entry point: Validate function calls in a QuickFuncs section
    /// Returns true if validation passed (no cycles), false otherwise
    pub fn validate_function_calls(&self, section: &QuickFuncsSection) -> bool {
        self.log_debug("Building function call graph...");
        
        // Step 1: Build call graph
        let call_graph = self.build_call_graph(section);
        
        // Log debug info if enabled
        if self.operational_settings.debug_mode >= crate::Compiler::AST::DebugMode::Regular {
            self.log_debug(&call_graph.to_debug_string());
            
            let stats = call_graph.get_statistics();
            self.log_debug(&format!("Graph statistics: {}", stats));
        }
        
        // Step 2: Detect cycles
        self.log_debug("Detecting cycles...");
        let cycles = call_graph.detect_cycles();
        
        let mut has_errors = false;
        
        if !cycles.is_empty() {
            self.error_manager.log_warning(&format!(
                "Detected {} circular dependency cycle(s)",
                cycles.len()
            ));
            
            // Step 3: Report each cycle
            for cycle in &cycles {
                self.report_cycle(cycle, &call_graph);
            }
            
            has_errors = true;
        } else {
            self.log_debug("No cycles detected - call graph is acyclic (DAG)");
        }
        
        // Step 4: Check for undefined function calls
        self.log_debug("Validating that all called functions are defined...");
        if !self.validate_all_calls_are_defined(section, &call_graph) {
            has_errors = true;
        }
        
        // Step 5: Report statistics
        if self.operational_settings.debug_mode >= crate::Compiler::AST::DebugMode::Regular {
            let stats = call_graph.get_statistics();
            self.error_manager.log_info(&format!(
                "Call graph validation complete: {}",
                stats
            ));
        }
        
        !has_errors
    }
    
    /// Build the call graph by scanning all function bodies
    fn build_call_graph(&self, section: &QuickFuncsSection) -> CallGraph {
        let mut call_graph = CallGraph::new();
        let mut collector = FunctionCallCollector::new(&mut call_graph);
        
        // Analyze each function
        for func in &section.functions {
            collector.analyze_function(func);
        }
        
        call_graph
    }
    
    /// Report a detected cycle with detailed error message
    fn report_cycle(&self, cycle: &[String], call_graph: &CallGraph) {
        if cycle.len() < 2 {
            return; // Not a real cycle
        }
        
        // Build cycle path for error message
        let cycle_path = cycle.join(" → ");
        
        // Primary error message
        let message = format!("Circular function call detected: {}", cycle_path);
        let suggestion = self.build_cycle_suggestion(cycle);
        
        // Use first function's position (or unknown if not available)
        let position = self.get_cycle_position(cycle, call_graph);
        
        self.error_manager.add_semantic_error(
            SemanticErrorType::InvalidReference, // Or add CircularFunctionCall type
            message,
            position.line as i32,
            position.column as i32,
            Some("QUICKFUNCS".to_string()),
            Some(suggestion),
        );
        
        // Add detailed call site information for each edge in the cycle
        self.report_cycle_details(cycle, call_graph);
    }
    
    /// Get position of first call in cycle
    fn get_cycle_position(&self, cycle: &[String], call_graph: &CallGraph) -> Position {
        if cycle.len() >= 2 {
            let sites = call_graph.get_call_sites(&cycle[0], &cycle[1]);
            if let Some(site) = sites.first() {
                return site.position;
            }
        }
        Position::UNKNOWN
    }
    
    /// Generate helpful suggestion for breaking a cycle
    fn build_cycle_suggestion(&self, cycle: &[String]) -> String {
        let mut suggestion = String::from("Remove or break the circular dependency.\n");
        
        if cycle.len() == 2 && cycle[0] == cycle[cycle.len() - 1] {
            // Direct recursion: funcA → funcA
            suggestion.push_str(&format!(
                "Function '{}' calls itself directly (recursion).\n\
                 DixScript does not support recursion.\n\
                 Consider restructuring '{}' to avoid self-reference.",
                cycle[0], cycle[0]
            ));
        } else {
            // Indirect cycle: funcA → funcB → funcC → funcA
            suggestion.push_str(
                "This is an indirect circular dependency.\n\
                 Consider one of these approaches:\n"
            );
            
            if cycle.len() >= 2 {
                let last_idx = cycle.len() - 2;
                suggestion.push_str(&format!(
                    "  1. Remove the call from '{}' to '{}'\n\
                     2. Extract shared logic into a new utility function\n\
                     3. Restructure the functions to avoid the circular dependency",
                    cycle[last_idx], cycle[cycle.len() - 1]
                ));
            }
        }
        
        suggestion
    }
    
    /// Report detailed information about each call in the cycle
    fn report_cycle_details(&self, cycle: &[String], call_graph: &CallGraph) {
        self.error_manager.log_error("  Cycle details:");
        
        for i in 0..cycle.len() - 1 {
            let caller = &cycle[i];
            let callee = &cycle[i + 1];
            
            // Find the call site(s) for this edge
            let call_sites = call_graph.get_call_sites(caller, callee);
            
            if let Some(call_site) = call_sites.first() {
                if call_site.position.is_valid() {
                    self.error_manager.log_error(&format!(
                        "    {}. {} calls {} at {}",
                        i + 1,
                        caller,
                        callee,
                        call_site.position
                    ));
                } else {
                    self.error_manager.log_error(&format!(
                        "    {}. {} calls {}",
                        i + 1,
                        caller,
                        callee
                    ));
                }
            } else {
                // Shouldn't happen, but handle gracefully
                self.error_manager.log_error(&format!(
                    "    {}. {} calls {} (location unknown)",
                    i + 1,
                    caller,
                    callee
                ));
            }
        }
    }
    
    /// Validate that all function calls reference defined functions
    /// Returns true if all calls are valid, false otherwise
    fn validate_all_calls_are_defined(
        &self,
        section: &QuickFuncsSection,
        call_graph: &CallGraph,
    ) -> bool {
        // Build set of all defined functions
        let defined_functions: HashSet<&str> = section
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        
        let mut all_valid = true;
        
        // Check each call
        for func in &section.functions {
            let callees = call_graph.get_callees(&func.name);
            
            for callee in callees {
                if !defined_functions.contains(callee) {
                    // Find the call site for better error reporting
                    let call_sites = call_graph.get_call_sites(&func.name, callee);
                    let position = call_sites
                        .first()
                        .map(|cs| cs.position)
                        .unwrap_or(Position::UNKNOWN);
                    
                    let message = format!(
                        "Function '{}' calls undefined function '{}'",
                        func.name, callee
                    );
                    let suggestion = format!(
                        "Define function '{}' in the @QUICKFUNCS section, \
                         or remove the call if it's not needed.",
                        callee
                    );
                    
                    self.error_manager.add_semantic_error(
                        SemanticErrorType::UndefinedReference,
                        message,
                        position.line as i32,
                        position.column as i32,
                        Some("QUICKFUNCS".to_string()),
                        Some(suggestion),
                    );
                    
                    all_valid = false;
                }
            }
        }
        
        all_valid
    }
    
    /// Optional: Get execution order for acyclic graphs (useful for debugging)
    /// Returns None if graph has cycles
    pub fn get_execution_order(&self, section: &QuickFuncsSection) -> Option<Vec<String>> {
        let call_graph = self.build_call_graph(section);
        call_graph.get_topological_sort()
    }
    
    // Helper for debug logging (checks debug mode before formatting)
    fn log_debug(&self, message: &str) {
        if self.operational_settings.debug_mode >= crate::Compiler::AST::DebugMode::Regular {
            self.error_manager.log_debug(&format!("[Cycle Detector] {}", message));
        }
    }
}
