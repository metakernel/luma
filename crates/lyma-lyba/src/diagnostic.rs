//! Structured diagnostics for LYBA parsing, validation, and policy enforcement.

use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::{Identifier, UVar};
use crate::string_table::StringTable;
use crate::symbol::SymbolTable;

/// `DIAG`
pub const DIAGNOSTIC_SECTION_NAME: &str = "DIAG";

/// No diagnostic flags are currently standardized in version 0.1.
pub const DIAGNOSTIC_FLAG_RESERVED_MASK: u64 = !0;

/// Severity associated with a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Severity {
    /// Informational note.
    #[default]
    Note,
    /// Non-fatal warning.
    Warning,
    /// Fatal error.
    Error,
}

/// Classifies whether a diagnostic comes from the wire format or local policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticClass {
    /// The encoded document is malformed or unsupported by the format.
    Format,
    /// The document is rejected due to implementation policy or configured limits.
    Policy,
}

impl Default for DiagnosticClass {
    fn default() -> Self {
        Self::Format
    }
}

/// Machine-readable diagnostic code for LYBA failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// File header magic did not match the LYBA signature.
    InvalidMagic,
    /// Encoded version is not supported by this implementation.
    UnsupportedVersion,
    /// Endian marker was not recognized.
    InvalidEndianMarker,
    /// Header size field was invalid.
    InvalidHeaderSize,
    /// Section table structure was invalid.
    InvalidSectionTable,
    /// Section spans overlapped.
    OverlappingSections,
    /// A declared offset pointed outside the file.
    OffsetOutsideFile,
    /// A required section is not supported by this implementation.
    UnsupportedRequiredSection,
    /// A required extension is not supported by this implementation.
    UnsupportedRequiredExtension,
    /// Referenced codec is not supported by this implementation.
    UnsupportedCodec,
    /// Checksum or digest verification failed.
    ChecksumMismatch,
    /// A variable-length integer used an invalid encoding.
    InvalidVarint,
    /// Byte content was not valid UTF-8 where UTF-8 was required.
    InvalidUtf8,
    /// A value reference was invalid or unresolved.
    InvalidValueReference,
    /// A syntax node reference was invalid or unresolved.
    InvalidSyntaxNodeReference,
    /// A canonical map contained the same key more than once.
    DuplicateKeyInCanonicalMap,
    /// Data was valid but not encoded canonically.
    NonCanonicalEncoding,
    /// Configured implementation limits were exceeded.
    ResourceLimitExceeded,
    /// Trusted-only content was rejected in an untrusted context.
    TrustedOnlyRejected,
    /// The document requested unsafe evaluation.
    UnsafeEvaluationRequest,
    /// Extension payload bytes were malformed.
    MalformedExtensionPayload,
    /// Source span data was invalid.
    InvalidSourceSpan,
    /// Document table structure was invalid.
    InvalidDocumentTable,
    /// Numeric value is not supported by this implementation.
    UnsupportedNumericValue,
    /// Reserved flag bits were set or invalid.
    InvalidReservedFlags,
}

impl Default for DiagnosticCode {
    fn default() -> Self {
        Self::InvalidReservedFlags
    }
}

impl DiagnosticCode {
    /// Returns the spec-defined error code string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidMagic => "LB0001",
            Self::UnsupportedVersion => "LB0002",
            Self::InvalidEndianMarker => "LB0003",
            Self::InvalidHeaderSize => "LB0004",
            Self::InvalidSectionTable => "LB0005",
            Self::OverlappingSections => "LB0006",
            Self::OffsetOutsideFile => "LB0007",
            Self::UnsupportedRequiredSection => "LB0008",
            Self::UnsupportedRequiredExtension => "LB0009",
            Self::UnsupportedCodec => "LB0010",
            Self::ChecksumMismatch => "LB0011",
            Self::InvalidVarint => "LB0012",
            Self::InvalidUtf8 => "LB0013",
            Self::InvalidValueReference => "LB0014",
            Self::InvalidSyntaxNodeReference => "LB0015",
            Self::DuplicateKeyInCanonicalMap => "LB0016",
            Self::NonCanonicalEncoding => "LB0017",
            Self::ResourceLimitExceeded => "LB0018",
            Self::TrustedOnlyRejected => "LB0019",
            Self::UnsafeEvaluationRequest => "LB0020",
            Self::MalformedExtensionPayload => "LB0021",
            Self::InvalidSourceSpan => "LB0022",
            Self::InvalidDocumentTable => "LB0023",
            Self::UnsupportedNumericValue => "LB0024",
            Self::InvalidReservedFlags => "LB0025",
        }
    }

    /// Returns the default severity for the diagnostic code.
    #[must_use]
    pub const fn severity(self) -> Severity {
        Severity::Error
    }

    /// Returns whether the code represents a format failure or a policy rejection.
    #[must_use]
    pub const fn class(self) -> DiagnosticClass {
        match self {
            Self::ResourceLimitExceeded
            | Self::TrustedOnlyRejected
            | Self::UnsafeEvaluationRequest => DiagnosticClass::Policy,
            _ => DiagnosticClass::Format,
        }
    }
}

