//! Trivia (`TRIV`) helpers.

use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::{UVar, read_u64_le, write_u64_le};
use crate::source::SourceSpanTable;
use crate::string_table::StringTable;

/// `TRIV`
pub const TRIVIA_SECTION_NAME: &str = "TRIV";

/// Horizontal whitespace trivia.
pub const TRIVIA_KIND_WHITESPACE: u64 = 0;
/// Line-break trivia.
pub const TRIVIA_KIND_NEWLINE: u64 = 1;
/// Comment trivia.
pub const TRIVIA_KIND_COMMENT: u64 = 2;
/// Blank-line trivia.
pub const TRIVIA_KIND_BLANK_LINE: u64 = 3;
/// Indentation trivia.
pub const TRIVIA_KIND_INDENTATION: u64 = 4;
/// Punctuation or delimiter trivia not modeled elsewhere.
pub const TRIVIA_KIND_PUNCTUATION: u64 = 5;
/// Malformed source fragment preserved for tooling.
pub const TRIVIA_KIND_MALFORMED: u64 = 6;
/// Extension-defined trivia payload.
pub const TRIVIA_KIND_EXTENSION: u64 = 7;
/// Reserved/unknown trivia kinds.
pub const TRIVIA_KIND_RESERVED_MASK: u64 = !0x07;

/// No core trivia flags are standardized in version 0.1.
pub const TRIVIA_FLAG_RESERVED_MASK: u64 = !0;

/// One decoded `TRIV` record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TriviaRecord {
    /// Stored trivia kind.
    pub kind: u64,
    /// Stored trivia flags.
    pub flags: u64,
    /// Optional source span reference into `SRCS`.
    pub span_ref: Option<u64>,
    /// Preserved source text resolved through `STRS`.
    pub text: String,
}

impl TriviaRecord {
    /// Creates a trivia record with required kind and text.
    #[must_use]
    pub fn new(kind: u64, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            ..Self::default()
        }
    }

    /// Sets stored raw flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional source span reference.
    #[must_use]
    pub fn with_span_ref(mut self, span_ref: Option<u64>) -> Self {
        self.span_ref = span_ref;
        self
    }
}

/// In-memory `TRIV` table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TriviaTable {
    /// Ordered trivia records.
    pub records: Vec<TriviaRecord>,
}

impl TriviaTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: TriviaRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Returns the record count.
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

pub(crate) fn decode_trivia_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    spans: Option<&SourceSpanTable>,
) -> Result<TriviaTable> {
    let mut offset = 0_usize;
    let trivia_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| LybaError::limit_exceeded("trivia count exceeds configured maximum"))?;
    if trivia_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "trivia count exceeds configured maximum",
        ));
    }

    let records_offset = offset
        .checked_add(trivia_count.checked_mul(8).ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new(
                "TRIV offset table length overflowed",
            ))
        })?)
        .ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new("TRIV offset table end overflowed"))
        })?;
    if records_offset > payload.len() {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(
            "TRIV offset table extended beyond payload",
        )));
    }

    let mut record_offsets = Vec::with_capacity(trivia_count);
    for _ in 0..trivia_count {
        record_offsets.push(read_u64_le(payload, &mut offset)?);
    }

    let record_bytes = &payload[records_offset..];
    let mut previous_offset = 0_u64;
    let mut records = Vec::with_capacity(trivia_count);
    let mut previous_key = None;
    for (record_index, start) in record_offsets.iter().copied().enumerate() {
        if start < previous_offset {
            return Err(LybaError::NonCanonicalEncoding(
                ErrorContext::new("TRIV record offsets were not in ascending order")
                    .with_record_index(record_index),
            ));
        }
        previous_offset = start;
        let end = record_offsets
            .get(record_index + 1)
            .copied()
            .unwrap_or(record_bytes.len() as u64);
        if start > end || end > record_bytes.len() as u64 {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("TRIV record offset range was out of bounds")
                    .with_record_index(record_index),
            ));
        }

        let start = usize::try_from(start).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("TRIV record offset exceeded platform limits")
                    .with_record_index(record_index),
            )
        })?;
        let end = usize::try_from(end).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("TRIV record end exceeded platform limits")
                    .with_record_index(record_index),
            )
        })?;
        let record_payload = &record_bytes[start..end];
        let mut record_offset = 0_usize;

        let kind_offset = record_offset;
        let kind = UVar::decode(record_payload, &mut record_offset)?.0;
        validate_trivia_kind(kind, record_index, kind_offset)?;

        let flags_offset = record_offset;
        let flags = UVar::decode(record_payload, &mut record_offset)?.0;
        if flags & TRIVIA_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved TRIV flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }

        let span_ref =
            decode_optional_span_ref(record_payload, &mut record_offset, spans, record_index)?;
        let text = decode_string_text(record_payload, &mut record_offset, strings, record_index)?;

        if record_offset != record_payload.len() {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("TRIV record had trailing bytes").with_record_index(record_index),
            ));
        }

        if let (Some(spans), Some(span_ref)) = (spans, span_ref) {
            let key = source_order_key(spans, span_ref, record_index)?;
            if let Some(previous) = previous_key {
                if key < previous {
                    return Err(LybaError::InvalidSourceSpan(
                        ErrorContext::new("TRIV records were not stored in source order")
                            .with_record_index(record_index),
                    ));
                }
            }
            previous_key = Some(key);
        }

        records.push(TriviaRecord {
            kind,
            flags,
            span_ref,
            text,
        });
    }

    Ok(TriviaTable { records })
}

