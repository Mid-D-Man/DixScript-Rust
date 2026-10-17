# dixscript: Runtime

## Modules

### `Runtime/format_options.rs`

**What it does:** Options for writing DixData back out as MDIX text: indentation
(spaces or tabs and the width), minifying, and the newline style. `get_indentation`
returns the indent string for a nesting level.

## Fixes and Problems

### `Runtime/format_options.rs`
- `get_indentation` borrowed a `String` built in the `else` arm of an `if` and used it
  after the `if` ended. Rust 1.79 lengthened temporaries in that position, so it built
  on 1.79 and later and failed on 1.75 with E0716. The `String` is now declared before
  the `if` and assigned inside the arm.
