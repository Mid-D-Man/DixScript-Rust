// benches/runtime_benchmark.rs

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dixscript::Runtime::{
    DixCompactor, DixConverter, DixData, DixDataBuilder, DixFormatOptions, DixLoader,
    DixLoadOptions, DixValue,
};
use std::collections::HashMap;

// ── Source fixtures ───────────────────────────────────────────────────────────

/// Minimal flat config — tests the fastest possible load path.
const SRC_MINIMAL: &str = r#"
@DATA(
  app_name = "BenchApp"
  version = "1.0.0"
  port = 8080
  debug = false
  max_connections = 1000
)
"#;

/// Mid-size config with enums and a mix of flat/grouped data.
const SRC_MEDIUM: &str = r#"
@ENUMS(
  LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@DATA(
  app_name = "BenchApp"
  version = "2.0.0"
  port = 9090
  log_level<enum> = LogLevel.INFO
  environment<enum> = Environment.PROD

  database.primary: host = "db.local", port = 5432, pool = 25, ssl = true
  database.replica: host = "db-ro.local", port = 5432, pool = 10, ssl = true

  cache.redis: host = "cache.local", port = 6379, ttl = 3600

  allowed_origins:: "https://app.example.com", "https://admin.example.com"

  feature_flags: dark_mode = true, beta_ui = false, new_onboarding = true
)
"#;

/// Heavy config with QuickFuncs — tests the full compilation pipeline.
const SRC_HEAVY_WITH_FUNCTIONS: &str = r#"
@ENUMS(
  Rarity { COMMON = 1, UNCOMMON = 2, RARE = 3, EPIC = 4, LEGENDARY = 5 }
  AIType { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }
)

@QUICKFUNCS(
  ~createEnemy<object>(name, health, damage, rarity<enum>, ai<enum>) {
    return {
      name = name,
      health = health,
      damage = damage,
      armor = health / 10,
      xp = health / 2,
      gold = health / 4,
      rarity = rarity,
      ai_type = ai,
      spawn_rate = ai == AIType.BOSS ? 0.01 : 0.3
    }
  }

  ~createServer<object>(host, port, pool) {
    return {
      host = host,
      port = port,
      pool_size = pool,
      timeout = 5000,
      ssl = true
    }
  }
)

@DATA(
  game_version = "3.0.0"
  max_players = 100

  enemies::
    createEnemy("Goblin",   50,   10, Rarity.COMMON,    AIType.AGGRESSIVE),
    createEnemy("Orc",      100,  20, Rarity.UNCOMMON,  AIType.AGGRESSIVE),
    createEnemy("Troll",    200,  40, Rarity.RARE,      AIType.AGGRESSIVE),
    createEnemy("Wyvern",   500,  80, Rarity.EPIC,      AIType.AGGRESSIVE),
    createEnemy("Dragon",   1000, 150, Rarity.LEGENDARY, AIType.BOSS)

  servers::
    createServer("us-east-1.db.local", 5432, 50),
    createServer("eu-west-1.db.local", 5432, 50),
    createServer("ap-south-1.db.local", 5432, 25)
)
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a pre-loaded DixData from the medium fixture for access benchmarks.
fn build_medium_data() -> DixData {
    let loader = DixLoader::new();
    loader
        .load_from_str(SRC_MEDIUM, &DixLoadOptions::new())
        .expect("medium fixture must compile")
}

/// Build a DixData with N flat integer entries for access/select benchmarks.
fn build_flat_data(n: usize) -> DixData {
    let mut src = "@DATA(\n".to_string();
    for i in 0..n {
        src.push_str(&format!("  key_{} = {}\n", i, i));
    }
    src.push(')');

    let loader = DixLoader::new();
    loader
        .load_from_str(&src, &DixLoadOptions::new())
        .expect("flat fixture must compile")
}

/// Build a HashMap with N entries for converter benchmarks.
fn build_hashmap(n: usize) -> HashMap<String, DixValue> {
    (0..n)
        .map(|i| (format!("key_{}", i), DixValue::Int(i as i32)))
        .collect()
}

// ── Benchmark groups ──────────────────────────────────────────────────────────

fn bench_load_from_str(c: &mut Criterion) {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    let mut group = c.benchmark_group("load_from_str");

    group.bench_function("minimal", |b| {
        b.iter(|| {
            loader
                .load_from_str(black_box(SRC_MINIMAL), black_box(&opts))
                .expect("minimal must succeed")
        })
    });

    group.bench_function("medium_with_enums", |b| {
        b.iter(|| {
            loader
                .load_from_str(black_box(SRC_MEDIUM), black_box(&opts))
                .expect("medium must succeed")
        })
    });

    group.bench_function("heavy_with_quickfuncs", |b| {
        b.iter(|| {
            loader
                .load_from_str(black_box(SRC_HEAVY_WITH_FUNCTIONS), black_box(&opts))
                .expect("heavy must succeed")
        })
    });

    group.finish();
}

fn bench_load_throughput(c: &mut Criterion) {
    let loader = DixLoader::new();
    let opts = DixLoadOptions::new();

    let mut group = c.benchmark_group("load_throughput");

    for src in [SRC_MINIMAL, SRC_MEDIUM, SRC_HEAVY_WITH_FUNCTIONS] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(src.len()),
            src,
            |b, input| {
                b.iter(|| {
                    loader
                        .load_from_str(black_box(input), black_box(&opts))
                        .expect("must succeed")
                })
            },
        );
    }

    group.finish();
}

