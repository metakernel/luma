//! Structural lexer for normalized Luma source.

use luma_syntax::{BlockKind, Diagnostic, DiagnosticCode, FileId, Span, Token, TokenKind};

use crate::{
    decode::{SourceText, decode_str},
    error::{diagnostic, diagnostic_with_message},
    indent::{IndentationState, LineIndent},
};

/// Lexed output for one source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    /// Normalized source.
    pub source: SourceText,
    /// Produced tokens.
    pub tokens: Vec<Token>,
    /// Non-fatal diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Per-line indentation accounting.
    pub indents: Vec<LineIndent>,
}

/// Decodes and lexes a UTF-8 string.
#[must_use]
pub fn lex_str(file_id: FileId, name: &str, text: &str) -> Lexed {
    match decode_str(file_id, name, text) {
        Ok(source) => lex_source(source),
        Err(diagnostic) => Lexed {
            source: SourceText {
                source: luma_syntax::LumaSource::new(file_id, name, String::new()),
            },
            tokens: vec![Token {
                kind: TokenKind::EndOfFile,
                lexeme: String::new(),
                span: Span::new(file_id, 0, 0),
            }],
            diagnostics: vec![diagnostic],
            indents: Vec::new(),
        },
    }
}

/// Lexes normalized source text.
#[must_use]
pub fn lex_source(source: SourceText) -> Lexed {
    let mut lexer = Lexer::new(source);
    lexer.lex();
    lexer.finish()
}

struct Lexer {
    source: SourceText,
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    indents: Vec<LineIndent>,
    indentation: IndentationState,
    line_number: usize,
}

impl Lexer {
    fn new(source: SourceText) -> Self {
        Self {
            source,
            offset: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            indents: Vec::new(),
            indentation: IndentationState::default(),
            line_number: 1,
        }
    }

    fn finish(mut self) -> Lexed {
        let span = Span::new(
            self.file_id(),
            self.source.as_str().len(),
            self.source.as_str().len(),
        );
        self.tokens.push(Token {
            kind: TokenKind::EndOfFile,
            lexeme: String::new(),
            span,
        });
        Lexed {
            source: self.source,
            tokens: self.tokens,
            diagnostics: self.diagnostics,
            indents: self.indents,
        }
    }

    fn lex(&mut self) {
        let text = self.source.as_str().to_owned();
        let bytes = text.as_bytes();

        while self.offset < bytes.len() {
            let line_start = self.offset;
            let line_end = text[line_start..]
                .find('\n')
                .map_or(text.len(), |relative| line_start + relative);
            let line = &text[line_start..line_end];

            if line.starts_with("--[[") {
                self.lex_block_comment(&text, line_start);
                continue;
            }

            let indent_width = self.consume_indent(line_start, line);
            let indent_prefix_len = line
                .chars()
                .take_while(|ch| matches!(ch, ' ' | '\t'))
                .count();
            let content = &line[indent_prefix_len..];
            let trimmed = content.trim_end();
            let is_comment_only = trimmed.starts_with("--");
            let is_blank = trimmed.is_empty();
            self.indents.push(LineIndent {
                line: self.line_number,
                width: indent_width,
                is_ignorable: is_blank || is_comment_only,
            });

            if is_blank {
                self.finish_line(line_end);
                continue;
            }

            if trimmed == "---" {
                self.push_token(
                    TokenKind::DocumentSeparator,
                    line_start + indent_prefix_len,
                    line_start + indent_prefix_len + 3,
                    "---",
                );
                self.finish_line(line_end);
                continue;
            }

            if trimmed == "..." {
                self.push_token(
                    TokenKind::DocumentTerminator,
                    line_start + indent_prefix_len,
                    line_start + indent_prefix_len + 3,
                    "...",
                );
                self.finish_line(line_end);
                continue;
            }

            if is_comment_only {
                self.push_token(
                    TokenKind::Comment,
                    line_start + indent_prefix_len,
                    line_end,
                    trimmed,
                );
                self.finish_line(line_end);
                continue;
            }

            let (code, comment) = split_line_comment(trimmed);
            let allows_child = allows_child_block(code);
            if let Some(diagnostic) = self.indentation.observe_line(
                self.file_id(),
                line_start,
                indent_width,
                allows_child,
            ) {
                self.diagnostics.push(diagnostic);
            }

            self.lex_inline(line_start + indent_prefix_len, code);

            if let Some(comment_text) = comment {
                let comment_start = line_start
                    + indent_prefix_len
                    + code.len()
                    + count_trailing_spaces(&trimmed[code.len()..]);
                self.push_token(TokenKind::Comment, comment_start, line_end, comment_text);
            }

            if let Some(header) = block_header_from_suffix(code) {
                self.lex_block_scalar(&text, line_start, line_end, indent_width, header);
                continue;
            }

            self.finish_line(line_end);
        }
    }

