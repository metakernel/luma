//! Evaluated document metadata extraction.

use lyma_syntax::{LymaProfile, LymaValue};

/// Metadata extracted alongside an evaluated document.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMetadata {
    /// Declared `@lyma` version, when present.
    pub version: Option<String>,
    /// Declared document profile, when present.
    pub profile: Option<LymaProfile>,
    /// Declared schema specifier, when present.
    pub schema: Option<String>,
    /// Evaluated `@meta` payload, when present.
    pub value: Option<LymaValue>,
}

impl DocumentMetadata {
    /// Creates an empty metadata payload.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            version: None,
            profile: None,
            schema: None,
            value: None,
        }
    }
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluated document root plus extracted metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedDocument {
    /// Evaluated root value.
    pub value: LymaValue,
    /// Extracted document metadata.
    pub metadata: DocumentMetadata,
}
