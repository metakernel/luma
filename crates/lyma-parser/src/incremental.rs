//! Incremental parsing API shell.

use std::{error::Error, fmt, sync::Arc};

use lyma_syntax::{FileId, TextEdit, TextRange, apply_text_edits};

use crate::{Parsed, parse_str};

/// One source change applied against the previous normalized document text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// Replaced range in the previous normalized source text.
    pub range: TextRange,
    /// Replacement text.
    pub text: String,
}

impl TextChange {
    /// Creates a source change.
    #[must_use]
    pub fn new(range: TextRange, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    /// Creates an insertion at `offset`.
    #[must_use]
    pub fn insert(offset: usize, text: impl Into<String>) -> Self {
        Self::new(TextRange::new(offset, offset), text)
    }

    /// Creates a deletion.
    #[must_use]
    pub fn delete(range: TextRange) -> Self {
        Self::new(range, String::new())
    }

    /// Creates a replacement.
    #[must_use]
    pub fn replace(range: TextRange, text: impl Into<String>) -> Self {
        Self::new(range, text)
    }

    #[must_use]
    fn as_text_edit(&self) -> TextEdit {
        TextEdit {
            range: self.range,
            text: self.text.clone(),
        }
    }
}

impl From<TextEdit> for TextChange {
    fn from(edit: TextEdit) -> Self {
        Self {
            range: edit.range,
            text: edit.text,
        }
    }
}

/// Batch of source changes applied as one incremental parse step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IncrementalParseInput {
    /// Changes relative to the previous normalized source text.
    pub changes: Vec<TextChange>,
}

impl IncrementalParseInput {
    /// Creates an incremental parse request.
    #[must_use]
    pub const fn new(changes: Vec<TextChange>) -> Self {
        Self { changes }
    }
}

/// Parse strategy used for the current update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalParseStrategy {
    /// Current implementation reparses the entire normalized source.
    FullReparse,
}

/// Typed failure for incremental parse application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncrementalParseError {
    /// No previous parsed document is available for change application.
    MissingBaseDocument,
    /// The requested range is out of bounds or has `start > end`.
    InvalidRange {
        /// Offending range.
        range: TextRange,
        /// Current normalized source length in bytes.
        source_len: usize,
    },
    /// The requested range is not aligned to UTF-8 character boundaries.
    NonBoundaryRange(TextRange),
    /// Changes must be sorted by range start and must not overlap.
    UnsortedOrOverlappingChanges {
        /// Earlier change range.
        previous: TextRange,
        /// Later conflicting range.
        next: TextRange,
    },
}

impl fmt::Display for IncrementalParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBaseDocument => {
                f.write_str("incremental parse requires a previously parsed document")
            }
            Self::InvalidRange { range, source_len } => write!(
                f,
                "incremental parse range {}..{} is invalid for source length {}",
                range.start, range.end, source_len
            ),
            Self::NonBoundaryRange(range) => write!(
                f,
                "incremental parse range {}..{} is not aligned to UTF-8 boundaries",
                range.start, range.end
            ),
            Self::UnsortedOrOverlappingChanges { previous, next } => write!(
                f,
                "incremental parse changes must be sorted and non-overlapping: {}..{} then {}..{}",
                previous.start, previous.end, next.start, next.end
            ),
        }
    }
}

impl Error for IncrementalParseError {}

/// Stored parsed document state owned by a [`ParseSession`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDocument {
    parsed: Parsed,
}

impl ParsedDocument {
    #[must_use]
    pub(crate) const fn new(parsed: Parsed) -> Self {
        Self { parsed }
    }

    /// Returns the parsed document.
    #[must_use]
    pub const fn parsed(&self) -> &Parsed {
        &self.parsed
    }

    /// Consumes the wrapper and returns the parsed document.
    #[must_use]
    pub fn into_parsed(self) -> Parsed {
        self.parsed
    }

    /// Returns the current normalized source text.
    #[must_use]
    pub fn source(&self) -> &str {
        self.parsed.source.as_str()
    }
}

