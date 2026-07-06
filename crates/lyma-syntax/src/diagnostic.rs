//! Diagnostics emitted by lexing, parsing, validation, or evaluation layers.

#![allow(missing_docs)]

use std::fmt;

use crate::source::Span;

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Informational message.
    Info,
    /// Warning that does not necessarily prevent progress.
    Warning,
    /// Error that prevents successful processing.
    Error,
}

/// Secondary span attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelatedDiagnosticSpan {
    /// Related source location.
    pub span: Span,
    /// Human-readable annotation.
    pub message: String,
}

/// Public diagnostic payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Diagnostic severity.
    pub severity: Severity,
    /// Primary human-readable message.
    pub message: String,
    /// Primary source location.
    pub primary_span: Option<Span>,
    /// Additional related spans.
    pub related_spans: Vec<RelatedDiagnosticSpan>,
    /// Additional notes or hints.
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Creates a new diagnostic with the recommended default message.
    #[must_use]
    pub fn new(code: DiagnosticCode, severity: Severity) -> Self {
        Self {
            code,
            severity,
            message: code.default_message().to_owned(),
            primary_span: None,
            related_spans: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Stable public diagnostic code set from spec section 36.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticCode {
    InvalidUtf8,
    InvalidIndentation,
    TabUsedForIndentation,
    UnterminatedString,
    UnterminatedBlockComment,
    InvalidMappingKey,
    DuplicateKey,
    InvalidSequenceIndentation,
    UnknownDirective,
    InvalidDirectiveSyntax,
    UnknownTag,
    LuaSyntaxError,
    LuaRuntimeError,
    ImportNotFound,
    ImportCycle,
    IncludeTypeMismatch,
    SpreadTypeMismatch,
    SchemaValidationError,
    UnsafeOperation,
    ResourceLimitExceeded,
    UnsupportedProfile,
    ReservedSyntax,
    InvalidNullKey,
    NonDeterministicTableIteration,
    FunctionValueNotAllowedInThisProfile,
    InvalidBlockScalar,
    InvalidExpressionKey,
    InvalidLoopTarget,
    InvalidTagResolverResult,
    SerializationError,
}

impl DiagnosticCode {
    /// Returns the stable textual code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidUtf8 => "E0001",
            Self::InvalidIndentation => "E0002",
            Self::TabUsedForIndentation => "E0003",
            Self::UnterminatedString => "E0004",
            Self::UnterminatedBlockComment => "E0005",
            Self::InvalidMappingKey => "E0006",
            Self::DuplicateKey => "E0007",
            Self::InvalidSequenceIndentation => "E0008",
            Self::UnknownDirective => "E0009",
            Self::InvalidDirectiveSyntax => "E0010",
            Self::UnknownTag => "E0011",
            Self::LuaSyntaxError => "E0012",
            Self::LuaRuntimeError => "E0013",
            Self::ImportNotFound => "E0014",
            Self::ImportCycle => "E0015",
            Self::IncludeTypeMismatch => "E0016",
            Self::SpreadTypeMismatch => "E0017",
            Self::SchemaValidationError => "E0018",
            Self::UnsafeOperation => "E0019",
            Self::ResourceLimitExceeded => "E0020",
            Self::UnsupportedProfile => "E0021",
            Self::ReservedSyntax => "E0022",
            Self::InvalidNullKey => "E0023",
            Self::NonDeterministicTableIteration => "E0024",
            Self::FunctionValueNotAllowedInThisProfile => "E0025",
            Self::InvalidBlockScalar => "E0026",
            Self::InvalidExpressionKey => "E0027",
            Self::InvalidLoopTarget => "E0028",
            Self::InvalidTagResolverResult => "E0029",
            Self::SerializationError => "E0030",
        }
    }

    /// Returns the recommended default message.
    #[must_use]
    pub const fn default_message(self) -> &'static str {
        match self {
            Self::InvalidUtf8 => "invalid UTF-8",
            Self::InvalidIndentation => "invalid indentation",
            Self::TabUsedForIndentation => "tab used for indentation",
            Self::UnterminatedString => "unterminated string",
            Self::UnterminatedBlockComment => "unterminated block comment",
            Self::InvalidMappingKey => "invalid mapping key",
            Self::DuplicateKey => "duplicate key",
            Self::InvalidSequenceIndentation => "invalid sequence indentation",
            Self::UnknownDirective => "unknown directive",
            Self::InvalidDirectiveSyntax => "invalid directive syntax",
            Self::UnknownTag => "unknown tag",
            Self::LuaSyntaxError => "Lua syntax error",
            Self::LuaRuntimeError => "Lua runtime error",
            Self::ImportNotFound => "import not found",
            Self::ImportCycle => "import cycle",
            Self::IncludeTypeMismatch => "include type mismatch",
            Self::SpreadTypeMismatch => "spread type mismatch",
            Self::SchemaValidationError => "schema validation error",
            Self::UnsafeOperation => "unsafe operation",
            Self::ResourceLimitExceeded => "resource limit exceeded",
            Self::UnsupportedProfile => "unsupported profile",
            Self::ReservedSyntax => "reserved syntax",
            Self::InvalidNullKey => "invalid null key",
            Self::NonDeterministicTableIteration => "non-deterministic table iteration",
            Self::FunctionValueNotAllowedInThisProfile => {
                "function value not allowed in this profile"
            }
            Self::InvalidBlockScalar => "invalid block scalar",
            Self::InvalidExpressionKey => "invalid expression key",
            Self::InvalidLoopTarget => "invalid loop target",
            Self::InvalidTagResolverResult => "invalid tag resolver result",
            Self::SerializationError => "serialization error",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
