//! Error types for the crate.

use crate::diagnostic::{Diagnostic, DiagnosticClass, DiagnosticCode, Severity};
use core::fmt;

/// Convenience result alias for LUMBA operations.
pub type Result<T, E = LumbaError> = core::result::Result<T, E>;

/// Shared context for a structured LUMBA error.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ErrorContext {
    /// Human-readable description of the failure.
    pub message: String,
    /// Byte offset of the failure, if known.
    pub byte_offset: Option<usize>,
    /// Section identifier associated with the failure, if known.
    pub section_id: Option<u8>,
    /// Section occurrence index associated with the failure, if known.
    pub section_index: Option<usize>,
    /// Record index associated with the failure, if known.
    pub record_index: Option<usize>,
}

impl ErrorContext {
    /// Creates a new error context with a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }

    /// Sets the byte offset.
    #[must_use]
    pub fn with_byte_offset(mut self, byte_offset: usize) -> Self {
        self.byte_offset = Some(byte_offset);
        self
    }

    /// Sets section metadata.
    #[must_use]
    pub fn with_section(mut self, section_id: u8, section_index: usize) -> Self {
        self.section_id = Some(section_id);
        self.section_index = Some(section_index);
        self
    }

    /// Sets the record index.
    #[must_use]
    pub fn with_record_index(mut self, record_index: usize) -> Self {
        self.record_index = Some(record_index);
        self
    }
}

/// Top-level error for LUMBA operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LumbaError {
    /// LB0001: file header magic did not match the LUMBA signature.
    InvalidMagic(ErrorContext),
    /// LB0002: encoded version is not supported by this implementation.
    UnsupportedVersion(ErrorContext),
    /// LB0003: endian marker was not recognized.
    InvalidEndianMarker(ErrorContext),
    /// LB0004: header size field was invalid.
    InvalidHeaderSize(ErrorContext),
    /// LB0005: section table structure was invalid.
    InvalidSectionTable(ErrorContext),
    /// LB0006: section spans overlapped.
    OverlappingSections(ErrorContext),
    /// LB0007: a declared offset pointed outside the file.
    OffsetOutsideFile(ErrorContext),
    /// LB0008: a required section is not supported by this implementation.
    UnsupportedRequiredSection(ErrorContext),
    /// LB0009: a required extension is not supported by this implementation.
    UnsupportedRequiredExtension(ErrorContext),
    /// LB0010: referenced codec is not supported by this implementation.
    UnsupportedCodec(ErrorContext),
    /// LB0011: checksum or digest verification failed.
    ChecksumMismatch(ErrorContext),
    /// LB0012: a variable-length integer used an invalid encoding.
    InvalidVarint(ErrorContext),
    /// LB0013: byte content was not valid UTF-8 where UTF-8 was required.
    InvalidUtf8(ErrorContext),
    /// LB0014: a value reference was invalid or unresolved.
    InvalidValueReference(ErrorContext),
    /// LB0015: a syntax node reference was invalid or unresolved.
    InvalidSyntaxNodeReference(ErrorContext),
    /// LB0016: a canonical map contained the same key more than once.
    DuplicateKeyInCanonicalMap(ErrorContext),
    /// LB0017: data was valid but not encoded canonically.
    NonCanonicalEncoding(ErrorContext),
    /// LB0018: configured implementation limits were exceeded.
    ResourceLimitExceeded(ErrorContext),
    /// LB0019: trusted-only content was rejected in an untrusted context.
    TrustedOnlyRejected(ErrorContext),
    /// LB0020: the document requested unsafe evaluation.
    UnsafeEvaluationRequest(ErrorContext),
    /// LB0021: extension payload bytes were malformed.
    MalformedExtensionPayload(ErrorContext),
    /// LB0022: source span data was invalid.
    InvalidSourceSpan(ErrorContext),
    /// LB0023: document table structure was invalid.
    InvalidDocumentTable(ErrorContext),
    /// LB0024: numeric value is not supported by this implementation.
    UnsupportedNumericValue(ErrorContext),
    /// LB0025: reserved flag bits were set or invalid.
    InvalidReservedFlags(ErrorContext),
    /// Native byte value is not supported by the portable `luma_syntax::LumaValue` boundary.
    UnsupportedByteValue(ErrorContext),
    /// Native decimal value is not supported by the portable `luma_syntax::LumaValue` boundary.
    UnsupportedDecimalValue(ErrorContext),
    /// Runtime value is not supported by the portable `luma_syntax::LumaValue` boundary.
    UnsupportedRuntimeValue(ErrorContext),
}

