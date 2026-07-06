//! Canonical serializer for portable Lyma values.

use crate::{
    Diagnostic, DiagnosticCode, LymaKey, LymaMapping, LymaNumber, LymaSequence, LymaTaggedValue,
    LymaValue, Severity,
};

/// Serializer configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializeOptions {
    /// Indentation width in spaces.
    pub indent_width: usize,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self { indent_width: 2 }
    }
}

/// Serializes a portable value into canonical Lyma text.
///
/// # Errors
///
/// Returns `E0030` when the value contains non-portable runtime values, non-string
/// mapping keys, or non-finite floating-point numbers.
pub fn serialize_value(value: &LymaValue) -> Result<String, Diagnostic> {
    serialize_value_with_options(value, SerializeOptions::default())
}

/// Serializes a portable value into canonical Lyma text with explicit options.
///
/// # Errors
///
/// Returns `E0030` when the value contains non-portable runtime values, non-string
/// mapping keys, or non-finite floating-point numbers.
pub fn serialize_value_with_options(
    value: &LymaValue,
    options: SerializeOptions,
) -> Result<String, Diagnostic> {
    let mut out = String::new();
    render_value(value, 0, &mut out, options)?;
    Ok(out)
}

fn render_value(
    value: &LymaValue,
    depth: usize,
    out: &mut String,
    options: SerializeOptions,
) -> Result<(), Diagnostic> {
    if let Some(inline) = inline_value(value)? {
        push_line(depth, options, &inline, out);
        return Ok(());
    }

    match value {
        LymaValue::Sequence(sequence) => render_sequence(sequence, depth, out, options),
        LymaValue::Mapping(mapping) => render_mapping(mapping, depth, out, options),
        LymaValue::Tagged(tagged) => render_tagged(tagged, depth, out, options),
        LymaValue::Function(host) => Err(serialization_error(format!(
            "non-portable function value `{}` cannot be serialized",
            host.kind
        ))),
        LymaValue::UserData(host) => Err(serialization_error(format!(
            "non-portable userdata value `{}` cannot be serialized",
            host.kind
        ))),
        LymaValue::HostObject(host) => Err(serialization_error(format!(
            "non-portable host object value `{}` cannot be serialized",
            host.kind
        ))),
        LymaValue::Null(_)
        | LymaValue::Boolean(_)
        | LymaValue::Number(_)
        | LymaValue::String(_) => {
            unreachable!()
        }
    }
}

fn render_sequence(
    sequence: &LymaSequence,
    depth: usize,
    out: &mut String,
    options: SerializeOptions,
) -> Result<(), Diagnostic> {
    for item in &sequence.items {
        if let Some(inline) = inline_value(item)? {
            push_line(depth, options, &format!("- {inline}"), out);
        } else {
            push_line(depth, options, "-", out);
            render_value(item, depth + 1, out, options)?;
        }
    }
    Ok(())
}

fn render_mapping(
    mapping: &LymaMapping,
    depth: usize,
    out: &mut String,
    options: SerializeOptions,
) -> Result<(), Diagnostic> {
    for entry in &mapping.entries {
        let key = match &entry.key {
            LymaKey::String(value) => format_string(value),
            LymaKey::Number(_) => {
                return Err(serialization_error(
                    "non-string mapping keys are not portable and cannot be serialized",
                ));
            }
            LymaKey::Boolean(_) => {
                return Err(serialization_error(
                    "boolean mapping keys are not portable and cannot be serialized",
                ));
            }
            LymaKey::Host(host) => {
                return Err(serialization_error(format!(
                    "host mapping key `{}` is not portable and cannot be serialized",
                    host.kind
                )));
            }
        };

        if let Some(inline) = inline_value(&entry.value)? {
            push_line(depth, options, &format!("{key}: {inline}"), out);
        } else {
            push_line(depth, options, &format!("{key}:"), out);
            render_value(&entry.value, depth + 1, out, options)?;
        }
    }
    Ok(())
}

fn render_tagged(
    tagged: &LymaTaggedValue,
    depth: usize,
    out: &mut String,
    options: SerializeOptions,
) -> Result<(), Diagnostic> {
    let tag = format!("!{}", tagged.tag.name.value);
    if let Some(inline) = inline_value(&tagged.value)? {
        push_line(depth, options, &format!("{tag} {inline}"), out);
        Ok(())
    } else {
        push_line(depth, options, &tag, out);
        render_value(&tagged.value, depth + 1, out, options)
    }
}

