// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md (referenced from the Compiler modules it
// benchmarks)
// ============================================================================
//! DixScript vs JSON vs TOML using the real chemistry_db data, not a
//! synthetic fixture — answers whether DixScript's parse-time advantage
//! over TOML holds, grows, or shrinks as file size grows, using a dataset
//! with the same structural complexity as the actual 5-element database
//! (`mdix_files/chemistry_db/elements_database.mdix`, 126 properties per
//! element, most values computed via QuickFunc calls like
//! `builders.createIdentity(...)`) rather than flat repeated key/value
//! pairs like `format_comparison_benchmark.rs`'s `bench_scaled_payloads`.
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
use std::time::Duration;

const REAL_DB_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../mdix_files/chemistry_db/elements_database.mdix");

/// The 5 element names actually present in the real file, in source order.
/// Used to rewrite `elements.hydrogen.*` -> `elements.hydrogen_b2.*` (etc.)
/// when building scaled copies -- see `build_scaled_source`.
const ELEMENT_KEYS: &[&str] = &["hydrogen", "helium", "lithium", "beryllium", "boron"];

/// Builds a source string containing `multiplier` back-to-back copies of
/// the real 5-element block, each copy's `elements.<name>` keys renamed to
/// `elements.<name>_b<n>` so they don't collide. Same `@CONFIG`/`@IMPORTS`
/// header and the same physical-constant fields as the real file every
/// time -- only the element block count changes. This is NOT a stand-in
/// for "N distinct real elements"; it is the real file's actual structural
/// complexity (126 properties/element, real builder/unit calls) repeated,
/// which is what actually matters for a parse-time-vs-size comparison.
fn build_scaled_source(real_source: &str, multiplier: usize) -> String {
    let data_start = real_source.find("@DATA(").expect("real file must have a @DATA section");
    let element_block_start = real_source
        .find("// ELEMENT: Hydrogen")
        .expect("real file must contain the Hydrogen block");
    // Back up to the start of that line's comment banner.
    let banner_start = real_source[..element_block_start].rfind("// ====").unwrap_or(element_block_start);

    let header = &real_source[..banner_start];
    let element_blocks = &real_source[banner_start..real_source.rfind(')').expect("closing @DATA paren")];
    let _ = data_start;

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
    json: String,
    toml: Option<String>, // None if the resolved data doesn't round-trip to TOML cleanly
}

fn build_fixtures() -> Vec<ScaledFixture> {
    let real_source = std::fs::read_to_string(REAL_DB_PATH)
        .unwrap_or_else(|e| panic!("failed to read {REAL_DB_PATH}: {e}"));

    let loader = DixLoader::new();
    let converter = DixConverter::new();

    [1usize, 4, 12, 24]
        .iter()
        .map(|&multiplier| {
            let mdix_source = build_scaled_source(&real_source, multiplier);
            let element_count = ELEMENT_KEYS.len() * multiplier;

            // Resolve through the real pipeline -- same imports, same
            // builder/unit QuickFunc calls, evaluated for real.
            let ast = loader
                .compile_to_resolved_ast_from_str(&mdix_source, &format!("chem_db_x{multiplier}"))
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

// ── Bench: real, unmodified file (ground truth, 5 elements) ────────────────

fn bench_real_file_compile(c: &mut Criterion) {
    let real_source = std::fs::read_to_string(REAL_DB_PATH)
        .unwrap_or_else(|e| panic!("failed to read {REAL_DB_PATH}: {e}"));

    let mut group = c.benchmark_group("chemistry_db_real_file");
    group.measurement_time(Duration::from_secs(8));
    group.throughput(Throughput::Bytes(real_source.len() as u64));
    group.bench_function("mdix_compile_with_real_imports", |b| {
        let loader = DixLoader::new();
        b.iter(|| {
            loader
                .compile_to_resolved_ast_from_str(black_box(&real_source), "chem_db_real")
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
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);

    let loader = DixLoader::new();

    for f in &fixtures {
        group.throughput(Throughput::Bytes(f.mdix_source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("mdix_compile", &f.label),
            &f.mdix_source,
            |b, src| {
                b.iter(|| {
                    loader
                        .compile_to_resolved_ast_from_str(black_box(src), "chem_db_bench")
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
    config  = Criterion::default().measurement_time(Duration::from_secs(10)).sample_size(30);
    targets = bench_real_file_compile, bench_scaled_comparison
);
criterion_main!(chemistry_db_benches);
