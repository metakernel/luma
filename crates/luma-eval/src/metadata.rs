//! Evaluated document metadata extraction.

use luma_syntax::{LumaProfile, LumaValue};

/// Metadata extracted alongside an evaluated document.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMetadata {
    /// Declared `@luma` version, when present.
    pub version: Option<String>,
    /// Declared document profile, when present.
    pub profile: Option<LumaProfile>,
    /// Declared schema specifier, when present.
    pub schema: Option<String>,
    /// Evaluated `@meta` payload, when present.
    pub value: Option<LumaValue>,
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
    pub value: LumaValue,
    /// Extracted document metadata.
    pub metadata: DocumentMetadata,
}
