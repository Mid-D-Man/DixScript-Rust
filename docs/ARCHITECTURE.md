# DixScript Architecture

## Module Hierarchy

### DixCore
C#-like collection types for seamless porting:
- `ImmutableArray<T>` - Immutable array
- `List<T>` - Dynamic list
- `Dictionary<K, V>` - Hash map
- `HashSet<T>` - Hash set

### Utilities
Core utility types:
- `Result<T, E>` - Error handling (C# style)
- `Token` - Lexical tokens
- `MID_Logger` - Logging system

### Compiler
Full compilation pipeline:
- Lexer → Parser → Semantic Analysis → AST Enhancement → Value Resolution → Binary Serialization

### Runtime
Public API for loading and manipulating DixScript files.

## Naming Convention
**C# naming conventions throughout** for easy porting and cross-language compatibility.