/// Structured diagnostic metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Diagnostic {
    /// Severity of the message.
    pub severity: Severity,
    /// Machine-readable diagnostic code.
    pub code: DiagnosticCode,
    /// Whether the failure is a format or policy issue.
    pub class: DiagnosticClass,
    /// Byte offset of the failure, if known.
    pub byte_offset: Option<usize>,
    /// Section identifier associated with the failure, if known.
    pub section_id: Option<u8>,
    /// Section occurrence index associated with the failure, if known.
    pub section_index: Option<usize>,
    /// Record index associated with the failure, if known.
    pub record_index: Option<usize>,
    /// Human-readable message text.
    pub message: String,
}

impl Diagnostic {
    /// Creates a new diagnostic using defaults derived from the code.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: code.severity(),
            code,
            class: code.class(),
            byte_offset: None,
            section_id: None,
            section_index: None,
            record_index: None,
            message: message.into(),
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

/// Stored severity used by the Level 2 `DIAG` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StoredDiagnosticSeverity {
    /// Informational note.
    #[default]
    Note,
    /// Informational help or hint.
    Help,
    /// Non-fatal warning.
    Warning,
    /// Error.
    Error,
    /// Fatal error.
    Fatal,
}

impl StoredDiagnosticSeverity {
    /// Returns the wire value used by the `DIAG` section.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        match self {
            Self::Note => 0,
            Self::Help => 1,
            Self::Warning => 2,
            Self::Error => 3,
            Self::Fatal => 4,
        }
    }

    fn decode(raw: u64, record_index: usize) -> Result<Self> {
        match raw {
            0 => Ok(Self::Note),
            1 => Ok(Self::Help),
            2 => Ok(Self::Warning),
            3 => Ok(Self::Error),
            4 => Ok(Self::Fatal),
            _ => Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "DIAG record {record_index} used unknown severity value {raw}"
                ))
                .with_record_index(record_index),
            )),
        }
    }

    /// Best-effort conversion from a syntax-layer severity.
    #[must_use]
    pub const fn from_lyma_syntax(severity: lyma_syntax::Severity) -> Self {
        match severity {
            lyma_syntax::Severity::Info => Self::Note,
            lyma_syntax::Severity::Warning => Self::Warning,
            lyma_syntax::Severity::Error => Self::Error,
        }
    }

    /// Best-effort conversion into a syntax-layer severity.
    #[must_use]
    pub const fn to_lyma_syntax(self) -> lyma_syntax::Severity {
        match self {
            Self::Note | Self::Help => lyma_syntax::Severity::Info,
            Self::Warning => lyma_syntax::Severity::Warning,
            Self::Error | Self::Fatal => lyma_syntax::Severity::Error,
        }
    }

    /// Returns true when the severity is warning-or-higher.
    #[must_use]
    pub const fn is_warning_or_higher(self) -> bool {
        matches!(self, Self::Warning | Self::Error | Self::Fatal)
    }

    /// Returns true when the severity is error-or-higher.
    #[must_use]
    pub const fn is_error_or_higher(self) -> bool {
        matches!(self, Self::Error | Self::Fatal)
    }
}

/// Reader policy for stored diagnostics found in a `DIAG` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiagnosticLoadPolicy {
    /// Always accept stored diagnostics as inert data.
    #[default]
    Allow,
    /// Reject files carrying warning, error, or fatal diagnostics.
    RejectWarnings,
    /// Reject files carrying error or fatal diagnostics.
    RejectErrors,
}