fn bench_data_access(c: &mut Criterion) {
    let data = build_medium_data();

    let mut group = c.benchmark_group("data_access");

    group.bench_function("get_string", |b| {
        b.iter(|| {
            let v: String = data.get(black_box("app_name")).expect("must exist");
            black_box(v)
        })
    });

    group.bench_function("get_int", |b| {
        b.iter(|| {
            let v: i32 = data.get(black_box("port")).expect("must exist");
            black_box(v)
        })
    });

    group.bench_function("get_nested", |b| {
        b.iter(|| {
            let v: String = data.get(black_box("database.primary.host")).expect("must exist");
            black_box(v)
        })
    });

    group.bench_function("exists_hit", |b| {
        b.iter(|| black_box(data.exists(black_box("port"))))
    });

    group.bench_function("exists_miss", |b| {
        b.iter(|| black_box(data.exists(black_box("nonexistent_key_xyz"))))
    });

    group.bench_function("get_or_default_miss", |b| {
        b.iter(|| {
            let v: i32 = data.get_or_default(black_box("nonexistent"), 42);
            black_box(v)
        })
    });

    group.finish();
}

fn bench_data_access_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_access_scaling");

    for n in [10usize, 100, 1_000, 10_000] {
        let data = build_flat_data(n);
        let key = format!("key_{}", n / 2); // mid-point key

        group.bench_with_input(BenchmarkId::new("get_by_index", n), &key, |b, k| {
            b.iter(|| {
                let v: i32 = data.get(black_box(k.as_str())).expect("must exist");
                black_box(v)
            })
        });

        group.bench_with_input(BenchmarkId::new("get_keys_prefix", n), &n, |b, _| {
            b.iter(|| black_box(data.get_keys(black_box(""))))
        });
    }

    group.finish();
}

fn bench_data_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_builder");

    group.bench_function("simple_flat", |b| {
        b.iter(|| {
            DixDataBuilder::new()
                .config(|cfg| {
                    cfg.with_version("1.0.0");
                    cfg.with_author("bench");
                })
                .data(|d| {
                    d.with_string("name", "BenchApp");
                    d.with_int("port", 8080);
                    d.with_bool("debug", false);
                    d.with_double("timeout", 5.0);
                })
                .build()
                .expect("must build")
        })
    });

    group.bench_function("with_table_properties", |b| {
        b.iter(|| {
            DixDataBuilder::new()
                .data(|d| {
                    d.with_string("version", "1.0.0");
                    d.with_table_properties("database", |t| {
                        t.with_string("host", "localhost");
                        t.with_int("port", 5432);
                        t.with_bool("ssl", true);
                        t.with_int("pool", 20);
                    });
                    d.with_table_properties("cache", |t| {
                        t.with_string("host", "redis.local");
                        t.with_int("port", 6379);
                        t.with_int("ttl", 3600);
                    });
                })
                .build()
                .expect("must build")
        })
    });

    group.bench_function("with_group_array", |b| {
        b.iter(|| {
            DixDataBuilder::new()
                .data(|d| {
                    d.with_string("version", "1.0.0");
                    d.with_group_array_builder("allowed_origins", |arr| {
                        arr.add_string("https://app.example.com");
                        arr.add_string("https://admin.example.com");
                        arr.add_string("https://api.example.com");
                    });
                })
                .build()
                .expect("must build")
        })
    });

    group.bench_function("large_flat_100_keys", |b| {
        b.iter(|| {
            let mut builder = DixDataBuilder::new();
            builder = builder.data(|d| {
                for i in 0..100 {
                    d.with_int(&format!("key_{}", i), i);
                }
            });
            builder.build().expect("must build")
        })
    });

    group.finish();
}

