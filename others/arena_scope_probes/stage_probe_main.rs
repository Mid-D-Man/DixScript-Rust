//! Per-stage time and allocation breakdown of a real compile (scratch probe).
use dixscript::Runtime::DixLoader;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;
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

static MARKS: Mutex<Vec<(&'static str, Instant, usize, usize)>> = Mutex::new(Vec::new());
fn hook(s: &'static str) { let (a, b) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed)); MARKS.lock().unwrap().push((s, Instant::now(), a, b)); }

fn main() {
    dixscript::probe::HOOK.set(hook).ok();
    for path in std::env::args().skip(1) {
        let loader = DixLoader::new_silent();
        let _ = loader.compile_to_resolved_ast(&path); // warm-up
        // take the fastest of 5 runs by total time, then report its stages
        let mut best: Option<(std::time::Duration, Vec<(&'static str, Instant, usize, usize)>, usize, usize)> = None;
        for _ in 0..5 {
            MARKS.lock().unwrap().clear();
            let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
            let t = Instant::now();
            let r = loader.compile_to_resolved_ast(&path);
            let dt = t.elapsed();
            let (a1, b1) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
            std::hint::black_box(&r);
            let m = MARKS.lock().unwrap().clone();
            if best.as_ref().map_or(true, |x| dt < x.0) { best = Some((dt, m, a1 - a0, b1 - b0)); }
        }
        let (dt, m, ta, tb) = best.unwrap();
        println!("{}  total {:?}, {} allocs, {} bytes", path, dt, ta, tb);
        println!("  {:14} {:>10} {:>7} {:>12} {:>7} {:>14}", "stage", "time", "% time", "allocs", "% alloc", "bytes");
        for w in m.windows(2) {
            let (n, t0, a0, b0) = (w[0].0, w[0].1, w[0].2, w[0].3);
            let (_, t1, a1, b1) = (w[1].0, w[1].1, w[1].2, w[1].3);
            let d = t1 - t0;
            println!("  {:14} {:>10.3?} {:>6.1}% {:>12} {:>6.1}% {:>14}", n, d, 100.0 * d.as_secs_f64() / dt.as_secs_f64(), a1 - a0, 100.0 * (a1 - a0) as f64 / ta as f64, b1 - b0);
        }
    }
}
