//! Symbol table (`SYMS`) helpers.

use crate::error::{ErrorContext, LumbaError, Result};
use crate::policy::Limits;
use crate::primitives::UVar;
use crate::string_table::{StringInterner, StringTable};
use std::collections::BTreeMap;

/// Symbol marks a common map key.
pub const SYMBOL_FLAG_KEY: u64 = 1 << 0;
/// Symbol marks a tag name.
pub const SYMBOL_FLAG_TAG: u64 = 1 << 1;
/// Symbol marks a directive name.
pub const SYMBOL_FLAG_DIRECTIVE: u64 = 1 << 2;
/// Symbol marks a syntax node kind.
pub const SYMBOL_FLAG_NODE_KIND: u64 = 1 << 3;
/// Symbol marks a profile name.
pub const SYMBOL_FLAG_PROFILE: u64 = 1 << 4;
/// Symbol belongs to an extension namespace.
pub const SYMBOL_FLAG_EXTENSION: u64 = 1 << 5;
/// Reserved symbol flag bits.
pub const SYMBOL_FLAG_RESERVED_MASK: u64 = !0x3f;

/// One decoded `SYMS` record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SymbolRecord {
    /// Referenced zero-based `STRS` string ID.
    pub string_id: u64,
    /// Optional namespace `STRS` string ID.
    pub namespace_string_id: Option<u64>,
    /// Stored flags.
    pub flags: u64,
}

impl SymbolRecord {
    /// Creates a symbol record.
    #[must_use]
    pub const fn new(string_id: u64) -> Self {
        Self {
            string_id,
            namespace_string_id: None,
            flags: 0,
        }
    }

    /// Sets an optional namespace string reference.
    #[must_use]
    pub const fn with_namespace_string_id(mut self, namespace_string_id: Option<u64>) -> Self {
        self.namespace_string_id = namespace_string_id;
        self
    }

    /// Sets explicit flags.
    #[must_use]
    pub const fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    fn validate_flags(&self, record_index: usize) -> Result<()> {
        if self.flags & SYMBOL_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved symbol flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        Ok(())
    }
}

/// In-memory `SYMS` table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SymbolTable {
    /// Ordered symbol records.
    pub symbols: Vec<SymbolRecord>,
}

impl SymbolTable {
    /// Creates an empty symbol table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_symbol(mut self, record: SymbolRecord) -> Self {
        self.symbols.push(record);
        self
    }

    /// Returns the record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true when the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Deterministic string+symbol interner.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SymbolInterner {
    strings: StringInterner,
    ids_by_key: BTreeMap<(u64, Option<u64>), u64>,
    symbols: Vec<SymbolRecord>,
}

impl SymbolInterner {
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns an arbitrary symbol role.
    pub fn intern_symbol(
        &mut self,
        value: impl AsRef<str>,
        namespace: Option<&str>,
        flags: u64,
    ) -> Result<u64> {
        if flags & SYMBOL_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::invalid_reserved_flags(
                "reserved symbol flag bits were non-zero",
            ));
        }

        let string_id = self.strings.intern(value);
        let namespace_string_id = namespace.map(|value| self.strings.intern(value));
        let key = (string_id, namespace_string_id);
        if let Some(id) = self.ids_by_key.get(&key).copied() {
            self.symbols[id as usize].flags |= flags;
            return Ok(id);
        }

        let id = self.symbols.len() as u64;
        self.symbols.push(SymbolRecord {
            string_id,
            namespace_string_id,
            flags,
        });
        self.ids_by_key.insert(key, id);
        Ok(id)
    }

    /// Interns a common map key symbol.
    pub fn intern_key(&mut self, value: impl AsRef<str>) -> Result<u64> {
        self.intern_symbol(value, None, SYMBOL_FLAG_KEY)
    }

    /// Interns a tag symbol.
    pub fn intern_tag(&mut self, value: impl AsRef<str>, namespace: Option<&str>) -> Result<u64> {
        self.intern_symbol(value, namespace, SYMBOL_FLAG_TAG)
    }

    /// Interns a directive symbol.
    pub fn intern_directive(
        &mut self,
        value: impl AsRef<str>,
        namespace: Option<&str>,
    ) -> Result<u64> {
        self.intern_symbol(value, namespace, SYMBOL_FLAG_DIRECTIVE)
    }

    /// Interns a node-kind symbol.
    pub fn intern_node_kind(
        &mut self,
        value: impl AsRef<str>,
        namespace: Option<&str>,
    ) -> Result<u64> {
        self.intern_symbol(value, namespace, SYMBOL_FLAG_NODE_KIND)
    }

    /// Interns a profile symbol.
    pub fn intern_profile(&mut self, value: impl AsRef<str>) -> Result<u64> {
        self.intern_symbol(value, Some("luma"), SYMBOL_FLAG_PROFILE)
    }

    /// Interns an extension-owned symbol.
    pub fn intern_extension_symbol(
        &mut self,
        value: impl AsRef<str>,
        namespace: &str,
    ) -> Result<u64> {
        self.intern_symbol(value, Some(namespace), SYMBOL_FLAG_EXTENSION)
    }

    /// Interns a plain string without creating a symbol record.
    pub fn intern_string(&mut self, value: impl AsRef<str>) -> u64 {
        self.strings.intern(value)
    }

    /// Consumes the interner and returns the resulting tables.
    #[must_use]
    pub fn into_tables(self) -> (StringTable, SymbolTable) {
        (
            self.strings.into_table(),
            SymbolTable {
                symbols: self.symbols,
            },
        )
    }
}

