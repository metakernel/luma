//! Editor-oriented helpers built on top of parser formatting and value serialization.

/// One-based editor range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Start byte offset.
    pub start: usize,
    /// End byte offset.
    pub end: usize,
}

/// Replace edit for a source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Replaced range.
    pub range: TextRange,
    /// Replacement text.
    pub text: String,
}

/// Formats a whole source buffer into a single replacement edit.
#[cfg(feature = "parser")]
#[must_use]
pub fn format_document_edit(name: &str, source: &str) -> luma_parser::ParsedFormatting {
    luma_parser::format_str(luma_parser::FileId(1), name, source)
}

/// Produces a whole-document canonical replace edit suitable for editors.
#[cfg(feature = "parser")]
#[must_use]
pub fn format_document_text_edit(
    name: &str,
    source: &str,
) -> (luma_parser::ParsedFormatting, TextEdit) {
    let formatted = format_document_edit(name, source);
    let edit = TextEdit {
        range: TextRange {
            start: 0,
            end: source.len(),
        },
        text: formatted.formatted.text.clone(),
    };
    (formatted, edit)
}

/// Serializes an evaluated portable value into canonical Luma text.
#[cfg(feature = "syntax")]
pub fn serialize_portable_value(
    value: &luma_syntax::LumaValue,
) -> Result<String, luma_syntax::Diagnostic> {
    luma_syntax::serialize_value(value)
}
