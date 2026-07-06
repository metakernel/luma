//! Value model types and Level 1 `VALS` helpers.

use crate::{
    blob::BlobId,
    error::{ErrorContext, LybaError, Result},
    policy::Limits,
    primitives::{Identifier, SVar, UVar, read_bounded_bytes, read_u64_le, write_u64_le},
    write::{CanonicalMode, WriterMode},
};
use lyma_syntax::{
    FileId, LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber,
    LymaSequence, LymaTag, LymaTagName, LymaTaggedValue, LymaValue, source::Span,
};
use std::collections::BTreeSet;

pub(crate) const VALUE_SECTION_NAME: &str = "VALS";

const RECORD_NULL: u64 = 0;
const RECORD_BOOL_FALSE: u64 = 1;
const RECORD_BOOL_TRUE: u64 = 2;
const RECORD_INT: u64 = 3;
const RECORD_UINT: u64 = 4;
const RECORD_FLOAT64: u64 = 5;
const RECORD_STRING: u64 = 6;
const RECORD_SEQUENCE: u64 = 7;
const RECORD_MAP: u64 = 8;
const RECORD_TAGGED: u64 = 9;
const RECORD_BYTES_INLINE: u64 = 10;
const RECORD_BYTES_BLOB: u64 = 11;
const RECORD_EXPRESSION_SOURCE: u64 = 12;
const RECORD_LUA_CHUNK_SOURCE: u64 = 13;
const RECORD_RUNTIME_DESCRIPTOR: u64 = 14;
const RECORD_EXTENSION_VALUE: u64 = 15;

/// Finite floating-point value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FiniteFloat(f64);

impl FiniteFloat {
    /// Creates a finite float, rejecting NaN and infinity.
    pub fn new(value: f64) -> Result<Self> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(LybaError::unsupported_numeric_value(format!(
                "non-finite float {value:?} is not supported in portable values",
            )))
        }
    }

    /// Returns the underlying floating-point value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Arbitrary-precision decimal text preserved as written for the native model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecimalString(String);

impl DecimalString {
    /// Creates a decimal string wrapper.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the decimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the decimal text.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for DecimalString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for DecimalString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Ordered map entry in the native value model.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    /// Entry key.
    pub key: Value,
    /// Entry value.
    pub value: Value,
}

/// Tagged value in the native model.
#[derive(Debug, Clone, PartialEq)]
pub struct TaggedValue {
    /// Logical tag identifier.
    pub tag: Identifier,
    /// Tagged payload.
    pub value: Box<Value>,
}

/// Inert expression source payload.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionSource {
    /// Inline UTF-8 source text.
    Text(String),
    /// Source text stored in the blob table.
    Blob(BlobId),
}

/// Inert expression source value.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionValue {
    /// Expression language identifier.
    pub language: Identifier,
    /// Source text or blob reference.
    pub source: ExpressionSource,
    /// Optional `CAPS` record reference.
    pub capability_set_ref: Option<u64>,
    /// Optional already-materialized inert result value.
    pub result_value: Option<Box<Value>>,
}

/// Inert Lua chunk source value.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaChunkValue {
    /// Chunk language or dialect identifier.
    pub language: Identifier,
    /// Source blob reference.
    pub source_blob_ref: BlobId,
    /// Optional `CAPS` record reference.
    pub capability_set_ref: Option<u64>,
    /// Optional already-materialized inert result value.
    pub result_value: Option<Box<Value>>,
}

/// Inert runtime descriptor value.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDescriptorValue {
    /// Stable descriptor kind identifier.
    pub kind: Identifier,
    /// Whether host resolution is required to materialize a runtime value.
    pub required: bool,
    /// Whether host resolution is allowed only under trusted policy.
    pub trusted_only: bool,
    /// Optional `CAPS` record reference.
    pub capability_set_ref: Option<u64>,
    /// Optional inert descriptor payload.
    pub descriptor_value: Option<Box<Value>>,
    /// Optional portable fallback value.
    pub fallback_value: Option<Box<Value>>,
}

/// Inert extension-backed value.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionValue {
    /// Declared extension name.
    pub extension_name: String,
    /// Producer-defined extension value kind.
    pub type_name: Identifier,
    /// Extension payload blob reference.
    pub payload_blob_ref: BlobId,
    /// Optional portable fallback value.
    pub fallback_value: Option<Box<Value>>,
}

/// Heterogeneous value stored inside a section.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Signed integer value.
    Int(i64),
    /// Unsigned integer value.
    UInt(u64),
    /// Finite floating-point value.
    Float(FiniteFloat),
    /// Arbitrary-precision decimal string.
    Decimal(DecimalString),
    /// UTF-8 string value.
    String(String),
    /// Small inline byte payload.
    BytesInline(Vec<u8>),
    /// Byte payload stored in the blob table.
    BytesBlob(BlobId),
    /// Nested sequence value.
    Sequence(Vec<Value>),
    /// Ordered mapping value.
    Map(Vec<MapEntry>),
    /// Tagged value.
    Tagged(TaggedValue),
    /// Inert expression source.
    ExpressionSource(ExpressionValue),
    /// Inert Lua chunk source.
    LuaChunkSource(LuaChunkValue),
    /// Inert runtime descriptor.
    RuntimeDescriptor(RuntimeDescriptorValue),
    /// Inert extension-backed value.
    ExtensionValue(ExtensionValue),
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueDecodeMode {
    Portable,
    Canonical,
}