impl LumbaError {
    /// Creates an invalid-magic error.
    #[must_use]
    pub fn invalid_magic(message: impl Into<String>) -> Self {
        Self::InvalidMagic(ErrorContext::new(message))
    }

    /// Creates an unsupported-version error.
    #[must_use]
    pub fn unsupported_version(message: impl Into<String>) -> Self {
        Self::UnsupportedVersion(ErrorContext::new(message))
    }

    /// Creates an invalid-endian-marker error.
    #[must_use]
    pub fn invalid_endian_marker(message: impl Into<String>) -> Self {
        Self::InvalidEndianMarker(ErrorContext::new(message))
    }

    /// Creates an invalid-header-size error.
    #[must_use]
    pub fn invalid_header_size(message: impl Into<String>) -> Self {
        Self::InvalidHeaderSize(ErrorContext::new(message))
    }

    /// Creates an invalid-section-table error.
    #[must_use]
    pub fn invalid_section_table(message: impl Into<String>) -> Self {
        Self::InvalidSectionTable(ErrorContext::new(message))
    }

    /// Creates a checksum-mismatch error.
    #[must_use]
    pub fn checksum_mismatch(message: impl Into<String>) -> Self {
        Self::ChecksumMismatch(ErrorContext::new(message))
    }

    /// Creates an invalid-varint error.
    #[must_use]
    pub fn invalid_varint(message: impl Into<String>) -> Self {
        Self::InvalidVarint(ErrorContext::new(message))
    }

    /// Creates an offset-outside-file error.
    #[must_use]
    pub fn offset_outside_file(message: impl Into<String>) -> Self {
        Self::OffsetOutsideFile(ErrorContext::new(message))
    }

    /// Creates an invalid-utf8 error.
    #[must_use]
    pub fn invalid_utf8(message: impl Into<String>) -> Self {
        Self::InvalidUtf8(ErrorContext::new(message))
    }

    /// Creates an unsupported-codec error.
    #[must_use]
    pub fn unsupported_codec(message: impl Into<String>) -> Self {
        Self::UnsupportedCodec(ErrorContext::new(message))
    }

    /// Creates a non-canonical-encoding error.
    #[must_use]
    pub fn non_canonical_encoding(message: impl Into<String>) -> Self {
        Self::NonCanonicalEncoding(ErrorContext::new(message))
    }

    /// Creates a resource-limit-exceeded error.
    #[must_use]
    pub fn resource_limit_exceeded(message: impl Into<String>) -> Self {
        Self::ResourceLimitExceeded(ErrorContext::new(message))
    }

    /// Creates a limit-exceeded error.
    #[must_use]
    pub fn limit_exceeded(message: impl Into<String>) -> Self {
        Self::resource_limit_exceeded(message)
    }

    /// Creates a trusted-only rejection error.
    #[must_use]
    pub fn trusted_only_rejected(message: impl Into<String>) -> Self {
        Self::TrustedOnlyRejected(ErrorContext::new(message))
    }

    /// Creates an unsupported-numeric-value error.
    #[must_use]
    pub fn unsupported_numeric_value(message: impl Into<String>) -> Self {
        Self::UnsupportedNumericValue(ErrorContext::new(message))
    }

