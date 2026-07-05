//! Spread helpers.

use luma_syntax::{LumaMapping, LumaMappingEntry, LumaSequence, LumaValue};

use crate::context::EvaluationError;

/// Applies a mapping spread.
///
/// # Errors
///
/// Returns an error when `value` is not a mapping.
pub fn spread_mapping(
    into: &mut Vec<LumaMappingEntry>,
    value: LumaValue,
) -> Result<(), EvaluationError> {
    match value {
        LumaValue::Mapping(LumaMapping { entries, .. }) => {
            into.extend(entries);
            Ok(())
        }
        _ => Err(EvaluationError::new(
            luma_syntax::DiagnosticCode::SpreadTypeMismatch,
            "mapping spread requires a mapping value",
        )),
    }
}

/// Applies a sequence spread.
///
/// # Errors
///
/// Returns an error when `value` is not a sequence.
pub fn spread_sequence(into: &mut Vec<LumaValue>, value: LumaValue) -> Result<(), EvaluationError> {
    match value {
        LumaValue::Sequence(LumaSequence { items, .. }) => {
            into.extend(items);
            Ok(())
        }
        _ => Err(EvaluationError::new(
            luma_syntax::DiagnosticCode::SpreadTypeMismatch,
            "sequence spread requires a sequence value",
        )),
    }
}
