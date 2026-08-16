// build.rs — macOS-only linker flag for mlua's "module" feature.
//
// See Cargo.toml's comment on why this crate uses "module" and not
// "vendored": it's meant to require()'d into a host that already embeds
// its own Lua interpreter, so it intentionally leaves every lua_*/luaL_*
// symbol unresolved at build time, to be resolved dynamically against
// whatever process loads it.
//
// Linux's default linker already permits a shared object with unresolved
// symbols like that (confirmed working for real — lua-ci.yml builds this
// crate and loads it into system lua5.4 successfully, no extra flags).
// macOS's linker is stricter by default (two-level namespace) and
// refuses to produce a dylib with unresolved symbols unless told
// otherwise — mlua's own README documents this exact requirement for
// module builds: `-undefined dynamic_lookup`
// (https://github.com/mlua-rs/mlua#modules). Without it, `cargo build
// --target x86_64-apple-darwin` / `aarch64-apple-darwin` fails at the
// link step with an undefined-symbol error for every Lua C API call.
//
// Confirmed this isn't already handled upstream by pulling mlua-sys's
// own build script directly (mlua-sys-*/build/main_inner.rs) — it only
// special-cases Windows (via Rust's `raw-dylib` linking; every
// `lua_*`/`luaL_*` declaration in mlua-sys/src/lua54/*.rs carries
// `#[link(name = "lua54", kind = "raw-dylib")]` for that target).
// Nothing for macOS, hence this file. Windows needs no equivalent
// build-time fix — its caveat is a *runtime* one instead: the resulting
// mdix.dll hard-requires an actual DLL named lua54.dll to be loaded in
// the host process, since that's the name raw-dylib linking resolves
// against. See the Windows note in README.md.
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-undefined");
        println!("cargo:rustc-link-arg=dynamic_lookup");
    }
}
