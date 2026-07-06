//! Blob table types and Level 1 `BLOB` helpers.

use crate::{
    error::{ErrorContext, LybaError, Result},
    policy::Limits,
    primitives::{UVar, read_bounded_bytes, read_u64_le, write_u64_le},
};
use std::{ops::Range, sync::Arc};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BLOB_SECTION_NAME: &str = "BLOB";

/// Blob flag marking valid UTF-8 text.
pub const BLOB_FLAG_UTF8_TEXT: u64 = 1 << 0;
/// Blob flag marking original or generated source text.
pub const BLOB_FLAG_SOURCE_TEXT: u64 = 1 << 1;
/// Blob flag marking Lua source text.
pub const BLOB_FLAG_LUA_SOURCE: u64 = 1 << 2;
/// Blob flag marking tool-generated content.
pub const BLOB_FLAG_GENERATED: u64 = 1 << 3;
/// Blob flag marking external digest participation.
pub const BLOB_FLAG_EXTERNAL_DIGEST_TARGET: u64 = 1 << 4;
/// Blob flag marking private-extension ownership.
pub const BLOB_FLAG_PRIVATE: u64 = 1 << 5;
/// Mask of reserved blob flag bits that must remain zero.
pub const BLOB_FLAG_RESERVED_MASK: u64 = !0x3f;

/// Zero-based blob-table identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BlobId(pub u64);

#[derive(Debug, Clone)]
enum BlobStorage {
    Shared {
        backing: Arc<[u8]>,
        range: Range<usize>,
    },
    Owned(Arc<[u8]>),
}

impl BlobStorage {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Shared { backing, range } => &backing[range.start..range.end],
            Self::Owned(bytes) => bytes,
        }
    }

    #[cfg(test)]
    fn is_shared(&self) -> bool {
        matches!(self, Self::Shared { .. })
    }
}

/// One blob-table record.
#[derive(Debug, Clone)]
pub struct BlobRecord {
    flags: u64,
    storage: BlobStorage,
}

impl BlobRecord {
    /// Creates a blob record from owned bytes.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            flags: 0,
            storage: BlobStorage::Owned(Arc::<[u8]>::from(bytes.into())),
        }
    }

    /// Returns the raw blob flags.
    #[must_use]
    pub const fn flags(&self) -> u64 {
        self.flags
    }

    /// Returns the blob bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.storage.as_bytes()
    }

    /// Returns the blob length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Returns whether the blob is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets the blob flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Returns whether the blob is marked as UTF-8 text.
    #[must_use]
    pub const fn is_utf8_text(&self) -> bool {
        self.flags & BLOB_FLAG_UTF8_TEXT != 0
    }

    /// Returns whether the blob is marked as source text.
    #[must_use]
    pub const fn is_source_text(&self) -> bool {
        self.flags & BLOB_FLAG_SOURCE_TEXT != 0
    }

    /// Returns whether the blob is marked as Lua source text.
    #[must_use]
    pub const fn is_lua_source(&self) -> bool {
        self.flags & BLOB_FLAG_LUA_SOURCE != 0
    }

    /// Returns whether the blob is marked as generated.
    #[must_use]
    pub const fn is_generated(&self) -> bool {
        self.flags & BLOB_FLAG_GENERATED != 0
    }

    pub(crate) fn from_shared(backing: Arc<[u8]>, flags: u64, range: Range<usize>) -> Self {
        Self {
            flags,
            storage: BlobStorage::Shared { backing, range },
        }
    }

    #[cfg(test)]
    fn storage_is_shared(&self) -> bool {
        self.storage.is_shared()
    }
}

impl PartialEq for BlobRecord {
    fn eq(&self, other: &Self) -> bool {
        self.flags == other.flags && self.as_bytes() == other.as_bytes()
    }
}

impl Eq for BlobRecord {}

/// Blob table attached to a LYBA file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlobTable {
    /// Ordered blob records.
    pub records: Vec<BlobRecord>,
}

impl BlobTable {
    /// Creates an empty blob table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a blob record and returns its identifier.
    pub fn push(&mut self, record: BlobRecord) -> Result<BlobId> {
        let blob_id = u64::try_from(self.records.len())
            .map_err(|_| LybaError::limit_exceeded("blob count exceeds supported range"))?;
        self.records.push(record);
        Ok(BlobId(blob_id))
    }