fn inline_value(value: &LymaValue) -> Result<Option<String>, Diagnostic> {
    match value {
        LymaValue::Null(_) => Ok(Some(String::from("null"))),
        LymaValue::Boolean(value) => Ok(Some(if *value {
            String::from("true")
        } else {
            String::from("false")
        })),
        LymaValue::Number(number) => Ok(Some(number_text(number)?)),
        LymaValue::String(value) => {
            if value.contains('\n') {
                Ok(None)
            } else {
                Ok(Some(format_string(value)))
            }
        }
        LymaValue::Tagged(tagged) => Ok(inline_value(&tagged.value)?
            .map(|inline| format!("!{} {inline}", tagged.tag.name.value))),
        LymaValue::Sequence(_) | LymaValue::Mapping(_) => Ok(None),
        LymaValue::Function(host) => Err(serialization_error(format!(
            "non-portable function value `{}` cannot be serialized",
            host.kind
        ))),
        LymaValue::UserData(host) => Err(serialization_error(format!(
            "non-portable userdata value `{}` cannot be serialized",
            host.kind
        ))),
        LymaValue::HostObject(host) => Err(serialization_error(format!(
            "non-portable host object value `{}` cannot be serialized",
            host.kind
        ))),
    }
}

fn number_text(number: &LymaNumber) -> Result<String, Diagnostic> {
    match number {
        LymaNumber::Integer(value) => Ok(value.to_string()),
        LymaNumber::Float(value) if value.is_finite() => Ok(value.to_string()),
        LymaNumber::Float(_) => Err(serialization_error(
            "NaN and infinity are not portable numeric values and cannot be serialized",
        )),
    }
}

fn format_string(value: &str) -> String {
    if is_plain(value) {
        value.to_owned()
    } else if value.contains('\n') {
        let mut out = String::from("|\n");
        for line in value.trim_end_matches('\n').split('\n') {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        out
    } else {
        quote_double(value)
    }
}

fn quote_double(value: &str) -> String {
    let mut out = String::new();
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn is_plain(value: &str) -> bool {
    if value.is_empty()
        || value != value.trim()
        || value.contains('\n')
        || value.starts_with(['-', '@', '!', '#'])
        || value.contains(": ")
        || value.contains("--")
        || matches!(value, "null" | "nil" | "true" | "false")
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
    {
        return false;
    }
    true
}

fn serialization_error(message: impl Into<String>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(DiagnosticCode::SerializationError, Severity::Error);
    diagnostic.message = message.into();
    diagnostic
}

fn indent(depth: usize, options: SerializeOptions) -> String {
    " ".repeat(depth.saturating_mul(options.indent_width))
}

fn push_line(depth: usize, options: SerializeOptions, text: &str, out: &mut String) {
    let rendered = text.replace("\r\n", "\n").replace('\r', "\n");
    if rendered.contains('\n') {
        let mut lines = rendered.lines();
        if let Some(first) = lines.next() {
            out.push_str(&indent(depth, options));
            out.push_str(first);
            out.push('\n');
        }
        for line in lines {
            out.push_str(&indent(depth, options));
            out.push_str(line);
            out.push('\n');
        }
    } else {
        out.push_str(&indent(depth, options));
        out.push_str(&rendered);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        FileId, LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNumber, LymaSequence,
        LymaTag, LymaTagName, LymaTaggedValue, LymaValue, Span,
    };

    use super::serialize_value;

    #[test]
    fn serializes_portable_values() {
        let value = LymaValue::Mapping(LymaMapping {
            entries: vec![
                LymaMappingEntry {
                    key: LymaKey::String(String::from("name")),
                    value: LymaValue::String(String::from("Example")),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String(String::from("list")),
                    value: LymaValue::Sequence(LymaSequence {
                        items: vec![LymaValue::Number(LymaNumber::Integer(1))],
                        span: None,
                    }),
                    span: None,
                },
            ],
            duplicate_keys: Vec::new(),
            span: None,
        });
        assert_eq!(
            serialize_value(&value).unwrap(),
            "name: Example\nlist:\n  - 1\n"
        );
    }

    #[test]
    fn rejects_non_portable_values() {
        let error = serialize_value(&LymaValue::Function(LymaHostValue {
            kind: String::from("fn"),
            label: None,
        }))
        .unwrap_err();
        assert_eq!(error.code.code(), "E0030");
    }

    #[test]
    fn rejects_non_string_keys_and_non_finite_numbers() {
        let bad_key = LymaValue::Mapping(LymaMapping {
            entries: vec![LymaMappingEntry {
                key: LymaKey::Boolean(true),
                value: LymaValue::Null(crate::LymaNull),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        });
        assert_eq!(serialize_value(&bad_key).unwrap_err().code.code(), "E0030");

        let tagged = LymaValue::Tagged(LymaTaggedValue {
            tag: LymaTag {
                name: LymaTagName {
                    value: String::from("Float"),
                    span: Span::new(FileId(1), 1, 6),
                },
                span: Span::new(FileId(1), 0, 0),
            },
            value: Box::new(LymaValue::Number(LymaNumber::Float(f64::INFINITY))),
            span: None,
        });
        assert_eq!(serialize_value(&tagged).unwrap_err().code.code(), "E0030");
    }
}
