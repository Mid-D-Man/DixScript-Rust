//! Comprehensive benchmarks for DixCore wrappers vs native Rust types
//!
//! Run with: cargo bench
//!
//! This benchmark suite measures:
//! 1. List<T> vs Vec<T> - add, iterate, access
//! 2. Dictionary<K,V> vs HashMap<K,V> - insert, lookup, iterate
//! 3. HashSet<T> vs HashSet<T> - insert, contains, iterate
//! 4. Clone vs Borrow performance
//! 5. Hot path scenarios (lexer-like operations)

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use dixscript::DixCore::{List, Dictionary, HashSet as DixHashSet};
use std::collections::{HashMap, HashSet};

// ============================================================================
// SCENARIO 1: List Operations (Most common in DixScript)
// ============================================================================

fn bench_list_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_add");

    for size in [100, 1000, 10000] {
        // DixCore List wrapper
        group.bench_with_input(BenchmarkId::new("wrapper", size), &size, |b, &size| {
            b.iter(|| {
                let mut list = List::<i32>::New();
                for i in 0..size {
                    list.Add(black_box(i as i32));
                }
                list
            });
        });

        // Native Vec
        group.bench_with_input(BenchmarkId::new("native", size), &size, |b, &size| {
            b.iter(|| {
                let mut vec = Vec::<i32>::new();
                for i in 0..size {
                    vec.push(black_box(i as i32));
                }
                vec
            });
        });

        // Native Vec with capacity (best practice)
        group.bench_with_input(BenchmarkId::new("native_with_capacity", size), &size, |b, &size| {
            b.iter(|| {
                let mut vec = Vec::<i32>::with_capacity(size);
                for i in 0..size {
                    vec.push(black_box(i as i32));
                }
                vec
            });
        });
    }

    group.finish();
}

fn bench_list_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_iterate");

    for size in [100, 1000, 10000] {
        // Setup
        let mut wrapper_list = List::<i32>::New();
        let mut native_vec = Vec::<i32>::new();
        for i in 0..size {
            wrapper_list.Add(i as i32);
            native_vec.push(i as i32);
        }

        // DixCore List iteration
        group.bench_with_input(BenchmarkId::new("wrapper", size), &size, |b, _| {
            b.iter(|| {
                let sum: i32 = wrapper_list.Iter().map(|x| *x).sum();
                black_box(sum)
            });
        });

        // Native Vec iteration
        group.bench_with_input(BenchmarkId::new("native", size), &size, |b, _| {
            b.iter(|| {
                let sum: i32 = native_vec.iter().copied().sum();
                black_box(sum)
            });
        });
    }

    group.finish();
}

fn bench_list_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_random_access");

    let size = 10000;
    let mut wrapper_list = List::<i32>::New();
    let mut native_vec = Vec::<i32>::new();
    for i in 0..size {
        wrapper_list.Add(i as i32);
        native_vec.push(i as i32);
    }

    // DixCore List random access
    group.bench_function("wrapper", |b| {
        b.iter(|| {
            let mut sum = 0i32;
            for i in (0..size).step_by(10) {
                sum += wrapper_list.Get(i).unwrap_or(&0);
            }
            black_box(sum)
        });
    });

    // Native Vec random access
    group.bench_function("native", |b| {
        b.iter(|| {
            let mut sum = 0i32;
            for i in (0..size).step_by(10) {
                sum += native_vec.get(i).unwrap_or(&0);
            }
            black_box(sum)
        });
    });

    group.finish();
}

// ============================================================================
// SCENARIO 2: Dictionary Operations
// ============================================================================

fn bench_dictionary_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("dictionary_insert");

    for size in [100, 1000, 10000] {
        // DixCore Dictionary wrapper
        group.bench_with_input(BenchmarkId::new("wrapper", size), &size, |b, &size| {
            b.iter(|| {
                let mut dict = Dictionary::<i32, String>::New();
                for i in 0..size {
                    dict.Add(black_box(i as i32), format!("value_{}", i));
                }
                dict
            });
        });

        // Native HashMap
        group.bench_with_input(BenchmarkId::new("native", size), &size, |b, &size| {
            b.iter(|| {
                let mut map = HashMap::<i32, String>::new();
                for i in 0..size {
                    map.insert(black_box(i as i32), format!("value_{}", i));
                }
                map
            });
        });

        // Native HashMap with capacity
        group.bench_with_input(BenchmarkId::new("native_with_capacity", size), &size, |b, &size| {
            b.iter(|| {
                let mut map = HashMap::<i32, String>::with_capacity(size);
                for i in 0..size {
                    map.insert(black_box(i as i32), format!("value_{}", i));
                }
                map
            });
        });
    }

    group.finish();
}

fn bench_dictionary_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("dictionary_lookup");

    let size = 10000;
    let mut wrapper_dict = Dictionary::<i32, String>::New();
    let mut native_map = HashMap::<i32, String>::new();
    for i in 0..size {
        wrapper_dict.Add(i as i32, format!("value_{}", i));
        native_map.insert(i as i32, format!("value_{}", i));
    }

    // DixCore Dictionary lookup
    group.bench_function("wrapper", |b| {
        b.iter(|| {
            let mut found = 0;
            for i in (0..size).step_by(10) {
                if wrapper_dict.ContainsKey(&black_box(i as i32)) {
                    found += 1;
                }
            }
            black_box(found)
        });
    });

    // Native HashMap lookup
    group.bench_function("native", |b| {
        b.iter(|| {
            let mut found = 0;
            for i in (0..size).step_by(10) {
                if native_map.contains_key(&black_box(i as i32)) {
                    found += 1;
                }
            }
            black_box(found)
        });
    });

    group.finish();
}

// ============================================================================
// SCENARIO 3: Clone vs Borrow (Most important for performance)
// ============================================================================

