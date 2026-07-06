//! Level-1 document table support.

use crate::error::{ErrorContext, LybaError, Result};
use crate::primitives::UVar;
use crate::value::Value;

/// Document flag: this record includes a root value reference.
pub const DOCUMENT_FLAG_HAS_VALUE_ROOT: u64 = 0x01;
/// Document flag: this record includes a schema reference.
pub const DOCUMENT_FLAG_HAS_SCHEMA: u64 = 0x20;
/// Document flag: this record includes a capability-set reference.
pub const DOCUMENT_FLAG_HAS_CAPABILITY_SET: u64 = 0x40;

/// Root-level document record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    /// Raw document flags from the DOCS record.
    pub flags: u64,
    /// Optional materialized root value.
    pub root_value: Option<Value>,
    /// Optional schema reference into `SCMA`.
    pub schema_ref: Option<u64>,
    /// Optional `CAPS` record reference.
    pub capability_set_ref: Option<u64>,
}

impl Document {
    /// Creates an empty document record.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a document with a root value.
    #[must_use]
    pub fn with_root_value(mut self, root_value: Value) -> Self {
        self.root_value = Some(root_value);
        self.flags |= DOCUMENT_FLAG_HAS_VALUE_ROOT;
        self
    }

    /// Replaces the raw flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional schema reference.
    #[must_use]
    pub fn with_schema_ref(mut self, schema_ref: Option<u64>) -> Self {
        self.schema_ref = schema_ref;
        if schema_ref.is_some() {
            self.flags |= DOCUMENT_FLAG_HAS_SCHEMA;
        } else {
            self.flags &= !DOCUMENT_FLAG_HAS_SCHEMA;
        }
        self
    }

    /// Sets an optional capability-set reference.
    #[must_use]
    pub fn with_capability_set_ref(mut self, capability_set_ref: Option<u64>) -> Self {
        self.capability_set_ref = capability_set_ref;
        if capability_set_ref.is_some() {
            self.flags |= DOCUMENT_FLAG_HAS_CAPABILITY_SET;
        } else {
            self.flags &= !DOCUMENT_FLAG_HAS_CAPABILITY_SET;
        }
        self
    }

    pub(crate) fn validate_capability_refs(&self, capability_count: usize) -> Result<()> {
        if self
            .capability_set_ref
            .is_some_and(|value| value >= capability_count as u64)
        {
            return Err(LybaError::InvalidValueReference(ErrorContext::new(
                "document capability-set reference pointed outside the CAPS table",
            )));
        }
        Ok(())
    }
}

