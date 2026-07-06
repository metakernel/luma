//! Syntax node (`ASTN`) helpers.

use crate::blob::BlobId;
use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::{SVar, UVar, read_u64_le, write_u64_le};
use crate::source::SourceSpanTable;
use crate::string_table::StringTable;
use crate::symbol::{SYMBOL_FLAG_NODE_KIND, SymbolTable};

/// `ASTN`
pub const SYNTAX_NODE_SECTION_NAME: &str = "ASTN";

/// No core node flags are standardized in version 0.1.
pub const SYNTAX_NODE_FLAG_RESERVED_MASK: u64 = !0;

/// Field kind: absent.
pub const SYNTAX_FIELD_KIND_ABSENT: u64 = 0;
/// Field kind: bool.
pub const SYNTAX_FIELD_KIND_BOOL: u64 = 1;
/// Field kind: unsigned varint.
pub const SYNTAX_FIELD_KIND_UVAR: u64 = 2;
/// Field kind: signed varint.
pub const SYNTAX_FIELD_KIND_SVAR: u64 = 3;
/// Field kind: `STRS` string reference.
pub const SYNTAX_FIELD_KIND_STRING: u64 = 4;
/// Field kind: `SYMS` symbol reference.
pub const SYNTAX_FIELD_KIND_SYMBOL: u64 = 5;
/// Field kind: `VALS` value reference.
pub const SYNTAX_FIELD_KIND_VALUE_REF: u64 = 6;
/// Field kind: `ASTN` node reference.
pub const SYNTAX_FIELD_KIND_NODE_REF: u64 = 7;
/// Field kind: list of `ASTN` node references.
pub const SYNTAX_FIELD_KIND_NODE_LIST: u64 = 8;
/// Field kind: `SRCS` span reference.
pub const SYNTAX_FIELD_KIND_SPAN_REF: u64 = 9;
/// Field kind: `BLOB` reference.
pub const SYNTAX_FIELD_KIND_BLOB_REF: u64 = 10;
/// Field kind: token text through `STRS`.
pub const SYNTAX_FIELD_KIND_TOKEN_TEXT: u64 = 11;
/// Field kind: opaque extension payload.
pub const SYNTAX_FIELD_KIND_EXTENSION: u64 = 12;
/// Reserved/unknown field kinds.
pub const SYNTAX_FIELD_KIND_RESERVED_MASK: u64 = !0x0f;

/// Byte span within an input or output document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Start byte offset, inclusive.
    pub start: usize,
    /// End byte offset, exclusive.
    pub end: usize,
}

impl Span {
    /// Creates a new span.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// One flexible ASTN field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxField {
    /// Field name symbol text.
    pub name: String,
    /// Field payload.
    pub value: SyntaxFieldValue,
}

impl SyntaxField {
    /// Creates a field.
    #[must_use]
    pub fn new(name: impl Into<String>, value: SyntaxFieldValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// Flexible ASTN field payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyntaxFieldValue {
    /// No value.
    Absent,
    /// Boolean payload.
    Bool(bool),
    /// Unsigned integer payload.
    UVar(u64),
    /// Signed integer payload.
    SVar(i64),
    /// String payload resolved through `STRS`.
    String(String),
    /// Symbol payload resolved through `SYMS`.
    Symbol(String),
    /// `VALS` reference.
    ValueRef(u64),
    /// `ASTN` reference.
    NodeRef(u64),
    /// `ASTN` node list.
    NodeList(Vec<u64>),
    /// `SRCS` span reference.
    SpanRef(u64),
    /// `BLOB` reference.
    BlobRef(BlobId),
    /// Token text resolved through `STRS`.
    TokenText(String),
    /// Opaque extension payload.
    Extension(Vec<u8>),
}

impl SyntaxFieldValue {
    /// Returns the binary field-kind discriminant.
    #[must_use]
    pub const fn kind(&self) -> u64 {
        match self {
            Self::Absent => SYNTAX_FIELD_KIND_ABSENT,
            Self::Bool(_) => SYNTAX_FIELD_KIND_BOOL,
            Self::UVar(_) => SYNTAX_FIELD_KIND_UVAR,
            Self::SVar(_) => SYNTAX_FIELD_KIND_SVAR,
            Self::String(_) => SYNTAX_FIELD_KIND_STRING,
            Self::Symbol(_) => SYNTAX_FIELD_KIND_SYMBOL,
            Self::ValueRef(_) => SYNTAX_FIELD_KIND_VALUE_REF,
            Self::NodeRef(_) => SYNTAX_FIELD_KIND_NODE_REF,
            Self::NodeList(_) => SYNTAX_FIELD_KIND_NODE_LIST,
            Self::SpanRef(_) => SYNTAX_FIELD_KIND_SPAN_REF,
            Self::BlobRef(_) => SYNTAX_FIELD_KIND_BLOB_REF,
            Self::TokenText(_) => SYNTAX_FIELD_KIND_TOKEN_TEXT,
            Self::Extension(_) => SYNTAX_FIELD_KIND_EXTENSION,
        }
    }
}

/// One syntax node record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SyntaxNodeRecord {
    /// Node kind symbol text.
    pub kind: String,
    /// Stored node flags.
    pub flags: u64,
    /// Optional primary `SRCS` span reference.
    pub primary_span_ref: Option<u64>,
    /// Optional leading `TRIV` reference.
    pub leading_trivia_ref: Option<u64>,
    /// Optional trailing `TRIV` reference.
    pub trailing_trivia_ref: Option<u64>,
    /// Ordered flexible fields.
    pub fields: Vec<SyntaxField>,
}

impl SyntaxNodeRecord {
    /// Creates an empty record for a kind symbol text.
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            ..Self::default()
        }
    }

    /// Sets flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets the primary span reference.
    #[must_use]
    pub fn with_primary_span_ref(mut self, primary_span_ref: Option<u64>) -> Self {
        self.primary_span_ref = primary_span_ref;
        self
    }

    /// Sets the leading trivia reference.
    #[must_use]
    pub fn with_leading_trivia_ref(mut self, leading_trivia_ref: Option<u64>) -> Self {
        self.leading_trivia_ref = leading_trivia_ref;
        self
    }

    /// Sets the trailing trivia reference.
    #[must_use]
    pub fn with_trailing_trivia_ref(mut self, trailing_trivia_ref: Option<u64>) -> Self {
        self.trailing_trivia_ref = trailing_trivia_ref;
        self
    }

    /// Appends a field.
    #[must_use]
    pub fn with_field(mut self, field: SyntaxField) -> Self {
        self.fields.push(field);
        self
    }
}

