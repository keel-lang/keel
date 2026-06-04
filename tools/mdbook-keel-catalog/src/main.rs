use std::io::{self, Read};

use keel_lang::builtins::{BuiltinMethod, BuiltinResult};
use keel_lang::types::prelude::catalog;

fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(cmd) = args.next()
        && cmd == "supports"
    {
        let renderer = args.next().unwrap_or_default();
        std::process::exit(if renderer == "html" { 0 } else { 1 });
    }

    // mdBook passes [context, book] as JSON on stdin.
    // We operate on the book value in-place and write the modified book to stdout.
    // We avoid depending on the mdbook crate directly since 0.5+ removed the lib target.
    let mut raw = String::new();
    io::stdin()
        .read_to_string(&mut raw)
        .expect("failed to read stdin");

    let mut pair: serde_json::Value =
        serde_json::from_str(&raw).expect("failed to parse mdbook JSON input");

    let book = pair
        .get_mut(1)
        .expect("mdBook JSON input must be a [context, book] array");

    expand_book(book);

    serde_json::to_writer(io::stdout(), book).expect("failed to write mdbook JSON output");
}

/// Walk every chapter in the book value, expanding `{{#catalog Ns}}` directives.
fn expand_book(book: &mut serde_json::Value) {
    // The book JSON has a list of items at `book["sections"]` (mdBook ≤0.4)
    // or `book["items"]` (mdBook ≥0.5). Accept both.
    let items_key = if book.get("items").is_some() {
        "items"
    } else {
        "sections"
    };
    if let Some(items) = book[items_key].as_array_mut() {
        for item in items {
            expand_item(item);
        }
    }
}

fn expand_item(item: &mut serde_json::Value) {
    if let Some(chapter) = item.get_mut("Chapter") {
        if let Some(content) = chapter["content"].as_str() {
            let expanded = expand_directives(content);
            chapter["content"] = serde_json::Value::String(expanded);
        }
        // Chapter children live under "sub_items", not the book-level "items"/"sections" key.
        if let Some(sub_items) = chapter["sub_items"].as_array_mut() {
            for sub in sub_items {
                expand_item(sub);
            }
        }
    }
}

fn expand_directives(content: &str) -> String {
    let had_trailing_newline = content.ends_with('\n');
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if let Some(ns) = parse_directive(line) {
            out.push_str(&render_namespace_table(ns));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !had_trailing_newline && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn parse_directive(line: &str) -> Option<&str> {
    let inner = line
        .trim()
        .strip_prefix("{{#catalog ")?
        .strip_suffix("}}")?;
    Some(inner.trim())
}

fn render_namespace_table(ns: &str) -> String {
    let methods: Vec<&BuiltinMethod> = catalog().filter(|m| m.namespace == ns).collect();
    if methods.is_empty() {
        return format!("<!-- no catalog entries for {ns} -->\n");
    }

    let mut out = String::new();
    out.push_str("| Method | Signature | Description |\n");
    out.push_str("|--------|-----------|-------------|\n");
    for m in methods {
        let sig = render_signature(m);
        out.push_str(&format!(
            "| `{}.{}` | `{}` | {} |\n",
            m.namespace, m.name, sig, m.doc
        ));
    }
    out
}

fn render_signature(m: &BuiltinMethod) -> String {
    let params = m
        .params
        .iter()
        .map(|p| {
            let opt = if p.optional { "?" } else { "" };
            format!("{}{}: {}", p.name, opt, p.ty.to_keel_str())
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret = match m.result {
        BuiltinResult::Fixed(spec) => spec.to_keel_str().to_string(),
        BuiltinResult::AiExtract => "T?".to_string(),
        BuiltinResult::AiClassify => "Enum?".to_string(),
        BuiltinResult::Unknown => "dynamic".to_string(),
    };

    format!("{}.{}({}) \u{2192} {}", m.namespace, m.name, params, ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_directive_extracts_namespace() {
        assert_eq!(parse_directive("{{#catalog File}}"), Some("File"));
        assert_eq!(parse_directive("  {{#catalog Random}}  "), Some("Random"));
        assert_eq!(parse_directive("{{#catalog Ai}}"), Some("Ai"));
        assert_eq!(parse_directive("regular text"), None);
        assert_eq!(parse_directive("{{#include foo.md}}"), None);
    }

    #[test]
    fn render_namespace_table_file() {
        let table = render_namespace_table("File");
        assert!(table.contains("| Method | Signature | Description |"));
        assert!(table.contains("`File.read`"));
        assert!(table.contains("`File.write`"));
        assert!(table.contains("path: str"));
        assert!(table.contains("→ str"));
        assert!(table.contains("→ none"));
    }

    #[test]
    fn render_namespace_table_unknown_ns() {
        let out = render_namespace_table("NonExistent");
        assert!(out.contains("no catalog entries"));
    }

    #[test]
    fn render_namespace_table_ai_special_returns() {
        let table = render_namespace_table("Ai");
        assert!(table.contains("Enum?"), "AiClassify should render as Enum?");
        assert!(table.contains("T?"), "AiExtract should render as T?");
    }

    #[test]
    fn expand_directives_replaces_catalog_line() {
        let input = "Before\n{{#catalog Random}}\nAfter";
        let out = expand_directives(input);
        assert!(out.contains("Before"));
        assert!(out.contains("`Random.float`"));
        assert!(out.contains("After"));
        assert!(!out.contains("{{#catalog Random}}"));
    }

    #[test]
    fn expand_directives_preserves_surrounding_prose() {
        let input = "# Title\n\nSome prose.\n\n{{#catalog Log}}\n\nMore prose.";
        let out = expand_directives(input);
        assert!(out.contains("# Title"));
        assert!(out.contains("Some prose."));
        assert!(out.contains("`Log.info`"));
        assert!(out.contains("More prose."));
    }

    #[test]
    fn expand_directives_preserves_trailing_newline() {
        let with_newline = "Some prose.\n";
        let out = expand_directives(with_newline);
        assert_eq!(out, "Some prose.\n", "trailing newline must be preserved");

        let without_newline = "Some prose.";
        let out = expand_directives(without_newline);
        assert_eq!(out, "Some prose.", "no trailing newline must not be added");
    }

    #[test]
    fn expand_directives_directive_at_end_preserves_trailing_newline() {
        // render_namespace_table always ends with \n; that newline should be the
        // trailing newline when the directive is the last line of the file.
        let input = "Intro.\n\n{{#catalog Random}}\n";
        let out = expand_directives(input);
        assert!(out.ends_with('\n'), "output must end with newline");
        assert!(out.contains("`Random.float`"));
    }

    #[test]
    fn optional_param_renders_with_question_mark() {
        let table = render_namespace_table("Cache");
        assert!(
            table.contains("ttl?: duration"),
            "optional ttl param should render with ?"
        );
    }

    #[test]
    fn expand_directives_in_sub_items() {
        let mut book = serde_json::json!({
            "items": [{
                "Chapter": {
                    "content": "",
                    "sub_items": [{
                        "Chapter": {
                            "content": "{{#catalog Random}}",
                            "sub_items": []
                        }
                    }]
                }
            }]
        });
        expand_book(&mut book);
        let inner = &book["items"][0]["Chapter"]["sub_items"][0]["Chapter"]["content"];
        assert!(
            inner.as_str().unwrap().contains("`Random.float`"),
            "directives in sub-chapters must be expanded"
        );
    }
}