pub(crate) fn encode_trivia_table(
    table: &TriviaTable,
    limits: &Limits,
    strings: &StringTable,
    spans: Option<&SourceSpanTable>,
) -> Result<Vec<u8>> {
    if table.records.len() > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "trivia count exceeds configured maximum",
        ));
    }

    let mut offsets = Vec::with_capacity(table.records.len());
    let mut record_bytes = Vec::new();
    let mut previous_key = None;
    for (record_index, record) in table.records.iter().enumerate() {
        validate_trivia_kind(record.kind, record_index, 0)?;
        if record.flags & TRIVIA_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved TRIV flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        if let (Some(spans), Some(span_ref)) = (spans, record.span_ref) {
            let key = source_order_key(spans, span_ref, record_index)?;
            if let Some(previous) = previous_key {
                if key < previous {
                    return Err(LybaError::InvalidSourceSpan(
                        ErrorContext::new("TRIV records must be encoded in source order")
                            .with_record_index(record_index),
                    ));
                }
            }
            previous_key = Some(key);
        }

        offsets.push(record_bytes.len() as u64);
        UVar(record.kind).encode_into(&mut record_bytes);
        UVar(record.flags).encode_into(&mut record_bytes);
        UVar(encode_optional_span_ref(
            record.span_ref,
            spans,
            record_index,
        )?)
        .encode_into(&mut record_bytes);
        UVar(find_string_id(strings, &record.text, record_index)? as u64)
            .encode_into(&mut record_bytes);
    }

    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);
    for offset in offsets {
        write_u64_le(&mut bytes, offset);
    }
    bytes.extend_from_slice(&record_bytes);
    Ok(bytes)
}

fn validate_trivia_kind(kind: u64, record_index: usize, byte_offset: usize) -> Result<()> {
    if kind & TRIVIA_KIND_RESERVED_MASK != 0 || kind > TRIVIA_KIND_EXTENSION {
        return Err(LybaError::InvalidReservedFlags(
            ErrorContext::new("reserved TRIV kind was used")
                .with_byte_offset(byte_offset)
                .with_record_index(record_index),
        ));
    }
    Ok(())
}

fn decode_optional_span_ref(
    payload: &[u8],
    offset: &mut usize,
    spans: Option<&SourceSpanTable>,
    record_index: usize,
) -> Result<Option<u64>> {
    let raw = UVar::decode(payload, offset)?.0;
    if raw == 0 {
        return Ok(None);
    }
    let span_ref = raw - 1;
    validate_span_ref(span_ref, spans, record_index)?;
    Ok(Some(span_ref))
}

