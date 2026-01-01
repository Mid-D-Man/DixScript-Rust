#!/bin/bash

# Function to create a stub file
create_stub() {
    local file_path="$1"
    local type_name="$2"
    
    cat > "$file_path" << EOF
//! $type_name - Placeholder

// TODO: Implement $type_name
EOF
}

# Builtins/Core
create_stub "src/Builtins/Core/dix_type.rs" "DixType"
create_stub "src/Builtins/Core/dix_value.rs" "DixValue"
create_stub "src/Builtins/Core/i_builtin_method.rs" "IBuiltinMethod"

# Builtins/Instance
create_stub "src/Builtins/Instance/array_methods.rs" "ArrayMethods"
create_stub "src/Builtins/Instance/number_methods.rs" "NumberMethods"
create_stub "src/Builtins/Instance/string_methods.rs" "StringMethods"
create_stub "src/Builtins/Instance/tuple_methods.rs" "TupleMethods"
create_stub "src/Builtins/Instance/universal_methods.rs" "UniversalMethods"
create_stub "src/Builtins/Instance/instance_method_registry.rs" "InstanceMethodRegistry"

# Builtins/Static
create_stub "src/Builtins/Static/array_object.rs" "ArrayObject"
create_stub "src/Builtins/Static/date_time_object.rs" "DateTimeObject"
create_stub "src/Builtins/Static/enum_object.rs" "EnumObject"
create_stub "src/Builtins/Static/guid_object.rs" "GuidObject"
create_stub "src/Builtins/Static/ip_address_object.rs" "IpAddressObject"
create_stub "src/Builtins/Static/math_object.rs" "MathObject"
create_stub "src/Builtins/Static/random_object.rs" "RandomObject"
create_stub "src/Builtins/Static/static_object_registry.rs" "StaticObjectRegistry"

# Builtins/Resolver
create_stub "src/Builtins/Resolver/builtin_call_resolver.rs" "BuiltinCallResolver"
create_stub "src/Builtins/Resolver/compile_time_validator.rs" "CompileTimeValidator"

# Compiler/AST
create_stub "src/Compiler/AST/ast_helpers.rs" "ASTHelpers"
create_stub "src/Compiler/AST/position.rs" "Position"

# Compiler/AST/Visitors
create_stub "src/Compiler/AST/Visitors/ast_visitor_base.rs" "ASTVisitorBase"
create_stub "src/Compiler/AST/Visitors/type_inference_visitor.rs" "TypeInferenceVisitor"

# Compiler/Core
create_stub "src/Compiler/Core/lexer.rs" "Lexer"
create_stub "src/Compiler/Core/general_parser.rs" "GeneralParser"
create_stub "src/Compiler/Core/general_semantics_analyzer.rs" "GeneralSemanticsAnalyzer"
create_stub "src/Compiler/Core/general_ast_enhancer.rs" "GeneralAstEnhancer"
create_stub "src/Compiler/Core/config_section_handler.rs" "ConfigSectionHandler"
create_stub "src/Compiler/Core/parser_collection_helper.rs" "ParserCollectionHelper"

# Runtime
create_stub "src/Runtime/dix.rs" "Dix"
create_stub "src/Runtime/dix_data.rs" "DixData"
create_stub "src/Runtime/dix_loader.rs" "DixLoader"
create_stub "src/Runtime/dix_data_builder.rs" "DixDataBuilder"
create_stub "src/Runtime/dix_serializer.rs" "DixSerializer"
create_stub "src/Runtime/dix_converter.rs" "DixConverter"
create_stub "src/Runtime/dix_compactor.rs" "DixCompactor"
create_stub "src/Runtime/dix_load_options.rs" "DixLoadOptions"
create_stub "src/Runtime/dix_format_options.rs" "DixFormatOptions"
create_stub "src/Runtime/key_file_resolver.rs" "KeyFileResolver"

echo "✅ All stub files created!"
