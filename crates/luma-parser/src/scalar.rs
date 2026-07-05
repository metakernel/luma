//! Scalar parsing helpers.

use luma_syntax::{
    Diagnostic, DiagnosticCode, FileId, LumaNode, NumberNode, Span, StringNode, StringStyle,
};

use crate::{
    error::{diagnostic, diagnostic_with_message},
    lua_capture::{inline_expression, inline_table},
};

pub(crate) fn parse_inline_scalar(
    text: &str,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> LumaNode {
    let text = text.trim();
    if text.is_empty() || matches!(text, "null" | "nil") {
        return LumaNode::Null {
            span: Span::new(file_id, start, start + text.len()),
        };
    }
    if text == "true" {
        return LumaNode::Boolean {
            value: true,
            span: Span::new(file_id, start, start + text.len()),
        };
    }
    if text == "false" {
        return LumaNode::Boolean {
            value: false,
            span: Span::new(file_id, start, start + text.len()),
        };
    }
    if let Some((value, style)) = parse_quoted_string(text, diagnostics, file_id, start) {
        return LumaNode::String(StringNode {
            value,
            source: text.to_owned(),
            style,
            block_kind: None,
            chomping: None,
            span: Span::new(file_id, start, start + text.len()),
        });
    }
    if text.starts_with('=') {
        return LumaNode::LuaExpression(inline_expression(text, start, file_id));
    }
    if text.starts_with('{') && text.ends_with('}') {
        return LumaNode::LuaTableConstructor(inline_table(text, start, file_id));
    }
    if is_non_finite_number(text) {
        diagnostics.push(diagnostic_with_message(
            DiagnosticCode::ReservedSyntax,
            Some(Span::new(file_id, start, start + text.len())),
            "NaN and infinity are not valid literal numbers",
        ));
        return LumaNode::String(StringNode {
            value: text.to_owned(),
            source: text.to_owned(),
            style: StringStyle::Plain,
            block_kind: None,
            chomping: None,
            span: Span::new(file_id, start, start + text.len()),
        });
    }
    if is_lua_number(text) {
        return LumaNode::Number(NumberNode {
            lexeme: text.to_owned(),
            span: Span::new(file_id, start, start + text.len()),
        });
    }

    LumaNode::String(StringNode {
        value: text.to_owned(),
        source: text.to_owned(),
        style: StringStyle::Plain,
        block_kind: None,
        chomping: None,
        span: Span::new(file_id, start, start + text.len()),
    })
}

pub(crate) fn parse_quoted_string(
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
    file_id: FileId,
    start: usize,
) -> Option<(String, StringStyle)> {
    let quote = text.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let style = if quote == '\'' {
        StringStyle::SingleQuoted
    } else {
        StringStyle::DoubleQuoted
    };
    let mut value = String::new();
    let mut chars = text.char_indices().peekable();
    chars.next();

    while let Some((index, ch)) = chars.next() {
        if ch == quote {
            if index + ch.len_utf8() != text.len() {
                return None;
            }
            return Some((value, style));
        }
        if ch != '\\' {
            value.push(ch);
            continue;
        }
        let Some((_, escaped)) = chars.next() else {
            diagnostics.push(diagnostic(
                DiagnosticCode::UnterminatedString,
                Some(Span::new(file_id, start, start + text.len())),
            ));
            return Some((value, style));
        };
        match escaped {
            'a' => value.push('\u{0007}'),
            'b' => value.push('\u{0008}'),
            'f' => value.push('\u{000c}'),
            'n' => value.push('\n'),
            'r' => value.push('\r'),
            't' => value.push('\t'),
            'v' => value.push('\u{000b}'),
            '\\' => value.push('\\'),
            '"' => value.push('"'),
            '\'' => value.push('\''),
            'z' => {
                while let Some((_, whitespace)) = chars.peek().copied() {
                    if whitespace.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            'x' => {
                let hi = chars.next().map(|(_, ch)| ch);
                let lo = chars.next().map(|(_, ch)| ch);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    if let Ok(byte) = u8::from_str_radix(&format!("{hi}{lo}"), 16) {
                        value.push(char::from(byte));
                    }
                }
            }
            'u' => {
                if chars.next().map(|(_, ch)| ch) != Some('{') {
                    continue;
                }
                let mut hex = String::new();
                for (_, ch) in chars.by_ref() {
                    if ch == '}' {
                        break;
                    }
                    hex.push(ch);
                }
                if let Ok(codepoint) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(codepoint) {
                        value.push(ch);
                    }
                }
            }
            digit if digit.is_ascii_digit() => {
                let mut decimal = String::from(digit);
                for _ in 0..2 {
                    if let Some((_, next)) = chars.peek().copied() {
                        if next.is_ascii_digit() {
                            decimal.push(next);
                            chars.next();
                        }
                    }
                }
                if let Ok(byte) = decimal.parse::<u8>() {
                    value.push(char::from(byte));
                }
            }
            other => value.push(other),
        }
    }

    diagnostics.push(diagnostic(
        DiagnosticCode::UnterminatedString,
        Some(Span::new(file_id, start, start + text.len())),
    ));
    Some((value, style))
}

pub(crate) fn split_line_comment(code: &str) -> (&str, Option<&str>) {
    let mut quote = None;
    let mut escape = false;
    for (index, ch) in code.char_indices() {
        if let Some(active) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == active {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '-' if code[index..].starts_with("--") => {
                return (code[..index].trim_end(), Some(&code[index..]));
            }
            _ => {}
        }
    }
    (code.trim_end(), None)
}

fn is_non_finite_number(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "nan" | "+nan" | "-nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
    )
}

