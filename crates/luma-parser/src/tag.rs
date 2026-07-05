//! Tag parsing helpers.

use luma_syntax::{FileId, LumaTag, LumaTagName, Span};

pub(crate) fn parse_tag_prefix(
    text: &str,
    start: usize,
    file_id: FileId,
) -> Option<(LumaTag, &str)> {
    let trimmed = text.trim_start();
    let leading_ws = text.len() - trimmed.len();
    let tag_text = trimmed.strip_prefix('!')?;
    let end = tag_text
        .char_indices()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(tag_text.len());
    if end == 0 {
        return None;
    }
    let name = &tag_text[..end];
    let span_start = start + leading_ws;
    let span_end = span_start + 1 + name.len();
    Some((
        LumaTag {
            name: LumaTagName {
                value: name.to_owned(),
                span: Span::new(file_id, span_start + 1, span_end),
            },
            span: Span::new(file_id, span_start, span_end),
        },
        tag_text[end..].trim_start(),
    ))
}
