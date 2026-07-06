//! Schema table (`SCMA`) helpers.

use crate::blob::BlobId;
use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::UVar;
use crate::string_table::StringTable;
use crate::value::Value;

/// `SCMA`
pub const SCHEMA_SECTION_NAME: &str = "SCMA";

/// Schema URI is present.
pub const SCHEMA_FLAG_URI_PRESENT: u64 = 1 << 0;
/// Embedded schema value is present.
pub const SCHEMA_FLAG_VALUE_PRESENT: u64 = 1 << 1;
/// Digest blob reference is present.
pub const SCHEMA_FLAG_DIGEST_PRESENT: u64 = 1 << 2;
/// Producer validated documents against this schema.
pub const SCHEMA_FLAG_VALIDATED_BY_PRODUCER: u64 = 1 << 3;
/// At least one document requires this schema.
pub const SCHEMA_FLAG_REQUIRED_BY_DOCUMENT: u64 = 1 << 4;
/// Schema requires trusted validator capabilities.
pub const SCHEMA_FLAG_TRUSTED_VALIDATOR_REQUIRED: u64 = 1 << 5;
/// Reserved schema flag bits.
pub const SCHEMA_FLAG_RESERVED_MASK: u64 = !0x3f;

/// One decoded `SCMA` record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchemaRecord {
    /// Stored flags.
    pub flags: u64,
    /// Optional schema URI stored through `STRS`.
    pub uri: Option<String>,
    /// Optional embedded schema value stored through `VALS`.
    pub value: Option<Value>,
    /// Optional digest blob reference into `BLOB`.
    pub digest_blob_ref: Option<BlobId>,
    /// Optional metadata value stored through `VALS`.
    pub metadata_value: Option<Value>,
}

impl SchemaRecord {
    /// Creates an empty schema record.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional schema URI.
    #[must_use]
    pub fn with_uri(mut self, uri: Option<String>) -> Self {
        self.uri = uri;
        self
    }

    /// Sets an optional embedded schema value.
    #[must_use]
    pub fn with_value(mut self, value: Option<Value>) -> Self {
        self.value = value;
        self
    }

    /// Sets an optional digest blob reference.
    #[must_use]
    pub fn with_digest_blob_ref(mut self, digest_blob_ref: Option<BlobId>) -> Self {
        self.digest_blob_ref = digest_blob_ref;
        self
    }

    /// Sets an optional metadata value.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Returns whether the schema carries a URI.
    #[must_use]
    pub const fn has_uri(&self) -> bool {
        self.flags & SCHEMA_FLAG_URI_PRESENT != 0
    }

    /// Returns whether the schema carries an embedded value.
    #[must_use]
    pub const fn has_value(&self) -> bool {
        self.flags & SCHEMA_FLAG_VALUE_PRESENT != 0
    }

    /// Returns whether the schema carries a digest blob reference.
    #[must_use]
    pub const fn has_digest(&self) -> bool {
        self.flags & SCHEMA_FLAG_DIGEST_PRESENT != 0
    }

    /// Returns whether the producer claims validation was performed.
    #[must_use]
    pub const fn is_validated_by_producer(&self) -> bool {
        self.flags & SCHEMA_FLAG_VALIDATED_BY_PRODUCER != 0
    }

    /// Returns whether the schema is required by at least one document.
    #[must_use]
    pub const fn is_required_by_document(&self) -> bool {
        self.flags & SCHEMA_FLAG_REQUIRED_BY_DOCUMENT != 0
    }

    /// Returns whether the schema requires trusted validator capabilities.
    #[must_use]
    pub const fn requires_trusted_validator(&self) -> bool {
        self.flags & SCHEMA_FLAG_TRUSTED_VALIDATOR_REQUIRED != 0
    }

    fn normalized_flags(&self) -> u64 {
        let mut flags = self.flags;
        if self.uri.is_some() {
            flags |= SCHEMA_FLAG_URI_PRESENT;
        } else {
            flags &= !SCHEMA_FLAG_URI_PRESENT;
        }
        if self.value.is_some() {
            flags |= SCHEMA_FLAG_VALUE_PRESENT;
        } else {
            flags &= !SCHEMA_FLAG_VALUE_PRESENT;
        }
        if self.digest_blob_ref.is_some() {
            flags |= SCHEMA_FLAG_DIGEST_PRESENT;
        } else {
            flags &= !SCHEMA_FLAG_DIGEST_PRESENT;
        }
        flags
    }
}

/// In-memory `SCMA` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchemaTable {
    /// Ordered schema records.
    pub records: Vec<SchemaRecord>,
}

impl SchemaTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: SchemaRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Returns the record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns true when no records are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

