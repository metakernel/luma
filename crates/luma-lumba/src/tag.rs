//! Tag registry (`TAGS`) helpers.

use crate::error::{ErrorContext, LumbaError, Result};
use crate::policy::Limits;
use crate::primitives::{Identifier, UVar};
use crate::string_table::StringTable;
use crate::symbol::{SYMBOL_FLAG_TAG, SymbolTable};
use crate::value::Value;

/// `TAGS`
pub const TAG_SECTION_NAME: &str = "TAGS";

/// Producer knew this tag.
pub const TAG_FLAG_KNOWN_TO_PRODUCER: u64 = 1 << 0;
/// Schema reference is present.
pub const TAG_FLAG_HAS_SCHEMA: u64 = 1 << 1;
/// Evaluating the tag would require a host resolver.
pub const TAG_FLAG_REQUIRES_RESOLVER: u64 = 1 << 2;
/// Tag has portable semantics.
pub const TAG_FLAG_PORTABLE: u64 = 1 << 3;
/// Resolver requires trusted policy.
pub const TAG_FLAG_TRUSTED_ONLY: u64 = 1 << 4;
/// Reserved tag flag bits.
pub const TAG_FLAG_RESERVED_MASK: u64 = !0x1f;

/// One decoded `TAGS` declaration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagDeclaration {
    /// Logical tag identifier.
    pub tag: Identifier,
    /// Stable URI for the tag definition.
    pub uri: String,
    /// Stored flags.
    pub flags: u64,
    /// Optional schema reference into `SCMA`.
    pub schema_ref: Option<u64>,
    /// Optional inert resolver hint carried via `VALS`.
    pub resolver_hint: Option<Value>,
    /// Optional metadata value carried via `VALS`.
    pub metadata_value: Option<Value>,
}

impl TagDeclaration {
    /// Creates a new declaration.
    #[must_use]
    pub fn new(tag: impl Into<Identifier>, uri: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            uri: uri.into(),
            ..Self::default()
        }
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional schema reference.
    #[must_use]
    pub fn with_schema_ref(mut self, schema_ref: Option<u64>) -> Self {
        self.schema_ref = schema_ref;
        self
    }

    /// Sets an optional resolver hint.
    #[must_use]
    pub fn with_resolver_hint(mut self, resolver_hint: Option<Value>) -> Self {
        self.resolver_hint = resolver_hint;
        self
    }

    /// Sets an optional metadata value.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_known_to_producer(mut self, enabled: bool) -> Self {
        self.set_flag(TAG_FLAG_KNOWN_TO_PRODUCER, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_requires_resolver(mut self, enabled: bool) -> Self {
        self.set_flag(TAG_FLAG_REQUIRES_RESOLVER, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_portable(mut self, enabled: bool) -> Self {
        self.set_flag(TAG_FLAG_PORTABLE, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_trusted_only(mut self, enabled: bool) -> Self {
        self.set_flag(TAG_FLAG_TRUSTED_ONLY, enabled);
        self
    }

    /// Returns whether the declaration is known to the producer.
    #[must_use]
    pub const fn is_known_to_producer(&self) -> bool {
        self.flags & TAG_FLAG_KNOWN_TO_PRODUCER != 0
    }

    /// Returns whether the declaration carries a schema reference.
    #[must_use]
    pub const fn has_schema(&self) -> bool {
        self.flags & TAG_FLAG_HAS_SCHEMA != 0
    }

    /// Returns whether the declaration requires a resolver.
    #[must_use]
    pub const fn requires_resolver(&self) -> bool {
        self.flags & TAG_FLAG_REQUIRES_RESOLVER != 0
    }

    /// Returns whether the declaration is portable.
    #[must_use]
    pub const fn is_portable(&self) -> bool {
        self.flags & TAG_FLAG_PORTABLE != 0
    }

    /// Returns whether the declaration is trusted-only.
    #[must_use]
    pub const fn is_trusted_only(&self) -> bool {
        self.flags & TAG_FLAG_TRUSTED_ONLY != 0
    }

    fn set_flag(&mut self, flag: u64, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    fn normalized_flags(&self) -> u64 {
        let mut flags = self.flags;
        if self.schema_ref.is_some() {
            flags |= TAG_FLAG_HAS_SCHEMA;
        } else {
            flags &= !TAG_FLAG_HAS_SCHEMA;
        }
        flags
    }
}

/// In-memory `TAGS` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagTable {
    /// Ordered tag declarations.
    pub declarations: Vec<TagDeclaration>,
}

impl TagTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a declaration.
    #[must_use]
    pub fn with_declaration(mut self, declaration: TagDeclaration) -> Self {
        self.declarations.push(declaration);
        self
    }

    /// Returns the declaration count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    /// Returns true when no declarations are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }
}

pub(crate) fn decode_tag_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: Option<&[Value]>,
    schema_count: usize,
) -> Result<TagTable> {
    let mut offset = 0_usize;
    let tag_count = UVar::decode(payload, &mut offset)?.0;
    let tag_count = usize::try_from(tag_count)
        .map_err(|_| LumbaError::limit_exceeded("tag count exceeds configured maximum"))?;
    if tag_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "tag count exceeds configured maximum",
        ));
    }

