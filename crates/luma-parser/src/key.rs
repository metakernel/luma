//! Mapping key parsing helpers.

use luma_syntax::{Diagnostic, DiagnosticCode, FileId, MappingKey, Span};

use crate::{
    error::{diagnostic, diagnostic_with_message},
    lua_capture::inline_expression,
    scalar::parse_quoted_string,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedKey {
    pub key: MappingKey,
    pub canonical: String,
    pub span: Span,
}

pub(crate) fn parse_mapping_key(
    text: &str,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ParsedKey> {
    let raw = text;
    let text = raw.trim();
    let start = start + raw.find(text).unwrap_or(0);
    let span = Span::new(file_id, start, start + text.len());
    if text.is_empty() {
        diagnostics.push(diagnostic(DiagnosticCode::InvalidMappingKey, Some(span)));
        return None;
    }
    if matches!(text, "null" | "nil") {
        diagnostics.push(diagnostic(DiagnosticCode::InvalidNullKey, Some(span)));
        return None;
    }
    if text.starts_with('[') {
        let Some(inner) = text.strip_prefix("[=").and_then(|s| s.strip_suffix(']')) else {
            diagnostics.push(diagnostic(DiagnosticCode::InvalidExpressionKey, Some(span)));
            return None;
        };
        let expression = inline_expression(inner, start + 1, file_id);
        return Some(ParsedKey {
            key: MappingKey::Expression { expression, span },
            canonical: String::new(),
            span,
        });
    }
    if text.starts_with(['{', '}', ',', '|', '>', '=']) {
        diagnostics.push(diagnostic_with_message(
            DiagnosticCode::InvalidMappingKey,
            Some(span),
            "invalid mapping key syntax",
        ));
        return None;
    }
    if let Some((value, style)) = parse_quoted_string(text, diagnostics, file_id, start) {
        let key = luma_syntax::StringNode {
            value: value.clone(),
            source: text.to_owned(),
            style,
            block_kind: None,
            chomping: None,
            span,
        };
        return Some(ParsedKey {
            key: MappingKey::Quoted(key),
            canonical: value,
            span,
        });
    }
    Some(ParsedKey {
        key: MappingKey::Plain {
            value: text.to_owned(),
            span,
            value_span: span,
        },
        canonical: text.to_owned(),
        span,
    })
}
