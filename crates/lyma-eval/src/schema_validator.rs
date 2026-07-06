//! Built-in schema loading and basic validation.

use lyma_runtime::LuaRuntimeEngine;
use lyma_syntax::{LymaKey, LymaMapping, LymaValue};

use crate::{
    context::{EvaluationError, ResourceContext},
    evaluator::AstEvaluator,
    imports::load_lyma_resource,
    resolver::{ResolutionContext, ResolutionKind},
};

pub(crate) fn validate_document_schema<E: LuaRuntimeEngine>(
    evaluator: &AstEvaluator<'_, E>,
    schema: &str,
    value: &LymaValue,
    resource: &ResourceContext,
    resolver_context: &mut ResolutionContext,
) -> Result<(), EvaluationError> {
    if let Some(validator) = evaluator.options.schema_validator {
        return validator
            .validate(crate::SchemaValidationRequest {
                schema,
                value,
                from: resource.locator.as_ref(),
                context: resolver_context,
            })
            .map_err(EvaluationError::from);
    }

    let resolver = evaluator.options.require_resolver("schema validation")?;
    let (locator, file, _) = load_lyma_resource(
        resolver,
        ResolutionKind::Schema,
        schema,
        resource.locator.as_ref(),
        resolver_context,
    )?;
    let child_resource = resource.child(schema.to_owned(), locator);
    let schema_document = evaluator.evaluate_schema_document(
        &file.documents[0],
        &child_resource,
        resolver_context,
    )?;
    validate_schema_value(&schema_document.value, value, schema)
}

fn validate_schema_value(
    schema: &LymaValue,
    value: &LymaValue,
    label: &str,
) -> Result<(), EvaluationError> {
    match schema {
        LymaValue::String(kind) => validate_type_name(kind, value, label),
        LymaValue::Mapping(mapping) => validate_schema_mapping(mapping, value, label),
        _ => Err(schema_error(
            label,
            "schema root must be a string or mapping",
        )),
    }
}

fn validate_schema_mapping(
    schema: &LymaMapping,
    value: &LymaValue,
    label: &str,
) -> Result<(), EvaluationError> {
    lookup_string(schema, "type").map_or_else(
        || {
            Err(schema_error(
                label,
                "schema mapping is missing a 'type' field",
            ))
        },
        |kind| match kind.as_str() {
            "object" => validate_object_schema(schema, value, label),
            "array" => validate_array_schema(schema, value, label),
            primitive => validate_type_name(primitive, value, label),
        },
    )
}

fn validate_object_schema(
    schema: &LymaMapping,
    value: &LymaValue,
    label: &str,
) -> Result<(), EvaluationError> {
    let LymaValue::Mapping(value_mapping) = value else {
        return Err(schema_error(label, "expected object value"));
    };

    if let Some(required) = lookup_mapping(schema, "required") {
        for entry in &required.entries {
            let key = string_key(&entry.key).ok_or_else(|| {
                schema_error(label, "required schema fields must use string keys")
            })?;
            let actual = lookup_value(value_mapping, key)
                .ok_or_else(|| schema_error(label, format!("missing required field '{key}'")))?;
            validate_schema_value(&entry.value, actual, label)?;
        }
    }

    if let Some(optional) = lookup_mapping(schema, "optional") {
        for entry in &optional.entries {
            let Some(key) = string_key(&entry.key) else {
                return Err(schema_error(
                    label,
                    "optional schema fields must use string keys",
                ));
            };
            if let Some(actual) = lookup_value(value_mapping, key) {
                validate_schema_value(&entry.value, actual, label)?;
            }
        }
    }

    Ok(())
}

fn validate_array_schema(
    schema: &LymaMapping,
    value: &LymaValue,
    label: &str,
) -> Result<(), EvaluationError> {
    let LymaValue::Sequence(sequence) = value else {
        return Err(schema_error(label, "expected array value"));
    };
    if let Some(items) = lookup_value(schema, "items") {
        for item in &sequence.items {
            validate_schema_value(items, item, label)?;
        }
    }
    Ok(())
}

fn validate_type_name(kind: &str, value: &LymaValue, label: &str) -> Result<(), EvaluationError> {
    let matches = match kind {
        "null" => matches!(value, LymaValue::Null(_)),
        "boolean" => matches!(value, LymaValue::Boolean(_)),
        "number" => matches!(value, LymaValue::Number(_)),
        "string" => matches!(value, LymaValue::String(_)),
        "array" => matches!(value, LymaValue::Sequence(_)),
        "object" => matches!(value, LymaValue::Mapping(_)),
        other => {
            return Err(schema_error(
                label,
                format!("unsupported schema type '{other}'"),
            ));
        }
    };
    if matches {
        Ok(())
    } else {
        Err(schema_error(
            label,
            format!("value did not satisfy schema type '{kind}'"),
        ))
    }
}

fn lookup_string(mapping: &LymaMapping, key: &str) -> Option<String> {
    lookup_value(mapping, key).and_then(|value| match value {
        LymaValue::String(value) => Some(value.clone()),
        _ => None,
    })
}

fn lookup_mapping<'a>(mapping: &'a LymaMapping, key: &str) -> Option<&'a LymaMapping> {
    lookup_value(mapping, key).and_then(|value| match value {
        LymaValue::Mapping(mapping) => Some(mapping),
        _ => None,
    })
}

fn lookup_value<'a>(mapping: &'a LymaMapping, key: &str) -> Option<&'a LymaValue> {
    mapping
        .entries
        .iter()
        .find_map(|entry| (entry.key == LymaKey::String(String::from(key))).then_some(&entry.value))
}

fn string_key(key: &LymaKey) -> Option<&str> {
    match key {
        LymaKey::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn schema_error(label: &str, message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(
        lyma_syntax::DiagnosticCode::SchemaValidationError,
        format!("schema '{label}': {}", message.into()),
    )
}
