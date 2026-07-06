use lyma_runtime::{LuaRuntimeError, LuaRuntimePhase, RuntimeLimitKind, RuntimeLimits};
use lyma_syntax::{Diagnostic, DiagnosticCode, Severity, source::Span};

use crate::engine_name;

pub(super) fn validate_limits_for_phase(
    limits: &RuntimeLimits,
    phase: LuaRuntimePhase,
    span: Option<Span>,
) -> Result<(), LuaRuntimeError> {
    if let Some(unsupported) = unsupported_limits(limits).next() {
        return Err(unsupported_limit_error(unsupported, phase, span));
    }
    Ok(())
}

pub(super) const fn max_table_entries(limits: &RuntimeLimits) -> Option<usize> {
    limits.max_table_entries
}

fn unsupported_limits(limits: &RuntimeLimits) -> impl Iterator<Item = RuntimeLimitKind> {
    let mut kinds = Vec::new();
    if limits.max_instructions.is_some() {
        kinds.push(RuntimeLimitKind::Instructions);
    }
    if limits.max_call_depth.is_some() {
        kinds.push(RuntimeLimitKind::CallDepth);
    }
    if limits.max_memory_bytes.is_some() {
        kinds.push(RuntimeLimitKind::Memory);
    }
    if limits.max_runtime_millis.is_some() {
        kinds.push(RuntimeLimitKind::Runtime);
    }
    kinds.into_iter()
}

pub(super) fn table_entry_limit_error(span: Option<Span>) -> LuaRuntimeError {
    LuaRuntimeError::limit_exceeded(engine_name(), RuntimeLimitKind::TableEntries, span)
}

fn unsupported_limit_error(
    limit: RuntimeLimitKind,
    phase: LuaRuntimePhase,
    span: Option<Span>,
) -> LuaRuntimeError {
    let mut diagnostic = Diagnostic::new(DiagnosticCode::ResourceLimitExceeded, Severity::Error);
    diagnostic.message =
        format!("omnilua safe mode cannot enforce configured {limit:?} limit; refusing to execute");
    diagnostic.primary_span = span;
    LuaRuntimeError::new(engine_name(), phase, Box::new(diagnostic))
}
