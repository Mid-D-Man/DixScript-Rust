// src/Builtins/Resolver/mod.rs
//! Resolver - Resolves builtin method calls and validates them

pub mod builtin_call_resolver;
pub mod compile_time_validator;
pub mod instance_method_registry;
pub mod static_object_registry;

// Re-export commonly used items from builtin_call_resolver
pub use builtin_call_resolver::{
    convert_to_dix_value, determine_call_type, generate_documentation,
    get_instance_method_signature, get_instance_methods, get_static_method_signature,
    get_static_methods, get_static_objects, has_instance_method, has_static_method,
    has_static_object, initialize, is_initialized, resolve_instance_call,
    resolve_instance_call_with_conversion, resolve_static_call,
    resolve_static_call_with_conversion, validate_instance_call, validate_static_call,
    validate_static_call_with_types, CallType, CallValidationResult,
};

// Re-export commonly used items from compile_time_validator
pub use compile_time_validator::{
    generate_validation_report, get_instance_completions,
    get_instance_method_signature as get_instance_sig,
    get_method_signature, get_static_completions,
    validate_instance_call as validate_instance,
    validate_instance_call_with_types, validate_multiple_calls,
    validate_static_call as validate_static,
    validate_static_call_with_types as validate_static_with_types,
    CallValidationRequest, CompletionInfo, MethodSignatureInfo, ParameterInfo, ValidationReport,
    ValidationSummary,
};

// Re-export from instance_method_registry
pub use instance_method_registry::{
    call_instance_method, get_universal_methods, is_universal_method,
};

// Re-export from static_object_registry
pub use static_object_registry::{
    initialize_static_registry, has_static_object as registry_has_static_object,
    has_static_method as registry_has_static_method,
    call_static_method as registry_call_static_method,
    get_object_names, get_method_names as get_static_method_names,
};