#[derive(Debug, Clone)]
enum RawValueRecord {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    BytesInline(Vec<u8>),
    BytesBlob(BlobId),
    Sequence(Vec<usize>),
    Map(Vec<(usize, usize)>),
    Tagged {
        tag: Identifier,
        value_ref: usize,
    },
    ExpressionSource {
        language: Identifier,
        source: ExpressionSource,
        capability_set_ref: Option<u64>,
        result_value_ref: Option<usize>,
    },
    LuaChunkSource {
        language: Identifier,
        source_blob_ref: BlobId,
        capability_set_ref: Option<u64>,
        result_value_ref: Option<usize>,
    },
    RuntimeDescriptor {
        kind: Identifier,
        required: bool,
        trusted_only: bool,
        capability_set_ref: Option<u64>,
        descriptor_value_ref: Option<usize>,
        fallback_value_ref: Option<usize>,
    },
    ExtensionValue {
        extension_name: String,
        type_name: Identifier,
        payload_blob_ref: BlobId,
        fallback_value_ref: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

impl Default for Value {
    fn default() -> Self {
        Self::Null
    }
}

impl Value {
    pub(crate) fn validate_capability_refs(&self, capability_count: usize) -> Result<()> {
        match self {
            Self::Sequence(items) => {
                for item in items {
                    item.validate_capability_refs(capability_count)?;
                }
            }
            Self::Map(entries) => {
                for entry in entries {
                    entry.key.validate_capability_refs(capability_count)?;
                    entry.value.validate_capability_refs(capability_count)?;
                }
            }
            Self::Tagged(tagged) => tagged.value.validate_capability_refs(capability_count)?,
            Self::ExpressionSource(expression) => {
                validate_capability_ref(expression.capability_set_ref, capability_count)?;
                if let Some(value) = &expression.result_value {
                    value.validate_capability_refs(capability_count)?;
                }
            }
            Self::LuaChunkSource(chunk) => {
                validate_capability_ref(chunk.capability_set_ref, capability_count)?;
                if let Some(value) = &chunk.result_value {
                    value.validate_capability_refs(capability_count)?;
                }
            }
            Self::RuntimeDescriptor(descriptor) => {
                validate_capability_ref(descriptor.capability_set_ref, capability_count)?;
                if let Some(value) = &descriptor.descriptor_value {
                    value.validate_capability_refs(capability_count)?;
                }
                if let Some(value) = &descriptor.fallback_value {
                    value.validate_capability_refs(capability_count)?;
                }
            }
            Self::ExtensionValue(extension) => {
                if let Some(value) = &extension.fallback_value {
                    value.validate_capability_refs(capability_count)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn validate_capability_ref(capability_set_ref: Option<u64>, capability_count: usize) -> Result<()> {
    if capability_set_ref.is_some_and(|value| value >= capability_count as u64) {
        return Err(LybaError::InvalidValueReference(ErrorContext::new(
            "capability-set reference pointed outside the CAPS table",
        )));
    }
    Ok(())
}

impl TryFrom<&LymaValue> for Value {
    type Error = LybaError;

    fn try_from(value: &LymaValue) -> Result<Self> {
        match value {
            LymaValue::Null(_) => Ok(Self::Null),
            LymaValue::Boolean(value) => Ok(Self::Bool(*value)),
            LymaValue::Number(LymaNumber::Integer(value)) => Ok(Self::Int(*value)),
            LymaValue::Number(LymaNumber::Float(value)) => {
                FiniteFloat::new(*value).map(Self::Float)
            }
            LymaValue::String(value) => Ok(Self::String(value.clone())),
            LymaValue::Sequence(sequence) => sequence
                .items
                .iter()
                .map(Self::try_from)
                .collect::<Result<Vec<_>>>()
                .map(Self::Sequence),
            LymaValue::Mapping(mapping) => mapping
                .entries
                .iter()
                .map(|entry| {
                    Ok(MapEntry {
                        key: native_key(&entry.key)?,
                        value: Self::try_from(&entry.value)?,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .map(Self::Map),
            LymaValue::Tagged(tagged) => Ok(Self::Tagged(TaggedValue {
                tag: Identifier::new(tagged.tag.name.value.clone()),
                value: Box::new(Self::try_from(tagged.value.as_ref())?),
            })),
            LymaValue::Function(host) => Err(unsupported_runtime_value("function", host)),
            LymaValue::UserData(host) => Err(unsupported_runtime_value("userdata", host)),
            LymaValue::HostObject(host) => Err(unsupported_runtime_value("host object", host)),
        }
    }
}

impl TryFrom<LymaValue> for Value {
    type Error = LybaError;

    fn try_from(value: LymaValue) -> Result<Self> {
        Self::try_from(&value)
    }
}

impl TryFrom<&Value> for LymaValue {
    type Error = LybaError;

    fn try_from(value: &Value) -> Result<Self> {
        match value {
            Value::Null => Ok(Self::Null(LymaNull)),
            Value::Bool(value) => Ok(Self::Boolean(*value)),
            Value::Int(value) => Ok(Self::Number(LymaNumber::Integer(*value))),
            Value::UInt(value) => i64::try_from(*value)
                .map(|value| Self::Number(LymaNumber::Integer(value)))
                .map_err(|_| {
                    LybaError::unsupported_numeric_value(format!(
                        "unsigned integer {value} exceeds lyma_syntax::LymaValue integer range",
                    ))
                }),
            Value::Float(value) => Ok(Self::Number(LymaNumber::Float(value.get()))),
            Value::Decimal(value) => Err(LybaError::unsupported_decimal_value(format!(
                "decimal value {:?} has no lossless lyma_syntax::LymaValue representation",
                value.as_str(),
            ))),
            Value::String(value) => Ok(Self::String(value.clone())),
            Value::BytesInline(bytes) => Err(LybaError::unsupported_byte_value(format!(
                "inline byte value of {} bytes has no lossless lyma_syntax::LymaValue representation",
                bytes.len(),
            ))),
            Value::BytesBlob(blob_id) => Err(LybaError::unsupported_byte_value(format!(
                "blob byte value {:?} has no lossless lyma_syntax::LymaValue representation",
                blob_id,
            ))),
            Value::Sequence(items) => items
                .iter()
                .map(Self::try_from)
                .collect::<Result<Vec<_>>>()
                .map(|items| Self::Sequence(LymaSequence { items, span: None })),
            Value::Map(entries) => entries
                .iter()
                .map(|entry| {
                    Ok(LymaMappingEntry {
                        key: syntax_key(&entry.key)?,
                        value: Self::try_from(&entry.value)?,
                        span: None,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .map(|entries| {
                    Self::Mapping(LymaMapping {
                        entries,
                        duplicate_keys: Vec::new(),
                        span: None,
                    })
                }),
            Value::Tagged(tagged) => Ok(Self::Tagged(LymaTaggedValue {
                tag: LymaTag {
                    name: LymaTagName {
                        value: tagged.tag.as_str().to_owned(),
                        span: empty_span(),
                    },
                    span: empty_span(),
                },
                value: Box::new(Self::try_from(tagged.value.as_ref())?),
                span: None,
            })),
            Value::ExpressionSource(expression) => {
                Err(LybaError::unsupported_runtime_value(format!(
                    "expression source value {} is inert evaluation metadata, not a portable lyma_syntax::LymaValue",
                    expression.language.as_str()
                )))
            }
            Value::LuaChunkSource(chunk) => Err(LybaError::unsupported_runtime_value(format!(
                "chunk source value {} is inert evaluation metadata, not a portable lyma_syntax::LymaValue",
                chunk.language.as_str()
            ))),
            Value::RuntimeDescriptor(descriptor) => {
                Err(LybaError::unsupported_runtime_value(format!(
                    "runtime descriptor {} is inert host-resolution metadata, not a portable lyma_syntax::LymaValue",
                    descriptor.kind.as_str()
                )))
            }
            Value::ExtensionValue(extension) => {
                Err(LybaError::unsupported_runtime_value(format!(
                    "extension value {}:{} is inert extension metadata, not a portable lyma_syntax::LymaValue",
                    extension.extension_name,
                    extension.type_name.as_str()
                )))
            }
        }
    }
}

const fn empty_span() -> Span {
    Span::new(FileId(0), 0, 0)
}

impl TryFrom<Value> for LymaValue {
    type Error = LybaError;

    fn try_from(value: Value) -> Result<Self> {
        Self::try_from(&value)
    }
}

pub(crate) fn decode_value_table(
    payload: &[u8],
    limits: &Limits,
    mode: ValueDecodeMode,
    blob_count: usize,
) -> Result<Vec<Value>> {
    let mut offset = 0_usize;
    let value_count = UVar::decode(payload, &mut offset)?.0;
    let value_count = usize::try_from(value_count)
        .map_err(|_| LybaError::limit_exceeded("value count exceeds configured maximum"))?;
    if value_count > limits.max_value_count {
        return Err(LybaError::limit_exceeded(
            "value count exceeds configured maximum",
        ));
    }

    let offset_table_bytes = value_count.checked_mul(8).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new("value offset table length overflowed"))
    })?;
    let records_start = offset.checked_add(offset_table_bytes).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new("value record start overflowed"))
    })?;
    if records_start > payload.len() {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(
            "value offset table extended beyond payload",
        )));
    }

    let mut table_offset = offset;
    let mut record_offsets = Vec::with_capacity(value_count);
    for record_index in 0..value_count {
        let relative = read_u64_le(payload, &mut table_offset).map_err(|error| {
            let mut context = error.context().clone();
            context.record_index = Some(record_index);
            error.with_context(context)
        })?;
        let relative = usize::try_from(relative).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("value record offset could not be represented on this platform")
                    .with_record_index(record_index),
            )
        })?;
        record_offsets.push(relative);
    }

    let records_len = payload.len() - records_start;
    let mut raw_records = Vec::with_capacity(value_count);
    for record_index in 0..value_count {
        let record_offset = record_offsets[record_index];
        let next_offset = if record_index + 1 < value_count {
            record_offsets[record_index + 1]
        } else {
            records_len
        };

        if record_offset > next_offset {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("value record offsets were not in ascending order")
                    .with_record_index(record_index),
            ));
        }
        if next_offset > records_len {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("value record extended beyond payload")
                    .with_record_index(record_index),
            ));
        }

        let record_start = records_start.checked_add(record_offset).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("value record start overflowed").with_record_index(record_index),
            )
        })?;
        let record_end = records_start.checked_add(next_offset).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("value record end overflowed").with_record_index(record_index),
            )
        })?;
        let record = payload.get(record_start..record_end).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("value record bounds were outside the payload")
                    .with_record_index(record_index),
            )
        })?;
        if record.is_empty() {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("value records must not be empty")
                    .with_record_index(record_index),
            ));
        }

        raw_records.push(decode_value_record(
            record,
            record_index,
            value_count,
            blob_count,
            limits,
            mode,
        )?);
    }

    let mut referenced = vec![false; value_count];
    for record in &raw_records {
        match record {
            RawValueRecord::Sequence(items) => {
                for item in items {
                    referenced[*item] = true;
                }
            }
            RawValueRecord::Map(entries) => {
                for (key, value) in entries {
                    referenced[*key] = true;
                    referenced[*value] = true;
                }
            }
            RawValueRecord::Tagged { value_ref, .. } => referenced[*value_ref] = true,
            RawValueRecord::ExpressionSource {
                result_value_ref, ..
            }
            | RawValueRecord::LuaChunkSource {
                result_value_ref, ..
            } => {
                if let Some(value_ref) = result_value_ref {
                    referenced[*value_ref] = true;
                }
            }
            RawValueRecord::RuntimeDescriptor {
                descriptor_value_ref,
                fallback_value_ref,
                ..
            } => {
                if let Some(value_ref) = descriptor_value_ref {
                    referenced[*value_ref] = true;
                }
                if let Some(value_ref) = fallback_value_ref {
                    referenced[*value_ref] = true;
                }
            }
            RawValueRecord::ExtensionValue {
                fallback_value_ref, ..
            } => {
                if let Some(value_ref) = fallback_value_ref {
                    referenced[*value_ref] = true;
                }
            }
            _ => {}
        }
    }

    let root_indices = referenced
        .iter()
        .enumerate()
        .filter_map(|(index, referenced)| (!*referenced).then_some(index))
        .collect::<Vec<_>>();
    if value_count > 0 && root_indices.is_empty() {
        return Err(LybaError::InvalidValueReference(ErrorContext::new(
            "value arena did not contain any root values",
        )));
    }

    let mut resolved = vec![None; value_count];
    let mut visit = vec![None; value_count];
    let mut values = Vec::with_capacity(root_indices.len());
    for index in root_indices {
        values.push(resolve_value(
            &raw_records,
            index,
            limits,
            1,
            &mut resolved,
            &mut visit,
        )?);
    }
    Ok(values)
}

