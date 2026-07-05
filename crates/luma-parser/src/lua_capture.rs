//! Lua expression capture helpers.

use luma_syntax::{BlockChomping, BlockKind, FileId, LuaExpression, Span};

pub(crate) fn inline_expression(text: &str, start: usize, file_id: FileId) -> LuaExpression {
    LuaExpression {
        source: text.trim().trim_start_matches('=').trim().to_owned(),
        span: Span::new(file_id, start, start + text.trim().len()),
        block_kind: None,
        chomping: None,
    }
}

pub(crate) fn inline_table(text: &str, start: usize, file_id: FileId) -> LuaExpression {
    LuaExpression {
        source: text.trim().to_owned(),
        span: Span::new(file_id, start, start + text.trim().len()),
        block_kind: None,
        chomping: None,
    }
}

#[allow(clippy::missing_const_for_fn)]
pub(crate) fn block_expression(
    source: String,
    start: usize,
    end: usize,
    file_id: FileId,
    block_kind: BlockKind,
    chomping: BlockChomping,
) -> LuaExpression {
    LuaExpression {
        source,
        span: Span::new(file_id, start, end),
        block_kind: Some(block_kind),
        chomping: Some(chomping),
    }
}