    /// Creates an invalid-reserved-flags error.
    #[must_use]
    pub fn invalid_reserved_flags(message: impl Into<String>) -> Self {
        Self::InvalidReservedFlags(ErrorContext::new(message))
    }

    /// Creates an unsupported-byte-value error.
    #[must_use]
    pub fn unsupported_byte_value(message: impl Into<String>) -> Self {
        Self::UnsupportedByteValue(ErrorContext::new(message))
    }

    /// Creates an unsupported-decimal-value error.
    #[must_use]
    pub fn unsupported_decimal_value(message: impl Into<String>) -> Self {
        Self::UnsupportedDecimalValue(ErrorContext::new(message))
    }

    /// Creates an unsupported-runtime-value error.
    #[must_use]
    pub fn unsupported_runtime_value(message: impl Into<String>) -> Self {
        Self::UnsupportedRuntimeValue(ErrorContext::new(message))
    }

    /// Creates a required-extension-unsupported error.
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::UnsupportedRequiredExtension(ErrorContext::new(message))
    }

    /// Returns the machine-readable code for the error.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::InvalidMagic(_) => DiagnosticCode::InvalidMagic,
            Self::UnsupportedVersion(_) => DiagnosticCode::UnsupportedVersion,
            Self::InvalidEndianMarker(_) => DiagnosticCode::InvalidEndianMarker,
            Self::InvalidHeaderSize(_) => DiagnosticCode::InvalidHeaderSize,
            Self::InvalidSectionTable(_) => DiagnosticCode::InvalidSectionTable,
            Self::OverlappingSections(_) => DiagnosticCode::OverlappingSections,
            Self::OffsetOutsideFile(_) => DiagnosticCode::OffsetOutsideFile,
            Self::UnsupportedRequiredSection(_) => DiagnosticCode::UnsupportedRequiredSection,
            Self::UnsupportedRequiredExtension(_) => DiagnosticCode::UnsupportedRequiredExtension,
            Self::UnsupportedCodec(_) => DiagnosticCode::UnsupportedCodec,
            Self::ChecksumMismatch(_) => DiagnosticCode::ChecksumMismatch,
            Self::InvalidVarint(_) => DiagnosticCode::InvalidVarint,
            Self::InvalidUtf8(_) => DiagnosticCode::InvalidUtf8,
            Self::InvalidValueReference(_) => DiagnosticCode::InvalidValueReference,
            Self::InvalidSyntaxNodeReference(_) => DiagnosticCode::InvalidSyntaxNodeReference,
            Self::DuplicateKeyInCanonicalMap(_) => DiagnosticCode::DuplicateKeyInCanonicalMap,
            Self::NonCanonicalEncoding(_) => DiagnosticCode::NonCanonicalEncoding,
            Self::ResourceLimitExceeded(_) => DiagnosticCode::ResourceLimitExceeded,
            Self::TrustedOnlyRejected(_) => DiagnosticCode::TrustedOnlyRejected,
            Self::UnsafeEvaluationRequest(_) => DiagnosticCode::UnsafeEvaluationRequest,
            Self::MalformedExtensionPayload(_) => DiagnosticCode::MalformedExtensionPayload,
            Self::InvalidSourceSpan(_) => DiagnosticCode::InvalidSourceSpan,
            Self::InvalidDocumentTable(_) => DiagnosticCode::InvalidDocumentTable,
            Self::UnsupportedNumericValue(_) => DiagnosticCode::UnsupportedNumericValue,
            Self::InvalidReservedFlags(_) => DiagnosticCode::InvalidReservedFlags,
            Self::UnsupportedByteValue(_) => DiagnosticCode::TrustedOnlyRejected,
            Self::UnsupportedDecimalValue(_) => DiagnosticCode::UnsupportedNumericValue,
            Self::UnsupportedRuntimeValue(_) => DiagnosticCode::TrustedOnlyRejected,
        }
    }

    /// Returns the severity derived from the error code.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.code().severity()
    }

    /// Returns whether the error is a format failure or a policy rejection.
    #[must_use]
    pub const fn class(&self) -> DiagnosticClass {
        self.code().class()
    }

    /// Returns the error context.
    #[must_use]
    pub const fn context(&self) -> &ErrorContext {
        match self {
            Self::InvalidMagic(context)
            | Self::UnsupportedVersion(context)
            | Self::InvalidEndianMarker(context)
            | Self::InvalidHeaderSize(context)
            | Self::InvalidSectionTable(context)
            | Self::OverlappingSections(context)
            | Self::OffsetOutsideFile(context)
            | Self::UnsupportedRequiredSection(context)
            | Self::UnsupportedRequiredExtension(context)
            | Self::UnsupportedCodec(context)
            | Self::ChecksumMismatch(context)
            | Self::InvalidVarint(context)
            | Self::InvalidUtf8(context)
            | Self::InvalidValueReference(context)
            | Self::InvalidSyntaxNodeReference(context)
            | Self::DuplicateKeyInCanonicalMap(context)
            | Self::NonCanonicalEncoding(context)
            | Self::ResourceLimitExceeded(context)
            | Self::TrustedOnlyRejected(context)
            | Self::UnsafeEvaluationRequest(context)
            | Self::MalformedExtensionPayload(context)
            | Self::InvalidSourceSpan(context)
            | Self::InvalidDocumentTable(context)
            | Self::UnsupportedNumericValue(context)
            | Self::InvalidReservedFlags(context)
            | Self::UnsupportedByteValue(context)
            | Self::UnsupportedDecimalValue(context)
            | Self::UnsupportedRuntimeValue(context) => context,
        }
    }

    /// Returns a copy of this error with replacement context.
    #[must_use]
    pub fn with_context(self, context: ErrorContext) -> Self {
        match self {
            Self::InvalidMagic(_) => Self::InvalidMagic(context),
            Self::UnsupportedVersion(_) => Self::UnsupportedVersion(context),
            Self::InvalidEndianMarker(_) => Self::InvalidEndianMarker(context),
            Self::InvalidHeaderSize(_) => Self::InvalidHeaderSize(context),
            Self::InvalidSectionTable(_) => Self::InvalidSectionTable(context),
            Self::OverlappingSections(_) => Self::OverlappingSections(context),
            Self::OffsetOutsideFile(_) => Self::OffsetOutsideFile(context),
            Self::UnsupportedRequiredSection(_) => Self::UnsupportedRequiredSection(context),
            Self::UnsupportedRequiredExtension(_) => Self::UnsupportedRequiredExtension(context),
            Self::UnsupportedCodec(_) => Self::UnsupportedCodec(context),
            Self::ChecksumMismatch(_) => Self::ChecksumMismatch(context),
            Self::InvalidVarint(_) => Self::InvalidVarint(context),
            Self::InvalidUtf8(_) => Self::InvalidUtf8(context),
            Self::InvalidValueReference(_) => Self::InvalidValueReference(context),
            Self::InvalidSyntaxNodeReference(_) => Self::InvalidSyntaxNodeReference(context),
            Self::DuplicateKeyInCanonicalMap(_) => Self::DuplicateKeyInCanonicalMap(context),
            Self::NonCanonicalEncoding(_) => Self::NonCanonicalEncoding(context),
            Self::ResourceLimitExceeded(_) => Self::ResourceLimitExceeded(context),
            Self::TrustedOnlyRejected(_) => Self::TrustedOnlyRejected(context),
            Self::UnsafeEvaluationRequest(_) => Self::UnsafeEvaluationRequest(context),
            Self::MalformedExtensionPayload(_) => Self::MalformedExtensionPayload(context),
            Self::InvalidSourceSpan(_) => Self::InvalidSourceSpan(context),
            Self::InvalidDocumentTable(_) => Self::InvalidDocumentTable(context),
            Self::UnsupportedNumericValue(_) => Self::UnsupportedNumericValue(context),
            Self::InvalidReservedFlags(_) => Self::InvalidReservedFlags(context),
            Self::UnsupportedByteValue(_) => Self::UnsupportedByteValue(context),
            Self::UnsupportedDecimalValue(_) => Self::UnsupportedDecimalValue(context),
            Self::UnsupportedRuntimeValue(_) => Self::UnsupportedRuntimeValue(context),
        }
    }

    /// Converts the error into a structured diagnostic.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        let context = self.context();

        Diagnostic {
            severity: self.severity(),
            code: self.code(),
            class: self.class(),
            byte_offset: context.byte_offset,
            section_id: context.section_id,
            section_index: context.section_index,
            record_index: context.record_index,
            message: context.message.clone(),
        }
    }
}