pub(crate) fn decode_document_table(
    payload: &[u8],
    values: Option<&[Value]>,
    schema_count: usize,
    capability_count: usize,
) -> Result<Vec<Document>> {
    let mut offset = 0_usize;
    let document_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| invalid_document_table("document count exceeded platform limits"))?;
    let mut documents = Vec::with_capacity(document_count);

    for record_index in 0..document_count {
        let flags = UVar::decode(payload, &mut offset)
            .map_err(|error| {
                let context = with_record_context(error.context(), record_index);
                error.with_context(context)
            })?
            .0;
        let root_value = if flags & DOCUMENT_FLAG_HAS_VALUE_ROOT != 0 {
            let value_ref = UVar::decode(payload, &mut offset)
                .map_err(|_| {
                    invalid_document_record(
                        record_index,
                        "document root value flag was set but the value reference was missing",
                    )
                })?
                .0;
            let value_ref = usize::try_from(value_ref).map_err(|_| {
                invalid_value_reference(
                    record_index,
                    "document root value reference exceeded platform limits",
                )
            })?;
            let value = values
                .and_then(|values| values.get(value_ref))
                .cloned()
                .ok_or_else(|| {
                    invalid_value_reference(
                        record_index,
                        format!("document root value reference {value_ref} was out of range"),
                    )
                })?;
            Some(value)
        } else {
            None
        };
        let schema_ref = if flags & DOCUMENT_FLAG_HAS_SCHEMA != 0 {
            let schema_ref = UVar::decode(payload, &mut offset)
                .map_err(|_| {
                    invalid_document_record(
                        record_index,
                        "document schema flag was set but the schema reference was missing",
                    )
                })?
                .0;
            let schema_ref = usize::try_from(schema_ref).map_err(|_| {
                invalid_syntax_reference(
                    record_index,
                    "document schema reference exceeded platform limits",
                )
            })?;
            if schema_ref >= schema_count {
                return Err(invalid_syntax_reference(
                    record_index,
                    format!("document schema reference {schema_ref} was out of range"),
                ));
            }
            Some(schema_ref as u64)
        } else {
            None
        };
        let capability_set_ref = if flags & DOCUMENT_FLAG_HAS_CAPABILITY_SET != 0 {
            let capability_set_ref = UVar::decode(payload, &mut offset)
                .map_err(|_| {
                    invalid_document_record(
                        record_index,
                        "document capability-set flag was set but the capability-set reference was missing",
                    )
                })?
                .0;
            let capability_set_ref = usize::try_from(capability_set_ref).map_err(|_| {
                invalid_value_reference(
                    record_index,
                    "document capability-set reference exceeded platform limits",
                )
            })?;
            if capability_set_ref >= capability_count {
                return Err(invalid_value_reference(
                    record_index,
                    format!(
                        "document capability-set reference {capability_set_ref} was out of range"
                    ),
                ));
            }
            Some(capability_set_ref as u64)
        } else {
            None
        };
        documents.push(Document {
            flags: {
                let mut normalized_flags = flags;
                if schema_ref.is_some() {
                    normalized_flags |= DOCUMENT_FLAG_HAS_SCHEMA;
                } else {
                    normalized_flags &= !DOCUMENT_FLAG_HAS_SCHEMA;
                }
                if capability_set_ref.is_some() {
                    normalized_flags |= DOCUMENT_FLAG_HAS_CAPABILITY_SET;
                } else {
                    normalized_flags &= !DOCUMENT_FLAG_HAS_CAPABILITY_SET;
                }
                normalized_flags
            },
            root_value,
            schema_ref,
            capability_set_ref,
        });
    }

    if offset != payload.len() {
        return Err(invalid_document_table(
            "document table contained trailing bytes",
        ));
    }

    Ok(documents)
}

pub(crate) fn encode_document_table(
    documents: &[Document],
    value_roots: &[Value],
    schema_count: usize,
    capability_count: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(documents.len() as u64).encode_into(&mut bytes);

    for (record_index, document) in documents.iter().enumerate() {
        let mut flags = document.flags;
        if document.root_value.is_some() {
            flags |= DOCUMENT_FLAG_HAS_VALUE_ROOT;
        } else {
            flags &= !DOCUMENT_FLAG_HAS_VALUE_ROOT;
        }
        if document.schema_ref.is_some() {
            flags |= DOCUMENT_FLAG_HAS_SCHEMA;
        } else {
            flags &= !DOCUMENT_FLAG_HAS_SCHEMA;
        }
        if document.capability_set_ref.is_some() {
            flags |= DOCUMENT_FLAG_HAS_CAPABILITY_SET;
        } else {
            flags &= !DOCUMENT_FLAG_HAS_CAPABILITY_SET;
        }
        UVar(flags).encode_into(&mut bytes);
        if let Some(root_value) = &document.root_value {
            let value_ref = value_roots
                .iter()
                .position(|value| value == root_value)
                .ok_or_else(|| {
                    invalid_value_reference(
                        record_index,
                        "document root value was not present in the encoded VALS table",
                    )
                })?;
            UVar(value_ref as u64).encode_into(&mut bytes);
        }
        if let Some(schema_ref) = document.schema_ref {
            if schema_ref >= schema_count as u64 {
                return Err(invalid_syntax_reference(
                    record_index,
                    "document schema reference was not present in the encoded SCMA table",
                ));
            }
            UVar(schema_ref).encode_into(&mut bytes);
        }
        if let Some(capability_set_ref) = document.capability_set_ref {
            if capability_set_ref >= capability_count as u64 {
                return Err(invalid_value_reference(
                    record_index,
                    "document capability-set reference was not present in the encoded CAPS table",
                ));
            }
            UVar(capability_set_ref).encode_into(&mut bytes);
        }
    }

    Ok(bytes)
}

