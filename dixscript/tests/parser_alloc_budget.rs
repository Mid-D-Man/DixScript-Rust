// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "tests/parser_alloc_budget.rs"
// ============================================================================
//! Allocation budget for compiling a file with many nested constructs.
//!
//! The section parsers reserve a Vec for every call argument list, array, object and
//! statement block. When those reservations were sized from the whole token stream,
//! compiling `fixtures/alloc_budget.mdix` (53 KB) requested tens of megabytes. This test
//! counts the bytes the allocator is asked for, which is deterministic where wall time is
//! not, and fails if the figure climbs back toward that.
//!
//! This binary holds one test on purpose. The counter is process-wide, so a second test
//! running in parallel would be counted too.

use dixscript::Runtime::DixLoader;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;

static REQUESTED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        REQUESTED.fetch_add(layout.size(), Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        REQUESTED.fetch_add(new_size, Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Bytes requested by one compile. Measured: about 13.2 MB with the reservation cap and about
/// 88 MB without it. The budget sits between the two.
const BUDGET_BYTES: usize = 20_000_000;

#[test]
fn compiling_a_nested_file_stays_inside_its_allocation_budget() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/alloc_budget.mdix");
    let loader = DixLoader::new_silent();

    // Warm-up: lazy statics and one-time caches are not part of a steady-state compile.
    let _ = loader.compile_to_resolved_ast(path);

    let before = REQUESTED.load(Relaxed);
    let result = loader.compile_to_resolved_ast(path);
    let used = REQUESTED.load(Relaxed) - before;

    assert!(result.is_ok(), "the fixture must compile");
    println!("requested bytes: {}", used);
    assert!(used < BUDGET_BYTES, "compile requested {} bytes, budget is {}", used, BUDGET_BYTES);
}