    fn consume_indent(&mut self, line_start: usize, line: &str) -> usize {
        let mut width = 0;
        for (index, ch) in line.char_indices() {
            match ch {
                ' ' => width += 1,
                '\t' => {
                    self.diagnostics.push(diagnostic(
                        DiagnosticCode::TabUsedForIndentation,
                        Some(Span::new(
                            self.file_id(),
                            line_start + index,
                            line_start + index + 1,
                        )),
                    ));
                    width += 1;
                }
                _ => break,
            }
        }
        width
    }

    fn lex_inline(&mut self, line_offset: usize, code: &str) {
        let mut index = 0;
        let bytes = code.as_bytes();
        while index < bytes.len() {
            let ch = code[index..].chars().next().unwrap();
            if ch.is_whitespace() {
                index += ch.len_utf8();
                continue;
            }

            if let Some((kind, len)) = try_fixed_token(&code[index..]) {
                self.push_token(
                    kind,
                    line_offset + index,
                    line_offset + index + len,
                    &code[index..index + len],
                );
                index += len;
                if kind == TokenKind::Equals {
                    let expr = code[index..].trim();
                    if !expr.is_empty() {
                        let expr_start =
                            line_offset + index + code[index..].find(expr).unwrap_or(0);
                        self.push_token(
                            TokenKind::PlainString,
                            expr_start,
                            expr_start + expr.len(),
                            expr,
                        );
                        break;
                    }
                }
                continue;
            }

            if matches!(ch, '\'' | '"') {
                let start = index;
                if let Some(len) = scan_quoted(&code[index..], ch) {
                    let kind = TokenKind::String;
                    self.push_token(
                        kind,
                        line_offset + start,
                        line_offset + start + len,
                        &code[start..start + len],
                    );
                    index += len;
                } else {
                    self.diagnostics.push(diagnostic(
                        DiagnosticCode::UnterminatedString,
                        Some(Span::new(
                            self.file_id(),
                            line_offset + start,
                            line_offset + code.len(),
                        )),
                    ));
                    self.push_token(
                        TokenKind::Error,
                        line_offset + start,
                        line_offset + code.len(),
                        &code[start..],
                    );
                    break;
                }
                continue;
            }

            let start = index;
            while index < bytes.len() {
                let next = code[index..].chars().next().unwrap();
                if next.is_whitespace() || starts_fixed_token(&code[index..]) {
                    break;
                }
                index += next.len_utf8();
            }
            let lexeme = &code[start..index];
            let kind = classify_word(lexeme);
            self.push_token(kind, line_offset + start, line_offset + index, lexeme);
        }
    }

    fn lex_block_comment(&mut self, text: &str, start: usize) {
        if let Some(end_relative) = text[start + 4..].find("]]") {
            let end = start + 4 + end_relative + 2;
            let lexeme = &text[start..end];
            self.push_token(TokenKind::Comment, start, end, lexeme);
            self.offset = end;
            let consumed = lexeme.bytes().filter(|byte| *byte == b'\n').count();
            self.line_number += consumed;
            if self.offset < text.len() && text.as_bytes()[self.offset] == b'\n' {
                self.finish_line(self.offset);
            }
        } else {
            self.diagnostics.push(diagnostic(
                DiagnosticCode::UnterminatedBlockComment,
                Some(Span::new(self.file_id(), start, text.len())),
            ));
            self.push_token(TokenKind::Error, start, text.len(), &text[start..]);
            self.offset = text.len();
        }
    }

    fn lex_block_scalar(
        &mut self,
        text: &str,
        header_line_start: usize,
        header_line_end: usize,
        parent_indent: usize,
        kind: BlockKind,
    ) {
        let mut cursor = if header_line_end < text.len() {
            header_line_end + 1
        } else {
            header_line_end
        };
        let mut content_indent = None;
        let mut lines = Vec::new();
        let body_start = cursor;

        while cursor < text.len() {
            let next_end = text[cursor..]
                .find('\n')
                .map_or(text.len(), |relative| cursor + relative);
            let raw_line = &text[cursor..next_end];
            let indent = raw_line.chars().take_while(|ch| *ch == ' ').count();
            let trimmed = raw_line.trim_end();

            if !trimmed.is_empty() {
                if indent <= parent_indent {
                    break;
                }
                content_indent.get_or_insert(indent);
            }

            lines.push(raw_line.to_owned());
            cursor = if next_end < text.len() {
                next_end + 1
            } else {
                next_end
            };
        }

        let Some(content_indent) = content_indent else {
            self.diagnostics.push(diagnostic_with_message(
                DiagnosticCode::InvalidBlockScalar,
                Some(Span::new(
                    self.file_id(),
                    header_line_start,
                    header_line_end,
                )),
                "block scalar header must be followed by an indented content block",
            ));
            self.finish_line(header_line_end);
            return;
        };

        let mut body = String::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = if line.trim_end().is_empty() {
                ""
            } else {
                &line[content_indent.min(line.len())..]
            };
            body.push_str(trimmed);
            if idx + 1 < lines.len() || cursor <= text.len() {
                body.push('\n');
            }
        }
        apply_chomping(
            &mut body,
            code_suffix_from_kind(kind, text, header_line_start, header_line_end),
        );