impl fmt::Display for LumbaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let context = self.context();
        write!(f, "{}: {}", self.code().as_str(), context.message)
    }
}

impl std::error::Error for LumbaError {}

#[cfg(test)]
mod tests {
    use super::{ErrorContext, LumbaError};
    use crate::diagnostic::{DiagnosticClass, Severity};

    #[test]
    fn code_strings_match_expected_spec_values() {
        let cases = [
            (LumbaError::invalid_magic("bad magic"), "LB0001"),
            (LumbaError::unsupported_version("version 9"), "LB0002"),
            (LumbaError::invalid_varint("malformed varint"), "LB0012"),
            (LumbaError::unsupported_codec("codec zstd"), "LB0010"),
            (
                LumbaError::non_canonical_encoding("non-canonical integer"),
                "LB0017",
            ),
            (
                LumbaError::limit_exceeded("document size exceeds configured maximum"),
                "LB0018",
            ),
            (
                LumbaError::trusted_only_rejected("trusted-only section present"),
                "LB0019",
            ),
            (
                LumbaError::invalid_reserved_flags("reserved flags set"),
                "LB0025",
            ),
            (
                LumbaError::unsupported_byte_value("bytes are native-only"),
                "LB0019",
            ),
            (
                LumbaError::unsupported_decimal_value("decimal values are native-only"),
                "LB0024",
            ),
            (
                LumbaError::unsupported_runtime_value("runtime values are not portable"),
                "LB0019",
            ),
        ];

        for (error, expected_code) in cases {
            assert_eq!(error.code().as_str(), expected_code);
        }
    }

