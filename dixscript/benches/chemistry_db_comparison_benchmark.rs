// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md (referenced from the Compiler modules it
// benchmarks)
// ============================================================================
//! DixScript vs JSON vs TOML using the real chemistry_db data, not a
//! synthetic fixture — answers whether DixScript's parse-time advantage
//! over TOML holds, grows, or shrinks as file size grows, using a dataset
//! with the same structural complexity as 5 real elements (126 properties
//! each, most values computed via QuickFunc calls like
//! `builders.createIdentity(...)`) rather than flat repeated key/value
//! pairs like `format_comparison_benchmark.rs`'s `bench_scaled_payloads`.
//!
//! Two fixes recorded here so they don't get silently re-broken:
//!
//! 1. `compile_to_resolved_ast_from_str`'s `label` argument is NOT a base
//!    path for `@IMPORTS` resolution — see its own doc comment in
//!    `loader.rs`: "label is used only for error messages... it does not
//!    need to be a real path." `elements_database.mdix`'s
//!    `@IMPORTS(enums from "core/enums.mdix", ...)` are real relative
//!    paths, so loading it via `from_str` fails every import with "Failed
//!    to read file: No such file or directory" the moment semantic
//!    analysis runs. Only `compile_to_resolved_ast(file_path)` resolves
//!    them, relative to the real file's own directory. `bench_real_file_compile`
//!    now uses that directly. The synthetic *scaled* sources below aren't
//!    real files on disk by construction, so they're written to real temp
//!    `.mdix` files inside `mdix_files/chemistry_db/` (same directory as
//!    `core/`) specifically so their imports resolve the same way.
//!
//! 2. The real `elements_database.mdix` has grown to 118 elements (was 5
//!    when this benchmark was written) and no longer has the
//!    `"// ELEMENT: Hydrogen"`-style comment banners this file used to
//!    locate slice boundaries with — elements are marked by their
//!    `elements.<name>.` key prefixes only now. Scaling the *entire*
//!    118-element file 1x/2x/4x would be compiling up to ~472 elements
//!    (~59,000 properties) per iteration, nothing like what `sample_size`/
//!    `measurement_time` below were tuned against and nothing like the
//!    original "repeat one real 5-element unit" intent. `build_scaled_source`
//!    now re-derives just the original 5-element (hydrogen..boron) slice
//!    from within the real file by finding where `elements.carbon.` starts
//!    (first element outside `ELEMENT_KEYS`, confirmed next in periodic-table
//!    order right after boron) and using that as the slice's end boundary,
//!    instead of a banner that no longer exists.
//!
//! JSON and TOML fixtures are never hand-transcribed and never committed
//! as static files: this benchmark loads the real .mdix source (resolving
//! its real `@IMPORTS` and evaluating every builder/unit call through the
//! normal compile pipeline), then converts the RESOLVED result to JSON and
//! TOML with `DixConverter::to_json`/`to_toml` — the same converter the
//! `mdix convert` CLI command uses. That conversion happens once, in
//! criterion's unmeasured setup, not per-iteration.
//!
//! Run:  cargo bench -p dixscript --bench chemistry_db_comparison_benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dixscript::Runtime::{DixConverter, DixLoader};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const REAL_DB_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/chemistry_db/elements_database.mdix");

/// Directory the real file lives in — also where `core/enums.mdix` etc.
/// live, and where the synthetic scaled fixtures get written so their
/// `@IMPORTS` resolve the same way the real file's do.
const REAL_DB_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/chemistry_db");

/// The 5 element names this benchmark has always scaled, in source order.
/// Used to rewrite `elements.hydrogen.*` -> `elements.hydrogen_b2.*` (etc.)
/// when building scaled copies -- see `build_scaled_source`.
const ELEMENT_KEYS: &[&str] = &["hydrogen", "helium", "lithium", "beryllium", "boron"];

/// First element key prefix *not* in `ELEMENT_KEYS` — marks where the
/// 5-element slice ends now that there's no comment banner to find instead.
const FIRST_EXCLUDED_ELEMENT_KEY: &str = "elements.carbon.";

