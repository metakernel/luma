//! Bundle-, dependency-, and embedded-resource metadata helpers.

use crate::blob::{BlobId, BlobRecord, BlobTable};
use crate::error::{ErrorContext, LumbaError, Result};
use crate::policy::Limits;
use crate::primitives::{Identifier, UVar};
use crate::string_table::StringTable;
use crate::symbol::SymbolTable;
use crate::value::Value;

/// `DEPS`
pub const DEPENDENCY_SECTION_NAME: &str = "DEPS";
/// `EMBD`
pub const EMBEDDED_RESOURCE_SECTION_NAME: &str = "EMBD";

/// LUMA `@import` dependency.
pub const DEPENDENCY_KIND_IMPORT: u64 = 0;
/// LUMA `@include` dependency.
pub const DEPENDENCY_KIND_INCLUDE: u64 = 1;
/// Host module dependency.
pub const DEPENDENCY_KIND_MODULE: u64 = 2;
/// Schema dependency.
pub const DEPENDENCY_KIND_SCHEMA: u64 = 3;
/// Original source dependency.
pub const DEPENDENCY_KIND_SOURCE: u64 = 4;
/// Generated intermediate dependency.
pub const DEPENDENCY_KIND_GENERATED: u64 = 5;
/// External non-LUMA resource dependency.
pub const DEPENDENCY_KIND_EXTERNAL_RESOURCE: u64 = 6;
/// Extension-defined dependency.
pub const DEPENDENCY_KIND_EXTENSION: u64 = 7;

/// Embedded LUMA text source.
pub const EMBEDDED_RESOURCE_KIND_LUMA_TEXT: u64 = 0;
/// Embedded nested LUMBA container.
pub const EMBEDDED_RESOURCE_KIND_LUMBA_CONTAINER: u64 = 1;
/// Embedded schema written in LUMA text.
pub const EMBEDDED_RESOURCE_KIND_SCHEMA_LUMA: u64 = 2;
/// Embedded inert Lua source text.
pub const EMBEDDED_RESOURCE_KIND_LUA_SOURCE: u64 = 3;
/// Embedded opaque bytes.
pub const EMBEDDED_RESOURCE_KIND_BYTES: u64 = 4;
/// Embedded extension-defined resource kind.
pub const EMBEDDED_RESOURCE_KIND_EXTENSION: u64 = 5;

/// Reserved embedded-resource flag bits.
pub const EMBEDDED_RESOURCE_FLAG_RESERVED_MASK: u64 = !0;

/// Dependency is required.
pub const DEPENDENCY_FLAG_REQUIRED: u64 = 1 << 0;
/// Dependency payload is embedded in `EMBD`.
pub const DEPENDENCY_FLAG_EMBEDDED: u64 = 1 << 1;
/// Producer resolved the dependency.
pub const DEPENDENCY_FLAG_RESOLVED: u64 = 1 << 2;
/// Resolved digest is present.
pub const DEPENDENCY_FLAG_DIGEST_PRESENT: u64 = 1 << 3;
/// URI may require network resolution.
pub const DEPENDENCY_FLAG_NETWORK_URI: u64 = 1 << 4;
/// URI may refer to a filesystem path.
pub const DEPENDENCY_FLAG_FILE_URI: u64 = 1 << 5;
/// Dependency is a host module.
pub const DEPENDENCY_FLAG_HOST_MODULE: u64 = 1 << 6;
/// Dependency requires trusted policy.
pub const DEPENDENCY_FLAG_TRUSTED_ONLY: u64 = 1 << 7;
/// Reserved dependency flag bits.
pub const DEPENDENCY_FLAG_RESERVED_MASK: u64 = !0xff;

/// Lightweight description of a bundle embedded in or referenced by a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BundleDescriptor {
    /// Optional logical identifier for the bundle.
    pub id: Option<Identifier>,
    /// Optional media type for bundle contents.
    pub media_type: Option<String>,
}

impl BundleDescriptor {
    /// Creates an empty descriptor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// One decoded `DEPS` record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DependencyRecord {
    /// Stored dependency kind.
    pub kind: u64,
    /// Stored raw flags.
    pub flags: u64,
    /// Optional inert URI stored through `STRS`.
    pub uri: Option<String>,
    /// Optional alias stored through `SYMS`.
    pub alias: Option<Identifier>,
    /// Optional source span reference into `SRCS`.
    pub source_span_ref: Option<u64>,
    /// Optional resolved digest blob reference into `BLOB`.
    pub resolved_digest_blob_ref: Option<BlobId>,
    /// Optional metadata value stored through `VALS`.
    pub metadata_value: Option<Value>,
}

