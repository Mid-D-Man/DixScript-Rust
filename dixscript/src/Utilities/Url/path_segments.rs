// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Url"
// ============================================================================
//! `last_path_segment`: the equivalent of
//! `url::Url::parse(s).ok()?.path_segments()?.next_back()`, written against the
//! WHATWG URL path algorithm that the `url` crate implements.
//!
//! ## Why this exists, and why it has to be exact
//! The result names the cache file: `<cache>/<hash16>/<segment>`. A build
//! without `cloud-import` still serves entries that a build *with* it cached
//! (see `ImportsResolver::download_cloud_file_sync`), so both builds must derive
//! the same file name from the same URL. Swapping to `reqwest::Url` behind
//! `cloud-import` (the first plan) would have given the cloud-off build a
//! different name source and silently missed those entries; so the logic is
//! written once, here, and used by every configuration.
//!
//! ## Scope
//! Exact (checked by differential fuzzing against `url` 2.x) for URLs that are
//! valid to the real parser, in two families:
//! - special schemes `http`, `https`, `ws`, `wss`, `ftp`: leading `/` and `\`
//!   runs, `\` as a path separator, empty path = `/`;
//! - any other scheme written `scheme://...`, plus `scheme:/path`.
//!
//! The path rules reproduced: trim of C0 control and space at both ends, removal
//! of tab/CR/LF, a final dot-segment (`.`, `..`, `%2e` in any case/mix) giving an
//! empty segment, and percent-encoding of the path set (C0 controls, space, `"`, `<`, `>`,
//! `` ` ``, `{`, `}` and every non-ASCII byte).
//!
//! Not reproduced: `file:` URLs (returns `None`, i.e. the caller's hash-name
//! fallback), IDNA / IP-literal host validation, and per-scheme default ports.
//! Only a cheap host/port sanity check is done, so for a URL the real parser
//! would reject for a subtle host reason this returns a segment where `url`
//! returns `None`. That cannot affect a cache lookup: a URL the real parser
//! rejects can never have been downloaded, so no entry was ever stored for it.

/// Last path segment of `input`, percent-encoded as the `url` crate would
/// serialize it, or `None` when the URL has no hierarchical path (cannot-be-a-
/// base, no scheme, empty special-scheme host, ...). The segment may be empty
/// (`https://h/dir/`); callers treat that as "no file name".
pub(crate) fn last_path_segment(input: &str) -> Option<String> {
    // Leading/trailing C0 control or space is stripped; tab, CR and LF are
    // removed wherever they occur.
    let trimmed = input.trim_matches(|c: char| c <= '\u{20}');
    let cleaned: String = trimmed.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();

    let colon = cleaned.find(':')?;
    let scheme = &cleaned[..colon];
    if !is_valid_scheme(scheme) {
        return None;
    }
    let lower = scheme.to_ascii_lowercase();
    if lower == "file" {
        return None; // out of scope, see the module docs
    }
    let special = matches!(lower.as_str(), "http" | "https" | "ws" | "wss" | "ftp");
    let rest = &cleaned[colon + 1..];

    let is_sep = |c: char| c == '/' || (special && c == '\\');

    // ── Authority ───────────────────────────────────────────────────────
    let owned: String;
    let path_and_after: &str = if special {
        // "special authority slashes" / "ignore slashes": any number of `/`
        // or `\` (including none) before the host.
        let after = rest.trim_start_matches(|c| c == '/' || c == '\\');
        let end = after.find(|c| c == '/' || c == '\\' || c == '?' || c == '#').unwrap_or(after.len());
        check_authority(&after[..end], true)?;
        &after[end..]
    } else if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(after.len());
        let auth = &after[..end];
        match check_authority(auth, false)? {
            None => &after[end..],
            // `host:port\more`: for a non-special scheme the `url` crate ends
            // the authority at the backslash after the port and starts the
            // path there (the `\` is an ordinary character, not a separator).
            Some(tail_len) => {
                owned = format!("/{}", &after[end - tail_len..]);
                &owned
            }
        }
    } else if rest.starts_with('/') {
        rest // `scheme:/path` -- a path with no authority
    } else {
        return None; // opaque ("cannot-be-a-base") path: no segments
    };

    // ── Path (ends at the first `?` or `#`) ─────────────────────────────
    let path = &path_and_after[..path_and_after.find(|c| c == '?' || c == '#').unwrap_or(path_and_after.len())];

    let body: &str = match path.chars().next() {
        None => {
            // Empty path: a special URL serializes it as "/", anything else
            // has no segments at all.
            return if special { Some(String::new()) } else { None };
        }
        Some(c) if is_sep(c) => &path[c.len_utf8()..],
        // Unreachable for well-formed input (the authority ended on a
        // separator), but never guess.
        Some(_) => return None,
    };

    // Only the LAST segment is wanted, and the WHATWG path algorithm makes it
    // trivial: whatever `..` and `.` do to the segments before it, the final
    // token always decides the result. A dot-segment as the final token (`.`,
    // `..`, `%2e` in any mix of case) leaves an empty last segment; anything
    // else is that token, percent-encoded. (The differential tests walk every
    // dot-segment combination up to depth four to keep this honest.)
    let tail = body.rsplit(is_sep).next().unwrap_or("");
    let mut segment = String::with_capacity(tail.len());
    for c in tail.chars() {
        push_encoded(&mut segment, c);
    }
    if is_double_dot(&segment) || is_single_dot(&segment) {
        segment.clear();
    }
    Some(segment)
}