pub(crate) fn materialize_value_only_documents(
    values: &[Value],
    root_document_count: u64,
) -> Result<Vec<Document>> {
    let expected_count = usize::try_from(root_document_count)
        .map_err(|_| invalid_document_table("root document count exceeded platform limits"))?;
    if expected_count != values.len() {
        return Err(LybaError::InvalidDocumentTable(ErrorContext::new(
            format!(
                "header root_document_count {root_document_count} did not match materialized document count {}",
                values.len()
            ),
        )));
    }

    Ok(values
        .iter()
        .cloned()
        .map(|root_value| Document {
            flags: DOCUMENT_FLAG_HAS_VALUE_ROOT,
            root_value: Some(root_value),
            schema_ref: None,
            capability_set_ref: None,
        })
        .collect())
}

fn invalid_document_record(record_index: usize, message: impl Into<String>) -> LybaError {
    LybaError::InvalidDocumentTable(ErrorContext::new(message).with_record_index(record_index))
}

fn invalid_document_table(message: impl Into<String>) -> LybaError {
    LybaError::InvalidDocumentTable(ErrorContext::new(message))
}

fn invalid_value_reference(record_index: usize, message: impl Into<String>) -> LybaError {
    LybaError::InvalidValueReference(ErrorContext::new(message).with_record_index(record_index))
}

fn invalid_syntax_reference(record_index: usize, message: impl Into<String>) -> LybaError {
    LybaError::InvalidSyntaxNodeReference(
        ErrorContext::new(message).with_record_index(record_index),
    )
}

fn with_record_context(context: &ErrorContext, record_index: usize) -> ErrorContext {
    let mut context = context.clone();
    context.record_index = Some(record_index);
    context
}

#[cfg(test)]
mod tests {
    use super::{
        DOCUMENT_FLAG_HAS_CAPABILITY_SET, DOCUMENT_FLAG_HAS_SCHEMA, DOCUMENT_FLAG_HAS_VALUE_ROOT,
        Document, decode_document_table, encode_document_table,
    };
    use crate::error::LybaError;
    use crate::value::Value;

    #[test]
    fn docs_round_trip_with_absent_optional_root() {
        let documents = vec![
            Document::new()
                .with_root_value(Value::Int(7))
                .with_schema_ref(Some(0))
                .with_capability_set_ref(Some(0)),
            Document::new().with_flags(0x10),
        ];
        let values = vec![Value::Int(7)];

        let encoded = encode_document_table(&documents, &values, 1, 1).expect("docs should encode");
        let decoded =
            decode_document_table(&encoded, Some(&values), 1, 1).expect("docs should decode");

        assert_eq!(decoded, documents);
    }

    #[test]
    fn docs_decode_rejects_missing_root_value_ref_with_lb0023() {
        let payload = [1_u8, DOCUMENT_FLAG_HAS_VALUE_ROOT as u8];

        let error =
            decode_document_table(&payload, Some(&[]), 0, 0).expect_err("missing ref should fail");

        assert!(matches!(error, LybaError::InvalidDocumentTable(_)));
        assert_eq!(error.code().as_str(), "LB0023");
    }

    #[test]
    fn docs_decode_rejects_missing_schema_ref_with_lb0023() {
        let payload = [1_u8, DOCUMENT_FLAG_HAS_SCHEMA as u8];

        let error = decode_document_table(&payload, Some(&[]), 1, 0)
            .expect_err("missing schema ref should fail");

        assert!(matches!(error, LybaError::InvalidDocumentTable(_)));
        assert_eq!(error.code().as_str(), "LB0023");
    }

    #[test]
    fn docs_decode_rejects_missing_capability_ref_with_lb0023() {
        let payload = [1_u8, DOCUMENT_FLAG_HAS_CAPABILITY_SET as u8];

        let error = decode_document_table(&payload, Some(&[]), 0, 1)
            .expect_err("missing capability ref should fail");

        assert!(matches!(error, LybaError::InvalidDocumentTable(_)));
        assert_eq!(error.code().as_str(), "LB0023");
    }
}