fn bench_clone_vs_borrow(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone_vs_borrow");

    // Setup large data structure
    let mut large_list = List::<String>::New();
    for i in 0..1000 {
        large_list.Add(format!("item_{}", i));
    }

    // BAD: Clone in every iteration
    group.bench_function("clone_every_iteration", |b| {
        b.iter(|| {
            let mut sum = 0;
            for _ in 0..100 {
                let cloned = large_list.clone(); // ❌ Expensive!
                sum += cloned.Count();
            }
            black_box(sum)
        });
    });

    // GOOD: Borrow
    group.bench_function("borrow", |b| {
        b.iter(|| {
            let mut sum = 0;
            for _ in 0..100 {
                sum += large_list.Count(); // ✅ Just borrow
            }
            black_box(sum)
        });
    });

    // Single clone (baseline)
    group.bench_function("single_clone", |b| {
        b.iter(|| {
            let cloned = large_list.clone();
            black_box(cloned.Count())
        });
    });

    group.finish();
}

// ============================================================================
// SCENARIO 4: Hot Path Simulation (Lexer-like operations)
// ============================================================================

fn bench_hot_path_token_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_path_token_buffer");

    // Simulate token buffer operations (accessed per-character)
    let iterations = 10000;

    // Using wrapper
    group.bench_function("wrapper_list", |b| {
        b.iter(|| {
            let mut tokens = List::<(usize, usize)>::New(); // (line, col)
            for i in 0..iterations {
                tokens.Add(black_box((i / 80, i % 80)));
            }
            // Simulate lookahead
            let mut sum = 0usize;
            for i in 0..tokens.Count().saturating_sub(1) {
                if let (Some(curr), Some(next)) = (tokens.Get(i), tokens.Get(i + 1)) {
                    sum += curr.0 + next.0;
                }
            }
            black_box(sum)
        });
    });

    // Using native Vec (should be similar performance)
    group.bench_function("native_vec", |b| {
        b.iter(|| {
            let mut tokens = Vec::<(usize, usize)>::new();
            for i in 0..iterations {
                tokens.push(black_box((i / 80, i % 80)));
            }
            // Simulate lookahead
            let mut sum = 0usize;
            for i in 0..tokens.len().saturating_sub(1) {
                sum += tokens[i].0 + tokens[i + 1].0;
            }
            black_box(sum)
        });
    });

    // Using native Vec with capacity (best practice)
    group.bench_function("native_vec_with_capacity", |b| {
        b.iter(|| {
            let mut tokens = Vec::<(usize, usize)>::with_capacity(iterations);
            for i in 0..iterations {
                tokens.push(black_box((i / 80, i % 80)));
            }
            // Simulate lookahead
            let mut sum = 0usize;
            for i in 0..tokens.len().saturating_sub(1) {
                sum += tokens[i].0 + tokens[i + 1].0;
            }
            black_box(sum)
        });
    });

    // Using slices (best for read-heavy operations)
    group.bench_function("native_vec_slice_access", |b| {
        let mut tokens = Vec::<(usize, usize)>::with_capacity(iterations);
        for i in 0..iterations {
            tokens.push((i / 80, i % 80));
        }

        b.iter(|| {
            let slice = &tokens[..];
            let mut sum = 0usize;
            for i in 0..slice.len().saturating_sub(1) {
                sum += slice[i].0 + slice[i + 1].0;
            }
            black_box(sum)
        });
    });

    group.finish();
}

// ============================================================================
// SCENARIO 5: Error Collection (ErrorManager pattern)
// ============================================================================

fn bench_error_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_collection");

    #[derive(Clone, Debug)]
    struct MockError {
        line: usize,
        column: usize,
        message: String,
    }

    let iterations = 1000;

    // Using wrapper (current ErrorManager approach)
    group.bench_function("wrapper_add_clone", |b| {
        b.iter(|| {
            let mut errors = List::<MockError>::New();
            for i in 0..iterations {
                let error = MockError {
                    line: i / 80,
                    column: i % 80,
                    message: format!("Error at position {}", i),
                };
                errors.Add(black_box(error)); // Moves, not clones
            }
            errors
        });
    });

    // Using native Vec
    group.bench_function("native_vec", |b| {
        b.iter(|| {
            let mut errors = Vec::<MockError>::new();
            for i in 0..iterations {
                let error = MockError {
                    line: i / 80,
                    column: i % 80,
                    message: format!("Error at position {}", i),
                };
                errors.push(black_box(error));
            }
            errors
        });
    });

    // Get errors by cloning (BAD pattern)
    group.bench_function("get_errors_clone", |b| {
        let mut errors = List::<MockError>::New();
        for i in 0..iterations {
            errors.Add(MockError {
                line: i / 80,
                column: i % 80,
                message: format!("Error {}", i),
            });
        }

        b.iter(|| {
            let cloned = errors.clone(); // ❌ Expensive!
            black_box(cloned.Count())
        });
    });

    // Get errors by reference (GOOD pattern)
    group.bench_function("get_errors_borrow", |b| {
        let mut errors = List::<MockError>::New();
        for i in 0..iterations {
            errors.Add(MockError {
                line: i / 80,
                column: i % 80,
                message: format!("Error {}", i),
            });
        }

        b.iter(|| {
            black_box(errors.Count()) // ✅ Just borrow
        });
    });

    group.finish();
}

// ============================================================================
// Criterion configuration
// ============================================================================

criterion_group!(
    benches,
    bench_list_add,
    bench_list_iterate,
    bench_list_access,
    bench_dictionary_insert,
    bench_dictionary_lookup,
    bench_clone_vs_borrow,
    bench_hot_path_token_buffer,
    bench_error_collection
);

criterion_main!(benches);