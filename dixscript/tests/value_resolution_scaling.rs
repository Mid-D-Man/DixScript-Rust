// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/value_resolution_scaling.rs"
// ============================================================================
//! Guards value resolution against becoming quadratic in the number of resolved calls.
//!
//! Every resolved call is written back into the data tree. When that write-back searched
//! the whole tree for the call's position, compile time grew with the square of the input:
//! a 4x larger input took about 16x as long. This test compiles a small and a 4x larger
//! source and checks that the time grew by far less than 16x.
//!
//! The check is a ratio of two timings taken in the same process, so machine speed cancels
//! out. Linear growth is 4x and quadratic is 16x; the limit sits between them with room for
//! timing noise. Each size is timed three times and the fastest run is used.

use dixscript::Runtime::DixLoader;
use std::time::{Duration, Instant};

/// Each resolved call sits after a data entry of 100 values, so a search of the whole tree per
/// call has a lot to walk past.
fn source(calls: usize) -> String {
    const FILLER: &str = "0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99";
    let mut s = String::from(
        "@CONFIG(\n    version -> \"1.0.0\",\n    encoding -> \"utf-8\",\n    debug_mode -> \"off\",\n    error_handling -> \"halt\",\n    features -> \"advanced\"\n)\n\n@QUICKFUNCS(\n    ~inc<int>(x<int>) {\n        return x + 1;\n    }\n)\n\n@DATA(\n",
    );
    for i in 0..calls {
        s.push_str(&format!(
            "    filler_{i} = [{}],\n",
            FILLER
        ));
        let sep = if i + 1 < calls { "," } else { "" };
        s.push_str(&format!("    call_{i} = inc({i}){sep}\n"));
    }
    s.push(')');
    s
}

fn fastest_compile(calls: usize) -> Duration {
    let path = std::env::temp_dir().join(format!("dixscript_scaling_{}_{}.mdix", std::process::id(), calls));
    std::fs::write(&path, source(calls)).expect("write the temporary source");
    let loader = DixLoader::new_silent();
    let path_str = path.to_str().expect("temp path is valid UTF-8");

    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        let result = loader.compile_to_resolved_ast(path_str);
        let elapsed = start.elapsed();
        assert!(result.is_ok(), "the generated source must compile");
        best = best.min(elapsed);
    }
    let _ = std::fs::remove_file(&path);
    best
}

#[test]
fn resolving_four_times_as_many_calls_takes_far_less_than_sixteen_times_as_long() {
    let _warm_up = fastest_compile(50);
    let small = fastest_compile(400);
    let large = fastest_compile(1600);
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    println!("400 calls: {:?}, 1600 calls: {:?}, ratio {:.1}x (linear 4x, quadratic 16x)", small, large, ratio);
    assert!(ratio < 7.0, "4x the calls took {:.1}x as long, which is not close to linear", ratio);
}
