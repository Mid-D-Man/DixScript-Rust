#![no_main]

use libfuzzer_sys::fuzz_target;
use dixscript::Runtime::{DixLoader, DixLoadOptions};

fuzz_target!(|data: &[u8]| {
    // DixLoader::load_from_str takes &str — invalid UTF-8 would just be
    // rejected here before ever reaching the parser, so skip it early
    // rather than waste cycles. libfuzzer still explores the full byte
    // space; this just means non-UTF-8 inputs are cheap no-ops instead of
    // dead ends deep in the corpus.
    let Ok(source) = std::str::from_utf8(data) else { return; };

    let loader = DixLoader::new();

    // The only property under test: parsing arbitrary text must never
    // panic — not on malformed sections, truncated tokens, deeply nested
    // structures, unbalanced parens/quotes, or garbage that merely
    // resembles DixScript syntax. A returned Ok or Err are both correct,
    // expected outcomes; a panic is the only thing this harness flags.
    let _ = loader.load_from_str(source, &DixLoadOptions::new());
});