fn is_valid_scheme(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if c.is_ascii_alphabetic())
        && it.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Cheap sanity check of `[userinfo@]host[:port]`, deliberately not a full host
/// parser (see the module docs): host non-empty (special schemes always;
/// others when a userinfo or port is present), no forbidden host code point,
/// port (if any) all digits and <= 65535.
///
/// Returns `None` for an invalid authority, `Some(None)` for a valid one, and
/// `Some(Some(n))` for the non-special `host:port\...` case where the last `n`
/// bytes of `authority` (starting at the backslash) belong to the path.
fn check_authority(authority: &str, special: bool) -> Option<Option<usize>> {
    let (has_userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (true, &authority[i + 1..]),
        None => (false, authority),
    };
    let (host, port) = if hostport.starts_with('[') {
        let close = hostport.find(']')?;
        let after = &hostport[close + 1..];
        let port = if after.is_empty() { None } else { Some(after.strip_prefix(':')?) };
        (&hostport[..=close], port)
    } else {
        match hostport.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (hostport, None),
        }
    };
    if host.is_empty() && (special || has_userinfo || port.map_or(false, |p| !p.is_empty())) {
        return None;
    }
    if !host.starts_with('[') {
        let forbidden = |c: char| {
            matches!(c, '\0' | '\t' | '\n' | '\r' | ' ' | '#' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|')
        };
        if special {
            // A domain is percent-decoded first, so `a%41` is the valid host
            // `aa` while `a%2Fb`, `a%25` and a bare `%` are not.
            let decoded = percent_decode_lossy(host);
            if decoded.is_empty() || decoded.chars().any(|c| forbidden(c) || c <= '\u{1f}' || c == '\u{7f}' || c == '%') {
                return None;
            }
        } else if host.chars().any(forbidden) {
            return None;
        }
    }
    let mut tail_len = None;
    if let Some(p) = port {
        let digits = p.bytes().take_while(u8::is_ascii_digit).count();
        let (num, tail) = p.split_at(digits);
        if !tail.is_empty() {
            if special || !tail.starts_with('\\') {
                return None;
            }
            tail_len = Some(tail.len());
        }
        if !num.is_empty() && (num.len() > 5 || num.parse::<u32>().map_or(true, |n| n > 65535)) {
            return None;
        }
    }
    Some(tail_len)
}

/// Percent-decode `s` (a `%` not followed by two hex digits stays literal) and
/// read the bytes as UTF-8, replacing invalid sequences.
fn percent_decode_lossy(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |i: usize| b.get(i).and_then(|&x| (x as char).to_digit(16));
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let (Some(h), Some(l)) = (hex(i + 1), hex(i + 2)) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// UTF-8 percent-encode `c` with the path percent-encode set.
fn push_encoded(out: &mut String, c: char) {
    let must_encode = !c.is_ascii()
        || c <= '\u{1f}'
        || c == '\u{7f}'
        || matches!(c, ' ' | '"' | '<' | '>' | '`' | '#' | '?' | '{' | '}');
    if !must_encode {
        out.push(c);
        return;
    }
    let mut utf8 = [0u8; 4];
    for b in c.encode_utf8(&mut utf8).bytes() {
        out.push('%');
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
    }
}

fn is_single_dot(s: &str) -> bool {
    s == "." || s.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), ".." | ".%2e" | "%2e." | "%2e%2e")
}