    let mut declarations = Vec::with_capacity(tag_count);
    for record_index in 0..tag_count {
        let tag_symbol_id = decode_symbol_ref(payload, &mut offset, symbols, record_index)?;
        let tag = resolve_tag_identifier(strings, symbols, tag_symbol_id, record_index)?;
        let uri_string_id = decode_string_ref(payload, &mut offset, strings, record_index, "URI")?;
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & TAG_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved tag flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let schema_ref = decode_schema_ref(payload, &mut offset, schema_count, record_index)?;
        let resolver_hint =
            decode_value_ref(payload, &mut offset, values, record_index, "resolver hint")?;
        let metadata_value =
            decode_value_ref(payload, &mut offset, values, record_index, "metadata")?;

        declarations.push(TagDeclaration {
            tag,
            uri: strings.strings[uri_string_id as usize].value.clone(),
            flags: if schema_ref.is_some() {
                flags | TAG_FLAG_HAS_SCHEMA
            } else {
                flags & !TAG_FLAG_HAS_SCHEMA
            },
            schema_ref,
            resolver_hint,
            metadata_value,
        });
    }

    if offset != payload.len() {
        return Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("tag table payload had trailing bytes").with_byte_offset(offset),
        ));
    }

    Ok(TagTable { declarations })
}

pub(crate) fn encode_tag_table(
    table: &TagTable,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: &[Value],
    schema_count: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.declarations.len() as u64).encode_into(&mut bytes);

    for (record_index, declaration) in table.declarations.iter().enumerate() {
        let flags = declaration.normalized_flags();
        if flags & TAG_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved tag flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }

        let tag_symbol_id = find_tag_symbol_id(strings, symbols, declaration.tag.as_str())
            .ok_or_else(|| {
                LumbaError::InvalidValueReference(
                    ErrorContext::new("tag symbol was not present in SYMS as a tag symbol")
                        .with_record_index(record_index),
                )
            })?;
        let uri_string_id = find_string_id(strings, &declaration.uri).ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new("tag URI string was not present in STRS")
                    .with_record_index(record_index),
            )
        })?;
        let schema_ref = encode_schema_ref(declaration.schema_ref, schema_count, record_index)?;
        let resolver_hint = encode_value_ref(
            declaration.resolver_hint.as_ref(),
            values,
            record_index,
            "tag resolver hint",
        )?;
        let metadata_value = encode_value_ref(
            declaration.metadata_value.as_ref(),
            values,
            record_index,
            "tag metadata value",
        )?;

        UVar(tag_symbol_id).encode_into(&mut bytes);
        UVar(uri_string_id).encode_into(&mut bytes);
        UVar(flags).encode_into(&mut bytes);
        UVar(schema_ref).encode_into(&mut bytes);
        UVar(resolver_hint).encode_into(&mut bytes);
        UVar(metadata_value).encode_into(&mut bytes);
    }

    Ok(bytes)
}

fn resolve_tag_identifier(
    strings: &StringTable,
    symbols: &SymbolTable,
    tag_symbol_id: u64,
    record_index: usize,
) -> Result<Identifier> {
    let record = symbols.symbols.get(tag_symbol_id as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new("tag symbol reference was out of range")
                .with_record_index(record_index),
        )
    })?;
    if record.flags & SYMBOL_FLAG_TAG == 0 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new("tag symbol reference did not point to a tag symbol")
                .with_record_index(record_index),
        ));
    }
    let tag = strings
        .strings
        .get(record.string_id as usize)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new("tag symbol string reference was out of range")
                    .with_record_index(record_index),
            )
        })?
        .value
        .clone();
    Ok(Identifier::new(tag))
}

fn find_tag_symbol_id(strings: &StringTable, symbols: &SymbolTable, tag: &str) -> Option<u64> {
    symbols
        .symbols
        .iter()
        .enumerate()
        .find_map(|(index, record)| {
            ((record.flags & SYMBOL_FLAG_TAG != 0)
                && strings
                    .strings
                    .get(record.string_id as usize)
                    .is_some_and(|string| string.value == tag))
            .then_some(index as u64)
        })
}

fn find_string_id(strings: &StringTable, value: &str) -> Option<u64> {
    strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .and_then(|index| u64::try_from(index).ok())
}

fn decode_string_ref(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    if value >= strings.strings.len() as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("tag {kind} string reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn decode_symbol_ref(
    payload: &[u8],
    offset: &mut usize,
    symbols: &SymbolTable,
    record_index: usize,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    if value >= symbols.symbols.len() as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new("tag symbol reference was out of range")
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn decode_schema_ref(
    payload: &[u8],
    offset: &mut usize,
    schema_count: usize,
    record_index: usize,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let schema_ref = encoded - 1;
    if schema_ref >= schema_count as u64 {
        return Err(LumbaError::InvalidSyntaxNodeReference(
            ErrorContext::new("tag schema reference was out of range")
                .with_record_index(record_index),
        ));
    }
    Ok(Some(schema_ref))
}

fn encode_schema_ref(
    schema_ref: Option<u64>,
    schema_count: usize,
    record_index: usize,
) -> Result<u64> {
    let Some(schema_ref) = schema_ref else {
        return Ok(0);
    };
    if schema_ref >= schema_count as u64 {
        return Err(LumbaError::InvalidSyntaxNodeReference(
            ErrorContext::new("tag schema reference was out of range")
                .with_record_index(record_index),
        ));
    }
    schema_ref
        .checked_add(1)
        .ok_or_else(|| LumbaError::invalid_section_table("tag schema reference overflowed u64"))
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
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("tag {kind} reference required VALS"))
                .with_record_index(record_index),
        )
    })?;
    let value = values.get(value_ref as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("tag {kind} reference was out of range"))
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
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} was not present in the encoded VALS table"))
                    .with_record_index(record_index),
            )
        })
        .and_then(|index| {
            u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| {
                    LumbaError::invalid_section_table("tag value reference overflowed u64")
                })
        })
}