fn encode_optional_span_ref(
    span_ref: Option<u64>,
    spans: Option<&SourceSpanTable>,
    record_index: usize,
) -> Result<u64> {
    let Some(span_ref) = span_ref else {
        return Ok(0);
    };
    validate_span_ref(span_ref, spans, record_index)?;
    span_ref.checked_add(1).ok_or_else(|| {
        LybaError::InvalidSectionTable(
            ErrorContext::new("TRIV optional span reference overflowed")
                .with_record_index(record_index),
        )
    })
}

fn validate_span_ref(
    span_ref: u64,
    spans: Option<&SourceSpanTable>,
    record_index: usize,
) -> Result<()> {
    let Some(spans) = spans else {
        return Err(LybaError::InvalidSourceSpan(
            ErrorContext::new("TRIV span reference required SRCS").with_record_index(record_index),
        ));
    };
    let index = usize::try_from(span_ref).map_err(|_| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new("TRIV span reference exceeded platform limits")
                .with_record_index(record_index),
        )
    })?;
    if index >= spans.records.len() {
        return Err(LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "TRIV span reference {span_ref} was out of range for SRCS count {}",
                spans.records.len()
            ))
            .with_record_index(record_index),
        ));
    }
    Ok(())
}

fn decode_string_text(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    record_index: usize,
) -> Result<String> {
    let string_id = usize::try_from(UVar::decode(payload, offset)?.0).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new("TRIV string reference exceeded platform limits")
                .with_record_index(record_index),
        )
    })?;
    strings
        .strings
        .get(string_id)
        .map(|record| record.value.clone())
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!(
                    "TRIV string reference {string_id} was out of range"
                ))
                .with_record_index(record_index),
            )
        })
}

fn find_string_id(strings: &StringTable, value: &str, record_index: usize) -> Result<usize> {
    strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new("TRIV string was not present in STRS")
                    .with_record_index(record_index),
            )
        })
}

fn source_order_key(
    spans: &SourceSpanTable,
    span_ref: u64,
    record_index: usize,
) -> Result<(u64, u64, u64, u64, u64, u64, u64)> {
    let index = usize::try_from(span_ref).map_err(|_| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new("TRIV span reference exceeded platform limits")
                .with_record_index(record_index),
        )
    })?;
    let span = spans.records.get(index).ok_or_else(|| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new("TRIV span reference was out of range")
                .with_record_index(record_index),
        )
    })?;
    Ok((
        span.source_file_ref,
        span.byte_offset,
        span.byte_length,
        span.start_line,
        span.start_column,
        span.end_line,
        span.end_column,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        TRIVIA_KIND_COMMENT, TRIVIA_KIND_RESERVED_MASK, TriviaRecord, TriviaTable,
        decode_trivia_table, encode_trivia_table,
    };
    use crate::policy::Limits;
    use crate::source::{SourceSpanRecord, SourceSpanTable};
    use crate::string_table::{StringRecord, StringTable};

    #[test]
    fn trivia_table_round_trips() {
        let strings = StringTable::new().with_string(StringRecord::new("-- note"));
        let spans = SourceSpanTable::new().with_record(SourceSpanRecord::new(0, 1, 7));
        let table = TriviaTable::new()
            .with_record(TriviaRecord::new(TRIVIA_KIND_COMMENT, "-- note").with_span_ref(Some(0)));

        let encoded = encode_trivia_table(&table, &Limits::public(), &strings, Some(&spans))
            .expect("TRIV should encode");
        let decoded = decode_trivia_table(&encoded, &Limits::public(), &strings, Some(&spans))
            .expect("TRIV should decode");

        assert_eq!(decoded, table);
    }

    #[test]
    fn trivia_table_rejects_reserved_kind_with_lb0025() {
        let strings = StringTable::new().with_string(StringRecord::new("bad"));
        let table =
            TriviaTable::new().with_record(TriviaRecord::new(TRIVIA_KIND_RESERVED_MASK, "bad"));

        let error = encode_trivia_table(&table, &Limits::public(), &strings, None)
            .expect_err("reserved kind should fail");

        assert_eq!(error.code().as_str(), "LB0025");
    }
}