impl DependencyRecord {
    /// Creates an empty dependency record of the provided kind.
    #[must_use]
    pub fn new(kind: u64) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional inert URI.
    #[must_use]
    pub fn with_uri(mut self, uri: Option<String>) -> Self {
        self.uri = uri;
        self
    }

    /// Sets an optional alias.
    #[must_use]
    pub fn with_alias(mut self, alias: Option<Identifier>) -> Self {
        self.alias = alias;
        self
    }

    /// Sets an optional source span reference.
    #[must_use]
    pub fn with_source_span_ref(mut self, source_span_ref: Option<u64>) -> Self {
        self.source_span_ref = source_span_ref;
        self
    }

    /// Sets an optional resolved digest blob reference.
    #[must_use]
    pub fn with_resolved_digest_blob_ref(
        mut self,
        resolved_digest_blob_ref: Option<BlobId>,
    ) -> Self {
        self.resolved_digest_blob_ref = resolved_digest_blob_ref;
        self
    }

    /// Sets an optional metadata value.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Returns whether the dependency is required.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_REQUIRED != 0
    }

    /// Returns whether the dependency is embedded.
    #[must_use]
    pub const fn is_embedded(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_EMBEDDED != 0
    }

    /// Returns whether the dependency was resolved by the producer.
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_RESOLVED != 0
    }

    /// Returns whether a digest is present.
    #[must_use]
    pub const fn has_digest(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_DIGEST_PRESENT != 0
    }

    /// Returns whether the URI may require network resolution.
    #[must_use]
    pub const fn is_network_uri(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_NETWORK_URI != 0
    }

    /// Returns whether the URI may refer to a filesystem path.
    #[must_use]
    pub const fn is_file_uri(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_FILE_URI != 0
    }

    /// Returns whether the dependency identifies a host module.
    #[must_use]
    pub const fn is_host_module(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_HOST_MODULE != 0
    }

    /// Returns whether the dependency requires trusted policy.
    #[must_use]
    pub const fn is_trusted_only(&self) -> bool {
        self.flags & DEPENDENCY_FLAG_TRUSTED_ONLY != 0
    }

    fn normalized_flags(&self) -> u64 {
        let mut flags = self.flags;
        if self.resolved_digest_blob_ref.is_some() {
            flags |= DEPENDENCY_FLAG_DIGEST_PRESENT;
        } else {
            flags &= !DEPENDENCY_FLAG_DIGEST_PRESENT;
        }
        flags
    }
}

/// In-memory `DEPS` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DependencyTable {
    /// Ordered dependency records.
    pub records: Vec<DependencyRecord>,
}

impl DependencyTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: DependencyRecord) -> Self {
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

/// One decoded `EMBD` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedResourceRecord {
    /// Zero-based dependency reference into `DEPS`.
    pub dependency_ref: u64,
    /// Stored resource kind.
    pub kind: u64,
    /// Stored raw flags.
    pub flags: u64,
    /// Zero-based payload blob reference into `BLOB`.
    pub blob_ref: BlobId,
    /// Optional extension-defined kind name stored through `SYMS`.
    pub extension_kind: Option<Identifier>,
}

impl EmbeddedResourceRecord {
    /// Creates a resource record tied to a dependency and blob.
    #[must_use]
    pub const fn new(dependency_ref: u64, kind: u64, blob_ref: BlobId) -> Self {
        Self {
            dependency_ref,
            kind,
            flags: 0,
            blob_ref,
            extension_kind: None,
        }
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an optional extension-defined kind name.
    #[must_use]
    pub fn with_extension_kind(mut self, extension_kind: Option<Identifier>) -> Self {
        self.extension_kind = extension_kind;
        self
    }

    /// Returns whether this resource is stored as text.
    #[must_use]
    pub const fn is_textual(&self) -> bool {
        matches!(
            self.kind,
            EMBEDDED_RESOURCE_KIND_LUMA_TEXT
                | EMBEDDED_RESOURCE_KIND_SCHEMA_LUMA
                | EMBEDDED_RESOURCE_KIND_LUA_SOURCE
        )
    }

    /// Returns whether this resource is inert Lua source text.
    #[must_use]
    pub const fn is_lua_source(&self) -> bool {
        self.kind == EMBEDDED_RESOURCE_KIND_LUA_SOURCE
    }

    /// Returns the referenced blob lazily.
    #[must_use]
    pub fn blob<'a>(&self, blob_table: &'a BlobTable) -> Option<&'a BlobRecord> {
        blob_table.get(self.blob_ref)
    }

    /// Returns UTF-8 text for textual resource kinds without evaluating it.
    pub fn utf8_text<'a>(&self, blob_table: &'a BlobTable) -> Result<Option<&'a str>> {
        if !self.is_textual() {
            return Ok(None);
        }
        let Some(blob) = self.blob(blob_table) else {
            return Ok(None);
        };
        core::str::from_utf8(blob.as_bytes())
            .map(Some)
            .map_err(|_| LumbaError::invalid_utf8("embedded resource text was not valid UTF-8"))
    }
}