/// In-memory `ASTN` table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SyntaxNodeTable {
    /// Ordered syntax node records.
    pub records: Vec<SyntaxNodeRecord>,
}

impl SyntaxNodeTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: SyntaxNodeRecord) -> Self {
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

    /// Builds a best-effort ASTN table from a parsed syntax tree.
    #[must_use]
    pub fn from_lyma_file(
        file: &lyma_syntax::LymaFile,
        source_spans: Option<&SourceSpanTable>,
    ) -> Self {
        let mut builder = SyntaxTableBuilder::new(source_spans);
        builder.push_file(file, None);
        builder.finish()
    }
}

pub(crate) fn decode_syntax_node_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    value_count: usize,
    span_count: usize,
    blob_count: usize,
    trivia_count: usize,
) -> Result<SyntaxNodeTable> {
    let mut offset = 0_usize;
    let node_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| LybaError::limit_exceeded("syntax node count exceeds configured maximum"))?;
    if node_count > limits.max_table_record_count || node_count > limits.max_syntax_node_count {
        return Err(LybaError::limit_exceeded(
            "syntax node count exceeds configured maximum",
        ));
    }

    let records_offset = offset
        .checked_add(node_count.checked_mul(8).ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new(
                "ASTN offset table length overflowed",
            ))
        })?)
        .ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new("ASTN offset table end overflowed"))
        })?;
    if records_offset > payload.len() {
        return Err(LybaError::InvalidSectionTable(ErrorContext::new(
            "ASTN offset table extended beyond payload",
        )));
    }

    let mut node_offsets = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        node_offsets.push(read_u64_le(payload, &mut offset)?);
    }

    let record_bytes = &payload[records_offset..];
    let mut previous = 0_u64;
    let mut records = Vec::with_capacity(node_count);
    for (record_index, start) in node_offsets.iter().copied().enumerate() {
        if start < previous {
            return Err(LybaError::NonCanonicalEncoding(
                ErrorContext::new("ASTN node offsets were not in ascending order")
                    .with_record_index(record_index),
            ));
        }
        previous = start;
        let end = node_offsets
            .get(record_index + 1)
            .copied()
            .unwrap_or(record_bytes.len() as u64);
        if start > end || end > record_bytes.len() as u64 {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN node offset range was out of bounds")
                    .with_record_index(record_index),
            ));
        }
        let start = usize::try_from(start).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN node offset exceeded platform limits")
                    .with_record_index(record_index),
            )
        })?;
        let end = usize::try_from(end).map_err(|_| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN node end offset exceeded platform limits")
                    .with_record_index(record_index),
            )
        })?;
        let mut node_offset = 0_usize;
        let node_payload = &record_bytes[start..end];
        let kind = decode_symbol_text(
            node_payload,
            &mut node_offset,
            strings,
            symbols,
            record_index,
            "node kind",
        )?;
        let flags_offset = node_offset;
        let flags = UVar::decode(node_payload, &mut node_offset)?.0;
        if flags & SYNTAX_NODE_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved ASTN node flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let primary_span_ref = decode_optional_span_ref(
            node_payload,
            &mut node_offset,
            span_count,
            record_index,
            "primary",
        )?;
        let leading_trivia_ref = decode_optional_node_like_ref(
            node_payload,
            &mut node_offset,
            trivia_count,
            record_index,
            "leading trivia",
        )?;
        let trailing_trivia_ref = decode_optional_node_like_ref(
            node_payload,
            &mut node_offset,
            trivia_count,
            record_index,
            "trailing trivia",
        )?;
        let field_count = usize::try_from(UVar::decode(node_payload, &mut node_offset)?.0)
            .map_err(|_| {
                LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN field count exceeded platform limits")
                        .with_record_index(record_index),
                )
            })?;
        let mut fields = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            let name = decode_symbol_text(
                node_payload,
                &mut node_offset,
                strings,
                symbols,
                record_index,
                "field name",
            )?;
            let kind_offset = node_offset;
            let field_kind = UVar::decode(node_payload, &mut node_offset)?.0;
            let value = decode_field_value(
                node_payload,
                &mut node_offset,
                strings,
                symbols,
                value_count,
                span_count,
                blob_count,
                node_count,
                record_index,
                field_kind,
                kind_offset,
            )?;
            fields.push(SyntaxField { name, value });
        }
        if node_offset != node_payload.len() {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN node record had trailing bytes")
                    .with_record_index(record_index),
            ));
        }
        records.push(SyntaxNodeRecord {
            kind,
            flags,
            primary_span_ref,
            leading_trivia_ref,
            trailing_trivia_ref,
            fields,
        });
    }

    Ok(SyntaxNodeTable { records })
}