pub(crate) fn encode_value_table(
    values: &[Value],
    limits: &Limits,
    mode: WriterMode,
) -> Result<Vec<u8>> {
    if values.len() > limits.max_value_count {
        return Err(LybaError::limit_exceeded(
            "value count exceeds configured maximum",
        ));
    }

    let mut builder = ValueArenaBuilder::new(limits, mode);
    let root_count = values.len();
    for value in values {
        builder.push(value, 1)?;
    }
    debug_assert!(builder.root_count() >= root_count);

    let table_len = builder.records.len().checked_mul(8).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new(
            "value offset table length overflowed during encoding",
        ))
    })?;
    let mut bytes = Vec::new();
    UVar(builder.records.len() as u64).encode_into(&mut bytes);
    bytes.reserve(table_len);

    let mut record_offsets = Vec::with_capacity(builder.records.len());
    let mut running_offset = 0_u64;
    for record in &builder.records {
        record_offsets.push(running_offset);
        running_offset = running_offset
            .checked_add(record.len() as u64)
            .ok_or_else(|| {
                LybaError::InvalidSectionTable(ErrorContext::new(
                    "encoded value records overflowed u64 length",
                ))
            })?;
    }

    for offset in record_offsets {
        write_u64_le(&mut bytes, offset);
    }
    for record in builder.records {
        bytes.extend_from_slice(&record);
    }
    Ok(bytes)
}

pub(crate) fn find_duplicate_canonical_map_key(values: &[Value]) -> Option<usize> {
    for (index, value) in values.iter().enumerate() {
        if contains_duplicate_map_key(value) {
            return Some(index);
        }
    }
    None
}

fn contains_duplicate_map_key(value: &Value) -> bool {
    match value {
        Value::Sequence(items) => items.iter().any(contains_duplicate_map_key),
        Value::Map(entries) => {
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = canonical_key_bytes(&entry.key);
                if !seen.insert(key) {
                    return true;
                }
                if contains_duplicate_map_key(&entry.key)
                    || contains_duplicate_map_key(&entry.value)
                {
                    return true;
                }
            }
            false
        }
        Value::Tagged(tagged) => contains_duplicate_map_key(tagged.value.as_ref()),
        _ => false,
    }
}

fn canonical_key_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_canonical_key(value, &mut out);
    out
}