/// Builds a source string containing `multiplier` back-to-back copies of
/// the real 5-element (hydrogen..boron) block, re-sliced out of the real
/// (now 118-element) file by key-prefix boundaries, each copy's
/// `elements.<n>` keys renamed to `elements.<n>_b<n>` so they don't
/// collide. Same `@CONFIG`/`@IMPORTS` header and the same physical-constant
/// fields as the real file every time -- only the element block count
/// changes. This is NOT a stand-in for "N distinct real elements"; it is 5
/// real elements' actual structural complexity (126 properties/element,
/// real builder/unit calls) repeated, which is what actually matters for a
/// parse-time-vs-size comparison.
fn build_scaled_source(real_source: &str, multiplier: usize) -> String {
    let hydrogen_start = real_source
        .find("elements.hydrogen.")
        .expect("real file must contain the hydrogen element block");
    let carbon_start = real_source[hydrogen_start..]
        .find(FIRST_EXCLUDED_ELEMENT_KEY)
        .map(|rel| hydrogen_start + rel)
        .expect("real file must contain the carbon element block (5-element slice end marker)");

    let header = &real_source[..hydrogen_start];
    let element_blocks = &real_source[hydrogen_start..carbon_start];

    let mut out = String::with_capacity(header.len() + element_blocks.len() * multiplier + 16);
    out.push_str(header);

    for batch in 0..multiplier {
        let mut block = element_blocks.to_string();
        if batch > 0 {
            for name in ELEMENT_KEYS {
                block = block.replace(&format!("elements.{name}"), &format!("elements.{name}_b{batch}"));
            }
        }
        out.push_str(&block);
        out.push('\n');
    }
    out.push(')');
    out
}

struct ScaledFixture {
    label: String,
    element_count: usize,
    mdix_source: String,
    /// Real file on disk (inside `mdix_files/chemistry_db/`, alongside
    /// `core/`) holding `mdix_source` — needed so `@IMPORTS` resolves; see
    /// the file-header note on `compile_to_resolved_ast_from_str`.
    mdix_temp_path: PathBuf,
    json: String,
    toml: Option<String>, // None if the resolved data doesn't round-trip to TOML cleanly
}

impl Drop for ScaledFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.mdix_temp_path); // best-effort; CI checkout is ephemeral anyway
    }
}

fn build_fixtures() -> Vec<ScaledFixture> {
    let real_source = std::fs::read_to_string(REAL_DB_PATH)
        .unwrap_or_else(|e| panic!("failed to read {REAL_DB_PATH}: {e}"));

    let loader = DixLoader::new_silent();
    let converter = DixConverter::new();

    // 1x/2x/4x (5/10/20 elements). Cut down from an earlier 1/4/12/24
    // attempt that was never actually confirmed against real CI timing --
    // same reasoning as raw_section_benchmark.rs's tiers: shrink first,
    // widen later once a real run confirms per-iteration cost.
    [1usize, 2, 4]
        .iter()
        .map(|&multiplier| {
            let mdix_source = build_scaled_source(&real_source, multiplier);
            let element_count = ELEMENT_KEYS.len() * multiplier;

            let mdix_temp_path =
                PathBuf::from(REAL_DB_DIR).join(format!(".bench_scaled_x{multiplier}.mdix"));
            fs::write(&mdix_temp_path, &mdix_source).unwrap_or_else(|e| {
                panic!("failed to write temp fixture {mdix_temp_path:?}: {e}")
            });

            // Resolve through the real pipeline -- same imports, same
            // builder/unit QuickFunc calls, evaluated for real.
            let ast = loader
                .compile_to_resolved_ast(
                    mdix_temp_path.to_str().expect("temp fixture path is valid UTF-8"),
                )
                .unwrap_or_else(|e| panic!("scaled chemistry_db (x{multiplier}) failed to compile: {e}"));

            let json = converter
                .to_json(&ast, false)
                .unwrap_or_else(|e| panic!("to_json failed for x{multiplier}: {e}"));

            let toml = match converter.to_toml(&ast) {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!("note: to_toml failed at x{multiplier} ({element_count} elements): {e} -- skipping TOML at this scale");
                    None
                }
            };

            ScaledFixture {
                label: format!("{element_count}_elements"),
                element_count,
                mdix_source,
                mdix_temp_path,
                json,
                toml,
            }
        })
        .collect()
}

