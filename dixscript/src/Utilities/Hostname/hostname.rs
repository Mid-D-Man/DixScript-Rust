// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Hostname/hostname.rs"
// ============================================================================
//! Best-effort replacement for the `hostname` crate, in safe `std` only.
//!
//! ## What is used
//! One call: `hostname::get().unwrap_or_default().to_string_lossy()` in
//! `ErrorManager/diagnostic_dumper.rs`, to print a machine name at the top of
//! a diagnostic dump. It is not used for anything functional -- no path, no
//! key, no comparison depends on it -- and `wasm32` never reaches it (that
//! branch of the call site returns a fixed string).
//!
//! ## This is NOT equivalent to the real crate, on purpose
//! The real crate calls the OS's `gethostname` (or `GetComputerNameExW` on
//! Windows), which always returns the current, authoritative name. Doing the
//! same from Rust without a dependency means either `unsafe` FFI or the
//! `libc` crate; for one cosmetic line in a diagnostic file, this crate
//! doesn't take on either. So this reads the same information from places
//! that are safe to read:
//!
//! 1. Windows: the `COMPUTERNAME` environment variable.
//! 2. Unix: `/proc/sys/kernel/hostname` (Linux, the kernel's own value --
//!    identical to what `gethostname` returns), then `/etc/hostname`.
//! 3. Unix: the `HOSTNAME`, then `HOST`, environment variables.
//! 4. Unix: run the `hostname` program (macOS and the BSDs have no `/proc`
//!    and no `/etc/hostname`, so this is what covers them).
//! 5. Otherwise `Err(NotFound)`, which the call site turns into an empty
//!    string exactly as it did when the real crate failed.
//!
//! Known ways this can differ from the real crate: on Windows,
//! `COMPUTERNAME` is the upper-cased NetBIOS name, which can differ in case
//! or content from the DNS host name; `/etc/hostname` can lag a hostname
//! that was changed at runtime (only reached when `/proc` is unavailable);
//! an environment variable can be stale or set by hand; on Android none of
//! (2)-(3) exist, and whether (4) works depends on the device. Every one of
//! those changes a line of diagnostic text, nothing else.
//!
//! The call is kept `io::Result<OsString>` so the call site's
//! `.unwrap_or_default().to_string_lossy().to_string()` does not change.

use std::ffi::OsString;
use std::io;

/// Trims a file's or command's output down to a hostname, or `None` if
/// nothing usable is left. Files like `/etc/hostname` end in a newline.
fn clean(raw: &str) -> Option<OsString> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(OsString::from(trimmed))
    }
}

#[cfg(windows)]
pub(crate) fn get() -> io::Result<OsString> {
    match std::env::var_os("COMPUTERNAME") {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(io::Error::new(io::ErrorKind::NotFound, "COMPUTERNAME is not set")),
    }
}

#[cfg(all(not(windows), not(target_arch = "wasm32")))]
pub(crate) fn get() -> io::Result<OsString> {
    for path in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Some(name) = clean(&contents) {
                return Ok(name);
            }
        }
    }

    for var in ["HOSTNAME", "HOST"] {
        if let Some(value) = std::env::var_os(var) {
            if let Some(name) = value.to_str().and_then(clean) {
                return Ok(name);
            }
        }
    }

    if let Ok(output) = std::process::Command::new("hostname").output() {
        if output.status.success() {
            if let Some(name) = std::str::from_utf8(&output.stdout).ok().and_then(clean) {
                return Ok(name);
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "could not determine the hostname"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn get() -> io::Result<OsString> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no hostname on wasm32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_trims_trailing_newline() {
        assert_eq!(clean("my-host\n"), Some(OsString::from("my-host")));
        assert_eq!(clean("  my-host \r\n"), Some(OsString::from("my-host")));
    }

    #[test]
    fn clean_rejects_empty_and_whitespace_only() {
        assert_eq!(clean(""), None);
        assert_eq!(clean("   \n\t"), None);
    }

    /// Runs against the machine the tests are on. Skips its assertion where
    /// no name can be found at all (a stripped-down container can have none),
    /// which is exactly the case the call site handles with `unwrap_or_default`.
    #[test]
    fn get_returns_a_nonempty_single_line_name_when_it_finds_one() {
        if let Ok(name) = get() {
            let s = name.to_string_lossy();
            assert!(!s.is_empty());
            assert!(!s.contains('\n'), "must be trimmed: {:?}", s);
        }
    }
}


/// On Linux the value read from `/proc/sys/kernel/hostname` is exactly what
/// the real crate's `gethostname` syscall returns, so this must match it
/// byte for byte. Linux only: on other systems the two legitimately differ
/// (see the module doc), so asserting equality there would be asserting
/// something false. When `hostname` is dropped from `[dependencies]` in the
/// wiring pass, move it to `[dev-dependencies]` to keep this.
#[cfg(all(test, target_os = "linux"))]
mod differential_against_real_crate {
    #[test]
    fn matches_the_real_crate_on_linux() {
        let mine = super::get();
        let real = ::hostname::get();
        match (mine, real) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            // A container with no /proc entry, no /etc/hostname and no env
            // var can genuinely have nothing for `get` to find while the
            // syscall still answers -- the documented gap, not a failure.
            (Err(_), Ok(_)) => {}
            (Ok(a), Err(e)) => panic!("mine found {:?} but the real crate failed: {}", a, e),
            (Err(_), Err(_)) => {}
        }
    }
}