/// In-memory `EMBD` table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmbeddedResourceTable {
    /// Ordered resource records.
    pub records: Vec<EmbeddedResourceRecord>,
}

impl EmbeddedResourceTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: EmbeddedResourceRecord) -> Self {
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

pub(crate) fn decode_dependency_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: Option<&[Value]>,
    span_count: usize,
    blob_count: usize,
) -> Result<DependencyTable> {
    let mut offset = 0_usize;
    let dependency_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
        .map_err(|_| LumbaError::limit_exceeded("dependency count exceeds configured maximum"))?;
    if dependency_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "dependency count exceeds configured maximum",
        ));
    }

    let mut records = Vec::with_capacity(dependency_count);
    for record_index in 0..dependency_count {
        let kind = UVar::decode(payload, &mut offset)?.0;
        if !is_known_dependency_kind(kind) {
            return Err(LumbaError::InvalidSectionTable(
                ErrorContext::new(format!("dependency kind {kind} was not recognized"))
                    .with_record_index(record_index),
            ));
        }
        let uri = decode_string_ref(
            payload,
            &mut offset,
            strings,
            record_index,
            "dependency URI",
        )?;
        let alias = decode_symbol_ref(payload, &mut offset, symbols, record_index)?;
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & DEPENDENCY_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved dependency flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let source_span_ref =
            decode_source_span_ref(payload, &mut offset, span_count, record_index)?;
        let resolved_digest_blob_ref = decode_blob_ref(
            payload,
            &mut offset,
            blob_count,
            record_index,
            "dependency digest",
        )?;
        let metadata_value = decode_value_ref(
            payload,
            &mut offset,
            values,
            record_index,
            "dependency metadata",
        )?;

        records.push(DependencyRecord {
            kind,
            flags: normalize_flags(flags, resolved_digest_blob_ref.is_some()),
            uri: uri.map(|string_id| strings.strings[string_id as usize].value.clone()),
            alias: alias.map(|symbol_id| {
                Identifier::new(
                    strings.strings[symbols.symbols[symbol_id as usize].string_id as usize]
                        .value
                        .clone(),
                )
            }),
            source_span_ref,
            resolved_digest_blob_ref: resolved_digest_blob_ref.map(BlobId),
            metadata_value,
        });
    }

    if offset != payload.len() {
        return Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("dependency table payload had trailing bytes")
                .with_byte_offset(offset),
        ));
    }

    Ok(DependencyTable { records })
}

pub(crate) fn encode_dependency_table(
    table: &DependencyTable,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: &[Value],
    span_count: usize,
    blob_count: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);

    for (record_index, record) in table.records.iter().enumerate() {
        if !is_known_dependency_kind(record.kind) {
            return Err(LumbaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "dependency kind {} was not recognized",
                    record.kind
                ))
                .with_record_index(record_index),
            ));
        }
        let flags = record.normalized_flags();
        if flags & DEPENDENCY_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved dependency flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }

        UVar(record.kind).encode_into(&mut bytes);
        UVar(encode_string_ref(
            record.uri.as_deref(),
            strings,
            record_index,
            "dependency URI",
        )?)
        .encode_into(&mut bytes);
        UVar(encode_symbol_ref(
            record.alias.as_ref(),
            strings,
            symbols,
            record_index,
        )?)
        .encode_into(&mut bytes);
        UVar(flags).encode_into(&mut bytes);
        UVar(encode_source_span_ref(
            record.source_span_ref,
            span_count,
            record_index,
        )?)
        .encode_into(&mut bytes);
        UVar(encode_blob_ref(
            record.resolved_digest_blob_ref,
            blob_count,
            record_index,
            "dependency digest",
        )?)
        .encode_into(&mut bytes);
        UVar(encode_value_ref(
            record.metadata_value.as_ref(),
            values,
            record_index,
            "dependency metadata",
        )?)
        .encode_into(&mut bytes);
    }

    Ok(bytes)
}