    /// Returns the record for the given blob identifier.
    #[must_use]
    pub fn get(&self, blob_id: BlobId) -> Option<&BlobRecord> {
        usize::try_from(blob_id.0)
            .ok()
            .and_then(|index| self.records.get(index))
    }

    /// Returns the number of blob records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

pub(crate) fn decode_blob_table(payload: &[u8], limits: &Limits) -> Result<BlobTable> {
    let owned = Arc::<[u8]>::from(payload.to_vec());
    let mut offset = 0_usize;
    let blob_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| LybaError::limit_exceeded("blob count exceeds configured maximum"))?;
    if blob_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "table record count exceeds configured maximum",
        ));
    }

    let declared_offset_table_len = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| {
            LybaError::InvalidSectionTable(ErrorContext::new(
                "blob offset table length could not be represented on this platform",
            ))
        })?;
    let expected_offset_table_len = blob_count.checked_mul(8).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new("blob offset table length overflowed"))
    })?;
    if declared_offset_table_len != expected_offset_table_len {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(format!(
            "blob offset table byte length {declared_offset_table_len} did not match expected {expected_offset_table_len}",
        ))));
    }

    let records_start = offset
        .checked_add(declared_offset_table_len)
        .ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new("blob record start overflowed"))
        })?;
    if records_start > payload.len() {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(
            "blob offset table extended beyond payload",
        )));
    }

    let mut table_offset = offset;
    let mut record_offsets = Vec::with_capacity(blob_count);
    for record_index in 0..blob_count {
        let relative = read_u64_le(payload, &mut table_offset).map_err(|error| {
            let mut context = error.context().clone();
            context.record_index = Some(record_index);
            error.with_context(context)
        })?;
        let relative = usize::try_from(relative).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("blob record offset could not be represented on this platform")
                    .with_record_index(record_index),
            )
        })?;
        record_offsets.push(relative);
    }

    let records_len = payload.len() - records_start;
    let mut records = Vec::with_capacity(blob_count);
    for record_index in 0..blob_count {
        let record_offset = record_offsets[record_index];
        let next_offset = if record_index + 1 < blob_count {
            record_offsets[record_index + 1]
        } else {
            records_len
        };

        if record_offset > next_offset {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("blob record offsets were not in ascending order")
                    .with_record_index(record_index),
            ));
        }
        if next_offset > records_len {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("blob record extended beyond payload")
                    .with_record_index(record_index),
            ));
        }

        let record_start = records_start.checked_add(record_offset).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("blob record start overflowed").with_record_index(record_index),
            )
        })?;
        let record_end = records_start.checked_add(next_offset).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("blob record end overflowed").with_record_index(record_index),
            )
        })?;
        let record = payload.get(record_start..record_end).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("blob record bounds were outside the payload")
                    .with_record_index(record_index),
            )
        })?;

        let mut record_cursor = 0_usize;
        let flags_offset = record_cursor;
        let flags = UVar::decode(record, &mut record_cursor)?.0;
        if flags & BLOB_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("blob record reserved flags were non-zero")
                    .with_record_index(record_index)
                    .with_byte_offset(record_start + flags_offset),
            ));
        }
        let byte_length = usize::try_from(UVar::decode(record, &mut record_cursor)?.0)
            .map_err(|_| LybaError::limit_exceeded("blob length exceeds configured maximum"))?;
        let bytes_start = record_cursor;
        read_bounded_bytes(
            record,
            &mut record_cursor,
            byte_length,
            limits.max_decoded_logical_bytes,
        )?;
        if record_cursor != record.len() {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("blob record had trailing bytes").with_record_index(record_index),
            ));
        }

        records.push(BlobRecord::from_shared(
            owned.clone(),
            flags,
            (record_start + bytes_start)..(record_start + bytes_start + byte_length),
        ));
    }

    Ok(BlobTable { records })
}

