use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{find_arg, ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Str", {
        "match" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.match: missing text argument"))?;
            let pattern = positional(&args, 1)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.match: missing pattern argument"))?;

            let re = regex::Regex::new(&pattern)
                .map_err(|e| miette::miette!("Str.match: invalid regex: {e}"))?;

            Ok(Value::Bool(re.is_match(&text)))
        }),
        "extract" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.extract: missing text argument"))?;
            let pattern = positional(&args, 1)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.extract: missing pattern argument"))?;

            let re = regex::Regex::new(&pattern)
                .map_err(|e| miette::miette!("Str.extract: invalid regex: {e}"))?;

            match re.captures(&text) {
                Some(caps) => {
                    match caps.get(1) {
                        Some(m) => Ok(Value::String(m.as_str().to_string())),
                        None => Ok(Value::None),
                    }
                }
                None => Ok(Value::None),
            }
        }),
        "truncate" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.truncate: missing text argument"))?;
            let max_i = positional(&args, 1)
                .and_then(|v| v.as_int())
                .ok_or_else(|| miette::miette!("Str.truncate: missing max argument"))?;
            if max_i < 0 {
                return Err(miette::miette!("Str.truncate: max must be non-negative, got {max_i}"));
            }
            let max_chars = max_i as usize;

            let char_count = text.chars().count();
            if char_count <= max_chars {
                Ok(Value::String(text))
            } else {
                let truncated: String = text.chars().take(max_chars).collect();
                Ok(Value::String(format!("{}…", truncated)))
            }
        }),
        "pad" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Str.pad: missing text argument"))?;
            let width_i = positional(&args, 1)
                .and_then(|v| v.as_int())
                .ok_or_else(|| miette::miette!("Str.pad: missing width argument"))?;
            if width_i < 0 {
                return Err(miette::miette!("Str.pad: width must be non-negative, got {width_i}"));
            }
            let width = width_i as usize;

            let pad_char_str = find_arg(&args, "char")
                .map(|v| v.to_display_string())
                .unwrap_or_else(|| " ".to_string());

            let pad_char = pad_char_str.chars().next().unwrap_or(' ');
            let len = text.chars().count();

            if len >= width {
                Ok(Value::String(text))
            } else {
                let padding: String = std::iter::repeat_n(pad_char, width - len).collect();
                Ok(Value::String(format!("{}{}", padding, text)))
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::CallArgValue;
    use crate::interpreter::Interpreter;

    #[test]
    fn namespace_has_expected_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Str");
        assert!(ns.methods.contains_key("match"));
        assert!(ns.methods.contains_key("extract"));
        assert!(ns.methods.contains_key("truncate"));
        assert!(ns.methods.contains_key("pad"));
    }

    fn arg(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    #[tokio::test]
    async fn match_returns_true_for_matching_pattern() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("match").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello".into())),
                arg(Value::String("ell".into())),
            ],
        )
        .await;
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[tokio::test]
    async fn match_returns_false_for_non_matching_pattern() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("match").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello".into())),
                arg(Value::String("xyz".into())),
            ],
        )
        .await;
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[tokio::test]
    async fn match_rejects_invalid_regex() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("match").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello".into())),
                arg(Value::String("[".into())),
            ],
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid regex"));
    }

    #[tokio::test]
    async fn extract_returns_captured_group() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("extract").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello world".into())),
                arg(Value::String(r"(\w+)$".into())),
            ],
        )
        .await;
        assert_eq!(result.unwrap(), Value::String("world".into()));
    }

    #[tokio::test]
    async fn extract_returns_none_when_no_match() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("extract").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello".into())),
                arg(Value::String(r"(\d+)".into())),
            ],
        )
        .await;
        assert_eq!(result.unwrap(), Value::None);
    }

    #[tokio::test]
    async fn truncate_shortens_text_and_adds_ellipsis() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("truncate").unwrap();
        let result = method(
            &mut interp,
            vec![
                arg(Value::String("hello world".into())),
                arg(Value::Integer(5)),
            ],
        )
        .await;
        assert_eq!(result.unwrap(), Value::String("hello…".into()));
    }

    #[tokio::test]
    async fn truncate_returns_unchanged_when_shorter_than_max() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("truncate").unwrap();
        let result = method(
            &mut interp,
            vec![arg(Value::String("hi".into())), arg(Value::Integer(10))],
        )
        .await;
        assert_eq!(result.unwrap(), Value::String("hi".into()));
    }

    #[tokio::test]
    async fn pad_adds_left_padding_with_spaces() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("pad").unwrap();
        let result = method(
            &mut interp,
            vec![arg(Value::String("42".into())), arg(Value::Integer(5))],
        )
        .await;
        assert_eq!(result.unwrap(), Value::String("   42".into()));
    }

    #[tokio::test]
    async fn pad_returns_unchanged_when_already_wide_enough() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("pad").unwrap();
        let result = method(
            &mut interp,
            vec![arg(Value::String("hello".into())), arg(Value::Integer(3))],
        )
        .await;
        assert_eq!(result.unwrap(), Value::String("hello".into()));
    }
}
