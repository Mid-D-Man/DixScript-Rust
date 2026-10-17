//! Correctness gate for compile-pipeline performance work: compiles every `.mdix` file under a
//! directory and prints fingerprints of the result (the Debug text of the resolved AST, or of the
//! error). The compiler's config-entry and object-property order varies from run to run and
//! `created` holds the wall clock, so there are two fingerprints:
//!   multiset  hash of the sorted atoms (the text split at , [ ] { } and spaces), after redacting timestamps. Insensitive
//!             to sibling order, sensitive to any added, removed or changed value.
//!   strict    hash of the whole text after redacting timestamps. Stable only for files whose
//!             output has no order variation.
//! Two builds that print identical multiset hashes compile every file to the same content.
use dixscript::Runtime::DixLoader;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { walk(&p, out); } else if p.extension().map_or(false, |x| x == "mdix") { out.push(p); }
    }
}

fn redact_timestamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("Timestamp(\"") {
        let head = i + "Timestamp(\"".len();
        out.push_str(&rest[..head]);
        match rest[head..].find('"') {
            Some(j) => { out.push('T'); rest = &rest[head + j..]; }
            None => { rest = &rest[head..]; break; }
        }
    }
    out.push_str(rest);
    out
}

fn h<T: Hash + ?Sized>(t: &T) -> u64 { let mut x = DefaultHasher::new(); t.hash(&mut x); x.finish() }

fn main() {
    let root = std::env::args().nth(1).expect("usage: golden <dir>");
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    for p in files {
        let loader = DixLoader::new_silent();
        let r = loader.compile_to_resolved_ast(p.to_str().unwrap());
        let tag = if r.is_ok() { "OK " } else { "ERR" };
        let s = redact_timestamps(&format!("{:?}", r));
        if let Ok(d) = std::env::var("GOLDEN_DUMP") { let name = p.to_string_lossy().replace('/', "_"); std::fs::write(format!("{d}/{name}.txt"), &s).ok(); }
        let mut pieces: Vec<&str> = s.split(|c: char| matches!(c, ',' | '[' | ']' | '{' | '}' | ' ')).filter(|t| !t.is_empty()).collect();
        pieces.sort_unstable();
        println!("{} {:>9} multiset={:016x} strict={:016x} {}", tag, s.len(), h(&pieces), h(&s), p.display());
    }
}