/// Decodes a `SYMS` payload.
pub fn decode_symbol_table(
    payload: &[u8],
    limits: &Limits,
    string_count: usize,
) -> Result<SymbolTable> {
    let mut offset = 0_usize;
    let symbol_count = UVar::decode(payload, &mut offset)?.0;
    let symbol_count = usize::try_from(symbol_count)
        .map_err(|_| LumbaError::limit_exceeded("symbol count exceeds configured maximum"))?;
    if symbol_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "symbol count exceeds configured maximum",
        ));
    }

    let mut symbols = Vec::with_capacity(symbol_count);
    for record_index in 0..symbol_count {
        let string_id = UVar::decode(payload, &mut offset)?.0;
        validate_string_ref(
            string_id,
            string_count,
            record_index,
            "symbol string reference was out of range",
        )?;

        let namespace_encoded = UVar::decode(payload, &mut offset)?.0;
        let namespace_string_id = if namespace_encoded == 0 {
            None
        } else {
            let namespace_string_id = namespace_encoded - 1;
            validate_string_ref(
                namespace_string_id,
                string_count,
                record_index,
                "symbol namespace reference was out of range",
            )?;
            Some(namespace_string_id)
        };

        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & SYMBOL_FLAG_RESERVED_MASK != 0 {
            return Err(LumbaError::InvalidReservedFlags(
                ErrorContext::new("reserved symbol flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }

        symbols.push(SymbolRecord {
            string_id,
            namespace_string_id,
            flags,
        });
    }

    if offset != payload.len() {
        return Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("symbol table payload had trailing bytes").with_byte_offset(offset),
        ));
    }

    Ok(SymbolTable { symbols })
}

/// Encodes a `SYMS` payload.
pub fn encode_symbol_table(table: &SymbolTable) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.symbols.len() as u64).encode_into(&mut bytes);

    for (record_index, record) in table.symbols.iter().enumerate() {
        record.validate_flags(record_index)?;
        UVar(record.string_id).encode_into(&mut bytes);
        let namespace_encoded = record
            .namespace_string_id
            .map(|value| {
                value.checked_add(1).ok_or_else(|| {
                    LumbaError::invalid_section_table("symbol namespace reference overflowed")
                })
            })
            .transpose()?
            .unwrap_or(0);
        UVar(namespace_encoded).encode_into(&mut bytes);
        UVar(record.flags).encode_into(&mut bytes);
    }

    Ok(bytes)
}

fn validate_string_ref(
    string_id: u64,
    string_count: usize,
    record_index: usize,
    message: &'static str,
) -> Result<()> {
    if string_id >= string_count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(message).with_record_index(record_index),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        SYMBOL_FLAG_DIRECTIVE, SYMBOL_FLAG_EXTENSION, SYMBOL_FLAG_KEY, SYMBOL_FLAG_NODE_KIND,
        SYMBOL_FLAG_PROFILE, SYMBOL_FLAG_RESERVED_MASK, SYMBOL_FLAG_TAG, SymbolInterner,
        SymbolRecord, SymbolTable, decode_symbol_table, encode_symbol_table,
    };
    use crate::error::LumbaError;
    use crate::policy::Limits;

    #[test]
    fn interner_is_deterministic_and_merges_flags_per_name_and_namespace() {
        let mut interner = SymbolInterner::new();

        let key = interner.intern_key("name").expect("key should intern");
        let key_again = interner
            .intern_key("name")
            .expect("duplicate key should reuse id");
        let tag = interner
            .intern_tag("name", Some("luma"))
            .expect("tag should intern separately by namespace");
        let directive = interner
            .intern_directive("schema", Some("luma"))
            .expect("directive should intern");
        let node_kind = interner
            .intern_node_kind("mapping", Some("syntax"))
            .expect("node kind should intern");
        let profile = interner
            .intern_profile("safe")
            .expect("profile should intern");
        let extension = interner
            .intern_extension_symbol("widget", "acme")
            .expect("extension symbol should intern");

        let (strings, symbols) = interner.into_tables();
        assert_eq!(key, 0);
        assert_eq!(key_again, 0);
        assert_eq!(tag, 1);
        assert_eq!(directive, 2);
        assert_eq!(node_kind, 3);
        assert_eq!(profile, 4);
        assert_eq!(extension, 5);
        assert_eq!(
            strings
                .strings
                .iter()
                .map(|record| record.value.as_str())
                .collect::<Vec<_>>(),
            vec![
                "name", "luma", "schema", "mapping", "syntax", "safe", "widget", "acme"
            ]
        );
        assert_eq!(symbols.symbols[0].flags, SYMBOL_FLAG_KEY);
        assert_eq!(symbols.symbols[1].flags, SYMBOL_FLAG_TAG);
        assert_eq!(symbols.symbols[2].flags, SYMBOL_FLAG_DIRECTIVE);
        assert_eq!(symbols.symbols[3].flags, SYMBOL_FLAG_NODE_KIND);
        assert_eq!(symbols.symbols[4].flags, SYMBOL_FLAG_PROFILE);
        assert_eq!(symbols.symbols[5].flags, SYMBOL_FLAG_EXTENSION);
    }

    #[test]
    fn symbol_table_round_trips_with_namespaces() {
        let table = SymbolTable::new()
            .with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_KEY))
            .with_symbol(
                SymbolRecord::new(1)
                    .with_namespace_string_id(Some(2))
                    .with_flags(SYMBOL_FLAG_TAG),
            );

        let encoded = encode_symbol_table(&table).expect("table should encode");
        let decoded =
            decode_symbol_table(&encoded, &Limits::public(), 3).expect("table should decode");

        assert_eq!(decoded, table);
    }

    #[test]
    fn encoding_rejects_reserved_symbol_flags_with_lb0025() {
        let table = SymbolTable::new()
            .with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_RESERVED_MASK));

        let error = encode_symbol_table(&table).expect_err("reserved flags should fail");

        assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }
}
