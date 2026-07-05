//! Stable deep-copy helpers for deterministic evaluator outputs.

use luma_syntax::{
    DiagnosticCode, LumaKey, LumaMapping, LumaMappingEntry, LumaSequence, LumaTaggedValue,
    LumaValue,
};

use crate::context::EvaluationError;

/// Produces a detached stable copy of `value`.
///
/// # Errors
///
/// Returns an error when `data_only` mode encounters runtime-only values or keys.
pub fn deep_copy_value(value: &LumaValue, data_only: bool) -> Result<LumaValue, EvaluationError> {
    match value {
        LumaValue::Null(value) => Ok(LumaValue::Null(*value)),
        LumaValue::Boolean(value) => Ok(LumaValue::Boolean(*value)),
        LumaValue::Number(value) => Ok(LumaValue::Number(value.clone())),
        LumaValue::String(value) => Ok(LumaValue::String(value.clone())),
        LumaValue::Sequence(LumaSequence { items, span }) => {
            Ok(LumaValue::Sequence(LumaSequence {
                items: items
                    .iter()
                    .map(|item| deep_copy_value(item, data_only))
                    .collect::<Result<Vec<_>, _>>()?,
                span: *span,
            }))
        }
        LumaValue::Mapping(LumaMapping {
            entries,
            duplicate_keys,
            span,
        }) => Ok(LumaValue::Mapping(LumaMapping {
            entries: entries
                .iter()
                .map(|entry| {
                    Ok(LumaMappingEntry {
                        key: deep_copy_key(&entry.key, data_only)?,
                        value: deep_copy_value(&entry.value, data_only)?,
                        span: entry.span,
                    })
                })
                .collect::<Result<Vec<_>, EvaluationError>>()?,
            duplicate_keys: duplicate_keys.clone(),
            span: *span,
        })),
        LumaValue::Tagged(LumaTaggedValue { tag, value, span }) => {
            Ok(LumaValue::Tagged(LumaTaggedValue {
                tag: tag.clone(),
                value: Box::new(deep_copy_value(value, data_only)?),
                span: *span,
            }))
        }
        LumaValue::Function(value) if data_only => Err(runtime_value_error(&value.kind)),
        LumaValue::UserData(value) if data_only => Err(runtime_value_error(&value.kind)),
        LumaValue::HostObject(value) if data_only => Err(runtime_value_error(&value.kind)),
        LumaValue::Function(value) => Ok(LumaValue::Function(value.clone())),
        LumaValue::UserData(value) => Ok(LumaValue::UserData(value.clone())),
        LumaValue::HostObject(value) => Ok(LumaValue::HostObject(value.clone())),
    }
}

fn deep_copy_key(key: &LumaKey, data_only: bool) -> Result<LumaKey, EvaluationError> {
    match key {
        LumaKey::String(value) => Ok(LumaKey::String(value.clone())),
        LumaKey::Number(value) => Ok(LumaKey::Number(value.clone())),
        LumaKey::Boolean(value) => Ok(LumaKey::Boolean(*value)),
        LumaKey::Host(value) if data_only => Err(runtime_value_error(&value.kind)),
        LumaKey::Host(value) => Ok(LumaKey::Host(value.clone())),
    }
}

fn runtime_value_error(kind: &str) -> EvaluationError {
    EvaluationError::new(
        DiagnosticCode::FunctionValueNotAllowedInThisProfile,
        format!("data-only output rejected runtime value '{kind}'"),
    )
}