impl DiagnosticLoadPolicy {
    /// Returns whether a stored severity is accepted by this policy.
    #[must_use]
    pub const fn allows(self, severity: StoredDiagnosticSeverity) -> bool {
        match self {
            Self::Allow => true,
            Self::RejectWarnings => !severity.is_warning_or_higher(),
            Self::RejectErrors => !severity.is_error_or_higher(),
        }
    }
}

/// One related stored diagnostic span.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RelatedDiagnosticRecord {
    /// Optional related span reference into `SRCS`.
    pub span_ref: Option<u64>,
    /// Human-readable related message.
    pub message: String,
}

impl RelatedDiagnosticRecord {
    /// Creates a related span annotation.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            span_ref: None,
            message: message.into(),
        }
    }

    /// Sets the optional related span reference.
    #[must_use]
    pub fn with_span_ref(mut self, span_ref: Option<u64>) -> Self {
        self.span_ref = span_ref;
        self
    }
}

/// One stored `DIAG` record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DiagnosticRecord {
    /// Stored severity.
    pub severity: StoredDiagnosticSeverity,
    /// Diagnostic code symbol text resolved through `SYMS`/`STRS`.
    pub code_symbol: Identifier,
    /// Human-readable message resolved through `STRS`.
    pub message: String,
    /// Optional primary span reference into `SRCS`.
    pub primary_span_ref: Option<u64>,
    /// Additional related spans.
    pub related_spans: Vec<RelatedDiagnosticRecord>,
    /// Stored raw diagnostic flags.
    pub flags: u64,
}

impl DiagnosticRecord {
    /// Creates a stored record.
    #[must_use]
    pub fn new(
        severity: StoredDiagnosticSeverity,
        code_symbol: impl Into<Identifier>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code_symbol: code_symbol.into(),
            message: message.into(),
            primary_span_ref: None,
            related_spans: Vec::new(),
            flags: 0,
        }
    }

    /// Sets the optional primary span reference.
    #[must_use]
    pub fn with_primary_span_ref(mut self, primary_span_ref: Option<u64>) -> Self {
        self.primary_span_ref = primary_span_ref;
        self
    }

    /// Appends a related span annotation.
    #[must_use]
    pub fn with_related_span(mut self, related: RelatedDiagnosticRecord) -> Self {
        self.related_spans.push(related);
        self
    }

    /// Replaces stored raw flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Converts a syntax-layer diagnostic, omitting spans unless the caller later adds them.
    #[must_use]
    pub fn from_lyma_syntax(diagnostic: &lyma_syntax::Diagnostic) -> Self {
        let mut record = Self::new(
            StoredDiagnosticSeverity::from_lyma_syntax(diagnostic.severity),
            diagnostic.code.code(),
            diagnostic.message.clone(),
        );
        for related in &diagnostic.related_spans {
            record
                .related_spans
                .push(RelatedDiagnosticRecord::new(related.message.clone()));
        }
        record
    }

    /// Converts a syntax-layer diagnostic using a caller-provided span encoder.
    #[must_use]
    pub fn from_lyma_syntax_with_span_encoder<F>(
        diagnostic: &lyma_syntax::Diagnostic,
        mut encode_span_ref: F,
    ) -> Self
    where
        F: FnMut(lyma_syntax::Span) -> Option<u64>,
    {
        let mut record = Self::from_lyma_syntax(diagnostic);
        record.primary_span_ref = diagnostic.primary_span.and_then(&mut encode_span_ref);
        record.related_spans = diagnostic
            .related_spans
            .iter()
            .map(|related| {
                RelatedDiagnosticRecord::new(related.message.clone())
                    .with_span_ref(encode_span_ref(related.span))
            })
            .collect();
        record
    }

    /// Best-effort conversion into a syntax-layer diagnostic without resolving stored spans.
    #[must_use]
    pub fn to_lyma_syntax_lossy(&self) -> Option<lyma_syntax::Diagnostic> {
        let code = parse_lyma_syntax_code(self.code_symbol.as_str())?;
        let mut diagnostic = lyma_syntax::Diagnostic::new(code, self.severity.to_lyma_syntax());
        diagnostic.message = self.message.clone();
        diagnostic.related_spans = Vec::new();
        Some(diagnostic)
    }

    /// Converts into a syntax-layer diagnostic using a caller-provided span resolver.
    #[must_use]
    pub fn to_lyma_syntax_with_span_resolver<F>(
        &self,
        mut resolve_span_ref: F,
    ) -> Option<lyma_syntax::Diagnostic>
    where
        F: FnMut(u64) -> Option<lyma_syntax::Span>,
    {
        let code = parse_lyma_syntax_code(self.code_symbol.as_str())?;
        let mut diagnostic = lyma_syntax::Diagnostic::new(code, self.severity.to_lyma_syntax());
        diagnostic.message = self.message.clone();
        diagnostic.primary_span = self.primary_span_ref.and_then(&mut resolve_span_ref);
        diagnostic.related_spans = self
            .related_spans
            .iter()
            .filter_map(|related| {
                Some(lyma_syntax::RelatedDiagnosticSpan {
                    span: related.span_ref.and_then(&mut resolve_span_ref)?,
                    message: related.message.clone(),
                })
            })
            .collect();
        Some(diagnostic)
    }
}