/// Result of one session parse/update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalParseResult {
    /// Updated parsed document.
    pub document: ParsedDocument,
    /// Update strategy used by the implementation.
    pub strategy: IncrementalParseStrategy,
    /// Whether any syntax/tree state was structurally reused.
    pub reused: bool,
}

impl IncrementalParseResult {
    /// Returns the updated parsed document.
    #[must_use]
    pub const fn document(&self) -> &ParsedDocument {
        &self.document
    }

    /// Returns the updated parse result.
    #[must_use]
    pub const fn parsed(&self) -> &Parsed {
        self.document.parsed()
    }
}

/// Stateful incremental parse shell.
///
/// The current implementation validates edits against the previous normalized
/// source text, applies them, and reparses the full document. Metadata is kept
/// in the API so future versions can report token/subtree reuse without breaking
/// callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSession {
    file_id: FileId,
    name: Arc<str>,
    current: Option<ParsedDocument>,
}

impl ParseSession {
    /// Creates a new parse session for one source buffer.
    #[must_use]
    pub fn new(file_id: FileId, name: impl Into<Arc<str>>) -> Self {
        Self {
            file_id,
            name: name.into(),
            current: None,
        }
    }

    /// Returns the current parsed document, if any.
    #[must_use]
    pub const fn current(&self) -> Option<&ParsedDocument> {
        self.current.as_ref()
    }

    /// Parses initial source text and installs it as the current document.
    #[must_use]
    pub fn parse(&mut self, text: &str) -> IncrementalParseResult {
        let document = ParsedDocument::new(parse_str(self.file_id, &self.name, text));
        self.current = Some(document.clone());
        IncrementalParseResult {
            document,
            strategy: IncrementalParseStrategy::FullReparse,
            reused: false,
        }
    }

    /// Applies source changes against the previous normalized source and reparses.
    ///
    /// # Errors
    ///
    /// Returns [`IncrementalParseError`] when there is no current document, when a
    /// change range is invalid, or when changes are not sorted/non-overlapping.
    ///
    /// # Panics
    ///
    /// Panics only if applying prevalidated edits fails unexpectedly.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply(
        &mut self,
        input: IncrementalParseInput,
    ) -> Result<IncrementalParseResult, IncrementalParseError> {
        let current = self
            .current
            .as_ref()
            .ok_or(IncrementalParseError::MissingBaseDocument)?;
        let source = current.source();
        validate_changes(source, &input.changes)?;

        let edits: Vec<_> = input.changes.iter().map(TextChange::as_text_edit).collect();
        let next_source = apply_text_edits(source, &edits).expect("validated incremental edits");
        let document = ParsedDocument::new(parse_str(self.file_id, &self.name, &next_source));
        self.current = Some(document.clone());
        Ok(IncrementalParseResult {
            document,
            strategy: IncrementalParseStrategy::FullReparse,
            reused: false,
        })
    }
}