#[cfg(test)]
mod tests {
    use super::last_path_segment as ours;
    use crate::Utilities::test_rng::XorShift;

    /// The real thing: what `CloudFileCache` called before this module existed.
    fn real(input: &str) -> Option<String> {
        ::url::Url::parse(input).ok()?.path_segments()?.next_back().map(str::to_string)
    }

    #[test]
    fn typical_cloud_urls_behave_as_before() {
        for (u, want) in [
            ("https://example.com/data/config.mdix", Some("config.mdix")),
            ("https://example.com/data/config.mdix?token=1#frag", Some("config.mdix")),
            ("https://example.com/dir/", Some("")),
            ("https://example.com", Some("")),
            ("https://example.com/a/../b.mdix", Some("b.mdix")),
            ("https://example.com/a b/c d.mdix", Some("c%20d.mdix")),
            ("https://example.com/caf\u{e9}.mdix", Some("caf%C3%A9.mdix")),
            ("mailto:someone@example.com", None),
            ("not a url", None),
            ("", None),
        ] {
            assert_eq!(ours(u).as_deref(), want, "ours({u:?})");
            assert_eq!(real(u).as_deref(), want, "real({u:?}) -- the table itself must match the crate");
        }
    }

    // ── structured generator: every piece is chosen to exercise one rule ──
    const SCHEMES: &[&str] = &["http", "https", "HTTP", "HtTpS", "ws", "wss", "ftp", "git", "ssh+git", "x-custom", "a.b-c+d"];
    const SLASHES: &[&str] = &["//", "///", "////", "\\\\", "/\\", "\\/", "", "/", "\\"];
    const USERS: &[&str] = &["", "", "", "u@", "user:pw@", "a@b@", ":@", "u%40@"];
    const HOSTS: &[&str] = &[
        "example.com", "EXAMPLE.com", "a.b.c", "localhost", "127.0.0.1", "[::1]", "[2001:db8::1]", "h", "xn--nxasmq6b.example", "a-b.example.org",
        // hosts the real parser accepts only after percent-decoding, or rejects
        "a%41", "%41%42", "a%2Fb", "a%25", "a b", "a<b", "a>b", "a^b", "a|b", "a]b", "a[b", "a@b", "a?b", "", "a\\b", "%", "a%zz", "a\u{1}b", "a\u{7f}b",
    ];
    const PORTS: &[&str] = &[
        "", "", "", ":80", ":8080", ":0", ":65535", ":", ":00443", ":65536", ":99999", ":123456", ":x", ":8o", ":-1", ":+1", ":80:80", ":\\", ":80\\", ": 80",
    ];
    const SEGS: &[&str] = &[
        "a", "b.mdix", "config", "..", ".", "%2e", "%2E", "%2e%2E", ".%2e", "%2e.", "...", "a.b", "", "", "x y", "caf\u{e9}", "\u{1F600}", "a%20b", "%", "%zz", "a%2",
        "{x}", "`t`", "<a>", "\"q\"", "'s'", "^c", "|p|", "~t", "a;b", "a=b", "a,b", "a:b", "a@b", "a\\b", "\u{7f}", "a\u{1}b", "%2f", "%5C", "a\tb", "a\nb", "a\rb", " ", "\u{a0}",
    ];
    const TAILS: &[&str] = &["", "", "", "?q=1", "?", "#f", "#", "?a=b#c", "?/x/y", "#/x/y"];
    const PADS: &[&str] = &["", "", "", " ", "\t", "\n ", "\u{1}", "  \u{1f}"];

    fn gen(rng: &mut XorShift, special_only: bool) -> String {
        let schemes: &[&str] = if special_only { &SCHEMES[..7] } else { SCHEMES };
        let mut s = String::new();
        s.push_str(*rng.pick(&PADS));
        s.push_str(*rng.pick(schemes));
        s.push(':');
        s.push_str(*rng.pick(&SLASHES));
        s.push_str(*rng.pick(&USERS));
        s.push_str(*rng.pick(&HOSTS));
        s.push_str(*rng.pick(&PORTS));
        let n = rng.below(6);
        for _ in 0..n {
            s.push(*rng.pick(&['/', '/', '/', '\\']));
            s.push_str(*rng.pick(&SEGS));
        }
        if rng.below(8) == 0 {
            s.push('/');
        }
        s.push_str(*rng.pick(&TAILS));
        s.push_str(*rng.pick(&PADS));
        s
    }