fn encode_canonical_key(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.push(0),
        Value::Bool(false) => out.push(1),
        Value::Bool(true) => out.push(2),
        Value::Int(value) => {
            out.push(3);
            out.extend_from_slice(&value.to_le_bytes());
        }
        Value::UInt(value) => {
            out.push(4);
            out.extend_from_slice(&value.to_le_bytes());
        }
        Value::Float(value) => {
            out.push(5);
            out.extend_from_slice(&value.get().to_bits().to_le_bytes());
        }
        Value::String(value) => {
            out.push(6);
            UVar(value.len() as u64).encode_into(out);
            out.extend_from_slice(value.as_bytes());
        }
        Value::Sequence(items) => {
            out.push(7);
            UVar(items.len() as u64).encode_into(out);
            for item in items {
                encode_canonical_key(item, out);
            }
        }
        Value::Map(entries) => {
            out.push(8);
            UVar(entries.len() as u64).encode_into(out);
            for entry in entries {
                encode_canonical_key(&entry.key, out);
                encode_canonical_key(&entry.value, out);
            }
        }
        Value::Tagged(tagged) => {
            out.push(9);
            UVar(tagged.tag.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(tagged.tag.as_str().as_bytes());
            encode_canonical_key(tagged.value.as_ref(), out);
        }
        Value::Decimal(value) => {
            out.push(10);
            UVar(value.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(value.as_str().as_bytes());
        }
        Value::BytesInline(bytes) => {
            out.push(11);
            UVar(bytes.len() as u64).encode_into(out);
            out.extend_from_slice(bytes);
        }
        Value::BytesBlob(blob) => {
            out.push(12);
            out.extend_from_slice(&blob.0.to_le_bytes());
        }
        Value::ExpressionSource(expression) => {
            out.push(13);
            UVar(expression.language.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(expression.language.as_str().as_bytes());
            match &expression.source {
                ExpressionSource::Text(source) => {
                    out.push(0);
                    UVar(source.len() as u64).encode_into(out);
                    out.extend_from_slice(source.as_bytes());
                }
                ExpressionSource::Blob(blob_id) => {
                    out.push(1);
                    out.extend_from_slice(&blob_id.0.to_le_bytes());
                }
            }
            out.extend_from_slice(
                &expression
                    .capability_set_ref
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            if let Some(value) = &expression.result_value {
                out.push(1);
                encode_canonical_key(value, out);
            } else {
                out.push(0);
            }
        }
        Value::LuaChunkSource(chunk) => {
            out.push(14);
            UVar(chunk.language.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(chunk.language.as_str().as_bytes());
            out.extend_from_slice(&chunk.source_blob_ref.0.to_le_bytes());
            out.extend_from_slice(&chunk.capability_set_ref.unwrap_or(u64::MAX).to_le_bytes());
            if let Some(value) = &chunk.result_value {
                out.push(1);
                encode_canonical_key(value, out);
            } else {
                out.push(0);
            }
        }
        Value::RuntimeDescriptor(descriptor) => {
            out.push(15);
            UVar(descriptor.kind.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(descriptor.kind.as_str().as_bytes());
            out.push(u8::from(descriptor.required));
            out.push(u8::from(descriptor.trusted_only));
            out.extend_from_slice(
                &descriptor
                    .capability_set_ref
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            if let Some(value) = &descriptor.descriptor_value {
                out.push(1);
                encode_canonical_key(value, out);
            } else {
                out.push(0);
            }
            if let Some(value) = &descriptor.fallback_value {
                out.push(1);
                encode_canonical_key(value, out);
            } else {
                out.push(0);
            }
        }
        Value::ExtensionValue(extension) => {
            out.push(16);
            UVar(extension.extension_name.len() as u64).encode_into(out);
            out.extend_from_slice(extension.extension_name.as_bytes());
            UVar(extension.type_name.as_str().len() as u64).encode_into(out);
            out.extend_from_slice(extension.type_name.as_str().as_bytes());
            out.extend_from_slice(&extension.payload_blob_ref.0.to_le_bytes());
            if let Some(value) = &extension.fallback_value {
                out.push(1);
                encode_canonical_key(value, out);
            } else {
                out.push(0);
            }
        }
    }
}

fn decode_value_record(
    record: &[u8],
    record_index: usize,
    value_count: usize,
    blob_count: usize,
    limits: &Limits,
    mode: ValueDecodeMode,
) -> Result<RawValueRecord> {
    let mut offset = 0_usize;
    let tag = UVar::decode(record, &mut offset)?.0;
    let raw = match tag {
        RECORD_NULL => RawValueRecord::Null,
        RECORD_BOOL_FALSE => RawValueRecord::Bool(false),
        RECORD_BOOL_TRUE => RawValueRecord::Bool(true),
        RECORD_INT => RawValueRecord::Int(SVar::decode(record, &mut offset)?.0),
        RECORD_UINT => RawValueRecord::UInt(UVar::decode(record, &mut offset)?.0),
        RECORD_FLOAT64 => {
            let bytes = read_bounded_bytes(record, &mut offset, 8, 8)?;
            let value = f64::from_bits(u64::from_le_bytes(
                bytes.try_into().expect("length checked"),
            ));
            if !value.is_finite() {
                let error = match mode {
                    ValueDecodeMode::Portable => LybaError::unsupported_numeric_value(
                        "non-finite float is not supported in portable values",
                    ),
                    ValueDecodeMode::Canonical => {
                        LybaError::non_canonical_encoding("canonical values require finite floats")
                    }
                };
                let context = error
                    .context()
                    .clone()
                    .with_record_index(record_index)
                    .with_byte_offset(offset - 8);
                return Err(error.with_context(context));
            }
            RawValueRecord::Float(value)
        }
        RECORD_STRING => {
            let len = UVar::decode(record, &mut offset)?.0;
            let len = usize::try_from(len).map_err(|_| {
                LybaError::limit_exceeded("string length exceeds configured maximum")
            })?;
            let string_offset = offset;
            let bytes = read_bounded_bytes(record, &mut offset, len, limits.max_string_bytes)?;
            let value = std::str::from_utf8(bytes).map_err(|_| {
                LybaError::InvalidUtf8(
                    ErrorContext::new("string bytes were not valid UTF-8")
                        .with_record_index(record_index)
                        .with_byte_offset(string_offset),
                )
            })?;
            RawValueRecord::String(value.to_owned())
        }
        RECORD_BYTES_INLINE => {
            let len = usize::try_from(UVar::decode(record, &mut offset)?.0).map_err(|_| {
                LybaError::limit_exceeded("blob length exceeds configured maximum")
            })?;
            let bytes =
                read_bounded_bytes(record, &mut offset, len, limits.max_decoded_logical_bytes)?;
            RawValueRecord::BytesInline(bytes.to_vec())
        }
        RECORD_BYTES_BLOB => {
            let blob_ref_offset = offset;
            let blob_id = UVar::decode(record, &mut offset)?.0;
            let blob_id_usize = usize::try_from(blob_id).map_err(|_| {
                LybaError::InvalidValueReference(
                    ErrorContext::new("blob reference could not be represented on this platform")
                        .with_record_index(record_index)
                        .with_byte_offset(blob_ref_offset),
                )
            })?;
            if blob_id_usize >= blob_count {
                return Err(LybaError::InvalidValueReference(
                    ErrorContext::new("blob reference pointed outside the blob table")
                        .with_record_index(record_index)
                        .with_byte_offset(blob_ref_offset),
                ));
            }
            RawValueRecord::BytesBlob(BlobId(blob_id))
        }
        RECORD_SEQUENCE => {
            let len = UVar::decode(record, &mut offset)?.0;
            let len = usize::try_from(len).map_err(|_| {
                LybaError::limit_exceeded("value count exceeds configured maximum")
            })?;
            if len > value_count {
                return Err(LybaError::InvalidValueReference(
                    ErrorContext::new("sequence length exceeded value count")
                        .with_record_index(record_index),
                ));
            }
            let mut refs = Vec::with_capacity(len);
            for _ in 0..len {
                refs.push(decode_value_ref(
                    record,
                    &mut offset,
                    record_index,
                    value_count,
                )?);
            }
            RawValueRecord::Sequence(refs)
        }
        RECORD_MAP => {
            let len = UVar::decode(record, &mut offset)?.0;
            let len = usize::try_from(len).map_err(|_| {
                LybaError::limit_exceeded("value count exceeds configured maximum")
            })?;
            if len > value_count {
                return Err(LybaError::InvalidValueReference(
                    ErrorContext::new("map entry count exceeded value count")
                        .with_record_index(record_index),
                ));
            }
            let mut entries = Vec::with_capacity(len);
            for _ in 0..len {
                let key = decode_value_ref(record, &mut offset, record_index, value_count)?;
                let value = decode_value_ref(record, &mut offset, record_index, value_count)?;
                entries.push((key, value));
            }
            RawValueRecord::Map(entries)
        }
        RECORD_TAGGED => {
            let len = UVar::decode(record, &mut offset)?.0;
            let len = usize::try_from(len).map_err(|_| {
                LybaError::limit_exceeded("string length exceeds configured maximum")
            })?;
            let tag_offset = offset;
            let bytes = read_bounded_bytes(record, &mut offset, len, limits.max_string_bytes)?;
            let tag = std::str::from_utf8(bytes).map_err(|_| {
                LybaError::InvalidUtf8(
                    ErrorContext::new("tag bytes were not valid UTF-8")
                        .with_record_index(record_index)
                        .with_byte_offset(tag_offset),
                )
            })?;
            let value_ref = decode_value_ref(record, &mut offset, record_index, value_count)?;
            RawValueRecord::Tagged {
                tag: Identifier::new(tag),
                value_ref,
            }
        }
        RECORD_EXPRESSION_SOURCE => {
            let language = decode_identifier(
                record,
                &mut offset,
                record_index,
                limits,
                "expression language",
            )?;
            let source_kind = UVar::decode(record, &mut offset)?.0;
            let source = match source_kind {
                0 => ExpressionSource::Text(decode_string(
                    record,
                    &mut offset,
                    record_index,
                    limits,
                    "expression source",
                )?),
                1 => ExpressionSource::Blob(decode_blob_id(
                    record,
                    &mut offset,
                    record_index,
                    blob_count,
                    "expression source blob",
                )?),
                _ => {
                    return Err(LybaError::InvalidSectionTable(
                        ErrorContext::new(format!("unknown expression source kind {source_kind}"))
                            .with_record_index(record_index),
                    ));
                }
            };
            let capability_set_ref = decode_optional_arena_ref(record, &mut offset)?;
            let result_value_ref =
                decode_optional_value_ref(record, &mut offset, record_index, value_count)?;
            RawValueRecord::ExpressionSource {
                language,
                source,
                capability_set_ref,
                result_value_ref,
            }
        }
        RECORD_LUA_CHUNK_SOURCE => {
            let language =
                decode_identifier(record, &mut offset, record_index, limits, "chunk language")?;
            let source_blob_ref = decode_blob_id(
                record,
                &mut offset,
                record_index,
                blob_count,
                "chunk source blob",
            )?;
            let capability_set_ref = decode_optional_arena_ref(record, &mut offset)?;
            let result_value_ref =
                decode_optional_value_ref(record, &mut offset, record_index, value_count)?;
            RawValueRecord::LuaChunkSource {
                language,
                source_blob_ref,
                capability_set_ref,
                result_value_ref,
            }
        }
        RECORD_RUNTIME_DESCRIPTOR => {
            let kind = decode_identifier(
                record,
                &mut offset,
                record_index,
                limits,
                "runtime descriptor",
            )?;
            let required =
                decode_bool_flag(record, &mut offset, record_index, "runtime descriptor")?;
            let trusted_only =
                decode_bool_flag(record, &mut offset, record_index, "runtime descriptor")?;
            let capability_set_ref = decode_optional_arena_ref(record, &mut offset)?;
            let descriptor_value_ref =
                decode_optional_value_ref(record, &mut offset, record_index, value_count)?;
            let fallback_value_ref =
                decode_optional_value_ref(record, &mut offset, record_index, value_count)?;
            RawValueRecord::RuntimeDescriptor {
                kind,
                required,
                trusted_only,
                capability_set_ref,
                descriptor_value_ref,
                fallback_value_ref,
            }
        }
        RECORD_EXTENSION_VALUE => {
            let extension_name =
                decode_string(record, &mut offset, record_index, limits, "extension name")?;
            let type_name = decode_identifier(
                record,
                &mut offset,
                record_index,
                limits,
                "extension value type",
            )?;
            let payload_blob_ref = decode_blob_id(
                record,
                &mut offset,
                record_index,
                blob_count,
                "extension payload blob",
            )?;
            let fallback_value_ref =
                decode_optional_value_ref(record, &mut offset, record_index, value_count)?;
            RawValueRecord::ExtensionValue {
                extension_name,
                type_name,
                payload_blob_ref,
                fallback_value_ref,
            }
        }
        _ => {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!("unknown value record tag {tag}"))
                    .with_record_index(record_index),
            ));
        }
    };

    if offset != record.len() {
        return Err(LybaError::InvalidSectionTable(
            ErrorContext::new("value record had trailing bytes").with_record_index(record_index),
        ));
    }
    Ok(raw)
}

fn decode_value_ref(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    value_count: usize,
) -> Result<usize> {
    let value_ref_offset = *offset;
    let value_ref = UVar::decode(record, offset)?.0;
    let value_ref = usize::try_from(value_ref).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new("value reference could not be represented on this platform")
                .with_record_index(record_index)
                .with_byte_offset(value_ref_offset),
        )
    })?;
    if value_ref >= value_count {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new("value reference pointed outside the value arena")
                .with_record_index(record_index)
                .with_byte_offset(value_ref_offset),
        ));
    }
    Ok(value_ref)
}

