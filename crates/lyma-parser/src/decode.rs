//! Input decoding and normalization.

use std::sync::Arc;

use lyma_syntax::{Diagnostic, DiagnosticCode, FileId, LymaSource, Span};

use crate::error::diagnostic;

/// UTF-8 normalized source text accepted by later phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    /// Indexed source buffer.
    pub source: LymaSource,
}

impl SourceText {
    /// Returns normalized source text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source.text
    }
}

/// Decode failure for byte-oriented entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// Diagnostics collected while decoding.
    pub diagnostics: Vec<Diagnostic>,
}

/// Decodes raw bytes as normalized UTF-8 Lyma source.
///
/// # Errors
///
/// Returns [`DecodeError`] when the byte slice is not valid UTF-8 or when the
/// decoded text contains an invalid NUL byte.
pub fn decode_bytes(
    file_id: FileId,
    name: impl Into<Arc<str>>,
    bytes: &[u8],
) -> Result<SourceText, DecodeError> {
    let mut diagnostics = Vec::new();
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            let span = Span::new(file_id, error.valid_up_to(), error.valid_up_to() + 1);
            diagnostics.push(diagnostic(DiagnosticCode::InvalidUtf8, Some(span)));
            return Err(DecodeError { diagnostics });
        }
    };

    decode_str(file_id, name, text).map_err(|diagnostic| DecodeError {
        diagnostics: vec![diagnostic],
    })
}

/// Normalizes already-decoded source text.
///
/// # Errors
///
/// Returns a diagnostic when the text contains an invalid NUL byte.
pub fn decode_str(
    file_id: FileId,
    name: impl Into<Arc<str>>,
    text: &str,
) -> Result<SourceText, Diagnostic> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    if let Some(offset) = text.bytes().position(|byte| byte == b'\0') {
        return Err(diagnostic(
            DiagnosticCode::InvalidUtf8,
            Some(Span::new(file_id, offset, offset + 1)),
        ));
    }

    let normalized = text.replace("\r\n", "\n");

    Ok(SourceText {
        source: LymaSource::new(file_id, name, normalized),
    })
}

#[cfg(test)]
mod tests {
    use lyma_syntax::{DiagnosticCode, FileId};

    use super::{decode_bytes, decode_str};

    #[test]
    fn removes_bom_and_normalizes_crlf() {
        let source = decode_str(FileId(1), "spec.lyma", "\u{feff}a\r\n b\r\n").unwrap();
        assert_eq!(source.as_str(), "a\n b\n");
    }

    #[test]
    fn rejects_invalid_utf8() {
        let error = decode_bytes(FileId(1), "spec.lyma", &[0xf0, 0x28, 0x8c, 0x28]).unwrap_err();
        assert_eq!(error.diagnostics[0].code, DiagnosticCode::InvalidUtf8);
        assert_eq!(error.diagnostics[0].code.code(), "E0001");
    }

    #[test]
    fn rejects_nul_bytes() {
        let error = decode_str(FileId(1), "spec.lyma", "a\0b").unwrap_err();
        assert_eq!(error.code, DiagnosticCode::InvalidUtf8);
        assert_eq!(error.code.code(), "E0001");
    }
}
