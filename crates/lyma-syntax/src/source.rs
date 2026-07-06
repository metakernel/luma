//! Source identity and span support.

use std::{ops::Range, sync::Arc};

/// Byte offset into a source buffer.
pub type Offset = usize;

/// Stable identifier for a source file or virtual buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FileId(pub u32);

/// Half-open byte range inside a source buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Source buffer identity.
    pub file_id: FileId,
    /// Inclusive starting byte offset.
    pub start: Offset,
    /// Exclusive ending byte offset.
    pub end: Offset,
}

impl Span {
    /// Creates a new span.
    #[must_use]
    pub const fn new(file_id: FileId, start: Offset, end: Offset) -> Self {
        Self {
            file_id,
            start,
            end,
        }
    }

    /// Returns the span length in bytes.
    #[must_use]
    pub const fn len(self) -> Offset {
        self.end.saturating_sub(self.start)
    }

    /// Returns whether the span is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// Returns whether `offset` falls within this half-open span.
    #[must_use]
    pub const fn contains_offset(self, offset: Offset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns whether `other` is fully contained within this span.
    #[must_use]
    pub fn contains_span(self, other: Self) -> bool {
        self.file_id == other.file_id && self.start <= other.start && other.end <= self.end
    }

    /// Returns whether this span overlaps `other`.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        if self.file_id != other.file_id {
            return false;
        }

        self.start < other.end && other.start < self.end
    }

    /// Returns this span as a byte range.
    #[must_use]
    pub const fn byte_range(self) -> Range<Offset> {
        self.start..self.end
    }
}

/// Value annotated with a source span.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    /// Wrapped value.
    pub value: T,
    /// Source location for the wrapped value.
    pub span: Span,
}

/// One-based line and column pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePosition {
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
}

/// Source-relative half-open byte range intended for text edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextRange {
    /// Inclusive starting byte offset.
    pub start: Offset,
    /// Exclusive ending byte offset.
    pub end: Offset,
}

impl TextRange {
    /// Creates a new text range.
    #[must_use]
    pub const fn new(start: Offset, end: Offset) -> Self {
        Self { start, end }
    }

    /// Returns the range length in bytes.
    #[must_use]
    pub const fn len(self) -> Offset {
        self.end.saturating_sub(self.start)
    }

    /// Returns whether the range is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// Returns this range as a byte range.
    #[must_use]
    pub const fn byte_range(self) -> Range<Offset> {
        self.start..self.end
    }

    /// Converts this text range into a file-aware span.
    #[must_use]
    pub const fn to_span(self, file_id: FileId) -> Span {
        Span::new(file_id, self.start, self.end)
    }

    /// Converts a file-aware span into a source-relative text range.
    #[must_use]
    pub const fn from_span(span: Span) -> Self {
        Self::new(span.start, span.end)
    }
}

/// Replace edit for a source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Replaced range.
    pub range: TextRange,
    /// Replacement text.
    pub text: String,
}

/// Applies a sorted, non-overlapping edit list to `source`.
#[must_use]
pub fn apply_text_edits(source: &str, edits: &[TextEdit]) -> Option<String> {
    if edits.is_empty() {
        return Some(source.to_owned());
    }

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;

    for edit in edits {
        let range = edit.range.byte_range();
        if range.start < cursor
            || range.start > range.end
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return None;
        }

        out.push_str(&source[cursor..range.start]);
        out.push_str(&edit.text);
        cursor = range.end;
    }

    out.push_str(&source[cursor..]);
    Some(out)
}

/// Source buffer plus a lightweight line index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LymaSource {
    /// Source identity.
    pub id: FileId,
    /// Human-readable source name.
    pub name: Arc<str>,
    /// UTF-8 source text.
    pub text: Arc<str>,
    line_starts: Arc<[Offset]>,
}

impl LymaSource {
    /// Creates a new source and builds a line index.
    #[must_use]
    pub fn new(id: FileId, name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (idx, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }

        Self {
            id,
            name: name.into(),
            text,
            line_starts: line_starts.into(),
        }
    }

    /// Returns the span for the entire source.
    #[must_use]
    pub fn full_span(&self) -> Span {
        Span::new(self.id, 0, self.text.len())
    }

    /// Clamps `offset` down to the nearest valid UTF-8 boundary at or before it.
    #[must_use]
    pub fn clamp_byte_offset(&self, offset: Offset) -> Offset {
        let clamped = offset.min(self.text.len());
        let mut boundary = clamped;

        while boundary > 0 && !self.text.is_char_boundary(boundary) {
            boundary -= 1;
        }

        boundary
    }

    /// Clamps a span to valid UTF-8 boundaries within this source.
    #[must_use]
    pub fn clamp_byte_span(&self, span: Span) -> Option<Span> {
        if span.file_id != self.id {
            return None;
        }

        let start = self.clamp_byte_offset(span.start);
        let end = self.clamp_byte_offset(span.end.max(start));
        Some(Span::new(self.id, start, end.max(start)))
    }

    /// Returns the full span of a one-based line, including its trailing newline when present.
    #[must_use]
    pub fn line_span(&self, line: usize) -> Option<Span> {
        let line_index = line.checked_sub(1)?;
        let start = *self.line_starts.get(line_index)?;
        let end = self
            .line_starts
            .get(line_index + 1)
            .copied()
            .unwrap_or(self.text.len());

        Some(Span::new(self.id, start, end))
    }