    #[test]
    fn metadata_and_classification_are_preserved_in_diagnostics() {
        let error = LumbaError::invalid_varint("malformed varint").with_context(
            ErrorContext::new("malformed varint")
                .with_byte_offset(17)
                .with_section(3, 1)
                .with_record_index(5),
        );

        let diagnostic = error.to_diagnostic();

        assert_eq!(diagnostic.code.as_str(), "LB0012");
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.class, DiagnosticClass::Format);
        assert_eq!(diagnostic.byte_offset, Some(17));
        assert_eq!(diagnostic.section_id, Some(3));
        assert_eq!(diagnostic.section_index, Some(1));
        assert_eq!(diagnostic.record_index, Some(5));
    }

    #[test]
    fn policy_errors_are_classified_as_policy() {
        let error = LumbaError::trusted_only_rejected("trusted-only section present");

        let diagnostic = error.to_diagnostic();

        assert_eq!(diagnostic.code.as_str(), "LB0019");
        assert_eq!(diagnostic.class, DiagnosticClass::Policy);
        assert_eq!(diagnostic.severity, Severity::Error);
    }

    #[test]
    fn unsupported_runtime_values_are_classified_as_policy() {
        let error = LumbaError::unsupported_runtime_value("runtime values are not portable");

        let diagnostic = error.to_diagnostic();

        assert_eq!(diagnostic.code.as_str(), "LB0019");
        assert_eq!(diagnostic.class, DiagnosticClass::Policy);
    }
}