pub(crate) fn decode_embedded_resource_table(
    payload: &[u8],
    limits: &Limits,
    dependency_count: usize,
    blob_count: usize,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
) -> Result<EmbeddedResourceTable> {
    let mut offset = 0_usize;
    let resource_count = usize::try_from(UVar::decode(payload, &mut offset)?.0).map_err(|_| {
        LumbaError::limit_exceeded("embedded resource count exceeds configured maximum")
    })?;
    if resource_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "embedded resource count exceeds configured maximum",
        ));
    }

    let mut records = Vec::with_capacity(resource_count);
    for record_index in 0..resource_count {
        let dependency_ref = decode_required_ref(
            payload,
            &mut offset,
            dependency_count,
            record_index,
            "embedded resource dependency",
        )?;
        let kind = UVar::decode(payload, &mut offset)?.0;
        if !is_known_embedded_resource_kind(kind) {
            return Err(LumbaError::InvalidSectionTable(
                ErrorContext::new(format!("embedded resource kind {kind} was not recognized"))
                    .with_record_index(record_index),
            ));
        }
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & EMBEDDED_RESOURCE_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved embedded resource flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let blob_ref = BlobId(decode_required_ref(
            payload,
            &mut offset,
            blob_count,
            record_index,
            "embedded resource blob",
        )?);
        let extension_kind = decode_optional_symbol_string_ref(
            payload,
            &mut offset,
            strings,
            symbols,
            record_index,
            "embedded resource extension kind",
        )?;
        validate_embedded_resource_record(kind, extension_kind.as_ref(), record_index)?;
        records.push(EmbeddedResourceRecord {
            dependency_ref,
            kind,
            flags,
            blob_ref,
            extension_kind,
        });
    }

    if offset != payload.len() {
        return Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("embedded resource table payload had trailing bytes")
                .with_byte_offset(offset),
        ));
    }

    Ok(EmbeddedResourceTable { records })
}

pub(crate) fn encode_embedded_resource_table(
    table: &EmbeddedResourceTable,
    dependency_count: usize,
    blob_count: usize,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);

    for (record_index, record) in table.records.iter().enumerate() {
        if !is_known_embedded_resource_kind(record.kind) {
            return Err(LumbaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "embedded resource kind {} was not recognized",
                    record.kind
                ))
                .with_record_index(record_index),
            ));
        }
        if record.flags & EMBEDDED_RESOURCE_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved embedded resource flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        validate_embedded_resource_record(
            record.kind,
            record.extension_kind.as_ref(),
            record_index,
        )?;

        UVar(encode_required_ref(
            record.dependency_ref,
            dependency_count,
            record_index,
            "embedded resource dependency",
        )?)
        .encode_into(&mut bytes);
        UVar(record.kind).encode_into(&mut bytes);
        UVar(record.flags).encode_into(&mut bytes);
        UVar(encode_required_ref(
            record.blob_ref.0,
            blob_count,
            record_index,
            "embedded resource blob",
        )?)
        .encode_into(&mut bytes);
        UVar(encode_optional_symbol_string_ref(
            record.extension_kind.as_ref(),
            strings,
            symbols,
            record_index,
            "embedded resource extension kind",
        )?)
        .encode_into(&mut bytes);
    }

    Ok(bytes)
}

fn is_known_dependency_kind(kind: u64) -> bool {
    kind <= DEPENDENCY_KIND_EXTENSION
}

fn is_known_embedded_resource_kind(kind: u64) -> bool {
    kind <= EMBEDDED_RESOURCE_KIND_EXTENSION
}

fn validate_embedded_resource_record(
    kind: u64,
    extension_kind: Option<&Identifier>,
    record_index: usize,
) -> Result<()> {
    match (kind, extension_kind) {
        (EMBEDDED_RESOURCE_KIND_EXTENSION, None) => Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("extension embedded resources require an extension kind symbol")
                .with_record_index(record_index),
        )),
        (EMBEDDED_RESOURCE_KIND_EXTENSION, Some(_)) | (_, None) => Ok(()),
        (_, Some(_)) => Err(LumbaError::InvalidSectionTable(
            ErrorContext::new(
                "only extension embedded resources may carry an extension kind symbol",
            )
            .with_record_index(record_index),
        )),
    }
}

