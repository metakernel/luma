//! Spread helpers.

use lyma_syntax::{LymaMapping, LymaMappingEntry, LymaSequence, LymaValue};

use crate::context::EvaluationError;

/// Applies a mapping spread.
///
/// # Errors
///
/// Returns an error when `value` is not a mapping.
pub fn spread_mapping(
    into: &mut Vec<LymaMappingEntry>,
    value: LymaValue,
) -> Result<(), EvaluationError> {
    match value {
        LymaValue::Mapping(LymaMapping { entries, .. }) => {
            into.extend(entries);
            Ok(())
        }
        _ => Err(EvaluationError::new(
            lyma_syntax::DiagnosticCode::SpreadTypeMismatch,
            "mapping spread requires a mapping value",
        )),
    }
}

/// Applies a sequence spread.
///
/// # Errors
///
/// Returns an error when `value` is not a sequence.
pub fn spread_sequence(into: &mut Vec<LymaValue>, value: LymaValue) -> Result<(), EvaluationError> {
    match value {
        LymaValue::Sequence(LymaSequence { items, .. }) => {
            into.extend(items);
            Ok(())
        }
        _ => Err(EvaluationError::new(
            lyma_syntax::DiagnosticCode::SpreadTypeMismatch,
            "sequence spread requires a sequence value",
        )),
    }
}
