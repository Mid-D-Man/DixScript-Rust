// dixscript-cli/src/output/table.rs
//! Minimal aligned table formatter — no external dependencies.

use colored::Colorize;

/// Print a table with a header row and data rows.
///
/// Column widths are computed from the widest value in each column.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }

    let col_count = headers.len();

    // Compute column widths.
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    // Header row.
    let header_line: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<width$}", h, width = widths[i]))
        .collect();
    println!("  {}", header_line.join("  ").bold());

    // Separator.
    let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    println!("  {}", sep.join("  ").dimmed());

    // Data rows.
    for row in rows {
        let cells: Vec<String> = (0..col_count)
            .map(|i| {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                format!("{:<width$}", cell, width = widths[i])
            })
            .collect();
        println!("  {}", cells.join("  "));
    }
}