/// In-memory `DIAG` table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DiagnosticTable {
    /// Ordered stored diagnostics.
    pub records: Vec<DiagnosticRecord>,
}

impl DiagnosticTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: DiagnosticRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Returns the record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns true when no diagnostics are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

pub(crate) fn decode_diagnostic_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    span_count: usize,
) -> Result<DiagnosticTable> {
    let mut offset = 0_usize;
    let diagnostic_count =
        usize::try_from(UVar::decode(payload, &mut offset)?.0).map_err(|_| {
            LybaError::InvalidSectionTable(ErrorContext::new(
                "DIAG record count exceeded platform limits",
            ))
        })?;
    if diagnostic_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "diagnostic count exceeds configured maximum",
        ));
    }
    let mut records = Vec::with_capacity(diagnostic_count);

    for record_index in 0..diagnostic_count {
        let severity = StoredDiagnosticSeverity::decode(
            decode_uvar_with_record_context(payload, &mut offset, record_index)?,
            record_index,
        )?;
        let code_symbol_id =
            decode_symbol_id(payload, &mut offset, strings, symbols, record_index)?;
        let message_string_id =
            decode_string_id(payload, &mut offset, strings, record_index, "message")?;
        let primary_span_ref =
            decode_optional_span_ref(payload, &mut offset, span_count, record_index, "primary")?;
        let related_count_offset = offset;
        let related_count = usize::try_from(decode_uvar_with_record_context(
            payload,
            &mut offset,
            record_index,
        )?)
        .map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("DIAG related span count exceeded platform limits")
                    .with_record_index(record_index),
            )
        })?;
        if related_count > limits.max_table_record_count {
            return Err(LybaError::ResourceLimitExceeded(
                ErrorContext::new("DIAG related span count exceeds configured maximum")
                    .with_byte_offset(related_count_offset)
                    .with_record_index(record_index),
            ));
        }
        let mut related_spans = Vec::with_capacity(related_count);
        for related_index in 0..related_count {
            let span_ref = decode_optional_span_ref(
                payload,
                &mut offset,
                span_count,
                record_index,
                "related",
            )?;
            let message = decode_string_id(
                payload,
                &mut offset,
                strings,
                record_index,
                "related message",
            )?;
            let _ = related_index;
            related_spans.push(RelatedDiagnosticRecord { span_ref, message });
        }
        let flags_offset = offset;
        let flags = decode_uvar_with_record_context(payload, &mut offset, record_index)?;
        validate_diagnostic_flags(flags, record_index, Some(flags_offset))?;

        records.push(DiagnosticRecord {
            severity,
            code_symbol: code_symbol_id.into(),
            message: message_string_id,
            primary_span_ref,
            related_spans,
            flags,
        });
    }

    if offset != payload.len() {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(
            "DIAG contained trailing bytes",
        )));
    }

    Ok(DiagnosticTable { records })
}

