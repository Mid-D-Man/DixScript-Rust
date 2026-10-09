//! Counts heap allocations in a real compile, and how many of them are the AST itself.
//! The AST's allocation count is measured by cloning it: every String / Vec / Box in
//! the tree is one allocation, and `clone()` makes exactly one per original.
use dixscript::Runtime::DixLoader;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES:  AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { ALLOCS.fetch_add(1, Relaxed); BYTES.fetch_add(l.size(), Relaxed); System.alloc(l) }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l) }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { ALLOCS.fetch_add(1, Relaxed); BYTES.fetch_add(n, Relaxed); System.realloc(p, l, n) }
}
#[global_allocator] static A: Counting = Counting;

fn snap() -> (usize, usize) { (ALLOCS.load(Relaxed), BYTES.load(Relaxed)) }

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    for path in files {
        let src_len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let loader = DixLoader::new_silent();
        // warm-up (caches, lazy statics) so the counts below are steady-state
        let _ = loader.compile_to_resolved_ast(&path);
        let (a0, b0) = snap();
        let t = Instant::now();
        let ast = match loader.compile_to_resolved_ast(&path) { Ok(a) => a, Err(e) => { println!("{path}: ERROR {e}"); continue } };
        let dt = t.elapsed();
        let (a1, b1) = snap();
        let (ca0, cb0) = snap();
        let t2 = Instant::now();
        let cloned = ast.clone();
        let ct = t2.elapsed();
        let (ca1, cb1) = snap();
        std::hint::black_box(&cloned);
        let total_a = a1 - a0; let total_b = b1 - b0;
        let ast_a = ca1 - ca0;  let ast_b = cb1 - cb0;
        // best-of-5 for time
        let mut best = dt; let mut best_c = ct;
        for _ in 0..4 {
            let t = Instant::now(); let a = loader.compile_to_resolved_ast(&path).unwrap(); best = best.min(t.elapsed()); std::hint::black_box(&a);
            let t = Instant::now(); let c = a.clone(); best_c = best_c.min(t.elapsed()); std::hint::black_box(&c);
        }
        println!("{path}");
        println!("  source: {src_len} bytes");
        println!("  compile_to_resolved_ast: {} allocs, {} bytes, best {:?}", total_a, total_b, best);
        println!("  resolved AST alone (= allocs made by one clone): {} allocs, {} bytes; one clone takes best {:?}", ast_a, ast_b, best_c);
        println!("  => AST is {:.1}% of the allocations and {:.1}% of the bytes of a full compile; a clone costs {:.1}% of a compile's time",
                 100.0*ast_a as f64/total_a as f64, 100.0*ast_b as f64/total_b as f64, 100.0*best_c.as_secs_f64()/best.as_secs_f64());
    }
}