        let span_end = cursor.min(text.len());
        self.push_token(TokenKind::String, body_start, span_end, &body);
        self.offset = cursor;
        self.line_number += text[header_line_end..cursor]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
    }

    fn finish_line(&mut self, line_end: usize) {
        if line_end < self.source.as_str().len() {
            self.push_token(TokenKind::LineBreak, line_end, line_end + 1, "\n");
            self.offset = line_end + 1;
            self.line_number += 1;
        } else {
            self.offset = line_end;
        }
    }

    fn push_token(&mut self, kind: TokenKind, start: usize, end: usize, lexeme: &str) {
        self.tokens.push(Token {
            kind,
            lexeme: lexeme.to_owned(),
            span: Span::new(self.file_id(), start, end),
        });
    }

    const fn file_id(&self) -> FileId {
        self.source.source.id
    }
}

fn split_line_comment(code: &str) -> (&str, Option<&str>) {
    let mut quote = None;
    let mut escape = false;
    for (index, ch) in code.char_indices() {
        if let Some(active) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' && active == '"' {
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

fn allows_child_block(code: &str) -> bool {
    let trimmed = code.trim_end();
    trimmed.ends_with(':') || trimmed == "-"
}

fn block_header_from_suffix(code: &str) -> Option<BlockKind> {
    for suffix in [
        "|expr-", "|expr+", "|expr", "|lua-", "|lua+", "|lua", "|-", "|+", "|", ">-", ">+", ">",
    ] {
        if code.trim_end().ends_with(suffix) {
            return Some(match suffix {
                "|expr-" | "|expr+" | "|expr" => BlockKind::LuaExpression,
                "|lua-" | "|lua+" | "|lua" => BlockKind::LuaChunk,
                ">-" | ">+" | ">" => BlockKind::Folded,
                _ => BlockKind::Literal,
            });
        }
    }
    None
}

fn code_suffix_from_kind(_kind: BlockKind, text: &str, line_start: usize, line_end: usize) -> &str {
    &text[line_start..line_end]
}

fn apply_chomping(body: &mut String, header_line: &str) {
    if header_line.trim_end().ends_with('-') {
        while body.ends_with('\n') {
            body.pop();
        }
    } else if !header_line.trim_end().ends_with('+') {
        while body.ends_with("\n\n") {
            body.pop();
        }
    }
}

fn count_trailing_spaces(fragment: &str) -> usize {
    fragment.len() - fragment.trim_start().len()
}

fn try_fixed_token(code: &str) -> Option<(TokenKind, usize)> {
    let pairs = [
        ("|expr-", TokenKind::BlockHeader(BlockKind::LuaExpression)),
        ("|expr+", TokenKind::BlockHeader(BlockKind::LuaExpression)),
        ("|expr", TokenKind::BlockHeader(BlockKind::LuaExpression)),
        ("|lua-", TokenKind::BlockHeader(BlockKind::LuaChunk)),
        ("|lua+", TokenKind::BlockHeader(BlockKind::LuaChunk)),
        ("|lua", TokenKind::BlockHeader(BlockKind::LuaChunk)),
        ("|-", TokenKind::BlockHeader(BlockKind::Literal)),
        ("|+", TokenKind::BlockHeader(BlockKind::Literal)),
        (">-", TokenKind::BlockHeader(BlockKind::Folded)),
        (">+", TokenKind::BlockHeader(BlockKind::Folded)),
        ("...", TokenKind::Spread),
        ("|", TokenKind::BlockHeader(BlockKind::Literal)),
        (">", TokenKind::BlockHeader(BlockKind::Folded)),
        (":", TokenKind::Colon),
        ("=", TokenKind::Equals),
        ("[", TokenKind::LeftBracket),
        ("]", TokenKind::RightBracket),
        ("{", TokenKind::LeftBrace),
        ("}", TokenKind::RightBrace),
        (",", TokenKind::PlainString),
    ];

    for (text, kind) in pairs {
        if code.starts_with(text) {
            return Some((kind, text.len()));
        }
    }

    if code.starts_with('-') && code[1..].chars().next().is_none_or(char::is_whitespace) {
        return Some((TokenKind::Dash, 1));
    }

    if let Some(stripped) = code.strip_prefix('@') {
        let len = stripped
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map_or(code.len(), |(idx, _)| idx + 1);
        return Some((TokenKind::DirectiveName, len));
    }

    if let Some(stripped) = code.strip_prefix('!') {
        let len = stripped
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace() || matches!(ch, ':' | '[' | ']'))
            .map_or(code.len(), |(idx, _)| idx + 1);
        return Some((TokenKind::TagName, len));
    }

    None
}

fn starts_fixed_token(code: &str) -> bool {
    try_fixed_token(code).is_some()
}

fn scan_quoted(code: &str, quote: char) -> Option<usize> {
    let mut escape = false;
    for (index, ch) in code.char_indices().skip(1) {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && quote == '"' {
            escape = true;
            continue;
        }
        if ch == quote {
            return Some(index + ch.len_utf8());
        }
    }
    None
}

fn classify_word(lexeme: &str) -> TokenKind {
    if lexeme == "let" {
        TokenKind::KeywordLet
    } else if lexeme == "as" {
        TokenKind::KeywordAs
    } else if lexeme == "in" {
        TokenKind::KeywordIn
    } else if lexeme.parse::<f64>().is_ok() {
        TokenKind::Number
    } else if lexeme
        .chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '.' | '/'))
    {
        TokenKind::Identifier
    } else {
        TokenKind::PlainString
    }
}