pub(crate) fn encode_syntax_node_table(
    table: &SyntaxNodeTable,
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    value_count: usize,
    span_count: usize,
    blob_count: usize,
    trivia_count: usize,
) -> Result<Vec<u8>> {
    if table.records.len() > limits.max_table_record_count
        || table.records.len() > limits.max_syntax_node_count
    {
        return Err(LybaError::limit_exceeded(
            "syntax node count exceeds configured maximum",
        ));
    }

    let mut offsets = Vec::with_capacity(table.records.len());
    let mut record_bytes = Vec::new();
    for (record_index, record) in table.records.iter().enumerate() {
        if record.flags & SYNTAX_NODE_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved ASTN node flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        offsets.push(record_bytes.len() as u64);
        UVar(find_symbol_id(
            strings,
            symbols,
            &record.kind,
            record_index,
            "node kind",
            Some(SYMBOL_FLAG_NODE_KIND),
        )? as u64)
        .encode_into(&mut record_bytes);
        UVar(record.flags).encode_into(&mut record_bytes);
        UVar(encode_optional_span_ref(
            record.primary_span_ref,
            span_count,
            record_index,
            "primary",
        )?)
        .encode_into(&mut record_bytes);
        UVar(encode_optional_node_like_ref(
            record.leading_trivia_ref,
            trivia_count,
            record_index,
            "leading trivia",
        )?)
        .encode_into(&mut record_bytes);
        UVar(encode_optional_node_like_ref(
            record.trailing_trivia_ref,
            trivia_count,
            record_index,
            "trailing trivia",
        )?)
        .encode_into(&mut record_bytes);
        UVar(record.fields.len() as u64).encode_into(&mut record_bytes);
        for field in &record.fields {
            UVar(find_symbol_id(
                strings,
                symbols,
                &field.name,
                record_index,
                "field name",
                None,
            )? as u64)
            .encode_into(&mut record_bytes);
            UVar(field.value.kind()).encode_into(&mut record_bytes);
            encode_field_value(
                &mut record_bytes,
                field,
                strings,
                symbols,
                value_count,
                span_count,
                blob_count,
                table.records.len(),
                record_index,
            )?;
        }
    }

    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);
    for offset in offsets {
        write_u64_le(&mut bytes, offset);
    }
    bytes.extend_from_slice(&record_bytes);
    Ok(bytes)
}

fn decode_field_value(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    symbols: &SymbolTable,
    value_count: usize,
    span_count: usize,
    blob_count: usize,
    node_count: usize,
    record_index: usize,
    field_kind: u64,
    kind_offset: usize,
) -> Result<SyntaxFieldValue> {
    Ok(match field_kind {
        SYNTAX_FIELD_KIND_ABSENT => SyntaxFieldValue::Absent,
        SYNTAX_FIELD_KIND_BOOL => match *payload.get(*offset).ok_or_else(|| {
            LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN bool field payload was truncated")
                    .with_record_index(record_index),
            )
        })? {
            0 => {
                *offset += 1;
                SyntaxFieldValue::Bool(false)
            }
            1 => {
                *offset += 1;
                SyntaxFieldValue::Bool(true)
            }
            _ => {
                return Err(LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN bool field payload must be 0 or 1")
                        .with_record_index(record_index),
                ));
            }
        },
        SYNTAX_FIELD_KIND_UVAR => SyntaxFieldValue::UVar(UVar::decode(payload, offset)?.0),
        SYNTAX_FIELD_KIND_SVAR => SyntaxFieldValue::SVar(SVar::decode(payload, offset)?.0),
        SYNTAX_FIELD_KIND_STRING => SyntaxFieldValue::String(decode_string_text(
            payload,
            offset,
            strings,
            record_index,
            "string field",
        )?),
        SYNTAX_FIELD_KIND_SYMBOL => SyntaxFieldValue::Symbol(decode_symbol_text(
            payload,
            offset,
            strings,
            symbols,
            record_index,
            "symbol field",
        )?),
        SYNTAX_FIELD_KIND_VALUE_REF => SyntaxFieldValue::ValueRef(decode_bounded_ref(
            payload,
            offset,
            value_count,
            record_index,
            "value",
        )?),
        SYNTAX_FIELD_KIND_NODE_REF => SyntaxFieldValue::NodeRef(decode_bounded_node_ref(
            payload,
            offset,
            node_count,
            record_index,
            "node",
        )?),
        SYNTAX_FIELD_KIND_NODE_LIST => {
            let count = usize::try_from(UVar::decode(payload, offset)?.0).map_err(|_| {
                LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN node list count exceeded platform limits")
                        .with_record_index(record_index),
                )
            })?;
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(decode_bounded_node_ref(
                    payload,
                    offset,
                    node_count,
                    record_index,
                    "node list",
                )?);
            }
            SyntaxFieldValue::NodeList(values)
        }
        SYNTAX_FIELD_KIND_SPAN_REF => SyntaxFieldValue::SpanRef(decode_bounded_span_ref(
            payload,
            offset,
            span_count,
            record_index,
            "field",
        )?),
        SYNTAX_FIELD_KIND_BLOB_REF => SyntaxFieldValue::BlobRef(BlobId(decode_bounded_ref(
            payload,
            offset,
            blob_count,
            record_index,
            "blob",
        )?)),
        SYNTAX_FIELD_KIND_TOKEN_TEXT => SyntaxFieldValue::TokenText(decode_string_text(
            payload,
            offset,
            strings,
            record_index,
            "token text",
        )?),
        SYNTAX_FIELD_KIND_EXTENSION => {
            let len = usize::try_from(UVar::decode(payload, offset)?.0).map_err(|_| {
                LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN extension field length exceeded platform limits")
                        .with_record_index(record_index),
                )
            })?;
            let end = offset.checked_add(len).ok_or_else(|| {
                LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN extension field length overflowed")
                        .with_record_index(record_index),
                )
            })?;
            let bytes = payload.get(*offset..end).ok_or_else(|| {
                LybaError::InvalidSectionTable(
                    ErrorContext::new("ASTN extension field payload was truncated")
                        .with_record_index(record_index),
                )
            })?;
            *offset = end;
            SyntaxFieldValue::Extension(bytes.to_vec())
        }
        _ => {
            if field_kind & SYNTAX_FIELD_KIND_RESERVED_MASK != 0
                || field_kind > SYNTAX_FIELD_KIND_EXTENSION
            {
                return Err(LybaError::InvalidReservedFlags(
                    ErrorContext::new("reserved ASTN field kind was used")
                        .with_byte_offset(kind_offset)
                        .with_record_index(record_index),
                ));
            }
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("ASTN field kind was not supported")
                    .with_record_index(record_index),
            ));
        }
    })
}