pub(crate) fn decode_schema_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    values: Option<&[Value]>,
    blob_count: usize,
) -> Result<SchemaTable> {
    let mut offset = 0_usize;
    let schema_count = UVar::decode(payload, &mut offset)?.0;
    let schema_count = usize::try_from(schema_count)
        .map_err(|_| LybaError::limit_exceeded("schema count exceeds configured maximum"))?;
    if schema_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "schema count exceeds configured maximum",
        ));
    }

    let mut records = Vec::with_capacity(schema_count);
    for record_index in 0..schema_count {
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & SCHEMA_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved schema flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let uri_ref = decode_string_ref(payload, &mut offset, strings, record_index, "schema URI")?;
        let value = decode_value_ref(payload, &mut offset, values, record_index, "schema value")?;
        let digest_blob_ref = decode_blob_ref(payload, &mut offset, blob_count, record_index)?;
        let metadata_value = decode_value_ref(
            payload,
            &mut offset,
            values,
            record_index,
            "schema metadata",
        )?;

        records.push(SchemaRecord {
            flags: normalize_flags(
                flags,
                uri_ref.is_some(),
                value.is_some(),
                digest_blob_ref.is_some(),
            ),
            uri: uri_ref.map(|string_id| strings.strings[string_id as usize].value.clone()),
            value,
            digest_blob_ref: digest_blob_ref.map(BlobId),
            metadata_value,
        });
    }

    if offset != payload.len() {
        return Err(LybaError::InvalidSectionTable(
            ErrorContext::new("schema table payload had trailing bytes").with_byte_offset(offset),
        ));
    }

    Ok(SchemaTable { records })
}

pub(crate) fn encode_schema_table(
    table: &SchemaTable,
    strings: &StringTable,
    values: &[Value],
    blob_count: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);

    for (record_index, record) in table.records.iter().enumerate() {
        let flags = record.normalized_flags();
        if flags & SCHEMA_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved schema flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        let uri_ref =
            encode_string_ref(record.uri.as_deref(), strings, record_index, "schema URI")?;
        let value_ref =
            encode_value_ref(record.value.as_ref(), values, record_index, "schema value")?;
        let digest_blob_ref = encode_blob_ref(record.digest_blob_ref, blob_count, record_index)?;
        let metadata_value_ref = encode_value_ref(
            record.metadata_value.as_ref(),
            values,
            record_index,
            "schema metadata",
        )?;

        UVar(flags).encode_into(&mut bytes);
        UVar(uri_ref).encode_into(&mut bytes);
        UVar(value_ref).encode_into(&mut bytes);
        UVar(digest_blob_ref).encode_into(&mut bytes);
        UVar(metadata_value_ref).encode_into(&mut bytes);
    }

    Ok(bytes)
}

fn normalize_flags(flags: u64, has_uri: bool, has_value: bool, has_digest: bool) -> u64 {
    let mut flags = flags;
    if has_uri {
        flags |= SCHEMA_FLAG_URI_PRESENT;
    } else {
        flags &= !SCHEMA_FLAG_URI_PRESENT;
    }
    if has_value {
        flags |= SCHEMA_FLAG_VALUE_PRESENT;
    } else {
        flags &= !SCHEMA_FLAG_VALUE_PRESENT;
    }
    if has_digest {
        flags |= SCHEMA_FLAG_DIGEST_PRESENT;
    } else {
        flags &= !SCHEMA_FLAG_DIGEST_PRESENT;
    }
    flags
}

fn decode_string_ref(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    record_index: usize,
    kind: &str,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let string_ref = encoded - 1;
    if string_ref >= strings.strings.len() as u64 {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} string reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(Some(string_ref))
}

fn encode_string_ref(
    value: Option<&str>,
    strings: &StringTable,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string was not present in STRS"))
                    .with_record_index(record_index),
            )
        })
        .and_then(|index| {
            u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| {
                    LybaError::invalid_section_table("schema string reference overflowed u64")
                })
        })
}

fn decode_value_ref(
    payload: &[u8],
    offset: &mut usize,
    values: Option<&[Value]>,
    record_index: usize,
    kind: &str,
) -> Result<Option<Value>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let value_ref = encoded - 1;
    let values = values.ok_or_else(|| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference required VALS"))
                .with_record_index(record_index),
        )
    })?;
    let value = values.get(value_ref as usize).ok_or_else(|| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        )
    })?;
    Ok(Some(value.clone()))
}

fn encode_value_ref(
    value: Option<&Value>,
    values: &[Value],
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    values
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} was not present in the encoded VALS table"))
                    .with_record_index(record_index),
            )
        })
        .and_then(|index| {
            u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| {
                    LybaError::invalid_section_table("schema value reference overflowed u64")
                })
        })
}

fn decode_blob_ref(
    payload: &[u8],
    offset: &mut usize,
    blob_count: usize,
    record_index: usize,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let blob_ref = encoded - 1;
    if blob_ref >= blob_count as u64 {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new("schema digest blob reference was out of range")
                .with_record_index(record_index),
        ));
    }
    Ok(Some(blob_ref))
}

fn encode_blob_ref(
    digest_blob_ref: Option<BlobId>,
    blob_count: usize,
    record_index: usize,
) -> Result<u64> {
    let Some(digest_blob_ref) = digest_blob_ref else {
        return Ok(0);
    };
    if digest_blob_ref.0 >= blob_count as u64 {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new("schema digest blob reference was out of range")
                .with_record_index(record_index),
        ));
    }
    digest_blob_ref.0.checked_add(1).ok_or_else(|| {
        LybaError::invalid_section_table("schema digest blob reference overflowed u64")
    })
}
