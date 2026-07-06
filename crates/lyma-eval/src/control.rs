//! Conditional and loop helpers.

use lyma_syntax::{LymaKey, LymaMapping, LymaNull, LymaNumber, LymaSequence, LymaValue};

use crate::context::EvaluationError;

/// Converts a value to Lua-style truthiness.
#[must_use]
pub const fn is_truthy(value: &LymaValue) -> bool {
    !matches!(value, LymaValue::Null(_) | LymaValue::Boolean(false))
}

/// Loop item yielded for deterministic iteration.
pub struct LoopItem<'a> {
    /// Optional key binding value.
    pub key: Option<LymaValue>,
    /// Value binding.
    pub value: &'a LymaValue,
}

/// Produces deterministic loop iteration order.
///
/// # Errors
///
/// Returns an error when `iterable` is not a sequence or mapping, or when a
/// sequence index cannot be represented as a stable 1-based integer key.
pub fn iter_items(iterable: &LymaValue) -> Result<Vec<LoopItem<'_>>, EvaluationError> {
    match iterable {
        LymaValue::Sequence(LymaSequence { items, .. }) => Ok(items
            .iter()
            .enumerate()
            .map(|(index, value)| {
                Ok(LoopItem {
                    key: Some(LymaValue::Number(LymaNumber::Integer(sequence_index(
                        index,
                    )?))),
                    value,
                })
            })
            .collect::<Result<Vec<_>, EvaluationError>>()?),
        LymaValue::Mapping(LymaMapping { entries, .. }) => Ok(entries
            .iter()
            .map(|entry| LoopItem {
                key: Some(match &entry.key {
                    LymaKey::String(v) => LymaValue::String(v.clone()),
                    LymaKey::Number(v) => LymaValue::Number(v.clone()),
                    LymaKey::Boolean(v) => LymaValue::Boolean(*v),
                    LymaKey::Host(host) => LymaValue::HostObject(host.clone()),
                }),
                value: &entry.value,
            })
            .collect()),
        _ => Err(EvaluationError::new(
            lyma_syntax::DiagnosticCode::InvalidLoopTarget,
            "loop target must evaluate to a sequence or mapping",
        )),
    }
}

#[allow(dead_code)]
const _: LymaNull = LymaNull;

fn sequence_index(index: usize) -> Result<i64, EvaluationError> {
    i64::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            EvaluationError::new(
                lyma_syntax::DiagnosticCode::InvalidLoopTarget,
                "loop index exceeded supported integer range",
            )
        })
}