fn bench_converter(c: &mut Criterion) {
    let converter = DixConverter::new();

    let mut group = c.benchmark_group("converter");

    for n in [10usize, 100, 500] {
        let map = build_hashmap(n);

        group.bench_with_input(
            BenchmarkId::new("from_hashmap", n),
            &map,
            |b, m| {
                b.iter(|| {
                    converter
                        .from_hashmap(black_box(m.clone()))
                        .expect("must convert")
                })
            },
        );
    }

    // to_hashmap: build an AST from the medium source then flatten it
    let medium_ast = {
        let loader = DixLoader::new();
        let data = loader
            .load_from_str(SRC_MEDIUM, &DixLoadOptions::new())
            .expect("must load");
        // Reconstruct a minimal DixScript for the converter to flatten
        use dixscript::Compiler::AST::{DixScript, DataSection, Position};
        DixScript {
            config: None,
            imports: None,
            dlm: None,
            enums: None,
            quick_functions: None,
            data: Some(DataSection {
                entries: vec![],
                position: Position::UNKNOWN,
            }),
            security: None,
        }
    };

    group.bench_function("to_hashmap_medium", |b| {
        b.iter(|| black_box(converter.to_hashmap(black_box(&medium_ast))))
    });

    group.finish();
}

fn bench_compactor(c: &mut Criterion) {
    let mut group = c.benchmark_group("compactor");

    for (label, src) in [
        ("minimal", SRC_MINIMAL),
        ("medium", SRC_MEDIUM),
        ("heavy", SRC_HEAVY_WITH_FUNCTIONS),
    ] {
        group.throughput(Throughput::Bytes(src.len() as u64));

        group.bench_with_input(BenchmarkId::new("minify", label), src, |b, input| {
            b.iter(|| black_box(DixCompactor::minify(black_box(input))))
        });

        group.bench_with_input(BenchmarkId::new("compact", label), src, |b, input| {
            b.iter(|| black_box(DixCompactor::compact(black_box(input))))
        });

        group.bench_with_input(
            BenchmarkId::new("remove_comments", label),
            src,
            |b, input| b.iter(|| black_box(DixCompactor::remove_comments(black_box(input)))),
        );
    }

    group.finish();
}

fn bench_format_options(c: &mut Criterion) {
    let converter = DixConverter::new();
    let loader = DixLoader::new();
    let opts_default  = DixFormatOptions::new();
    let opts_minified = DixFormatOptions::minified();
    let opts_pretty   = DixFormatOptions::pretty();

    // Build a representative AST
    use dixscript::Compiler::AST::{
        ConfigEntry, ConfigSection, ConfigValue, DataEntry, DataSection, DixScript, Position, Value,
    };
    let ast = DixScript {
        config: Some(ConfigSection {
            entries: vec![ConfigEntry {
                key: "version".to_string(),
                value: ConfigValue::String("1.0.0".to_string()),
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        }),
        data: Some(DataSection {
            entries: (0..20)
                .map(|i| DataEntry::SimpleProperty {
                    name: format!("key_{}", i),
                    data_type: None,
                    value: Value::Integer { value: i, position: Position::UNKNOWN },
                    position: Position::UNKNOWN,
                })
                .collect(),
            position: Position::UNKNOWN,
        }),
        imports: None,
        dlm: None,
        enums: None,
        quick_functions: None,
        security: None,
    };

    let mut group = c.benchmark_group("to_mdix");

    group.bench_function("default_format", |b| {
        b.iter(|| {
            converter
                .to_mdix(black_box(&ast), Some(black_box(&opts_default)))
                .expect("must format")
        })
    });

    group.bench_function("minified_format", |b| {
        b.iter(|| {
            converter
                .to_mdix(black_box(&ast), Some(black_box(&opts_minified)))
                .expect("must format")
        })
    });

    group.bench_function("pretty_format", |b| {
        b.iter(|| {
            converter
                .to_mdix(black_box(&ast), Some(black_box(&opts_pretty)))
                .expect("must format")
        })
    });

    group.finish();
}

fn bench_dix_value(c: &mut Criterion) {
    let mut group = c.benchmark_group("dix_value");

    group.bench_function("create_string", |b| {
        b.iter(|| black_box(DixValue::string(black_box("hello world"))))
    });

    group.bench_function("create_array_10", |b| {
        b.iter(|| {
            let items: Vec<DixValue> = (0..10).map(|i| DixValue::Int(i)).collect();
            black_box(DixValue::array(items))
        })
    });

    group.bench_function("create_object_10", |b| {
        b.iter(|| {
            let map: HashMap<String, DixValue> = (0..10)
                .map(|i| (format!("k{}", i), DixValue::Int(i)))
                .collect();
            black_box(DixValue::object(map))
        })
    });

    group.bench_function("as_int_hit", |b| {
        let v = DixValue::Int(42);
        b.iter(|| black_box(v.as_int()))
    });

    group.bench_function("as_int_miss", |b| {
        let v = DixValue::String("hello".to_string());
        b.iter(|| black_box(v.as_int()))
    });

    group.bench_function("type_name", |b| {
        let v = DixValue::Array(vec![]);
        b.iter(|| black_box(v.type_name()))
    });

    group.finish();
}

// ── Registration ──────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_load_from_str,
    bench_load_throughput,
    bench_data_access,
    bench_data_access_scaling,
    bench_data_builder,
    bench_converter,
    bench_compactor,
    bench_format_options,
    bench_dix_value,
);
criterion_main!(benches);