    /// Expands `span` to cover each intersecting full source line.
    #[must_use]
    pub fn expand_span_to_line_span(&self, span: Span) -> Option<Span> {
        let byte_range = self.span_to_byte_range(span)?;
        let start = self.position(byte_range.start).line;
        let end_offset = byte_range.end.saturating_sub(1);
        let end = self.position(end_offset).line;
        let start_span = self.line_span(start)?;
        let end_span = self.line_span(end)?;

        Some(Span::new(self.id, start_span.start, end_span.end))
    }

    /// Converts a source-relative byte range into a span.
    #[must_use]
    pub fn span_from_byte_range(&self, byte_range: Range<Offset>) -> Option<Span> {
        if byte_range.start > byte_range.end || byte_range.end > self.text.len() {
            return None;
        }

        Some(Span::new(self.id, byte_range.start, byte_range.end))
    }

    /// Converts a span belonging to this source into a source-relative byte range.
    #[must_use]
    pub fn span_to_byte_range(&self, span: Span) -> Option<Range<Offset>> {
        if span.file_id != self.id {
            return None;
        }

        if span.start > span.end || span.end > self.text.len() {
            return None;
        }

        Some(span.byte_range())
    }

    /// Converts an offset into a one-based line and column.
    #[must_use]
    pub fn position(&self, offset: Offset) -> SourcePosition {
        let clamped = offset.min(self.text.len());
        let line_index = self.line_starts.partition_point(|start| *start <= clamped) - 1;
        let line_start = self.line_starts[line_index];

        SourcePosition {
            line: line_index + 1,
            column: clamped.saturating_sub(line_start) + 1,
        }
    }

    /// Returns the text slice covered by `span` when it belongs to this source.
    #[must_use]
    pub fn slice(&self, span: Span) -> Option<&str> {
        if span.file_id != self.id {
            return None;
        }

        if span.start > span.end || span.end > self.text.len() {
            return None;
        }

        self.text.get(span.start..span.end)
    }
}

#[cfg(test)]
mod tests {
    use super::{FileId, LymaSource, Span, TextEdit, TextRange, apply_text_edits};

    #[test]
    fn span_containment_and_intersection_are_half_open() {
        let file_id = FileId(1);
        let span = Span::new(file_id, 2, 8);

        assert!(span.contains_offset(2));
        assert!(span.contains_offset(7));
        assert!(!span.contains_offset(8));

        assert!(span.contains_span(Span::new(file_id, 2, 8)));
        assert!(span.contains_span(Span::new(file_id, 3, 5)));
        assert!(!span.contains_span(Span::new(file_id, 1, 5)));
        assert!(!span.contains_span(Span::new(FileId(2), 3, 5)));

        assert!(span.intersects(Span::new(file_id, 0, 3)));
        assert!(span.intersects(Span::new(file_id, 7, 9)));
        assert!(!span.intersects(Span::new(file_id, 8, 10)));
        assert!(!span.intersects(Span::new(FileId(2), 3, 5)));
    }

    #[test]
    fn line_span_and_byte_range_helpers_are_source_relative() {
        let source = LymaSource::new(FileId(7), "sample", "alpha\nbeta\nγ");

        assert_eq!(source.line_span(1), Some(Span::new(FileId(7), 0, 6)));
        assert_eq!(source.line_span(2), Some(Span::new(FileId(7), 6, 11)));
        assert_eq!(source.line_span(3), Some(Span::new(FileId(7), 11, 13)));
        assert_eq!(source.line_span(4), None);

        let full = source.full_span();
        assert_eq!(source.span_to_byte_range(full), Some(0..source.text.len()));
        assert_eq!(
            source.span_from_byte_range(6..11),
            Some(Span::new(FileId(7), 6, 11))
        );
        assert_eq!(
            source.expand_span_to_line_span(Span::new(FileId(7), 2, 7)),
            Some(Span::new(FileId(7), 0, 11))
        );
    }

    #[test]
    fn clamp_byte_boundaries_handles_ascii_and_multibyte_input() {
        let ascii = LymaSource::new(FileId(1), "ascii", "hello");
        assert_eq!(ascii.clamp_byte_offset(3), 3);
        assert_eq!(ascii.clamp_byte_offset(99), 5);
        assert_eq!(
            ascii.clamp_byte_span(Span::new(FileId(1), 1, 4)),
            Some(Span::new(FileId(1), 1, 4))
        );

        let utf8 = LymaSource::new(FileId(2), "utf8", "aé🙂z");
        assert_eq!(utf8.clamp_byte_offset(0), 0);
        assert_eq!(utf8.clamp_byte_offset(2), 1);
        assert_eq!(utf8.clamp_byte_offset(6), 3);
        assert_eq!(utf8.clamp_byte_offset(99), utf8.text.len());
        assert_eq!(
            utf8.clamp_byte_span(Span::new(FileId(2), 2, 6)),
            Some(Span::new(FileId(2), 1, 3))
        );
        assert_eq!(utf8.clamp_byte_span(Span::new(FileId(3), 2, 6)), None);
    }

    #[test]
    fn text_edit_application_rewrites_source_relative_ranges() {
        let edits = vec![
            TextEdit {
                range: TextRange::new(1, 4),
                text: String::from("ello"),
            },
            TextEdit {
                range: TextRange::new(4, 4),
                text: String::from(" world"),
            },
        ];

        assert_eq!(
            apply_text_edits("hayo!", &edits),
            Some(String::from("hello world!"))
        );
    }
}

/// Duplicate-key tracking record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DuplicateKey<K> {
    /// Canonical duplicate key identity.
    pub key: K,
    /// Index of the first entry in source order.
    pub first_index: usize,
    /// Index of the duplicate entry in source order.
    pub duplicate_index: usize,
    /// Span of the first entry key.
    pub first_span: Span,
    /// Span of the duplicate entry key.
    pub duplicate_span: Span,
}
