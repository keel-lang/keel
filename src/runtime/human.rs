use std::collections::HashSet;
use std::io::{self, Write};

use colored::Colorize;

use crate::interpreter::value::{MapKey, Value};
use crate::runtime::context::EnvProvider;

/// Display a notification to the user (non-blocking).
pub fn notify(message: &str) {
    println!("  {} {}", "▸".bright_cyan(), message);
}

/// Show structured data to the user with formatted output.
pub fn show(value: &Value) {
    let repl_mode = crate::runtime::context::NativeEnv
        .var("KEEL_REPL")
        .as_deref()
        == Some("1");
    show_with_repl(value, repl_mode);
}

pub fn show_with_repl(value: &Value, repl_mode: bool) {
    // In REPL mode, suppress none output (from statements that don't return values)
    if matches!(value, Value::None) && repl_mode {
        return;
    }
    match value {
        Value::Map(fields) => {
            // Determine the longest key for alignment
            let max_key = fields.keys().map(|k| k.to_string().len()).max().unwrap_or(0);
            println!("  {}", "┌".dimmed());
            let mut pairs: Vec<(&MapKey, &Value)> = fields.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            for (key, val) in pairs {
                let key_str = key.to_string();
                println!(
                    "  {} {:width$}  {}",
                    "│".dimmed(),
                    key_str.bright_white().bold(),
                    format_display_value(val),
                    width = max_key
                );
            }
            println!("  {}", "└".dimmed());
        }
        Value::Range(lo, hi) => {
            println!("  {}", format!("{lo}..{hi}").bright_yellow());
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
    // Collect all column keys in insertion order, O(1) membership check.
    let mut columns: Vec<MapKey> = Vec::new();
    let mut seen: HashSet<MapKey> = HashSet::new();
    for item in items {
        if let Value::Map(fields) = item {
            for key in fields.keys() {
                if seen.insert(key.clone()) {
                    columns.push(key.clone());
                }
            }
        }
    }

    if columns.is_empty() {
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = columns.iter().map(|c| c.to_string().len()).collect();
    for item in items {
        if let Value::Map(fields) = item {
            for (i, col) in columns.iter().enumerate() {
                let val_len = fields
                    .get(col)
                    .map(|v| v.to_display_string().len())
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
        .map(|(i, c)| format!("{:width$}", c.to_string(), width = widths[i]))
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
                        .map(|v| v.to_display_string())
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
        Value::Range(lo, hi) => format!("{lo}..{hi}"),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(format_display_value).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(fields) => {
            let mut pairs: Vec<(&MapKey, &Value)> = fields.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_display_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        other => format!("{other}"),
    }
}

/// Ask the user a question and wait for their response.
pub fn ask(prompt: &str) -> io::Result<String> {
    println!();
    print!(
        "  {} {} ",
        "?".bright_yellow().bold(),
        prompt.bright_white()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Ask the user for yes/no confirmation.
pub fn confirm(message: &str) -> io::Result<bool> {
    println!();
    println!("  {}", message.dimmed());
    print!(
        "  {} {} ",
        "?".bright_yellow().bold(),
        "Confirm? (y/n)".bright_white()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(parse_confirmation(&input))
}

fn parse_confirmation(input: &str) -> bool {
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::interpreter::value::MapKey;

    fn strip_ansi(input: &str) -> String {
        let mut out = String::new();
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' && chars.peek() == Some(&'[') {
                let _ = chars.next();
                for code in chars.by_ref() {
                    if code.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn parse_confirmation_accepts_yes_answers() {
        assert!(parse_confirmation("y\n"));
        assert!(parse_confirmation("yes"));
        assert!(parse_confirmation(" YES "));
    }

    #[test]
    fn parse_confirmation_rejects_other_answers() {
        assert!(!parse_confirmation("n"));
        assert!(!parse_confirmation("no"));
        assert!(!parse_confirmation(""));
        assert!(!parse_confirmation("yep"));
    }

    #[test]
    fn format_display_value_formats_scalars() {
        assert_eq!(
            strip_ansi(&format_display_value(&Value::String("hi".into()))),
            "hi"
        );
        assert_eq!(strip_ansi(&format_display_value(&Value::Integer(42))), "42");
        assert_eq!(strip_ansi(&format_display_value(&Value::Float(3.5))), "3.5");
        assert_eq!(
            strip_ansi(&format_display_value(&Value::Bool(true))),
            "true"
        );
        assert_eq!(strip_ansi(&format_display_value(&Value::None)), "none");
    }

    #[test]
    fn format_display_value_formats_structured_values() {
        let mut map = HashMap::new();
        map.insert(MapKey::Str("name".into()), Value::String("Ada".into()));
        map.insert(MapKey::Str("age".into()), Value::Integer(42));

        let list = Value::List(vec![
            Value::String("x".into()),
            Value::Range(1, 3),
            Value::Map(map),
        ]);
        let rendered = strip_ansi(&format_display_value(&list));

        assert!(rendered.starts_with("[x, 1..3, {"));
        assert!(rendered.contains("name: Ada"));
        assert!(rendered.contains("age: 42"));
    }

    #[test]
    fn format_display_value_formats_enum_variants() {
        let rendered = strip_ansi(&format_display_value(&Value::EnumVariant(
            "Status".into(),
            "open".into(),
            None,
        )));

        assert_eq!(rendered, "Status.open");
    }
}
