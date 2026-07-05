//! Source identity and span support.

use std::sync::Arc;

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

/// Source buffer plus a lightweight line index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LumaSource {
    /// Source identity.
    pub id: FileId,
    /// Human-readable source name.
    pub name: Arc<str>,
    /// UTF-8 source text.
    pub text: Arc<str>,
    line_starts: Arc<[Offset]>,
}

impl LumaSource {
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