pub(crate) fn encode_blob_table(blob_table: &BlobTable, limits: &Limits) -> Result<Vec<u8>> {
    if blob_table.records.len() > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "table record count exceeds configured maximum",
        ));
    }

    let table_len = blob_table.records.len().checked_mul(8).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new(
            "blob offset table length overflowed during encoding",
        ))
    })?;
    let mut bytes = Vec::new();
    UVar(blob_table.records.len() as u64).encode_into(&mut bytes);
    UVar(table_len as u64).encode_into(&mut bytes);

    let mut records = Vec::with_capacity(blob_table.records.len());
    let mut offsets = Vec::with_capacity(blob_table.records.len());
    let mut running_offset = 0_u64;
    for record in &blob_table.records {
        if record.flags & BLOB_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::invalid_reserved_flags(
                "blob record reserved flags were non-zero",
            ));
        }
        if record.len() > limits.max_decoded_logical_bytes {
            return Err(LybaError::limit_exceeded(
                "blob length exceeds configured maximum",
            ));
        }

        let mut encoded = Vec::new();
        UVar(record.flags).encode_into(&mut encoded);
        UVar(record.len() as u64).encode_into(&mut encoded);
        encoded.extend_from_slice(record.as_bytes());
        offsets.push(running_offset);
        running_offset = running_offset
            .checked_add(encoded.len() as u64)
            .ok_or_else(|| {
                LybaError::InvalidSectionTable(ErrorContext::new(
                    "encoded blob records overflowed u64 length",
                ))
            })?;
        records.push(encoded);
    }

    for offset in offsets {
        write_u64_le(&mut bytes, offset);
    }
    for record in records {
        bytes.extend_from_slice(&record);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        BLOB_FLAG_GENERATED, BLOB_FLAG_LUA_SOURCE, BLOB_FLAG_RESERVED_MASK, BLOB_FLAG_SOURCE_TEXT,
        BLOB_FLAG_UTF8_TEXT, BLOB_SECTION_NAME, BlobRecord, BlobTable, decode_blob_table,
        encode_blob_table,
    };
    use crate::{error::LybaError, policy::Limits};

    #[test]
    fn blob_constant_matches_section_name() {
        assert_eq!(BLOB_SECTION_NAME, "BLOB");
    }

    #[test]
    fn blob_round_trip_preserves_flags_and_bytes() {
        let table = BlobTable {
            records: vec![
                BlobRecord::new(b"plain".to_vec()),
                BlobRecord::new(b"print('x')".to_vec()).with_flags(
                    BLOB_FLAG_UTF8_TEXT
                        | BLOB_FLAG_SOURCE_TEXT
                        | BLOB_FLAG_LUA_SOURCE
                        | BLOB_FLAG_GENERATED,
                ),
            ],
        };

        let encoded =
            encode_blob_table(&table, &Limits::public()).expect("blob table should encode");
        let decoded =
            decode_blob_table(&encoded, &Limits::public()).expect("blob table should decode");

        assert_eq!(decoded, table);
        assert!(decoded.records[1].is_utf8_text());
        assert!(decoded.records[1].is_source_text());
        assert!(decoded.records[1].is_lua_source());
        assert!(decoded.records[1].is_generated());
    }

    #[test]
    fn blob_decode_rejects_reserved_flags_with_lb0025() {
        let payload = [1, 8, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0].to_vec();

        let error =
            decode_blob_table(&payload, &Limits::public()).expect_err("reserved flags should fail");

        assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }

    #[test]
    fn blob_records_keep_shared_backing_after_decode() {
        let table = BlobTable {
            records: vec![BlobRecord::new(vec![7; 256])],
        };
        let encoded =
            encode_blob_table(&table, &Limits::public()).expect("blob table should encode");
        let decoded =
            decode_blob_table(&encoded, &Limits::public()).expect("blob table should decode");

        assert!(decoded.records[0].storage_is_shared());
        assert_eq!(decoded.records[0].as_bytes(), vec![7; 256].as_slice());
    }

    #[test]
    fn blob_encode_rejects_reserved_flags() {
        let table = BlobTable {
            records: vec![BlobRecord::new(Vec::new()).with_flags(BLOB_FLAG_RESERVED_MASK)],
        };

        let error =
            encode_blob_table(&table, &Limits::public()).expect_err("reserved flags should fail");

        assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }
}