// ── Size report ────────────────────────────────────────────────────────────

fn print_size_report(fixtures: &[ScaledFixture]) {
    println!("\n=== chemistry_db real-structure size/scaling comparison ===");
    println!("{:<14} {:>10} {:>10} {:>10} {:>10}", "elements", "mdix(B)", "json(B)", "toml(B)", "json/mdix");
    for f in fixtures {
        let toml_str = f.toml.as_ref().map(|t| t.len().to_string()).unwrap_or_else(|| "n/a".to_string());
        println!(
            "{:<14} {:>10} {:>10} {:>10} {:>9.2}x",
            f.element_count,
            f.mdix_source.len(),
            f.json.len(),
            toml_str,
            f.json.len() as f64 / f.mdix_source.len() as f64,
        );
    }
    println!();
}

// ── Bench: real, unmodified file (ground truth, now 118 elements) ──────────

fn bench_real_file_compile(c: &mut Criterion) {
    // Read once just for throughput sizing -- compile_to_resolved_ast does
    // its own read internally (needed for @IMPORTS resolution, see the
    // file-header note), so this file gets read twice per iteration now.
    // Real file is 445KB; that overhead is a rounding error next to actual
    // compile cost and the only way to make imports resolve at all.
    let real_source = std::fs::read_to_string(REAL_DB_PATH)
        .unwrap_or_else(|e| panic!("failed to read {REAL_DB_PATH}: {e}"));

    let mut group = c.benchmark_group("chemistry_db_real_file");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));
    group.throughput(Throughput::Bytes(real_source.len() as u64));
    group.bench_function("mdix_compile_with_real_imports", |b| {
        let loader = DixLoader::new_silent();
        b.iter(|| {
            loader
                .compile_to_resolved_ast(black_box(REAL_DB_PATH))
                .expect("real chemistry_db should always compile")
        });
    });
    group.finish();
}

// ── Bench: DixScript vs JSON vs TOML, across scales ─────────────────────────

fn bench_scaled_comparison(c: &mut Criterion) {
    let fixtures = build_fixtures();
    print_size_report(&fixtures);

    let mut group = c.benchmark_group("chemistry_db_scaled_vs_toml_json");
    // Flat, small, and floor-value sample_size on purpose -- see the note
    // on the multiplier list above and raw_section_benchmark.rs's matching
    // note. mdix_compile is a full compile (real imports, real builder/
    // unit QuickFunc calls evaluated) so it's the actual cost driver here;
    // json_parse/toml_parse (plain deserialization) will finish well under
    // budget regardless.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let loader = DixLoader::new_silent();

    for f in &fixtures {
        group.throughput(Throughput::Bytes(f.mdix_source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("mdix_compile", &f.label),
            &f.mdix_temp_path,
            |b, path| {
                b.iter(|| {
                    loader
                        .compile_to_resolved_ast(black_box(path.to_str().expect("valid utf8")))
                        .expect("generated scaled source should always compile")
                });
            },
        );

        group.throughput(Throughput::Bytes(f.json.len() as u64));
        group.bench_with_input(BenchmarkId::new("json_parse", &f.label), &f.json, |b, src| {
            b.iter(|| {
                let v: serde_json::Value = serde_json::from_str(black_box(src)).unwrap();
                black_box(v)
            });
        });

        if let Some(toml_src) = &f.toml {
            group.throughput(Throughput::Bytes(toml_src.len() as u64));
            group.bench_with_input(BenchmarkId::new("toml_parse", &f.label), toml_src, |b, src| {
                b.iter(|| {
                    let v: toml::Value = toml::from_str(black_box(src)).unwrap();
                    black_box(v)
                });
            });
        }
    }

    group.finish();
}

criterion_group!(
    name    = chemistry_db_benches;
    config  = Criterion::default().measurement_time(Duration::from_secs(5)).sample_size(10);
    targets = bench_real_file_compile, bench_scaled_comparison
);
criterion_main!(chemistry_db_benches);