fn is_lua_number(text: &str) -> bool {
    if text.is_empty() || is_non_finite_number(text) {
        return false;
    }
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        return is_hex_number(hex);
    }
    is_decimal_number(body)
}

fn is_decimal_number(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    let mut digits_before = 0usize;
    while chars.peek().is_some_and(char::is_ascii_digit) {
        digits_before += 1;
        chars.next();
    }
    let mut digits_after = 0usize;
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            digits_after += 1;
            chars.next();
        }
    }
    if digits_before == 0 && digits_after == 0 {
        return false;
    }
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        let mut exponent_digits = 0usize;
        while chars.peek().is_some_and(char::is_ascii_digit) {
            exponent_digits += 1;
            chars.next();
        }
        if exponent_digits == 0 {
            return false;
        }
    }
    chars.next().is_none()
}

fn is_hex_number(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    let mut whole_digits = 0usize;
    while chars.peek().is_some_and(char::is_ascii_hexdigit) {
        whole_digits += 1;
        chars.next();
    }
    let mut fractional_digits = 0usize;
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(char::is_ascii_hexdigit) {
            fractional_digits += 1;
            chars.next();
        }
    }
    if whole_digits == 0 && fractional_digits == 0 {
        return false;
    }
    if matches!(chars.peek(), Some('p' | 'P')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        let mut exponent_digits = 0usize;
        while chars.peek().is_some_and(char::is_ascii_digit) {
            exponent_digits += 1;
            chars.next();
        }
        if exponent_digits == 0 {
            return false;
        }
    }
    chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use luma_syntax::{DiagnosticCode, FileId, LumaNode};

    use super::parse_inline_scalar;

    #[test]
    fn parses_lua_numbers() {
        for literal in ["42", "-12", "3.5", "1.2e-4", "0xff", "0x1.8p1"] {
            let mut diagnostics = Vec::new();
            assert!(matches!(
                parse_inline_scalar(literal, 0, FileId(1), &mut diagnostics),
                LumaNode::Number(_)
            ));
            assert!(diagnostics.is_empty());
        }
    }

    #[test]
    fn rejects_non_finite_number_literals() {
        let mut diagnostics = Vec::new();
        let _ = parse_inline_scalar("NaN", 0, FileId(1), &mut diagnostics);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::ReservedSyntax)
        );
    }
}
