//! Editor-oriented helpers built on top of parser formatting and value serialization.

#[cfg(feature = "syntax")]
pub use luma_syntax::{TextEdit, TextRange, apply_text_edits};

#[cfg(feature = "parser")]
pub use luma_parser::{FormatRangeError, FormatRangeFallback, FormatRangeOptions};

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
        range: TextRange::new(0, source.len()),
        text: formatted.formatted.text.clone(),
    };
    (formatted, edit)
}

/// Produces canonical minimal edits suitable for editors.
#[cfg(feature = "parser")]
#[must_use]
pub fn format_document_text_edits(
    name: &str,
    source: &str,
) -> (luma_parser::ParsedFormatting, Vec<TextEdit>) {
    let formatted = format_document_edit(name, source);
    let edits = formatted.text_edits_for_source(source);
    (formatted, edits)
}

/// Produces canonical edits for a requested source range.
///
/// When canonical formatting would also require edits outside the expanded
/// range, the default behavior is to return a single whole-document replacement
/// edit. Set [`FormatRangeOptions::fallback`] to
/// [`FormatRangeFallback::Reject`] to receive a typed error instead.
#[cfg(feature = "parser")]
pub fn format_document_range_text_edits(
    name: &str,
    source: &str,
    range: TextRange,
    options: FormatRangeOptions,
) -> Result<(luma_parser::ParsedFormatting, Vec<TextEdit>), FormatRangeError> {
    luma_parser::format_range_edits(luma_parser::FileId(1), name, source, range, options)
}

/// Serializes an evaluated portable value into canonical Luma text.
///
/// This remains the stable tooling entry point for callers that already work
/// with `luma_syntax::LumaValue` directly. For typed Rust/Serde data models,
/// prefer `luma_serde` or the facade helpers under `luma::serde` to produce a
/// portable value first and then serialize it canonically.
#[cfg(feature = "syntax")]
pub fn serialize_portable_value(
    value: &luma_syntax::LumaValue,
) -> Result<String, luma_syntax::Diagnostic> {
    luma_syntax::serialize_value(value)
}