pub(crate) fn encode_diagnostic_table(
    table: &DiagnosticTable,
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    span_count: usize,
) -> Result<Vec<u8>> {
    if table.records.len() > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "diagnostic count exceeds configured maximum",
        ));
    }

    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);

    for (record_index, record) in table.records.iter().enumerate() {
        if record.related_spans.len() > limits.max_table_record_count {
            return Err(LybaError::ResourceLimitExceeded(
                ErrorContext::new("DIAG related span count exceeds configured maximum")
                    .with_record_index(record_index),
            ));
        }
        validate_diagnostic_flags(record.flags, record_index, None)?;
        UVar(record.severity.as_u64()).encode_into(&mut bytes);
        UVar(find_symbol_id(strings, symbols, record.code_symbol.as_str(), record_index)? as u64)
            .encode_into(&mut bytes);
        UVar(find_string_id(strings, &record.message, record_index, "message")? as u64)
            .encode_into(&mut bytes);
        UVar(encode_optional_span_ref(
            record.primary_span_ref,
            span_count,
            record_index,
            "primary",
        )?)
        .encode_into(&mut bytes);
        UVar(record.related_spans.len() as u64).encode_into(&mut bytes);
        for related in &record.related_spans {
            UVar(encode_optional_span_ref(
                related.span_ref,
                span_count,
                record_index,
                "related",
            )?)
            .encode_into(&mut bytes);
            UVar(
                find_string_id(strings, &related.message, record_index, "related message")? as u64,
            )
            .encode_into(&mut bytes);
        }
        UVar(record.flags).encode_into(&mut bytes);
    }

    Ok(bytes)
}

fn decode_string_id(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    record_index: usize,
    field: &str,
) -> Result<String> {
    let string_id = usize::try_from(decode_uvar_with_record_context(
        payload,
        offset,
        record_index,
    )?)
    .map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "DIAG {field} string reference exceeded platform limits"
            ))
            .with_record_index(record_index),
        )
    })?;
    strings
        .strings
        .get(string_id)
        .map(|record| record.value.clone())
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!(
                    "DIAG {field} string reference {string_id} was out of range"
                ))
                .with_record_index(record_index),
            )
        })
}

fn decode_symbol_id(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    symbols: &SymbolTable,
    record_index: usize,
) -> Result<String> {
    let symbol_id = usize::try_from(decode_uvar_with_record_context(
        payload,
        offset,
        record_index,
    )?)
    .map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new("DIAG code symbol reference exceeded platform limits")
                .with_record_index(record_index),
        )
    })?;
    let symbol = symbols.symbols.get(symbol_id).ok_or_else(|| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "DIAG code symbol reference {symbol_id} was out of range"
            ))
            .with_record_index(record_index),
        )
    })?;
    let string_id = usize::try_from(symbol.string_id).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new("DIAG code symbol string reference exceeded platform limits")
                .with_record_index(record_index),
        )
    })?;
    strings
        .strings
        .get(string_id)
        .map(|record| record.value.clone())
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!(
                    "DIAG code symbol string reference {string_id} was out of range"
                ))
                .with_record_index(record_index),
            )
        })
}

fn decode_optional_span_ref(
    payload: &[u8],
    offset: &mut usize,
    span_count: usize,
    record_index: usize,
    field: &str,
) -> Result<Option<u64>> {
    let raw = decode_uvar_with_record_context(payload, offset, record_index)?;
    if raw == 0 {
        return Ok(None);
    }
    let span_ref = raw - 1;
    let span_index = usize::try_from(span_ref).map_err(|_| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "DIAG {field} span reference exceeded platform limits"
            ))
            .with_record_index(record_index),
        )
    })?;
    if span_index >= span_count {
        return Err(LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "DIAG {field} span reference {span_ref} was out of range for SRCS count {span_count}"
            ))
            .with_record_index(record_index),
        ));
    }
    Ok(Some(span_ref as u64))
}

fn encode_optional_span_ref(
    span_ref: Option<u64>,
    span_count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let Some(span_ref) = span_ref else {
        return Ok(0);
    };
    let span_index = usize::try_from(span_ref).map_err(|_| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "DIAG {field} span reference exceeded platform limits"
            ))
            .with_record_index(record_index),
        )
    })?;
    if span_index >= span_count {
        return Err(LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "DIAG {field} span reference {span_ref} was out of range for SRCS count {span_count}"
            ))
            .with_record_index(record_index),
        ));
    }
    span_ref.checked_add(1).ok_or_else(|| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new(format!("DIAG {field} span reference overflowed"))
                .with_record_index(record_index),
        )
    })
}