fn validate_changes(source: &str, changes: &[TextChange]) -> Result<(), IncrementalParseError> {
    let source_len = source.len();
    let mut previous = None::<TextRange>;

    for change in changes {
        let range = change.range;
        if range.start > range.end || range.end > source_len {
            return Err(IncrementalParseError::InvalidRange { range, source_len });
        }
        if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
            return Err(IncrementalParseError::NonBoundaryRange(range));
        }
        if let Some(previous_range) = previous {
            if previous_range.end > range.start {
                return Err(IncrementalParseError::UnsortedOrOverlappingChanges {
                    previous: previous_range,
                    next: range,
                });
            }
        }
        previous = Some(range);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use lyma_syntax::{DiagnosticCode, FileId, LymaNode, MappingItem, TextRange};

    use super::{
        IncrementalParseError, IncrementalParseInput, IncrementalParseStrategy, ParseSession,
        TextChange,
    };

    #[test]
    fn session_parses_initial_text_and_reports_full_reparse_metadata() {
        let mut session = ParseSession::new(FileId(1), "example.lyma");
        let result = session.parse("name: Example\r\n");

        assert_eq!(result.strategy, IncrementalParseStrategy::FullReparse);
        assert!(!result.reused);
        assert_eq!(result.parsed().source.as_str(), "name: Example\n");
        assert!(result.parsed().diagnostics.is_empty());
        assert_eq!(session.current().unwrap().source(), "name: Example\n");
    }

    #[test]
    fn session_applies_insert_replace_and_delete_changes() {
        let mut session = ParseSession::new(FileId(1), "example.lyma");
        let _ = session.parse("name: one\n");

        let inserted = session
            .apply(IncrementalParseInput::new(vec![TextChange::insert(
                "name: one\n".len(),
                "enabled: true\n",
            )]))
            .unwrap();
        assert_eq!(
            inserted.parsed().source.as_str(),
            "name: one\nenabled: true\n"
        );
        assert!(inserted.parsed().diagnostics.is_empty());

        let replaced_source = inserted.parsed().source.as_str();
        let one_start = replaced_source.find("one").unwrap();
        let replaced = session
            .apply(IncrementalParseInput::new(vec![TextChange::replace(
                TextRange::new(one_start, one_start + 3),
                "two",
            )]))
            .unwrap();
        assert_eq!(
            replaced.parsed().source.as_str(),
            "name: two\nenabled: true\n"
        );

        let deleted_source = replaced.parsed().source.as_str();
        let enabled_start = deleted_source.find("enabled").unwrap();
        let deleted = session
            .apply(IncrementalParseInput::new(vec![TextChange::delete(
                TextRange::new(enabled_start, deleted_source.len()),
            )]))
            .unwrap();
        assert_eq!(deleted.parsed().source.as_str(), "name: two\n");
        assert!(deleted.parsed().diagnostics.is_empty());

        let document = &deleted.parsed().file.documents[0];
        let lyma_syntax::DocumentItem::Root(LymaNode::Mapping(mapping)) = &document.items[0] else {
            panic!();
        };
        assert_eq!(mapping.items.len(), 1);
    }

    #[test]
    fn session_preserves_diagnostic_and_span_updates_after_incremental_change() {
        let mut session = ParseSession::new(FileId(1), "example.lyma");
        let _ = session.parse("name: one\n");

        let result = session
            .apply(IncrementalParseInput::new(vec![TextChange::insert(
                "name: one\n".len(),
                "name: two\n",
            )]))
            .unwrap();

        assert_eq!(result.parsed().diagnostics.len(), 1);
        assert_eq!(
            result.parsed().diagnostics[0].code,
            DiagnosticCode::DuplicateKey
        );

        let source = result.parsed().source.as_str();
        let second_key_start = source.rfind("name").unwrap();
        let document = &result.parsed().file.documents[0];
        let lyma_syntax::DocumentItem::Root(LymaNode::Mapping(mapping)) = &document.items[0] else {
            panic!();
        };
        let MappingItem::Entry(entry) = &mapping.items[1] else {
            panic!();
        };
        assert_eq!(entry.span.start, second_key_start);
        let lyma_syntax::MappingKey::Plain { span, .. } = &entry.key else {
            panic!();
        };
        assert_eq!(span.start, second_key_start);
        assert_eq!(span.end, second_key_start + "name".len());
    }

    #[test]
    fn session_rejects_invalid_and_non_boundary_ranges() {
        let mut session = ParseSession::new(FileId(1), "example.lyma");
        let _ = session.parse("title: café\n");

        let invalid = session
            .apply(IncrementalParseInput::new(vec![TextChange::delete(
                TextRange::new(0, 100),
            )]))
            .unwrap_err();
        assert_eq!(
            invalid,
            IncrementalParseError::InvalidRange {
                range: TextRange::new(0, 100),
                source_len: "title: café\n".len(),
            }
        );

        let accent = "title: caf".len();
        let boundary = session
            .apply(IncrementalParseInput::new(vec![TextChange::delete(
                TextRange::new(accent, accent + 1),
            )]))
            .unwrap_err();
        assert_eq!(
            boundary,
            IncrementalParseError::NonBoundaryRange(TextRange::new(accent, accent + 1))
        );
    }
}
