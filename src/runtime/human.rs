use std::io::{self, Write};

use colored::Colorize;

use crate::interpreter::value::Value;

/// Display a notification to the user (non-blocking).
pub fn notify(message: &str) {
    println!("  {} {}", "▸".bright_cyan(), message);
}

/// Show structured data to the user with formatted output.
pub fn show(value: &Value) {
    // In REPL mode, suppress none output (from statements that don't return values)
    if matches!(value, Value::None) && std::env::var("KEEL_REPL").as_deref() == Ok("1") {
        return;
    }
    match value {
        Value::Map(fields) => {
            // Determine the longest key for alignment
            let max_key = fields.keys().map(|k| k.len()).max().unwrap_or(0);
            println!("  {}", "┌".dimmed());
            for (key, val) in fields {
                println!(
                    "  {} {:width$}  {}",
                    "│".dimmed(),
                    key.bright_white().bold(),
                    format_display_value(val),
                    width = max_key
                );
            }
            println!("  {}", "└".dimmed());
        }
        Value::List(items) => {
            if items.is_empty() {
                println!("  {}", "(empty list)".dimmed());
                return;
            }
            // Check if items are maps (table display)
            if items.iter().all(|i| matches!(i, Value::Map(_))) {
                show_table(items);
            } else {
                for (i, item) in items.iter().enumerate() {
                    println!(
                        "  {} {}",
                        format!("{}.", i + 1).dimmed(),
                        format_display_value(item)
                    );
                }
            }
        }
        other => {
            println!("  {}", format_display_value(other));
        }
    }
}

/// Render a list of maps as a table.
fn show_table(items: &[Value]) {
    // Collect all column names
    let mut columns: Vec<String> = Vec::new();
    for item in items {
        if let Value::Map(fields) = item {
            for key in fields.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
    }

    if columns.is_empty() {
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
    for item in items {
        if let Value::Map(fields) = item {
            for (i, col) in columns.iter().enumerate() {
                let val_len = fields
                    .get(col)
                    .map(|v| v.as_string().len())
                    .unwrap_or(0);
                if val_len > widths[i] {
                    widths[i] = val_len;
                }
            }
        }
    }

    // Cap column widths at 40
    for w in &mut widths {
        if *w > 40 {
            *w = 40;
        }
    }

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:width$}", c, width = widths[i]))
        .collect();
    let separator: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();

    println!("  {}", header.join("  ").bright_white().bold());
    println!("  {}", separator.join("──").dimmed());

    // Rows
    for item in items {
        if let Value::Map(fields) = item {
            let row: Vec<String> = columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let val = fields
                        .get(col)
                        .map(|v| v.as_string())
                        .unwrap_or_default();
                    let truncated = if val.len() > widths[i] {
                        format!("{}…", &val[..widths[i] - 1])
                    } else {
                        val
                    };
                    format!("{:width$}", truncated, width = widths[i])
                })
                .collect();
            println!("  {}", row.join("  "));
        }
    }
}

fn format_display_value(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Integer(n) => n.to_string().bright_yellow().to_string(),
        Value::Float(n) => format!("{n}").bright_yellow().to_string(),
        Value::Bool(b) => format!("{b}").bright_magenta().to_string(),
        Value::None => "none".dimmed().to_string(),
        Value::EnumVariant(ty, var, _) => format!("{ty}.{var}").bright_cyan().to_string(),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(|i| format_display_value(i)).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_display_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        other => format!("{other}"),
    }
}

/// Ask the user a question and wait for their response.
pub fn ask(prompt: &str) -> String {
    println!();
    print!(
        "  {} {} ",
        "?".bright_yellow().bold(),
        prompt.bright_white()
    );
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

/// Ask the user for yes/no confirmation.
pub fn confirm(message: &str) -> bool {
    println!();
    println!("  {}", message.dimmed());
    print!(
        "  {} {} ",
        "?".bright_yellow().bold(),
        "Confirm? (y/n)".bright_white()
    );
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let answer = input.trim().to_lowercase();
    answer == "y" || answer == "yes"
}
