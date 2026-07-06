//! Stable deep-copy helpers for deterministic evaluator outputs.

use lyma_syntax::{
    DiagnosticCode, LymaKey, LymaMapping, LymaMappingEntry, LymaSequence, LymaTaggedValue,
    LymaValue,
};

use crate::context::EvaluationError;

/// Produces a detached stable copy of `value`.
///
/// # Errors
///
/// Returns an error when `data_only` mode encounters runtime-only values or keys.
pub fn deep_copy_value(value: &LymaValue, data_only: bool) -> Result<LymaValue, EvaluationError> {
    match value {
        LymaValue::Null(value) => Ok(LymaValue::Null(*value)),
        LymaValue::Boolean(value) => Ok(LymaValue::Boolean(*value)),
        LymaValue::Number(value) => Ok(LymaValue::Number(value.clone())),
        LymaValue::String(value) => Ok(LymaValue::String(value.clone())),
        LymaValue::Sequence(LymaSequence { items, span }) => {
            Ok(LymaValue::Sequence(LymaSequence {
                items: items
                    .iter()
                    .map(|item| deep_copy_value(item, data_only))
                    .collect::<Result<Vec<_>, _>>()?,
                span: *span,
            }))
        }
        LymaValue::Mapping(LymaMapping {
            entries,
            duplicate_keys,
            span,
        }) => Ok(LymaValue::Mapping(LymaMapping {
            entries: entries
                .iter()
                .map(|entry| {
                    Ok(LymaMappingEntry {
                        key: deep_copy_key(&entry.key, data_only)?,
                        value: deep_copy_value(&entry.value, data_only)?,
                        span: entry.span,
                    })
                })
                .collect::<Result<Vec<_>, EvaluationError>>()?,
            duplicate_keys: duplicate_keys.clone(),
            span: *span,
        })),
        LymaValue::Tagged(LymaTaggedValue { tag, value, span }) => {
            Ok(LymaValue::Tagged(LymaTaggedValue {
                tag: tag.clone(),
                value: Box::new(deep_copy_value(value, data_only)?),
                span: *span,
            }))
        }
        LymaValue::Function(value) if data_only => Err(runtime_value_error(&value.kind)),
        LymaValue::UserData(value) if data_only => Err(runtime_value_error(&value.kind)),
        LymaValue::HostObject(value) if data_only => Err(runtime_value_error(&value.kind)),
        LymaValue::Function(value) => Ok(LymaValue::Function(value.clone())),
        LymaValue::UserData(value) => Ok(LymaValue::UserData(value.clone())),
        LymaValue::HostObject(value) => Ok(LymaValue::HostObject(value.clone())),
    }
}

fn deep_copy_key(key: &LymaKey, data_only: bool) -> Result<LymaKey, EvaluationError> {
    match key {
        LymaKey::String(value) => Ok(LymaKey::String(value.clone())),
        LymaKey::Number(value) => Ok(LymaKey::Number(value.clone())),
        LymaKey::Boolean(value) => Ok(LymaKey::Boolean(*value)),
        LymaKey::Host(value) if data_only => Err(runtime_value_error(&value.kind)),
        LymaKey::Host(value) => Ok(LymaKey::Host(value.clone())),
    }
}

fn runtime_value_error(kind: &str) -> EvaluationError {
    EvaluationError::new(
        DiagnosticCode::FunctionValueNotAllowedInThisProfile,
        format!("data-only output rejected runtime value '{kind}'"),
    )
}
