//! Conditional and loop helpers.

use luma_syntax::{LumaKey, LumaMapping, LumaNull, LumaNumber, LumaSequence, LumaValue};

use crate::context::EvaluationError;

/// Converts a value to Lua-style truthiness.
#[must_use]
pub const fn is_truthy(value: &LumaValue) -> bool {
    !matches!(value, LumaValue::Null(_) | LumaValue::Boolean(false))
}

/// Loop item yielded for deterministic iteration.
pub struct LoopItem<'a> {
    /// Optional key binding value.
    pub key: Option<LumaValue>,
    /// Value binding.
    pub value: &'a LumaValue,
}

/// Produces deterministic loop iteration order.
///
/// # Errors
///
/// Returns an error when `iterable` is not a sequence or mapping, or when a
/// sequence index cannot be represented as a stable 1-based integer key.
pub fn iter_items(iterable: &LumaValue) -> Result<Vec<LoopItem<'_>>, EvaluationError> {
    match iterable {
        LumaValue::Sequence(LumaSequence { items, .. }) => Ok(items
            .iter()
            .enumerate()
            .map(|(index, value)| {
                Ok(LoopItem {
                    key: Some(LumaValue::Number(LumaNumber::Integer(sequence_index(
                        index,
                    )?))),
                    value,
                })
            })
            .collect::<Result<Vec<_>, EvaluationError>>()?),
        LumaValue::Mapping(LumaMapping { entries, .. }) => Ok(entries
            .iter()
            .map(|entry| LoopItem {
                key: Some(match &entry.key {
                    LumaKey::String(v) => LumaValue::String(v.clone()),
                    LumaKey::Number(v) => LumaValue::Number(v.clone()),
                    LumaKey::Boolean(v) => LumaValue::Boolean(*v),
                    LumaKey::Host(host) => LumaValue::HostObject(host.clone()),
                }),
                value: &entry.value,
            })
            .collect()),
        _ => Err(EvaluationError::new(
            luma_syntax::DiagnosticCode::InvalidLoopTarget,
            "loop target must evaluate to a sequence or mapping",
        )),
    }
}

#[allow(dead_code)]
const _: LumaNull = LumaNull;

fn sequence_index(index: usize) -> Result<i64, EvaluationError> {
    i64::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            EvaluationError::new(
                luma_syntax::DiagnosticCode::InvalidLoopTarget,
                "loop index exceeded supported integer range",
            )
        })
}