#[cfg(test)]
mod tests {
    use luma_syntax::{BlockKind, DiagnosticCode, FileId, TokenKind};

    use super::lex_str;

    #[test]
    fn normalizes_spec_section_5_encoding_rules() {
        let lexed = lex_str(FileId(1), "spec.luma", "\u{feff}---\r\n...");
        assert_eq!(lexed.diagnostics.len(), 0);
        assert_eq!(lexed.source.as_str(), "---\n...");
        assert_eq!(lexed.tokens[0].kind, TokenKind::DocumentSeparator);
        assert_eq!(lexed.tokens[2].kind, TokenKind::DocumentTerminator);
    }

    #[test]
    fn recognizes_structural_tokens_from_sections_7_and_8() {
        let lexed = lex_str(
            FileId(1),
            "spec.luma",
            "@luma 0.1\nservice: !http =base_port + 1 -- note\nitems:\n  - alpha\nspread: ...defaults\nscript: |lua\n  return 42\n",
        );

        assert!(lexed.diagnostics.is_empty());
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::DirectiveName)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Colon)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::TagName)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Equals)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Dash)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Spread)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Comment)
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::BlockHeader(BlockKind::LuaChunk))
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.lexeme == "base_port + 1")
        );
        assert!(
            lexed
                .tokens
                .iter()
                .any(|token| token.lexeme.contains("return 42"))
        );
    }

    #[test]
    fn treats_hash_as_plain_text() {
        let lexed = lex_str(FileId(1), "spec.luma", "label: Section #1\n");
        assert!(lexed.diagnostics.is_empty());
        assert!(lexed.tokens.iter().any(|token| token.lexeme == "Section"));
        assert!(lexed.tokens.iter().any(|token| token.lexeme == "#1"));
    }

    #[test]
    fn validates_indentation_rules_from_section_9() {
        let lexed = lex_str(
            FileId(1),
            "spec.luma",
            "limits:\n  memory_mb: 512\n    workers: 4\n",
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidIndentation)
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.code() == "E0002")
        );
    }

    #[test]
    fn emits_e0003_for_tabs_used_as_indentation() {
        let lexed = lex_str(FileId(1), "spec.luma", "root:\n\tchild: value\n");
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::TabUsedForIndentation)
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.code() == "E0003")
        );
    }

    #[test]
    fn emits_e0004_for_unterminated_strings() {
        let lexed = lex_str(FileId(1), "spec.luma", "message: \"oops\n");
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnterminatedString)
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.code() == "E0004")
        );
    }

    #[test]
    fn emits_e0005_for_unterminated_block_comments() {
        let lexed = lex_str(FileId(1), "spec.luma", "--[[\ncomment\n");
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnterminatedBlockComment)
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.code() == "E0005")
        );
    }

    #[test]
    fn emits_e0026_for_missing_block_content() {
        let lexed = lex_str(FileId(1), "spec.luma", "script: |expr\nnext: value\n");
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidBlockScalar)
        );
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.code() == "E0026")
        );
    }
}