    /// The URLs the real parser accepts must give the same segment. (URLs it
    /// rejects are checked separately: they may only differ in the documented
    /// host-validation way.)
    #[test]
    fn matches_the_real_crate_on_300k_generated_urls() {
        let mut rng = XorShift::new(0x0C10_0D1F);
        let (mut accepted, mut rejected_both, mut diverged) = (0u32, 0u32, Vec::new());
        for i in 0..300_000 {
            let u = gen(&mut rng, i % 2 == 0);
            let (a, b) = (ours(&u), real(&u));
            if a == b {
                if b.is_some() { accepted += 1 } else { rejected_both += 1 }
            } else {
                diverged.push((u, a, b));
            }
        }
        assert!(accepted > 50_000, "generator too weak: only {accepted} accepted");
        assert!(rejected_both > 1_000, "generator too weak: only {rejected_both} rejected-by-both");
        // The one documented divergence class: a NON-ASCII host that the real
        // parser rejects through IDNA (e.g. U+00A0 maps to a space) and this
        // module, which does not implement IDNA, accepts. Anything else is a bug.
        let (known, unknown): (Vec<_>, Vec<_>) = diverged
            .into_iter()
            .partition(|(u, a, b)| b.is_none() && a.is_some() && host_zone_has_non_ascii(u));
        assert!(unknown.is_empty(), "{} unexplained divergences, first 40: {:?}", unknown.len(), &unknown[..unknown.len().min(40)]);
        assert!(known.len() < 300, "the documented IDNA class should be rare, got {}", known.len());
    }

    /// Is there a non-ASCII char between `scheme:` (plus any slash run) and the
    /// next separator -- i.e. in the host position?
    fn host_zone_has_non_ascii(u: &str) -> bool {
        let t = u.trim_matches(|c: char| c <= ' ');
        let Some(colon) = t.find(':') else { return false };
        let after = t[colon + 1..].trim_start_matches(|c| c == '/' || c == '\\');
        let end = after.find(|c| matches!(c, '/' | '\\' | '?' | '#')).unwrap_or(after.len());
        !after[..end].is_ascii()
    }

    #[test]
    fn dot_segments_agree_exhaustively_up_to_depth_four() {
        let atoms = ["a", "b", ".", "..", "%2e", "%2E%2e", "", ".%2E"];
        let mut stack: Vec<Vec<&str>> = vec![vec![]];
        let mut checked = 0;
        while let Some(path) = stack.pop() {
            for sep in ["/", "\\"] {
                let u = format!("https://h{}{}", sep, path.join(sep));
                assert_eq!(ours(&u), real(&u), "{u:?}");
                checked += 1;
            }
            if path.len() < 4 {
                for a in atoms {
                    let mut p = path.clone();
                    p.push(a);
                    stack.push(p);
                }
            }
        }
        assert!(checked > 9_000);
    }

    #[test]
    fn every_ascii_byte_and_a_few_unicode_chars_encode_as_the_real_crate_does() {
        for b in 0u32..=0x7f {
            let c = char::from_u32(b).unwrap();
            let u = format!("https://h/a{c}b");
            assert_eq!(ours(&u), real(&u), "byte {b:#04x}");
        }
        for c in ['\u{80}', '\u{ff}', '\u{7ff}', '\u{800}', '\u{ffff}', '\u{10000}', '\u{10ffff}', '\u{200b}', '\u{feff}'] {
            let u = format!("https://h/a{c}b");
            assert_eq!(ours(&u), real(&u), "char {:#x}", c as u32);
        }
    }

    #[test]
    fn rejects_what_the_real_parser_rejects_for_obvious_reasons() {
        for u in [
            "https://", "https:///", "https://:80/a", "https://h:99999/a", "https://h:x/a", "https://a b/c", "https://a<b/c",
            "http:", "://h/a", "1http://h/a", "h ttp://x/a", "relative/path", "/abs/path", "//h/a", "?x", "#y",
        ] {
            assert_eq!(ours(u), None, "ours({u:?})");
            assert_eq!(real(u), None, "real({u:?}) -- the table itself must match the crate");
        }
    }

    #[test]
    fn file_urls_are_out_of_scope_and_say_so() {
        assert_eq!(ours("file:///etc/hosts"), None);
        assert_eq!(ours("FILE://h/x"), None);
    }
}
