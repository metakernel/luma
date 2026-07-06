//! Block scalar and Lua block parsing helpers.

use lyma_syntax::{
    BlockChomping, BlockKind, Diagnostic, DiagnosticCode, FileId, LymaNode, Span, StringNode,
    StringStyle,
};

use crate::{
    decode::SourceText, error::diagnostic_with_message, lua_capture::block_expression,
    parser::LineInfo,
};

pub(crate) fn parse_block_node(
    source: &SourceText,
    lines: &[LineInfo],
    header_index: usize,
    parent_indent: usize,
    header_start: usize,
    header_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> (LymaNode, usize) {
    let (kind, chomping) = parse_header(header_text.trim());
    let mut index = header_index + 1;
    let mut content_indent = None;
    let mut collected = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        let raw = &source.as_str()[line.start..line.end];
        let trimmed = raw.trim_end();
        let is_blank = trimmed.trim_start().is_empty();
        if !is_blank {
            if line.indent <= parent_indent {
                break;
            }
            content_indent.get_or_insert(line.indent);
        }
        collected.push(index);
        index += 1;
    }

    let Some(content_indent) = content_indent else {
        diagnostics.push(diagnostic_with_message(
            DiagnosticCode::InvalidBlockScalar,
            Some(lines[header_index].span(source.source.id)),
            "block header must be followed by an indented content block",
        ));
        return empty_block(
            source.source.id,
            header_start,
            header_text.len(),
            kind,
            chomping,
            header_index + 1,
        );
    };

    let mut pieces = Vec::new();
    for line_index in &collected {
        let line = lines[*line_index];
        let raw = &source.as_str()[line.start..line.end];
        if raw.trim_end().trim_start().is_empty() {
            pieces.push(String::new());
        } else {
            pieces.push(
                raw.get(content_indent..)
                    .unwrap_or("")
                    .trim_end()
                    .to_owned(),
            );
        }
    }

    let mut body = match kind {
        BlockKind::Folded | BlockKind::LuaExpression => fold_lines(&pieces),
        _ => pieces.join("\n"),
    };
    apply_chomping(&mut body, chomping, !pieces.is_empty());
    let end = collected
        .last()
        .copied()
        .map_or_else(|| lines[header_index].end, |last| lines[last].end);

    let span = Span::new(source.source.id, header_start, end);
    let node = match kind {
        BlockKind::Literal | BlockKind::Folded => LymaNode::String(StringNode {
            value: body.clone(),
            source: body,
            style: StringStyle::Block,
            block_kind: Some(kind),
            chomping: Some(chomping),
            span,
        }),
        BlockKind::LuaExpression => LymaNode::LuaExpressionBlock(block_expression(
            body,
            span.start,
            span.end,
            source.source.id,
            kind,
            chomping,
        )),
        BlockKind::LuaChunk => LymaNode::LuaChunk(block_expression(
            body,
            span.start,
            span.end,
            source.source.id,
            kind,
            chomping,
        )),
    };
    (node, index)
}

fn parse_header(text: &str) -> (BlockKind, BlockChomping) {
    let chomping = if text.ends_with('-') {
        BlockChomping::Strip
    } else if text.ends_with('+') {
        BlockChomping::Keep
    } else {
        BlockChomping::Clip
    };
    let base = text.trim_end_matches(['-', '+']);
    let kind = match base {
        ">" => BlockKind::Folded,
        "|expr" => BlockKind::LuaExpression,
        "|lua" => BlockKind::LuaChunk,
        _ => BlockKind::Literal,
    };
    (kind, chomping)
}

fn fold_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            let prev_blank = lines[index - 1].is_empty();
            let next_blank = line.is_empty();
            if prev_blank || next_blank {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        out.push_str(line);
    }
    out
}

fn empty_block(
    file_id: FileId,
    start: usize,
    len: usize,
    kind: BlockKind,
    chomping: BlockChomping,
    next: usize,
) -> (LymaNode, usize) {
    let span = Span::new(file_id, start, start + len);
    let node = match kind {
        BlockKind::Literal | BlockKind::Folded => LymaNode::String(StringNode {
            value: String::new(),
            source: String::new(),
            style: StringStyle::Block,
            block_kind: Some(kind),
            chomping: Some(chomping),
            span,
        }),
        BlockKind::LuaExpression => LymaNode::LuaExpressionBlock(block_expression(
            String::new(),
            span.start,
            span.end,
            file_id,
            kind,
            chomping,
        )),
        BlockKind::LuaChunk => LymaNode::LuaChunk(block_expression(
            String::new(),
            span.start,
            span.end,
            file_id,
            kind,
            chomping,
        )),
    };
    (node, next)
}

fn apply_chomping(body: &mut String, chomping: BlockChomping, had_content: bool) {
    match chomping {
        BlockChomping::Strip => {}
        BlockChomping::Keep => {
            if had_content {
                body.push('\n');
            }
        }
        BlockChomping::Clip => {
            if had_content && !body.ends_with('\n') {
                body.push('\n');
            }
        }
    }
}

impl LineInfo {
    pub(crate) const fn span(self, file_id: FileId) -> Span {
        Span::new(file_id, self.start, self.end)
    }
}