fn find_string_id(
    strings: &StringTable,
    value: &str,
    record_index: usize,
    field: &str,
) -> Result<usize> {
    strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!(
                    "DIAG {field} string '{value}' was not present in STRS"
                ))
                .with_record_index(record_index),
            )
        })
}

fn find_symbol_id(
    strings: &StringTable,
    symbols: &SymbolTable,
    code_symbol: &str,
    record_index: usize,
) -> Result<usize> {
    let string_id = find_string_id(strings, code_symbol, record_index, "code symbol")? as u64;
    symbols
        .symbols
        .iter()
        .position(|record| {
            record.string_id == string_id
                && record.namespace_string_id.is_none()
                && record.flags == 0
        })
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!(
                    "DIAG code symbol '{code_symbol}' was not present in SYMS"
                ))
                .with_record_index(record_index),
            )
        })
}

fn parse_lyma_syntax_code(code: &str) -> Option<lyma_syntax::DiagnosticCode> {
    use lyma_syntax::DiagnosticCode;

    Some(match code {
        "E0001" => DiagnosticCode::InvalidUtf8,
        "E0002" => DiagnosticCode::InvalidIndentation,
        "E0003" => DiagnosticCode::TabUsedForIndentation,
        "E0004" => DiagnosticCode::UnterminatedString,
        "E0005" => DiagnosticCode::UnterminatedBlockComment,
        "E0006" => DiagnosticCode::InvalidMappingKey,
        "E0007" => DiagnosticCode::DuplicateKey,
        "E0008" => DiagnosticCode::InvalidSequenceIndentation,
        "E0009" => DiagnosticCode::UnknownDirective,
        "E0010" => DiagnosticCode::InvalidDirectiveSyntax,
        "E0011" => DiagnosticCode::UnknownTag,
        "E0012" => DiagnosticCode::LuaSyntaxError,
        "E0013" => DiagnosticCode::LuaRuntimeError,
        "E0014" => DiagnosticCode::ImportNotFound,
        "E0015" => DiagnosticCode::ImportCycle,
        "E0016" => DiagnosticCode::IncludeTypeMismatch,
        "E0017" => DiagnosticCode::SpreadTypeMismatch,
        "E0018" => DiagnosticCode::SchemaValidationError,
        "E0019" => DiagnosticCode::UnsafeOperation,
        "E0020" => DiagnosticCode::ResourceLimitExceeded,
        "E0021" => DiagnosticCode::UnsupportedProfile,
        "E0022" => DiagnosticCode::ReservedSyntax,
        "E0023" => DiagnosticCode::InvalidNullKey,
        "E0024" => DiagnosticCode::NonDeterministicTableIteration,
        "E0025" => DiagnosticCode::FunctionValueNotAllowedInThisProfile,
        "E0026" => DiagnosticCode::InvalidBlockScalar,
        "E0027" => DiagnosticCode::InvalidExpressionKey,
        "E0028" => DiagnosticCode::InvalidLoopTarget,
        "E0029" => DiagnosticCode::InvalidTagResolverResult,
        "E0030" => DiagnosticCode::SerializationError,
        _ => return None,
    })
}

fn validate_diagnostic_flags(
    flags: u64,
    record_index: usize,
    byte_offset: Option<usize>,
) -> Result<()> {
    if flags & DIAGNOSTIC_FLAG_RESERVED_MASK != 0 {
        let context = ErrorContext::new("reserved DIAG flag bits were non-zero")
            .with_record_index(record_index);
        let context = if let Some(byte_offset) = byte_offset {
            context.with_byte_offset(byte_offset)
        } else {
            context
        };
        return Err(LybaError::InvalidReservedFlags(context));
    }

    Ok(())
}

fn decode_uvar_with_record_context(
    payload: &[u8],
    offset: &mut usize,
    record_index: usize,
) -> Result<u64> {
    UVar::decode(payload, offset)
        .map(|value| value.0)
        .map_err(|error| {
            let mut context = error.context().clone();
            context.record_index = Some(record_index);
            error.with_context(context)
        })
}
