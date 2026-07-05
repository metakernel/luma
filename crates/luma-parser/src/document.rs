//! Document/comment helpers.

use luma_syntax::{Comment, CommentKind, Diagnostic, DiagnosticCode, FileId, Span};

use crate::parser::LineInfo;

pub(crate) fn parse_line_comment(text: &str, start: usize, file_id: FileId) -> Comment {
    Comment {
        kind: CommentKind::Line,
        text: text.trim_start_matches("--").trim().to_owned(),
        span: Span::new(file_id, start, start + text.len()),
    }
}

pub(crate) fn parse_block_comment(
    source: &str,
    lines: &[LineInfo],
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
    file_id: FileId,
) -> (Comment, usize) {
    let start = lines[index].start + lines[index].indent;
    let mut end = lines[index].end;
    let mut text = String::new();
    let mut cursor = index;
    let first = source[lines[index].start + lines[index].indent..lines[index].end].trim_end();
    let fragment = first.trim_start_matches("--[[");
    if let Some((prefix, _)) = fragment.split_once("]]") {
        return (
            Comment {
                kind: CommentKind::Block,
                text: prefix.to_owned(),
                span: Span::new(file_id, start, start + first.len()),
            },
            index + 1,
        );
    }
    if !fragment.is_empty() {
        text.push_str(fragment);
    }
    cursor += 1;
    while cursor < lines.len() {
        let raw = &source[lines[cursor].start..lines[cursor].end];
        if !text.is_empty() {
            text.push('\n');
        }
        if let Some((prefix, _)) = raw.split_once("]]") {
            text.push_str(prefix.trim_end());
            end = lines[cursor].end;
            return (
                Comment {
                    kind: CommentKind::Block,
                    text,
                    span: Span::new(file_id, start, end),
                },
                cursor + 1,
            );
        }
        text.push_str(raw.trim_end());
        cursor += 1;
    }
    diagnostics.push(crate::error::diagnostic(
        DiagnosticCode::UnterminatedBlockComment,
        Some(Span::new(file_id, start, end)),
    ));
    (
        Comment {
            kind: CommentKind::Block,
            text,
            span: Span::new(file_id, start, end),
        },
        cursor,
    )
}
