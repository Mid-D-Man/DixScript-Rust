use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use std::time::Instant;

fn main() {
    // Load test file
    let test_input = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read test file");

    println!("\n=== DixScript Lexer Benchmark ===");
    println!("Input size: {} bytes", test_input.len());

    // Warmup
    println!("\nWarming up...");
    for _ in 0..10 {
        let tokenizer = Tokenizer::new(test_input.clone());
        let _ = tokenizer.tokenize();
    }

    // Actual benchmark
    println!("Running benchmark (100 iterations)...\n");
    let mut times = Vec::new();

    for i in 0..100 {
        let start = Instant::now();
        let tokenizer = Tokenizer::new(test_input.clone());
        let tokens = tokenizer.tokenize();
        let duration = start.elapsed();

        times.push(duration);

        if i == 0 {
            println!("Token count: {}", tokens.tokens.len());
        }

        if (i + 1) % 10 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    println!("\n");

    // Calculate stats
    times.sort();
    let min = times[0];
    let max = times[times.len() - 1];
    let median = times[50];
    let avg: std::time::Duration = times.iter().sum::<std::time::Duration>() / times.len() as u32;

    let bytes = test_input.len();
    let throughput_mb_s = (bytes as f64 / 1_000_000.0) / median.as_secs_f64();

    println!("Results:");
    println!("  Min time:     {:?}", min);
    println!("  Median time:  {:?}", median);
    println!("  Avg time:     {:?}", avg);
    println!("  Max time:     {:?}", max);
    println!("\n  Throughput:   {:.2} MB/s", throughput_mb_s);
}