fn encode_field_value(
    bytes: &mut Vec<u8>,
    field: &SyntaxField,
    strings: &StringTable,
    symbols: &SymbolTable,
    value_count: usize,
    span_count: usize,
    blob_count: usize,
    node_count: usize,
    record_index: usize,
) -> Result<()> {
    match &field.value {
        SyntaxFieldValue::Absent => {}
        SyntaxFieldValue::Bool(value) => bytes.push(u8::from(*value)),
        SyntaxFieldValue::UVar(value) => UVar(*value).encode_into(bytes),
        SyntaxFieldValue::SVar(value) => SVar(*value).encode_into(bytes),
        SyntaxFieldValue::String(value) | SyntaxFieldValue::TokenText(value) => {
            UVar(find_string_id(strings, value, record_index, "string field")? as u64)
                .encode_into(bytes);
        }
        SyntaxFieldValue::Symbol(value) => {
            UVar(
                find_symbol_id(strings, symbols, value, record_index, "symbol field", None)? as u64,
            )
            .encode_into(bytes);
        }
        SyntaxFieldValue::ValueRef(value_ref) => {
            validate_ref(*value_ref, value_count, record_index, "value")?;
            UVar(*value_ref).encode_into(bytes);
        }
        SyntaxFieldValue::NodeRef(node_ref) => {
            validate_node_ref(*node_ref, node_count, record_index, "node")?;
            UVar(*node_ref).encode_into(bytes);
        }
        SyntaxFieldValue::NodeList(node_refs) => {
            UVar(node_refs.len() as u64).encode_into(bytes);
            for node_ref in node_refs {
                validate_node_ref(*node_ref, node_count, record_index, "node list")?;
                UVar(*node_ref).encode_into(bytes);
            }
        }
        SyntaxFieldValue::SpanRef(span_ref) => {
            validate_span_ref(*span_ref, span_count, record_index, "field")?;
            UVar(*span_ref).encode_into(bytes);
        }
        SyntaxFieldValue::BlobRef(blob_ref) => {
            validate_ref(blob_ref.0, blob_count, record_index, "blob")?;
            UVar(blob_ref.0).encode_into(bytes);
        }
        SyntaxFieldValue::Extension(payload) => {
            UVar(payload.len() as u64).encode_into(bytes);
            bytes.extend_from_slice(payload);
        }
    }
    Ok(())
}

fn decode_string_text(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    record_index: usize,
    field: &str,
) -> Result<String> {
    let string_id = usize::try_from(UVar::decode(payload, offset)?.0).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "ASTN {field} string reference exceeded platform limits"
            ))
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
                    "ASTN {field} string reference {string_id} was out of range"
                ))
                .with_record_index(record_index),
            )
        })
}

fn decode_symbol_text(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    symbols: &SymbolTable,
    record_index: usize,
    field: &str,
) -> Result<String> {
    let symbol_id = usize::try_from(UVar::decode(payload, offset)?.0).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "ASTN {field} symbol reference exceeded platform limits"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbol = symbols.symbols.get(symbol_id).ok_or_else(|| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "ASTN {field} symbol reference {symbol_id} was out of range"
            ))
            .with_record_index(record_index),
        )
    })?;
    let string_id = usize::try_from(symbol.string_id).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!(
                "ASTN {field} symbol string exceeded platform limits"
            ))
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
                    "ASTN {field} symbol string reference {string_id} was out of range"
                ))
                .with_record_index(record_index),
            )
        })
}

fn find_string_id(
    strings: &StringTable,
    value: &str,
    record_index: usize,
    field: &str,
) -> Result<usize> {
    strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("ASTN {field} string was not present in STRS"))
                    .with_record_index(record_index),
            )
        })
}

fn find_symbol_id(
    strings: &StringTable,
    symbols: &SymbolTable,
    value: &str,
    record_index: usize,
    field: &str,
    required_flags: Option<u64>,
) -> Result<usize> {
    let string_id = find_string_id(strings, value, record_index, field)? as u64;
    symbols
        .symbols
        .iter()
        .position(|record| {
            record.string_id == string_id
                && required_flags.is_none_or(|flags| record.flags & flags == flags)
        })
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("ASTN {field} symbol was not present in SYMS"))
                    .with_record_index(record_index),
            )
        })
}

fn decode_optional_span_ref(
    payload: &[u8],
    offset: &mut usize,
    span_count: usize,
    record_index: usize,
    field: &str,
) -> Result<Option<u64>> {
    let raw = UVar::decode(payload, offset)?.0;
    if raw == 0 {
        return Ok(None);
    }
    let span_ref = raw - 1;
    validate_span_ref(span_ref, span_count, record_index, field)?;
    Ok(Some(span_ref))
}

fn encode_optional_span_ref(
    span_ref: Option<u64>,
    span_count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let Some(span_ref) = span_ref else {
        return Ok(0);
    };
    validate_span_ref(span_ref, span_count, record_index, field)?;
    span_ref.checked_add(1).ok_or_else(|| {
        LybaError::InvalidSectionTable(
            ErrorContext::new("ASTN optional span reference overflowed")
                .with_record_index(record_index),
        )
    })
}

fn decode_optional_node_like_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<Option<u64>> {
    let raw = UVar::decode(payload, offset)?.0;
    if raw == 0 {
        return Ok(None);
    }
    let reference = raw - 1;
    validate_node_like_ref(reference, count, record_index, field)?;
    Ok(Some(reference))
}

fn encode_optional_node_like_ref(
    reference: Option<u64>,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let Some(reference) = reference else {
        return Ok(0);
    };
    validate_node_like_ref(reference, count, record_index, field)?;
    reference.checked_add(1).ok_or_else(|| {
        LybaError::InvalidSectionTable(
            ErrorContext::new("ASTN optional syntax reference overflowed")
                .with_record_index(record_index),
        )
    })
}

fn decode_bounded_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    validate_ref(value, count, record_index, field)?;
    Ok(value)
}

fn decode_bounded_node_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    validate_node_ref(value, count, record_index, field)?;
    Ok(value)
}

fn decode_bounded_span_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    validate_span_ref(value, count, record_index, field)?;
    Ok(value)
}

