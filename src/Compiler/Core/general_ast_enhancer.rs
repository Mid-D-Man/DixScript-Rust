

        analysis_result: Option<&SemanticAnalysisResult>,
        result: &mut EnhancementResult,
    ) -> DixScript {
        if ast.quick_functions.is_none() {
            self.error_manager.log_debug("No QuickFunctions section to enhance");
            return ast;
        }
        
        // Check if feature is enabled
        let version_manager = VersionManager::instance().read().unwrap();
        if !version_manager.supports_feature("parameter_defaults") {
            self.error_manager.log_warning(
                "Parameter defaults not supported in this version"
            );
            return ast;
        }
        drop(version_manager);
        
        let section = ast.quick_functions.as_ref().unwrap();
        
        self.error_manager.log_debug(&format!(
            "Enhancing {} functions",
            section.functions.len()
        ));
        
        let mut enhancer = QuickFunctionsAstEnhancer::new(self.operational_settings.clone());
        
        // ⭐ Get QuickFunctions analysis result (if available)
        let quickfuncs_analysis = analysis_result
            .and_then(|ar| ar.section_results.get("QUICKFUNCS"));
        
        let enhanced_section = enhancer.enhance(section, quickfuncs_analysis);
        
        // Track enhancements
        result.enhancements_by_section.insert(
            "QUICKFUNCS".to_string(),
            SectionEnhancementInfo {
                section_name: "QUICKFUNCS".to_string(),
                enhancements_applied: enhancer.get_enhancement_count(),
                duration: enhancer.get_enhancement_duration(),
            },
        );
        
        // Update AST with enhanced section
        ast.quick_functions = Some(enhanced_section);
        
        ast
    }
    
    /// Check if enhancement should terminate early
    fn should_terminate(&self, result: &EnhancementResult) -> bool {
        !result.warnings.is_empty()
            && matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            )
    }
    
    /// Finalize enhancement result
    fn finalize_result(&self, original_ast: DixScript, mut result: EnhancementResult) -> EnhancementResult {
        if result.enhanced_ast.is_none() {
            result.enhanced_ast = Some(original_ast);
        }
        
        result
    }
}


/// Result of AST enhancement phase
#[derive(Debug, Clone)]
pub struct EnhancementResult {
    pub is_success: bool,
    pub enhanced_ast: Option<DixScript>,
    pub enhancement_duration: Duration,
    pub warnings: Vec<String>,
    pub enhancements_by_section: HashMap<String, SectionEnhancementInfo>,
}


impl EnhancementResult {
    pub fn new() -> Self {
        EnhancementResult {
            is_success: false,
            enhanced_ast: None,
            enhancement_duration: Duration::default(),
            warnings: Vec::new(),
            enhancements_by_section: HashMap::new(),
        }
    }
    
    pub fn total_enhancements(&self) -> usize {
        self.enhancements_by_section
            .values()
            .map(|info| info.enhancements_applied)
            .sum()
    }
}


impl Default for EnhancementResult {
    fn default() -> Self {
        Self::new()
    }
}


/// Information about enhancements applied to a section
#[derive(Debug, Clone)]
pub struct SectionEnhancementInfo {
    pub section_name: String,
    pub enhancements_applied: usize,
    pub duration: Duration,
}