fn decode_optional_value_ref(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    value_count: usize,
) -> Result<Option<usize>> {
    let value_ref_offset = *offset;
    let encoded = UVar::decode(record, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let value_ref = usize::try_from(encoded - 1).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new("value reference could not be represented on this platform")
                .with_record_index(record_index)
                .with_byte_offset(value_ref_offset),
        )
    })?;
    if value_ref >= value_count {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new("value reference pointed outside the value arena")
                .with_record_index(record_index)
                .with_byte_offset(value_ref_offset),
        ));
    }
    Ok(Some(value_ref))
}

fn decode_optional_arena_ref(record: &[u8], offset: &mut usize) -> Result<Option<u64>> {
    let encoded = UVar::decode(record, offset)?.0;
    Ok(if encoded == 0 {
        None
    } else {
        Some(encoded - 1)
    })
}

fn decode_bool_flag(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    kind: &str,
) -> Result<bool> {
    let flag_offset = *offset;
    match UVar::decode(record, offset)?.0 {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(LybaError::InvalidSectionTable(
            ErrorContext::new(format!("{kind} boolean flag had invalid value {other}"))
                .with_record_index(record_index)
                .with_byte_offset(flag_offset),
        )),
    }
}

fn decode_blob_id(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    blob_count: usize,
    kind: &str,
) -> Result<BlobId> {
    let blob_ref_offset = *offset;
    let blob_id = UVar::decode(record, offset)?.0;
    let blob_id_usize = usize::try_from(blob_id).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} could not be represented on this platform"))
                .with_record_index(record_index)
                .with_byte_offset(blob_ref_offset),
        )
    })?;
    if blob_id_usize >= blob_count {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} pointed outside the blob table"))
                .with_record_index(record_index)
                .with_byte_offset(blob_ref_offset),
        ));
    }
    Ok(BlobId(blob_id))
}

fn decode_string(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    limits: &Limits,
    kind: &str,
) -> Result<String> {
    let len = UVar::decode(record, offset)?.0;
    let len = usize::try_from(len)
        .map_err(|_| LybaError::limit_exceeded("string length exceeds configured maximum"))?;
    let string_offset = *offset;
    let bytes = read_bounded_bytes(record, offset, len, limits.max_string_bytes)?;
    let value = std::str::from_utf8(bytes).map_err(|_| {
        LybaError::InvalidUtf8(
            ErrorContext::new(format!("{kind} bytes were not valid UTF-8"))
                .with_record_index(record_index)
                .with_byte_offset(string_offset),
        )
    })?;
    Ok(value.to_owned())
}

fn decode_identifier(
    record: &[u8],
    offset: &mut usize,
    record_index: usize,
    limits: &Limits,
    kind: &str,
) -> Result<Identifier> {
    decode_string(record, offset, record_index, limits, kind).map(Identifier::new)
}