fn validate_ref(value: u64, count: usize, record_index: usize, field: &str) -> Result<()> {
    let index = usize::try_from(value).map_err(|_| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!("ASTN {field} reference exceeded platform limits"))
                .with_record_index(record_index),
        )
    })?;
    if index >= count {
        return Err(LybaError::InvalidValueReference(
            ErrorContext::new(format!("ASTN {field} reference {value} was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(())
}

fn validate_node_ref(value: u64, count: usize, record_index: usize, field: &str) -> Result<()> {
    let index = usize::try_from(value).map_err(|_| {
        LybaError::InvalidSyntaxNodeReference(
            ErrorContext::new(format!("ASTN {field} reference exceeded platform limits"))
                .with_record_index(record_index),
        )
    })?;
    if index >= count {
        return Err(LybaError::InvalidSyntaxNodeReference(
            ErrorContext::new(format!(
                "ASTN {field} reference {value} was out of range for ASTN count {count}"
            ))
            .with_record_index(record_index),
        ));
    }
    Ok(())
}

fn validate_node_like_ref(
    value: u64,
    count: usize,
    record_index: usize,
    field: &str,
) -> Result<()> {
    let index = usize::try_from(value).map_err(|_| {
        LybaError::InvalidSyntaxNodeReference(
            ErrorContext::new(format!("ASTN {field} reference exceeded platform limits"))
                .with_record_index(record_index),
        )
    })?;
    if index >= count {
        return Err(LybaError::InvalidSyntaxNodeReference(
            ErrorContext::new(format!("ASTN {field} reference {value} was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(())
}

fn validate_span_ref(value: u64, count: usize, record_index: usize, field: &str) -> Result<()> {
    let index = usize::try_from(value).map_err(|_| {
        LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "ASTN {field} span reference exceeded platform limits"
            ))
            .with_record_index(record_index),
        )
    })?;
    if index >= count {
        return Err(LybaError::InvalidSourceSpan(
            ErrorContext::new(format!(
                "ASTN {field} span reference {value} was out of range for SRCS count {count}"
            ))
            .with_record_index(record_index),
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct SyntaxTableBuilder<'a> {
    span_lookup: SpanLookup<'a>,
    pending: Vec<PendingNode>,
}

#[derive(Debug, Default)]
struct SpanLookup<'a> {
    source_spans: Option<&'a SourceSpanTable>,
}

impl<'a> SpanLookup<'a> {
    fn new(source_spans: Option<&'a SourceSpanTable>) -> Self {
        Self { source_spans }
    }

    fn span_ref(&self, span: lyma_syntax::Span) -> Option<u64> {
        let source_spans = self.source_spans?;
        source_spans
            .records
            .iter()
            .enumerate()
            .find_map(|(index, record)| {
                record
                    .to_lyma_syntax_span()
                    .filter(|candidate| *candidate == span)
                    .map(|_| index as u64)
            })
    }
}

#[derive(Debug, Default)]
struct PendingNode {
    kind: String,
    primary_span_ref: Option<u64>,
    fields: Vec<SyntaxField>,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl<'a> SyntaxTableBuilder<'a> {
    fn new(source_spans: Option<&'a SourceSpanTable>) -> Self {
        Self {
            span_lookup: SpanLookup::new(source_spans),
            pending: Vec::new(),
        }
    }

    fn finish(self) -> SyntaxNodeTable {
        SyntaxNodeTable {
            records: self
                .pending
                .into_iter()
                .enumerate()
                .map(|(_index, node)| SyntaxNodeRecord {
                    kind: node.kind,
                    flags: 0,
                    primary_span_ref: node.primary_span_ref,
                    leading_trivia_ref: None,
                    trailing_trivia_ref: None,
                    fields: std::iter::once(SyntaxField::new(
                        "parent",
                        node.parent
                            .map(|parent| SyntaxFieldValue::NodeRef(parent as u64))
                            .unwrap_or(SyntaxFieldValue::Absent),
                    ))
                    .chain(std::iter::once(SyntaxField::new(
                        "children",
                        SyntaxFieldValue::NodeList(
                            node.children
                                .into_iter()
                                .map(|child| child as u64)
                                .collect(),
                        ),
                    )))
                    .chain(node.fields)
                    .collect(),
                })
                .collect(),
        }
    }

    fn push_node(
        &mut self,
        kind: &str,
        span: Option<lyma_syntax::Span>,
        parent: Option<usize>,
        fields: Vec<SyntaxField>,
    ) -> usize {
        let id = self.pending.len();
        self.pending.push(PendingNode {
            kind: kind.to_owned(),
            primary_span_ref: span.and_then(|span| self.span_lookup.span_ref(span)),
            fields,
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            self.pending[parent].children.push(id);
        }
        id
    }

    fn push_file(&mut self, file: &lyma_syntax::LymaFile, parent: Option<usize>) -> usize {
        let id = self.push_node("file", Some(file.span), parent, Vec::new());
        for document in &file.documents {
            self.push_document(document, Some(id));
        }
        id
    }

    fn push_document(&mut self, document: &lyma_syntax::Document, parent: Option<usize>) -> usize {
        let mut fields = Vec::new();
        if let Some(span) = document
            .separator_span
            .and_then(|span| self.span_lookup.span_ref(span))
        {
            fields.push(SyntaxField::new(
                "separator_span",
                SyntaxFieldValue::SpanRef(span),
            ));
        }
        if let Some(span) = document
            .terminator_span
            .and_then(|span| self.span_lookup.span_ref(span))
        {
            fields.push(SyntaxField::new(
                "terminator_span",
                SyntaxFieldValue::SpanRef(span),
            ));
        }
        let id = self.push_node("document", Some(document.span), parent, fields);
        for item in &document.items {
            self.push_document_item(item, Some(id));
        }
        id
    }

    fn push_document_item(&mut self, item: &lyma_syntax::DocumentItem, parent: Option<usize>) {
        match item {
            lyma_syntax::DocumentItem::Directive(directive) => {
                self.push_directive(directive, parent);
            }
            lyma_syntax::DocumentItem::Let(binding) => {
                self.push_let_binding(binding, parent);
            }
            lyma_syntax::DocumentItem::Root(node) => {
                self.push_lyma_node(node, parent);
            }
            lyma_syntax::DocumentItem::Comment(comment) => {
                self.push_comment(comment, parent);
            }
        }
    }

    fn push_lyma_node(&mut self, node: &lyma_syntax::LymaNode, parent: Option<usize>) -> usize {
        match node {
            lyma_syntax::LymaNode::Null { span } => {
                self.push_node("null", Some(*span), parent, Vec::new())
            }
            lyma_syntax::LymaNode::Boolean { value, span } => self.push_node(
                "boolean",
                Some(*span),
                parent,
                vec![SyntaxField::new("value", SyntaxFieldValue::Bool(*value))],
            ),
            lyma_syntax::LymaNode::Number(number) => self.push_node(
                "number",
                Some(number.span),
                parent,
                vec![SyntaxField::new(
                    "text",
                    SyntaxFieldValue::TokenText(number.lexeme.clone()),
                )],
            ),
            lyma_syntax::LymaNode::String(string) => {
                let kind = match (string.style, string.block_kind) {
                    (lyma_syntax::StringStyle::Plain, None) => "plain_scalar",
                    (_, Some(_)) => "block_string",
                    _ => "quoted_scalar",
                };
                self.push_node(
                    kind,
                    Some(string.span),
                    parent,
                    vec![
                        SyntaxField::new(
                            "text",
                            SyntaxFieldValue::TokenText(string.source.clone()),
                        ),
                        SyntaxField::new("value", SyntaxFieldValue::String(string.value.clone())),
                    ],
                )
            }
            lyma_syntax::LymaNode::Sequence(sequence) => self.push_sequence(sequence, parent),
            lyma_syntax::LymaNode::Mapping(mapping) => self.push_mapping(mapping, parent),
            lyma_syntax::LymaNode::Tagged(tagged) => self.push_tagged(tagged, parent),
            lyma_syntax::LymaNode::LuaExpression(expression) => {
                self.push_lua_expression("lua_expression", expression, parent)
            }
            lyma_syntax::LymaNode::LuaExpressionBlock(expression) => {
                self.push_lua_expression("lua_expression_block", expression, parent)
            }
            lyma_syntax::LymaNode::LuaChunk(expression) => {
                self.push_lua_expression("lua_chunk", expression, parent)
            }
            lyma_syntax::LymaNode::LuaTableConstructor(expression) => {
                self.push_lua_expression("lua_table_constructor", expression, parent)
            }
        }
    }

    fn push_mapping(
        &mut self,
        mapping: &lyma_syntax::MappingBlock,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("mapping", Some(mapping.span), parent, Vec::new());
        for item in &mapping.items {
            match item {
                lyma_syntax::MappingItem::Entry(entry) => {
                    self.push_mapping_entry(entry, Some(id));
                }
                lyma_syntax::MappingItem::Spread(spread) => {
                    self.push_spread(spread, Some(id));
                }
                lyma_syntax::MappingItem::Directive(directive) => {
                    self.push_directive(directive, Some(id));
                }
                lyma_syntax::MappingItem::Conditional(block) => {
                    self.push_mapping_conditional(block, Some(id));
                }
                lyma_syntax::MappingItem::Loop(block) => {
                    self.push_mapping_loop(block, Some(id));
                }
                lyma_syntax::MappingItem::Let(binding) => {
                    self.push_let_binding(binding, Some(id));
                }
                lyma_syntax::MappingItem::Comment(comment) => {
                    self.push_comment(comment, Some(id));
                }
            }
        }
        id
    }

    fn push_sequence(
        &mut self,
        sequence: &lyma_syntax::SequenceBlock,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("sequence", Some(sequence.span), parent, Vec::new());
        for item in &sequence.items {
            match item {
                lyma_syntax::SequenceItem::Value(node) => {
                    self.push_lyma_node(node, Some(id));
                }
                lyma_syntax::SequenceItem::Spread(spread) => {
                    self.push_spread(spread, Some(id));
                }
                lyma_syntax::SequenceItem::Directive(directive) => {
                    self.push_directive(directive, Some(id));
                }
                lyma_syntax::SequenceItem::Conditional(block) => {
                    self.push_sequence_conditional(block, Some(id));
                }
                lyma_syntax::SequenceItem::Loop(block) => {
                    self.push_sequence_loop(block, Some(id));
                }
                lyma_syntax::SequenceItem::Comment(comment) => {
                    self.push_comment(comment, Some(id));
                }
            }
        }
        id
    }

    fn push_mapping_entry(
        &mut self,
        entry: &lyma_syntax::MappingEntry,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("map_entry", Some(entry.span), parent, Vec::new());
        self.push_mapping_key(&entry.key, Some(id));
        self.push_lyma_node(&entry.value, Some(id));
        id
    }

    fn push_mapping_key(&mut self, key: &lyma_syntax::MappingKey, parent: Option<usize>) -> usize {
        match key {
            lyma_syntax::MappingKey::Plain { value, span, .. } => self.push_node(
                "plain_key",
                Some(*span),
                parent,
                vec![SyntaxField::new(
                    "text",
                    SyntaxFieldValue::TokenText(value.clone()),
                )],
            ),
            lyma_syntax::MappingKey::Quoted(node) => self.push_node(
                "quoted_key",
                Some(node.span),
                parent,
                vec![SyntaxField::new(
                    "text",
                    SyntaxFieldValue::TokenText(node.source.clone()),
                )],
            ),
            lyma_syntax::MappingKey::Expression { expression, span } => {
                let id = self.push_node("expression_key", Some(*span), parent, Vec::new());
                self.push_lua_expression("lua_expression", expression, Some(id));
                id
            }
        }
    }

    fn push_tagged(&mut self, tagged: &lyma_syntax::TaggedNode, parent: Option<usize>) -> usize {
        let id = self.push_node("tagged_value", Some(tagged.span), parent, Vec::new());
        let tag_id = self.push_node("tag", Some(tagged.tag.span), Some(id), Vec::new());
        self.push_node(
            "tag_name",
            Some(tagged.tag.name.span),
            Some(tag_id),
            vec![SyntaxField::new(
                "text",
                SyntaxFieldValue::TokenText(tagged.tag.name.value.clone()),
            )],
        );
        if let Some(value) = &tagged.value {
            self.push_lyma_node(value, Some(id));
        }
        id
    }

    fn push_lua_expression(
        &mut self,
        kind: &str,
        expression: &lyma_syntax::LuaExpression,
        parent: Option<usize>,
    ) -> usize {
        self.push_node(
            kind,
            Some(expression.span),
            parent,
            vec![SyntaxField::new(
                "text",
                SyntaxFieldValue::TokenText(expression.source.clone()),
            )],
        )
    }

    fn push_spread(&mut self, spread: &lyma_syntax::SpreadEntry, parent: Option<usize>) -> usize {
        let id = self.push_node("spread_entry", Some(spread.span), parent, Vec::new());
        self.push_lua_expression("lua_expression", &spread.expression, Some(id));
        id
    }

    fn push_let_binding(
        &mut self,
        binding: &lyma_syntax::LetBinding,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node(
            "let_binding",
            Some(binding.span),
            parent,
            vec![SyntaxField::new(
                "name",
                SyntaxFieldValue::TokenText(binding.name.clone()),
            )],
        );
        self.push_lyma_node(&binding.value, Some(id));
        id
    }

    fn push_comment(&mut self, comment: &lyma_syntax::Comment, parent: Option<usize>) -> usize {
        self.push_node(
            "comment",
            Some(comment.span),
            parent,
            vec![SyntaxField::new(
                "text",
                SyntaxFieldValue::TokenText(comment.text.clone()),
            )],
        )
    }

    fn push_mapping_conditional(
        &mut self,
        conditional: &lyma_syntax::ConditionalBlock<lyma_syntax::MappingBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("if_block", Some(conditional.span), parent, Vec::new());
        self.push_mapping_branch("if_branch", &conditional.if_branch, Some(id));
        for branch in &conditional.else_if_branches {
            self.push_mapping_branch("else_if_branch", branch, Some(id));
        }
        if let Some(branch) = &conditional.else_branch {
            self.push_mapping_else_branch(branch, Some(id));
        }
        id
    }

    fn push_sequence_conditional(
        &mut self,
        conditional: &lyma_syntax::ConditionalBlock<lyma_syntax::SequenceBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("if_block", Some(conditional.span), parent, Vec::new());
        self.push_sequence_branch("if_branch", &conditional.if_branch, Some(id));
        for branch in &conditional.else_if_branches {
            self.push_sequence_branch("else_if_branch", branch, Some(id));
        }
        if let Some(branch) = &conditional.else_branch {
            self.push_sequence_else_branch(branch, Some(id));
        }
        id
    }

    fn push_mapping_branch(
        &mut self,
        kind: &str,
        branch: &lyma_syntax::ConditionalBranch<lyma_syntax::MappingBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node(kind, Some(branch.span), parent, Vec::new());
        self.push_lua_expression("lua_expression", &branch.condition, Some(id));
        self.push_mapping(&branch.body, Some(id));
        id
    }

    fn push_sequence_branch(
        &mut self,
        kind: &str,
        branch: &lyma_syntax::ConditionalBranch<lyma_syntax::SequenceBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node(kind, Some(branch.span), parent, Vec::new());
        self.push_lua_expression("lua_expression", &branch.condition, Some(id));
        self.push_sequence(&branch.body, Some(id));
        id
    }

    fn push_mapping_else_branch(
        &mut self,
        branch: &lyma_syntax::ElseBranch<lyma_syntax::MappingBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("else_block", Some(branch.span), parent, Vec::new());
        self.push_mapping(&branch.body, Some(id));
        id
    }

    fn push_sequence_else_branch(
        &mut self,
        branch: &lyma_syntax::ElseBranch<lyma_syntax::SequenceBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_node("else_block", Some(branch.span), parent, Vec::new());
        self.push_sequence(&branch.body, Some(id));
        id
    }

    fn push_mapping_loop(
        &mut self,
        block: &lyma_syntax::LoopBlock<lyma_syntax::MappingBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_loop_header(block, parent);
        self.push_mapping(&block.body, Some(id));
        id
    }

    fn push_sequence_loop(
        &mut self,
        block: &lyma_syntax::LoopBlock<lyma_syntax::SequenceBlock>,
        parent: Option<usize>,
    ) -> usize {
        let id = self.push_loop_header(block, parent);
        self.push_sequence(&block.body, Some(id));
        id
    }

    fn push_loop_header<T>(
        &mut self,
        block: &lyma_syntax::LoopBlock<T>,
        parent: Option<usize>,
    ) -> usize {
        let mut fields = Vec::new();
        match &block.bindings {
            lyma_syntax::LoopBindings::One { value, .. } => fields.push(SyntaxField::new(
                "binding",
                SyntaxFieldValue::TokenText(value.clone()),
            )),
            lyma_syntax::LoopBindings::Two { key, value, .. } => {
                fields.push(SyntaxField::new(
                    "key",
                    SyntaxFieldValue::TokenText(key.clone()),
                ));
                fields.push(SyntaxField::new(
                    "binding",
                    SyntaxFieldValue::TokenText(value.clone()),
                ));
            }
        }
        let id = self.push_node("for_block", Some(block.span), parent, fields);
        self.push_lua_expression("lua_expression", &block.iterable, Some(id));
        id
    }

    fn push_directive(
        &mut self,
        directive: &lyma_syntax::Directive,
        parent: Option<usize>,
    ) -> usize {
        match directive {
            lyma_syntax::Directive::Version(value) => self.push_node(
                "version_directive",
                Some(value.span),
                parent,
                vec![SyntaxField::new(
                    "version",
                    SyntaxFieldValue::TokenText(value.version.clone()),
                )],
            ),
            lyma_syntax::Directive::Profile(value) => self.push_node(
                "profile_directive",
                Some(value.span),
                parent,
                vec![SyntaxField::new(
                    "profile",
                    SyntaxFieldValue::TokenText(match &value.profile {
                        lyma_syntax::LymaProfile::Data => "data".to_owned(),
                        lyma_syntax::LymaProfile::Safe => "safe".to_owned(),
                        lyma_syntax::LymaProfile::Trusted => "trusted".to_owned(),
                        lyma_syntax::LymaProfile::Custom(value) => value.clone(),
                    }),
                )],
            ),
            lyma_syntax::Directive::Schema(value) => {
                let id = self.push_node("schema_directive", Some(value.span), parent, Vec::new());
                self.push_lyma_node(
                    &lyma_syntax::LymaNode::String(value.location.clone()),
                    Some(id),
                );
                id
            }
            lyma_syntax::Directive::Import(value) => {
                let id = self.push_node(
                    "import_directive",
                    Some(value.span),
                    parent,
                    vec![SyntaxField::new(
                        "alias",
                        SyntaxFieldValue::TokenText(value.alias.clone()),
                    )],
                );
                self.push_lyma_node(
                    &lyma_syntax::LymaNode::String(value.location.clone()),
                    Some(id),
                );
                id
            }
            lyma_syntax::Directive::Include(value) => {
                let id = self.push_node("include_directive", Some(value.span), parent, Vec::new());
                self.push_lyma_node(
                    &lyma_syntax::LymaNode::String(value.location.clone()),
                    Some(id),
                );
                id
            }
            lyma_syntax::Directive::Use(value) => self.push_node(
                "use_directive",
                Some(value.span),
                parent,
                vec![
                    SyntaxField::new("module", SyntaxFieldValue::TokenText(value.module.clone())),
                    SyntaxField::new("alias", SyntaxFieldValue::TokenText(value.alias.clone())),
                ],
            ),
            lyma_syntax::Directive::LuaPrelude(value) => {
                let id = self.push_node(
                    "lua_prelude_directive",
                    Some(value.span),
                    parent,
                    Vec::new(),
                );
                self.push_lua_expression("lua_expression", &value.block, Some(id));
                id
            }
            lyma_syntax::Directive::Meta(value) => {
                let id = self.push_node("meta_directive", Some(value.span), parent, Vec::new());
                self.push_mapping(&value.value, Some(id));
                id
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SYNTAX_FIELD_KIND_RESERVED_MASK, SYNTAX_NODE_FLAG_RESERVED_MASK, SyntaxField,
        SyntaxFieldValue, SyntaxNodeRecord, SyntaxNodeTable, decode_syntax_node_table,
        encode_syntax_node_table,
    };
    use crate::blob::BlobTable;
    use crate::policy::Limits;
    use crate::primitives::UVar;
    use crate::source::{SourceSpanRecord, SourceSpanTable};
    use crate::string_table::StringTable;
    use crate::symbol::{SymbolInterner, SymbolTable};
    use lyma_parser::parse_str;
    use lyma_syntax::FileId;

    fn tables_for_astn(table: &SyntaxNodeTable) -> (StringTable, SymbolTable) {
        let mut interner = SymbolInterner::new();
        for record in &table.records {
            let _ = interner.intern_node_kind(&record.kind, None);
            for field in &record.fields {
                let _ = interner.intern_symbol(&field.name, None, 0);
                match &field.value {
                    SyntaxFieldValue::String(value) | SyntaxFieldValue::TokenText(value) => {
                        interner.intern_string(value);
                    }
                    SyntaxFieldValue::Symbol(value) => {
                        let _ = interner.intern_symbol(value, None, 0);
                    }
                    _ => {}
                }
            }
        }
        interner.into_tables()
    }

    #[test]
    fn astn_round_trips_generic_records() {
        let table = SyntaxNodeTable::new()
            .with_record(
                SyntaxNodeRecord::new("document")
                    .with_primary_span_ref(Some(0))
                    .with_field(SyntaxField::new("parent", SyntaxFieldValue::Absent))
                    .with_field(SyntaxField::new(
                        "children",
                        SyntaxFieldValue::NodeList(vec![1]),
                    )),
            )
            .with_record(
                SyntaxNodeRecord::new("plain_scalar")
                    .with_primary_span_ref(Some(0))
                    .with_field(SyntaxField::new("parent", SyntaxFieldValue::NodeRef(0)))
                    .with_field(SyntaxField::new(
                        "children",
                        SyntaxFieldValue::NodeList(vec![]),
                    ))
                    .with_field(SyntaxField::new(
                        "text",
                        SyntaxFieldValue::TokenText(String::from("doc")),
                    )),
            );
        let (strings, symbols) = tables_for_astn(&table);
        let encoded =
            encode_syntax_node_table(&table, &Limits::public(), &strings, &symbols, 0, 1, 0, 0)
                .expect("ASTN should encode");
        let decoded =
            decode_syntax_node_table(&encoded, &Limits::public(), &strings, &symbols, 0, 1, 0, 0)
                .expect("ASTN should decode");
        assert_eq!(decoded, table);
    }

    #[test]
    fn astn_builder_uses_preorder_and_maps_spans_when_available() {
        let source = "title: Hello\nitems:\n  - one\n";
        let parsed = parse_str(FileId(0), "fixture.lyma", source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let spans = SourceSpanTable::new()
            .with_record(SourceSpanRecord::new(0, 0, source.len() as u64))
            .with_record(SourceSpanRecord::new(0, 0, 12))
            .with_record(SourceSpanRecord::new(0, 0, 5))
            .with_record(SourceSpanRecord::new(0, 7, 5))
            .with_record(SourceSpanRecord::new(0, 13, 14))
            .with_record(SourceSpanRecord::new(0, 13, 5))
            .with_record(SourceSpanRecord::new(0, 20, 7))
            .with_record(SourceSpanRecord::new(0, 24, 3));
        let table = SyntaxNodeTable::from_lyma_file(&parsed.file, Some(&spans));
        assert_eq!(table.records[0].kind, "file");
        assert_eq!(table.records[1].kind, "document");
        assert_eq!(table.records[2].kind, "mapping");
        let child_field = &table.records[0].fields[1];
        assert_eq!(child_field.name, "children");
        assert_eq!(child_field.value, SyntaxFieldValue::NodeList(vec![1]));
    }

    #[test]
    fn astn_rejects_reserved_bits_with_lb0025() {
        let table = SyntaxNodeTable::new().with_record(
            SyntaxNodeRecord::new("document").with_flags(SYNTAX_NODE_FLAG_RESERVED_MASK),
        );
        let (strings, symbols) = tables_for_astn(&table);
        let error =
            encode_syntax_node_table(&table, &Limits::public(), &strings, &symbols, 0, 0, 0, 0)
                .expect_err("reserved node flags should fail");
        assert_eq!(error.code().as_str(), "LB0025");

        let mut payload = Vec::new();
        UVar(1).encode_into(&mut payload);
        crate::primitives::write_u64_le(&mut payload, 0);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(1).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(SYNTAX_FIELD_KIND_RESERVED_MASK).encode_into(&mut payload);
        let error =
            decode_syntax_node_table(&payload, &Limits::public(), &strings, &symbols, 0, 0, 0, 0)
                .expect_err("reserved field kind should fail");
        assert_eq!(error.code().as_str(), "LB0025");
        let _ = BlobTable::new();
    }
}