fn decode_required_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    if value >= count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn encode_required_ref(value: u64, count: usize, record_index: usize, kind: &str) -> Result<u64> {
    if value >= count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn normalize_flags(flags: u64, has_digest: bool) -> u64 {
    let mut flags = flags;
    if has_digest {
        flags |= DEPENDENCY_FLAG_DIGEST_PRESENT;
    } else {
        flags &= !DEPENDENCY_FLAG_DIGEST_PRESENT;
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
        return Err(LumbaError::InvalidValueReference(
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
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string was not present in STRS"))
                    .with_record_index(record_index),
            )
        })
        .and_then(|index| {
            u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| {
                    LumbaError::invalid_section_table("dependency string reference overflowed u64")
                })
        })
}

fn decode_symbol_ref(
    payload: &[u8],
    offset: &mut usize,
    symbols: &SymbolTable,
    record_index: usize,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let symbol_ref = encoded - 1;
    if symbol_ref >= symbols.symbols.len() as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new("dependency alias symbol reference was out of range")
                .with_record_index(record_index),
        ));
    }
    Ok(Some(symbol_ref))
}

fn encode_symbol_ref(
    alias: Option<&Identifier>,
    strings: &StringTable,
    symbols: &SymbolTable,
    record_index: usize,
) -> Result<u64> {
    let Some(alias) = alias else {
        return Ok(0);
    };
    let string_id = strings
        .strings
        .iter()
        .position(|record| record.value == alias.as_str())
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new("dependency alias string was not present in STRS")
                    .with_record_index(record_index),
            )
        })? as u64;
    symbols
        .symbols
        .iter()
        .position(|record| record.string_id == string_id)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new("dependency alias symbol was not present in SYMS")
                    .with_record_index(record_index),
            )
        })
        .and_then(|index| {
            u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| {
                    LumbaError::invalid_section_table(
                        "dependency alias symbol reference overflowed u64",
                    )
                })
        })
}

fn decode_optional_symbol_string_ref(
    payload: &[u8],
    offset: &mut usize,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    record_index: usize,
    kind: &str,
) -> Result<Option<Identifier>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let symbol_ref = encoded - 1;
    let strings = strings.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires STRS so symbol text can be resolved"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbols = symbols.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires SYMS so symbol references can be resolved"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbol = symbols.symbols.get(symbol_ref as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        )
    })?;
    let string = strings
        .strings
        .get(symbol.string_id as usize)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string reference was out of range"))
                    .with_record_index(record_index),
            )
        })?;
    Ok(Some(Identifier::new(string.value.clone())))
}

fn encode_optional_symbol_string_ref(
    value: Option<&Identifier>,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    let strings = strings.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires STRS so symbol text can be encoded"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbols = symbols.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires SYMS so symbol references can be encoded"
            ))
            .with_record_index(record_index),
        )
    })?;
    encode_symbol_ref(Some(value), strings, symbols, record_index)
}

fn decode_source_span_ref(
    payload: &[u8],
    offset: &mut usize,
    span_count: usize,
    record_index: usize,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let span_ref = encoded - 1;
    if span_ref >= span_count as u64 {
        return Err(LumbaError::InvalidSourceSpan(
            ErrorContext::new("dependency source span reference was out of range")
                .with_record_index(record_index),
        ));
    }
    Ok(Some(span_ref))
}

fn encode_source_span_ref(
    source_span_ref: Option<u64>,
    span_count: usize,
    record_index: usize,
) -> Result<u64> {
    let Some(source_span_ref) = source_span_ref else {
        return Ok(0);
    };
    if source_span_ref >= span_count as u64 {
        return Err(LumbaError::InvalidSourceSpan(
            ErrorContext::new("dependency source span reference was out of range")
                .with_record_index(record_index),
        ));
    }
    source_span_ref.checked_add(1).ok_or_else(|| {
        LumbaError::invalid_section_table("dependency source span reference overflowed u64")
    })
}

fn decode_blob_ref(
    payload: &[u8],
    offset: &mut usize,
    blob_count: usize,
    record_index: usize,
    kind: &str,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let blob_ref = encoded - 1;
    if blob_ref >= blob_count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(Some(blob_ref))
}

fn encode_blob_ref(
    blob_ref: Option<BlobId>,
    blob_count: usize,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(blob_ref) = blob_ref else {
        return Ok(0);
    };
    if blob_ref.0 >= blob_count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    blob_ref.0.checked_add(1).ok_or_else(|| {
        LumbaError::invalid_section_table("dependency blob reference overflowed u64")
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
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference required VALS"))
                .with_record_index(record_index),
        )
    })?;
    let value = values.get(value_ref as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
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
                    LumbaError::invalid_section_table("dependency value reference overflowed u64")
                })
        })
}
