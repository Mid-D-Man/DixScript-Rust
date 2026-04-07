
//! Colored terminal output helpers.
//!
//! All functions respect `colored::control::set_override(false)` — pass
//! `--no-color` at the CLI level and every call here becomes plain text.

use colored::Colorize;
use std::time::Duration;

pub fn success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

pub fn warning(msg: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{}", msg.dimmed());
}

pub fn section(title: &str) {
    println!("\n{}", title.cyan().bold());
    println!("{}", "─".repeat(title.len()).cyan());
}

/// Print a key-value pair with right-aligned key column.
pub fn kv(key: &str, value: &str) {
    println!("  {:<24} {}", key.bold(), value);
}

/// Print a list of file paths with an arrow prefix.
pub fn file_list(files: &[String]) {
    for f in files {
        println!("  {} {}", "→".cyan(), f);
    }
}

/// Print elapsed duration in a human-readable form.
pub fn duration(elapsed: Duration) {
    let ms = elapsed.as_secs_f64() * 1000.0;
    if ms < 1.0 {
        info(&format!("  completed in {:.2}μs", elapsed.as_micros()));
    } else if ms < 1000.0 {
        info(&format!("  completed in {:.2}ms", ms));
    } else {
        info(&format!("  completed in {:.2}s", elapsed.as_secs_f64()));
    }
}

/// Print a labeled count badge, colored green if count is zero.
pub fn count_badge(label: &str, count: usize, zero_is_good: bool) {
    let formatted = if zero_is_good && count == 0 {
        count.to_string().green().to_string()
    } else if !zero_is_good && count == 0 {
        count.to_string().dimmed().to_string()
    } else {
        count.to_string().yellow().to_string()
    };
    kv(label, &formatted);
}
