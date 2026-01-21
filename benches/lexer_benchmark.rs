use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use std::time::Instant;

fn main() {
    // Test with your largest .mdix file
    let input = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read file");

    // Warm up
    for _ in 0..10 {
        let tokenizer = Tokenizer::new(input.clone());
        let _ = tokenizer.tokenize();
    }

    // Profile run
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let tokenizer = Tokenizer::new(input.clone());
        let result = tokenizer.tokenize();
        std::hint::black_box(result); // Prevent optimization
    }

    let duration = start.elapsed();
    let avg = duration / iterations;
    let mb_per_sec = (input.len() as f64 * iterations as f64)
        / (1_000_000.0 * duration.as_secs_f64());

    println!("Avg time per tokenization: {:?}", avg);
    println!("Throughput: {:.2} MB/sec", mb_per_sec);
}