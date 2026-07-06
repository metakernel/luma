//! Level 1 string table (`STRS`) helpers.

use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::{UVar, read_bounded_bytes};
use std::collections::BTreeMap;

/// Producer claims the string is normalized to Unicode NFC.
pub const STRING_FLAG_NORMALIZED_NFC: u64 = 1 << 0;
/// String contains ASCII bytes only.
pub const STRING_FLAG_ASCII_ONLY: u64 = 1 << 1;
/// String belongs to a private extension.
pub const STRING_FLAG_PRIVATE: u64 = 1 << 2;
/// Reserved string flag bits.
pub const STRING_FLAG_RESERVED_MASK: u64 = !0x07;

/// One decoded `STRS` record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StringRecord {
    /// UTF-8 string value.
    pub value: String,
    /// Stored flags.
    pub flags: u64,
}

impl StringRecord {
    /// Creates a record and derives non-private flags where feasible.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            flags: computed_flags(&value),
            value,
        }
    }

    /// Creates a record with an explicit PRIVATE-bit policy.
    #[must_use]
    pub fn with_private(mut self, is_private: bool) -> Self {
        if is_private {
            self.flags |= STRING_FLAG_PRIVATE;
        } else {
            self.flags &= !STRING_FLAG_PRIVATE;
        }
        self
    }

    fn effective_flags(&self) -> Result<u64> {
        if self.flags & STRING_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(ErrorContext::new(
                "reserved string flag bits were non-zero",
            )));
        }

        Ok((self.flags & STRING_FLAG_PRIVATE) | computed_flags(&self.value))
    }
}

/// In-memory `STRS` table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StringTable {
    /// Ordered interned strings.
    pub strings: Vec<StringRecord>,
}

impl StringTable {
    /// Creates an empty string table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_string(mut self, record: StringRecord) -> Self {
        self.strings.push(record);
        self
    }

    /// Returns the record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Returns true when the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// Deterministic first-occurrence string interner.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringInterner {
    ids_by_value: BTreeMap<String, u64>,
    strings: Vec<StringRecord>,
}

impl StringInterner {
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a string and returns its stable zero-based ID.
    pub fn intern(&mut self, value: impl AsRef<str>) -> u64 {
        let value = value.as_ref();
        if let Some(id) = self.ids_by_value.get(value) {
            return *id;
        }

        let id = self.strings.len() as u64;
        let owned = value.to_owned();
        self.strings.push(StringRecord::new(owned.clone()));
        self.ids_by_value.insert(owned, id);
        id
    }

    /// Consumes the interner and returns the resulting table.
    #[must_use]
    pub fn into_table(self) -> StringTable {
        StringTable {
            strings: self.strings,
        }
    }
}

/// Computes flags derivable from the string contents.
#[must_use]
pub fn computed_flags(value: &str) -> u64 {
    if value.is_ascii() {
        STRING_FLAG_ASCII_ONLY | STRING_FLAG_NORMALIZED_NFC
    } else {
        0
    }
}

/// Decodes a `STRS` payload.
pub fn decode_string_table(payload: &[u8], limits: &Limits) -> Result<StringTable> {
    let mut offset = 0_usize;
    let string_count = UVar::decode(payload, &mut offset)?.0;
    let string_count = usize::try_from(string_count)
        .map_err(|_| LybaError::limit_exceeded("string count exceeds configured maximum"))?;
    if string_count > limits.max_string_count {
        return Err(LybaError::limit_exceeded(
            "string count exceeds configured maximum",
        ));
    }

    let mut strings = Vec::with_capacity(string_count);
    for record_index in 0..string_count {
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & STRING_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved string flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }

        let byte_length = UVar::decode(payload, &mut offset)?.0;
        let byte_length = usize::try_from(byte_length)
            .map_err(|_| LybaError::limit_exceeded("string length exceeds configured maximum"))?;
        if byte_length > limits.max_string_bytes {
            return Err(LybaError::limit_exceeded(
                "string length exceeds configured maximum",
            ));
        }

        let bytes_offset = offset;
        let bytes = read_bounded_bytes(payload, &mut offset, byte_length, byte_length)?;
        let value = core::str::from_utf8(bytes).map_err(|_| {
            LybaError::InvalidUtf8(
                ErrorContext::new("string bytes were not valid UTF-8")
                    .with_byte_offset(bytes_offset)
                    .with_record_index(record_index),
            )
        })?;
        strings.push(StringRecord {
            value: value.to_owned(),
            flags,
        });
    }

    if offset != payload.len() {
        return Err(LybaError::InvalidSectionTable(
            ErrorContext::new("string table payload had trailing bytes").with_byte_offset(offset),
        ));
    }

    Ok(StringTable { strings })
}

/// Encodes a `STRS` payload.
pub fn encode_string_table(table: &StringTable) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.strings.len() as u64).encode_into(&mut bytes);

    for record in &table.strings {
        let flags = record.effective_flags()?;
        UVar(flags).encode_into(&mut bytes);
        UVar(record.value.len() as u64).encode_into(&mut bytes);
        bytes.extend_from_slice(record.value.as_bytes());
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        STRING_FLAG_ASCII_ONLY, STRING_FLAG_NORMALIZED_NFC, STRING_FLAG_PRIVATE,
        STRING_FLAG_RESERVED_MASK, StringInterner, StringRecord, StringTable, computed_flags,
        decode_string_table, encode_string_table,
    };
    use crate::error::LybaError;
    use crate::policy::Limits;
    use crate::primitives::UVar;

    #[test]
    fn interning_uses_first_occurrence_ids_deterministically() {
        let mut interner = StringInterner::new();

        let alpha = interner.intern("alpha");
        let beta = interner.intern("beta");
        let alpha_again = interner.intern("alpha");

        let table = interner.into_table();
        assert_eq!(alpha, 0);
        assert_eq!(beta, 1);
        assert_eq!(alpha_again, 0);
        assert_eq!(table.strings.len(), 2);
        assert_eq!(table.strings[0].value, "alpha");
        assert_eq!(table.strings[1].value, "beta");
    }

    #[test]
    fn empty_string_table_round_trips() {
        let encoded = encode_string_table(&StringTable::new()).expect("empty table should encode");
        assert_eq!(encoded, vec![0]);

        let decoded =
            decode_string_table(&encoded, &Limits::public()).expect("empty table should decode");
        assert!(decoded.is_empty());
    }

    #[test]
    fn writer_derives_ascii_and_nfc_flags_for_ascii_strings() {
        let encoded = encode_string_table(
            &StringTable::new().with_string(StringRecord::new("plain-ascii").with_private(true)),
        )
        .expect("table should encode");
        let mut offset = 1;
        let flags = UVar::decode(&encoded, &mut offset)
            .expect("flags should decode")
            .0;

        assert_eq!(
            flags,
            STRING_FLAG_PRIVATE | STRING_FLAG_ASCII_ONLY | STRING_FLAG_NORMALIZED_NFC
        );
        assert_eq!(
            computed_flags("plain-ascii"),
            STRING_FLAG_ASCII_ONLY | STRING_FLAG_NORMALIZED_NFC
        );
        assert_eq!(computed_flags("é"), 0);
    }

    #[test]
    fn encoding_rejects_reserved_string_flags_with_lb0025() {
        let table = StringTable::new().with_string(StringRecord {
            value: "abc".to_owned(),
            flags: STRING_FLAG_RESERVED_MASK,
        });

        let error = encode_string_table(&table).expect_err("reserved string flags should fail");

        assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }
}
