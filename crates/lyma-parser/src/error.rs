//! Diagnostic helpers used by parser phases.

use lyma_syntax::{Diagnostic, DiagnosticCode, Severity, Span};

/// Creates an error diagnostic with the default message.
#[must_use]
pub fn diagnostic(code: DiagnosticCode, primary_span: Option<Span>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(code, Severity::Error);
    diagnostic.primary_span = primary_span;
    diagnostic
}

/// Creates an error diagnostic with an overridden message.
#[must_use]
pub fn diagnostic_with_message(
    code: DiagnosticCode,
    primary_span: Option<Span>,
    message: impl Into<String>,
) -> Diagnostic {
    let mut diagnostic = diagnostic(code, primary_span);
    diagnostic.message = message.into();
    diagnostic
}