fn resolve_value(
    raw_records: &[RawValueRecord],
    index: usize,
    limits: &Limits,
    depth: usize,
    resolved: &mut [Option<Value>],
    visit: &mut [Option<VisitState>],
) -> Result<Value> {
    if depth > limits.max_nesting_depth {
        return Err(LybaError::limit_exceeded(
            "nesting depth exceeds configured maximum",
        ));
    }
    if let Some(value) = &resolved[index] {
        return Ok(value.clone());
    }
    if visit[index] == Some(VisitState::Visiting) {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new("cyclic value reference is not supported").with_record_index(index),
        ));
    }

    visit[index] = Some(VisitState::Visiting);
    let value = match &raw_records[index] {
        RawValueRecord::Null => Value::Null,
        RawValueRecord::Bool(value) => Value::Bool(*value),
        RawValueRecord::Int(value) => Value::Int(*value),
        RawValueRecord::UInt(value) => Value::UInt(*value),
        RawValueRecord::Float(value) => Value::Float(FiniteFloat::new(*value)?),
        RawValueRecord::String(value) => Value::String(value.clone()),
        RawValueRecord::BytesInline(bytes) => Value::BytesInline(bytes.clone()),
        RawValueRecord::BytesBlob(blob_id) => Value::BytesBlob(*blob_id),
        RawValueRecord::Sequence(items) => Value::Sequence(
            items
                .iter()
                .map(|item| resolve_value(raw_records, *item, limits, depth + 1, resolved, visit))
                .collect::<Result<Vec<_>>>()?,
        ),
        RawValueRecord::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    Ok(MapEntry {
                        key: resolve_value(raw_records, *key, limits, depth + 1, resolved, visit)?,
                        value: resolve_value(
                            raw_records,
                            *value,
                            limits,
                            depth + 1,
                            resolved,
                            visit,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        RawValueRecord::Tagged { tag, value_ref } => Value::Tagged(TaggedValue {
            tag: tag.clone(),
            value: Box::new(resolve_value(
                raw_records,
                *value_ref,
                limits,
                depth + 1,
                resolved,
                visit,
            )?),
        }),
        RawValueRecord::ExpressionSource {
            language,
            source,
            capability_set_ref,
            result_value_ref,
        } => Value::ExpressionSource(ExpressionValue {
            language: language.clone(),
            source: source.clone(),
            capability_set_ref: *capability_set_ref,
            result_value: result_value_ref
                .map(|value_ref| {
                    resolve_value(raw_records, value_ref, limits, depth + 1, resolved, visit)
                        .map(Box::new)
                })
                .transpose()?,
        }),
        RawValueRecord::LuaChunkSource {
            language,
            source_blob_ref,
            capability_set_ref,
            result_value_ref,
        } => Value::LuaChunkSource(LuaChunkValue {
            language: language.clone(),
            source_blob_ref: *source_blob_ref,
            capability_set_ref: *capability_set_ref,
            result_value: result_value_ref
                .map(|value_ref| {
                    resolve_value(raw_records, value_ref, limits, depth + 1, resolved, visit)
                        .map(Box::new)
                })
                .transpose()?,
        }),
        RawValueRecord::RuntimeDescriptor {
            kind,
            required,
            trusted_only,
            capability_set_ref,
            descriptor_value_ref,
            fallback_value_ref,
        } => Value::RuntimeDescriptor(RuntimeDescriptorValue {
            kind: kind.clone(),
            required: *required,
            trusted_only: *trusted_only,
            capability_set_ref: *capability_set_ref,
            descriptor_value: descriptor_value_ref
                .map(|value_ref| {
                    resolve_value(raw_records, value_ref, limits, depth + 1, resolved, visit)
                        .map(Box::new)
                })
                .transpose()?,
            fallback_value: fallback_value_ref
                .map(|value_ref| {
                    resolve_value(raw_records, value_ref, limits, depth + 1, resolved, visit)
                        .map(Box::new)
                })
                .transpose()?,
        }),
        RawValueRecord::ExtensionValue {
            extension_name,
            type_name,
            payload_blob_ref,
            fallback_value_ref,
        } => Value::ExtensionValue(ExtensionValue {
            extension_name: extension_name.clone(),
            type_name: type_name.clone(),
            payload_blob_ref: *payload_blob_ref,
            fallback_value: fallback_value_ref
                .map(|value_ref| {
                    resolve_value(raw_records, value_ref, limits, depth + 1, resolved, visit)
                        .map(Box::new)
                })
                .transpose()?,
        }),
    };
    visit[index] = Some(VisitState::Done);
    resolved[index] = Some(value.clone());
    Ok(value)
}

struct ValueArenaBuilder<'a> {
    limits: &'a Limits,
    mode: WriterMode,
    records: Vec<Vec<u8>>,
}

impl<'a> ValueArenaBuilder<'a> {
    fn new(limits: &'a Limits, mode: WriterMode) -> Self {
        Self {
            limits,
            mode,
            records: Vec::new(),
        }
    }

    fn root_count(&self) -> usize {
        self.records.len()
    }

    fn push(&mut self, value: &Value, depth: usize) -> Result<usize> {
        if depth > self.limits.max_nesting_depth {
            return Err(LybaError::limit_exceeded(
                "nesting depth exceeds configured maximum",
            ));
        }
        if matches!(
            self.mode,
            WriterMode::Canonical(_) | WriterMode::RuntimeData
        ) {
            validate_canonical_value(value, self.mode)?;
        }

        let mut record = Vec::new();
        match value {
            Value::Null => UVar(RECORD_NULL).encode_into(&mut record),
            Value::Bool(false) => UVar(RECORD_BOOL_FALSE).encode_into(&mut record),
            Value::Bool(true) => UVar(RECORD_BOOL_TRUE).encode_into(&mut record),
            Value::Int(value) => {
                UVar(RECORD_INT).encode_into(&mut record);
                SVar(*value).encode_into(&mut record);
            }
            Value::UInt(value) => {
                UVar(RECORD_UINT).encode_into(&mut record);
                UVar(*value).encode_into(&mut record);
            }
            Value::Float(value) => {
                UVar(RECORD_FLOAT64).encode_into(&mut record);
                record.extend_from_slice(&value.get().to_bits().to_le_bytes());
            }
            Value::String(value) => {
                if value.len() > self.limits.max_string_bytes {
                    return Err(LybaError::limit_exceeded(
                        "string length exceeds configured maximum",
                    ));
                }
                UVar(RECORD_STRING).encode_into(&mut record);
                UVar(value.len() as u64).encode_into(&mut record);
                record.extend_from_slice(value.as_bytes());
            }
            Value::BytesInline(bytes) => {
                UVar(RECORD_BYTES_INLINE).encode_into(&mut record);
                UVar(bytes.len() as u64).encode_into(&mut record);
                record.extend_from_slice(bytes);
            }
            Value::BytesBlob(blob_id) => {
                UVar(RECORD_BYTES_BLOB).encode_into(&mut record);
                UVar(blob_id.0).encode_into(&mut record);
            }
            Value::Sequence(items) => {
                UVar(RECORD_SEQUENCE).encode_into(&mut record);
                UVar(items.len() as u64).encode_into(&mut record);
                for item in items {
                    let id = self.push(item, depth + 1)?;
                    UVar(id as u64).encode_into(&mut record);
                }
            }
            Value::Map(entries) => {
                UVar(RECORD_MAP).encode_into(&mut record);
                UVar(entries.len() as u64).encode_into(&mut record);
                for entry in entries {
                    let key = self.push(&entry.key, depth + 1)?;
                    let value = self.push(&entry.value, depth + 1)?;
                    UVar(key as u64).encode_into(&mut record);
                    UVar(value as u64).encode_into(&mut record);
                }
            }
            Value::Tagged(tagged) => {
                if tagged.tag.as_str().len() > self.limits.max_string_bytes {
                    return Err(LybaError::limit_exceeded(
                        "string length exceeds configured maximum",
                    ));
                }
                UVar(RECORD_TAGGED).encode_into(&mut record);
                UVar(tagged.tag.as_str().len() as u64).encode_into(&mut record);
                record.extend_from_slice(tagged.tag.as_str().as_bytes());
                let value = self.push(tagged.value.as_ref(), depth + 1)?;
                UVar(value as u64).encode_into(&mut record);
            }
            Value::ExpressionSource(expression) => {
                UVar(RECORD_EXPRESSION_SOURCE).encode_into(&mut record);
                encode_identifier(&mut record, expression.language.as_str());
                match &expression.source {
                    ExpressionSource::Text(source) => {
                        UVar(0).encode_into(&mut record);
                        encode_string(&mut record, source);
                    }
                    ExpressionSource::Blob(blob_id) => {
                        UVar(1).encode_into(&mut record);
                        UVar(blob_id.0).encode_into(&mut record);
                    }
                }
                UVar(encode_optional_arena_ref(expression.capability_set_ref)?)
                    .encode_into(&mut record);
                UVar(
                    expression
                        .result_value
                        .as_ref()
                        .map(|value| self.push(value.as_ref(), depth + 1))
                        .transpose()?
                        .map(|value_ref| value_ref as u64 + 1)
                        .unwrap_or(0),
                )
                .encode_into(&mut record);
            }
            Value::LuaChunkSource(chunk) => {
                UVar(RECORD_LUA_CHUNK_SOURCE).encode_into(&mut record);
                encode_identifier(&mut record, chunk.language.as_str());
                UVar(chunk.source_blob_ref.0).encode_into(&mut record);
                UVar(encode_optional_arena_ref(chunk.capability_set_ref)?).encode_into(&mut record);
                UVar(
                    chunk
                        .result_value
                        .as_ref()
                        .map(|value| self.push(value.as_ref(), depth + 1))
                        .transpose()?
                        .map(|value_ref| value_ref as u64 + 1)
                        .unwrap_or(0),
                )
                .encode_into(&mut record);
            }
            Value::RuntimeDescriptor(descriptor) => {
                UVar(RECORD_RUNTIME_DESCRIPTOR).encode_into(&mut record);
                encode_identifier(&mut record, descriptor.kind.as_str());
                UVar(u64::from(descriptor.required)).encode_into(&mut record);
                UVar(u64::from(descriptor.trusted_only)).encode_into(&mut record);
                UVar(encode_optional_arena_ref(descriptor.capability_set_ref)?)
                    .encode_into(&mut record);
                UVar(
                    descriptor
                        .descriptor_value
                        .as_ref()
                        .map(|value| self.push(value.as_ref(), depth + 1))
                        .transpose()?
                        .map(|value_ref| value_ref as u64 + 1)
                        .unwrap_or(0),
                )
                .encode_into(&mut record);
                UVar(
                    descriptor
                        .fallback_value
                        .as_ref()
                        .map(|value| self.push(value.as_ref(), depth + 1))
                        .transpose()?
                        .map(|value_ref| value_ref as u64 + 1)
                        .unwrap_or(0),
                )
                .encode_into(&mut record);
            }
            Value::ExtensionValue(extension) => {
                if extension.extension_name.len() > self.limits.max_string_bytes {
                    return Err(LybaError::limit_exceeded(
                        "string length exceeds configured maximum",
                    ));
                }
                UVar(RECORD_EXTENSION_VALUE).encode_into(&mut record);
                encode_string(&mut record, &extension.extension_name);
                encode_identifier(&mut record, extension.type_name.as_str());
                UVar(extension.payload_blob_ref.0).encode_into(&mut record);
                UVar(
                    extension
                        .fallback_value
                        .as_ref()
                        .map(|value| self.push(value.as_ref(), depth + 1))
                        .transpose()?
                        .map(|value_ref| value_ref as u64 + 1)
                        .unwrap_or(0),
                )
                .encode_into(&mut record);
            }
            Value::Decimal(value) => {
                return Err(LybaError::unsupported_decimal_value(format!(
                    "decimal value {:?} is native-only and cannot be written to VALS",
                    value.as_str(),
                )));
            }
        }

        if self.records.len() >= self.limits.max_value_count {
            return Err(LybaError::limit_exceeded(
                "value count exceeds configured maximum",
            ));
        }
        self.records.push(record);
        Ok(self.records.len() - 1)
    }
}

fn validate_canonical_value(value: &Value, mode: WriterMode) -> Result<()> {
    match value {
        Value::Map(entries) => {
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = canonical_key_bytes(&entry.key);
                if !seen.insert(key) {
                    return Err(LybaError::DuplicateKeyInCanonicalMap(ErrorContext::new(
                        "duplicate canonical map key",
                    )));
                }
                validate_canonical_value(&entry.key, mode)?;
                validate_canonical_value(&entry.value, mode)?;
            }
        }
        Value::Sequence(items) => {
            for item in items {
                validate_canonical_value(item, mode)?;
            }
        }
        Value::Tagged(tagged) => validate_canonical_value(tagged.value.as_ref(), mode)?,
        Value::ExpressionSource(_)
        | Value::LuaChunkSource(_)
        | Value::RuntimeDescriptor(_)
        | Value::ExtensionValue(_) => {
            return Err(LybaError::non_canonical_encoding(
                "canonical values cannot encode inert runtime-only or extension-backed values",
            ));
        }
        Value::Float(value)
            if matches!(
                mode,
                WriterMode::Canonical(CanonicalMode::Strict) | WriterMode::RuntimeData
            ) && !value.get().is_finite() =>
        {
            return Err(LybaError::non_canonical_encoding(
                "canonical values require finite floats",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn native_key(key: &LymaKey) -> Result<Value> {
    match key {
        LymaKey::String(value) => Ok(Value::String(value.clone())),
        LymaKey::Number(LymaNumber::Integer(value)) => Ok(Value::Int(*value)),
        LymaKey::Number(LymaNumber::Float(value)) => FiniteFloat::new(*value).map(Value::Float),
        LymaKey::Boolean(value) => Ok(Value::Bool(*value)),
        LymaKey::Host(host) => Err(unsupported_runtime_value("host key", host)),
    }
}

fn syntax_key(value: &Value) -> Result<LymaKey> {
    match value {
        Value::String(value) => Ok(LymaKey::String(value.clone())),
        Value::Int(value) => Ok(LymaKey::Number(LymaNumber::Integer(*value))),
        Value::UInt(value) => i64::try_from(*value)
            .map(|value| LymaKey::Number(LymaNumber::Integer(value)))
            .map_err(|_| {
                LybaError::unsupported_numeric_value(format!(
                    "unsigned integer key {value} exceeds lyma_syntax::LymaKey integer range",
                ))
            }),
        Value::Float(value) => Ok(LymaKey::Number(LymaNumber::Float(value.get()))),
        Value::Bool(value) => Ok(LymaKey::Boolean(*value)),
        Value::Decimal(decimal) => Err(LybaError::unsupported_decimal_value(format!(
            "decimal map key {:?} has no lossless lyma_syntax::LymaKey representation",
            decimal.as_str(),
        ))),
        Value::BytesInline(bytes) => Err(LybaError::unsupported_byte_value(format!(
            "inline byte map key of {} bytes is not portable",
            bytes.len(),
        ))),
        Value::BytesBlob(blob_id) => Err(LybaError::unsupported_byte_value(format!(
            "blob byte map key {:?} is not portable",
            blob_id,
        ))),
        Value::Null
        | Value::Sequence(_)
        | Value::Map(_)
        | Value::Tagged(_)
        | Value::ExpressionSource(_)
        | Value::LuaChunkSource(_)
        | Value::RuntimeDescriptor(_)
        | Value::ExtensionValue(_) => Err(LybaError::InvalidValueReference(ErrorContext::new(
            "map keys must convert to string, number, or boolean syntax keys",
        ))),
    }
}

fn encode_string(record: &mut Vec<u8>, value: &str) {
    UVar(value.len() as u64).encode_into(record);
    record.extend_from_slice(value.as_bytes());
}

fn encode_identifier(record: &mut Vec<u8>, value: &str) {
    encode_string(record, value);
}

fn encode_optional_arena_ref(value: Option<u64>) -> Result<u64> {
    value
        .map(|value| {
            value.checked_add(1).ok_or_else(|| {
                LybaError::invalid_section_table("optional arena reference overflowed u64")
            })
        })
        .transpose()
        .map(|value| value.unwrap_or(0))
}

fn unsupported_runtime_value(kind: &str, host: &LymaHostValue) -> LybaError {
    LybaError::unsupported_runtime_value(format!(
        "{kind} runtime value {}{} is not portable",
        host.kind,
        host.label
            .as_deref()
            .map_or(String::new(), |label| format!(" ({label})")),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        DecimalString, FiniteFloat, MapEntry, TaggedValue, VALUE_SECTION_NAME, Value,
        ValueDecodeMode, canonical_key_bytes, decode_value_table, empty_span, encode_value_table,
    };
    use crate::{
        blob::BlobId,
        error::LybaError,
        policy::Limits,
        primitives::{Identifier, UVar, write_u64_le},
        write::{CanonicalMode, WriterMode},
    };
    use lyma_syntax::{
        LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaSequence,
        LymaTag, LymaTagName, LymaTaggedValue, LymaValue,
    };

    fn portable_values_fixture() -> Vec<Value> {
        vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(-7),
            Value::UInt(9),
            Value::Float(FiniteFloat::new(3.5).unwrap()),
            Value::String(String::from("text")),
            Value::Sequence(vec![Value::Int(1), Value::String(String::from("two"))]),
            Value::Map(vec![
                MapEntry {
                    key: Value::String(String::from("answer")),
                    value: Value::Int(42),
                },
                MapEntry {
                    key: Value::Bool(false),
                    value: Value::Sequence(vec![Value::UInt(3), Value::Null]),
                },
            ]),
            Value::Tagged(TaggedValue {
                tag: Identifier::new("example"),
                value: Box::new(Value::Bool(false)),
            }),
        ]
    }

    fn encode_raw_value_table(count: u64, offsets: &[u64], records: &[&[u8]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        UVar(count).encode_into(&mut bytes);
        for offset in offsets {
            write_u64_le(&mut bytes, *offset);
        }
        for record in records {
            bytes.extend_from_slice(record);
        }
        bytes
    }

    #[test]
    fn vals_constant_matches_section_name() {
        assert_eq!(VALUE_SECTION_NAME, "VALS");
    }

    #[test]
    fn native_value_variants_cover_level1_portable_and_native_only_cases() {
        let value = Value::Sequence(vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(-7),
            Value::UInt(9),
            Value::Float(FiniteFloat::new(3.5).unwrap()),
            Value::Decimal(DecimalString::new("12.3400")),
            Value::String(String::from("text")),
            Value::BytesInline(vec![1, 2, 3]),
            Value::BytesBlob(BlobId(42)),
            Value::Map(vec![MapEntry {
                key: Value::String(String::from("answer")),
                value: Value::Int(42),
            }]),
            Value::Tagged(TaggedValue {
                tag: Identifier::new("example"),
                value: Box::new(Value::Bool(false)),
            }),
        ]);

        assert!(matches!(value, Value::Sequence(_)));
    }

    #[test]
    fn vals_round_trip_preserves_order_and_values() {
        let values = portable_values_fixture();

        let encoded = encode_value_table(&values, &Limits::public(), WriterMode::Pretty)
            .expect("table should encode");
        let decoded = decode_value_table(&encoded, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect("table should decode");

        assert_eq!(decoded, values);
    }

    #[test]
    fn vals_round_trip_from_lyma_values_preserves_order_and_values() {
        let syntax_values = vec![
            LymaValue::Null(LymaNull),
            LymaValue::Boolean(true),
            LymaValue::Sequence(LymaSequence {
                items: vec![
                    LymaValue::Number(LymaNumber::Integer(7)),
                    LymaValue::String("x".into()),
                ],
                span: None,
            }),
            LymaValue::Tagged(LymaTaggedValue {
                tag: LymaTag {
                    name: LymaTagName {
                        value: String::from("tag"),
                        span: empty_span(),
                    },
                    span: empty_span(),
                },
                value: Box::new(LymaValue::Mapping(LymaMapping {
                    entries: vec![LymaMappingEntry {
                        key: LymaKey::String(String::from("k")),
                        value: LymaValue::Number(LymaNumber::Integer(9)),
                        span: None,
                    }],
                    duplicate_keys: Vec::new(),
                    span: None,
                })),
                span: None,
            }),
        ];
        let native = syntax_values
            .iter()
            .map(Value::try_from)
            .collect::<Result<Vec<_>, _>>()
            .expect("syntax values should convert");

        let encoded = encode_value_table(&native, &Limits::public(), WriterMode::Pretty)
            .expect("table should encode");
        let decoded = decode_value_table(&encoded, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect("table should decode");
        let round_tripped = decoded
            .iter()
            .map(LymaValue::try_from)
            .collect::<Result<Vec<_>, _>>()
            .expect("native values should convert back");

        assert_eq!(round_tripped, syntax_values);
    }

    #[test]
    fn vals_decode_rejects_malformed_reference_with_lb0014() {
        let seq_record = vec![7, 1, 1];
        let payload = encode_raw_value_table(1, &[0], &[&seq_record]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect_err("bad ref should fail");

        assert!(matches!(error, LybaError::InvalidValueReference(_)));
        assert_eq!(error.code().as_str(), "LB0014");
    }

    #[test]
    fn vals_canonical_encoding_rejects_duplicate_map_keys_with_lb0016() {
        let values = vec![Value::Map(vec![
            MapEntry {
                key: Value::String(String::from("dup")),
                value: Value::Int(1),
            },
            MapEntry {
                key: Value::String(String::from("dup")),
                value: Value::Int(2),
            },
        ])];

        let error = encode_value_table(
            &values,
            &Limits::public(),
            WriterMode::Canonical(CanonicalMode::Strict),
        )
        .expect_err("duplicate canonical keys should fail");

        assert!(matches!(error, LybaError::DuplicateKeyInCanonicalMap(_)));
        assert_eq!(error.code().as_str(), "LB0016");
    }

    #[test]
    fn vals_portable_decode_rejects_non_finite_float_with_lb0024() {
        let mut record = vec![5];
        record.extend_from_slice(&f64::INFINITY.to_bits().to_le_bytes());
        let payload = encode_raw_value_table(1, &[0], &[&record]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect_err("non-finite float should fail");

        assert!(matches!(error, LybaError::UnsupportedNumericValue(_)));
        assert_eq!(error.code().as_str(), "LB0024");
    }

    #[test]
    fn vals_canonical_decode_rejects_non_finite_float_with_lb0017() {
        let mut record = vec![5];
        record.extend_from_slice(&f64::NAN.to_bits().to_le_bytes());
        let payload = encode_raw_value_table(1, &[0], &[&record]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Canonical, 0)
            .expect_err("non-finite float should fail canonically");

        assert!(matches!(error, LybaError::NonCanonicalEncoding(_)));
        assert_eq!(error.code().as_str(), "LB0017");
    }

    #[test]
    fn vals_decode_rejects_offset_table_length_overflow_before_indexing() {
        let mut limits = Limits::public();
        limits.max_value_count = usize::MAX;
        let mut payload = Vec::new();
        UVar((usize::MAX as u64 / 8) + 1).encode_into(&mut payload);

        let error = decode_value_table(&payload, &limits, ValueDecodeMode::Portable, 0)
            .expect_err("overflow should fail");

        assert!(matches!(error, LybaError::InvalidSectionTable(_)));
    }

    #[test]
    fn vals_decode_rejects_offset_plus_len_overflow_before_slicing() {
        let payload = encode_raw_value_table(1, &[u64::MAX], &[&[0]]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect_err("overflow should fail");

        assert!(matches!(error, LybaError::InvalidSectionTable(_)));
    }

    #[test]
    fn vals_decode_rejects_u64_to_usize_offset_conversion_before_indexing() {
        let payload = encode_raw_value_table(1, &[u64::MAX], &[]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect_err("conversion should fail");

        assert!(matches!(error, LybaError::InvalidSectionTable(_)));
    }

    #[test]
    fn vals_decode_rejects_u64_to_usize_ref_conversion_before_indexing() {
        let mut record = vec![7, 1];
        UVar(u64::MAX).encode_into(&mut record);
        let payload = encode_raw_value_table(1, &[0], &[&record]);

        let error = decode_value_table(&payload, &Limits::public(), ValueDecodeMode::Portable, 0)
            .expect_err("conversion should fail");

        assert!(matches!(error, LybaError::InvalidValueReference(_)));
        assert_eq!(error.code().as_str(), "LB0014");
    }

    #[test]
    fn vals_decode_enforces_nesting_depth() {
        let values = vec![Value::Sequence(vec![Value::Sequence(vec![Value::Null])])];
        let encoded = encode_value_table(&values, &Limits::public(), WriterMode::Pretty)
            .expect("table should encode");
        let mut limits = Limits::public();
        limits.max_nesting_depth = 2;

        let error = decode_value_table(&encoded, &limits, ValueDecodeMode::Portable, 0)
            .expect_err("depth should fail");

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn converts_from_lyma_value_for_supported_portable_values() {
        let syntax = LymaValue::Mapping(LymaMapping {
            entries: vec![
                LymaMappingEntry {
                    key: LymaKey::String(String::from("name")),
                    value: LymaValue::String(String::from("lyma")),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::Number(LymaNumber::Integer(7)),
                    value: LymaValue::Sequence(LymaSequence {
                        items: vec![LymaValue::Null(LymaNull), LymaValue::Boolean(true)],
                        span: None,
                    }),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::Boolean(false),
                    value: LymaValue::Tagged(LymaTaggedValue {
                        tag: LymaTag {
                            name: LymaTagName {
                                value: String::from("tag"),
                                span: empty_span(),
                            },
                            span: empty_span(),
                        },
                        value: Box::new(LymaValue::Number(LymaNumber::Float(1.25))),
                        span: None,
                    }),
                    span: None,
                },
            ],
            duplicate_keys: Vec::new(),
            span: None,
        });

        let native = Value::try_from(syntax).unwrap();

        assert!(matches!(native, Value::Map(entries) if entries.len() == 3));
    }

    #[test]
    fn converts_to_lyma_value_for_supported_portable_subset() {
        let native = Value::Tagged(TaggedValue {
            tag: Identifier::new("outer"),
            value: Box::new(Value::Map(vec![
                MapEntry {
                    key: Value::String(String::from("count")),
                    value: Value::UInt(12),
                },
                MapEntry {
                    key: Value::Bool(true),
                    value: Value::Sequence(vec![Value::Int(-1), Value::Bool(false)]),
                },
            ])),
        });

        let syntax = LymaValue::try_from(native).unwrap();

        assert!(matches!(syntax, LymaValue::Tagged(_)));
    }

    #[test]
    fn conversions_reject_non_portable_bytes_decimal_and_runtime_cases() {
        let byte_error = LymaValue::try_from(Value::BytesInline(vec![1, 2, 3])).unwrap_err();
        assert!(matches!(byte_error, LybaError::UnsupportedByteValue(_)));

        let decimal_error =
            LymaValue::try_from(Value::Decimal(DecimalString::new("1.23"))).unwrap_err();
        assert!(matches!(
            decimal_error,
            LybaError::UnsupportedDecimalValue(_)
        ));

        let runtime_error = Value::try_from(LymaValue::HostObject(LymaHostValue {
            kind: String::from("mock"),
            label: Some(String::from("object")),
        }))
        .unwrap_err();
        assert!(matches!(
            runtime_error,
            LybaError::UnsupportedRuntimeValue(_)
        ));
    }

    #[test]
    fn conversions_reject_non_finite_and_out_of_range_numeric_cases() {
        let non_finite =
            Value::try_from(LymaValue::Number(LymaNumber::Float(f64::INFINITY))).unwrap_err();
        assert!(matches!(non_finite, LybaError::UnsupportedNumericValue(_)));

        let too_large = LymaValue::try_from(Value::UInt((i64::MAX as u64) + 1)).unwrap_err();
        assert!(matches!(too_large, LybaError::UnsupportedNumericValue(_)));
    }

    #[test]
    fn canonical_key_encoding_is_stable_for_equal_values() {
        let left = Value::Map(vec![MapEntry {
            key: Value::String(String::from("k")),
            value: Value::Int(1),
        }]);
        let right = Value::Map(vec![MapEntry {
            key: Value::String(String::from("k")),
            value: Value::Int(1),
        }]);

        assert_eq!(canonical_key_bytes(&left), canonical_key_bytes(&right));
    }
}
