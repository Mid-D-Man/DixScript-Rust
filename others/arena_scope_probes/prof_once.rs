use dixscript::Runtime::DixLoader;
#[inline(never)]
fn measured(loader: &DixLoader, path: &str) {
    let r = loader.compile_to_resolved_ast(path);
    std::hint::black_box(&r);
}
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let loader = DixLoader::new_silent();
    let _ = loader.compile_to_resolved_ast(&path); // warm-up: lazy statics, regexes
    measured(&loader, &path);
}
