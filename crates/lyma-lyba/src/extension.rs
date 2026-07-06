//! Extension declaration (`EXTS`) helpers.

use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::{ExtensionNamePolicy, Limits};
use crate::primitives::UVar;
use crate::string_table::StringTable;
use crate::value::Value;

/// Extension must be understood by the reader.
pub const EXTENSION_FLAG_REQUIRED: u64 = 1 << 0;
/// Extension requires trusted policy.
pub const EXTENSION_FLAG_TRUSTED_ONLY: u64 = 1 << 1;
/// Extension affects canonical byte verification.
pub const EXTENSION_FLAG_AFFECTS_CANONICAL: u64 = 1 << 2;
/// Extension may contain source or bytecode descriptors.
pub const EXTENSION_FLAG_MAY_CONTAIN_CODE: u64 = 1 << 3;
/// Extension may resolve external resources.
pub const EXTENSION_FLAG_MAY_RESOLVE_EXTERNAL: u64 = 1 << 4;
/// Reserved extension flag bits.
pub const EXTENSION_FLAG_RESERVED_MASK: u64 = !0x1f;

/// One decoded `EXTS` declaration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtensionDeclaration {
    /// Stable extension name.
    pub name: String,
    /// Producer-defined version string.
    pub version: String,
    /// Stored flags.
    pub flags: u64,
    /// Optional metadata value carried via `VALS`.
    pub metadata_value: Option<Value>,
    /// Whether the extension name violated the active reverse-DNS recommendation.
    pub reverse_dns_warning: bool,
}

impl ExtensionDeclaration {
    /// Creates a new declaration.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ..Self::default()
        }
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
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
    pub fn with_required(mut self, enabled: bool) -> Self {
        self.set_flag(EXTENSION_FLAG_REQUIRED, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_trusted_only(mut self, enabled: bool) -> Self {
        self.set_flag(EXTENSION_FLAG_TRUSTED_ONLY, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_affects_canonical(mut self, enabled: bool) -> Self {
        self.set_flag(EXTENSION_FLAG_AFFECTS_CANONICAL, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_may_contain_code(mut self, enabled: bool) -> Self {
        self.set_flag(EXTENSION_FLAG_MAY_CONTAIN_CODE, enabled);
        self
    }

    /// Sets a boolean flag.
    #[must_use]
    pub fn with_may_resolve_external(mut self, enabled: bool) -> Self {
        self.set_flag(EXTENSION_FLAG_MAY_RESOLVE_EXTERNAL, enabled);
        self
    }

    /// Returns whether the declaration is required.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.flags & EXTENSION_FLAG_REQUIRED != 0
    }

    /// Returns whether the declaration is trusted-only.
    #[must_use]
    pub const fn is_trusted_only(&self) -> bool {
        self.flags & EXTENSION_FLAG_TRUSTED_ONLY != 0
    }

    /// Returns whether the declaration affects canonicalization.
    #[must_use]
    pub const fn affects_canonical(&self) -> bool {
        self.flags & EXTENSION_FLAG_AFFECTS_CANONICAL != 0
    }

    /// Returns whether the declaration may contain code.
    #[must_use]
    pub const fn may_contain_code(&self) -> bool {
        self.flags & EXTENSION_FLAG_MAY_CONTAIN_CODE != 0
    }

    /// Returns whether the declaration may resolve external resources.
    #[must_use]
    pub const fn may_resolve_external(&self) -> bool {
        self.flags & EXTENSION_FLAG_MAY_RESOLVE_EXTERNAL != 0
    }

    fn set_flag(&mut self, flag: u64, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

/// In-memory `EXTS` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtensionTable {
    /// Ordered extension declarations.
    pub declarations: Vec<ExtensionDeclaration>,
}

impl ExtensionTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a declaration.
    #[must_use]
    pub fn with_declaration(mut self, declaration: ExtensionDeclaration) -> Self {
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

    /// Looks up a declaration by stable extension name.
    #[must_use]
    pub fn declaration(&self, name: &str) -> Option<&ExtensionDeclaration> {
        self.declarations
            .iter()
            .find(|declaration| declaration.name == name)
    }
}

pub(crate) fn decode_extension_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    values: Option<&[Value]>,
) -> Result<ExtensionTable> {
    let mut offset = 0_usize;
    let extension_count = UVar::decode(payload, &mut offset)?.0;
    let extension_count = usize::try_from(extension_count)
        .map_err(|_| LybaError::limit_exceeded("extension count exceeds configured maximum"))?;
    if extension_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "extension count exceeds configured maximum",
        ));
    }

    let mut declarations = Vec::with_capacity(extension_count);
    for record_index in 0..extension_count {
        let name_string_id =
            decode_string_ref(payload, &mut offset, strings, record_index, "name")?;
        let version_string_id =
            decode_string_ref(payload, &mut offset, strings, record_index, "version")?;
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags & EXTENSION_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved extension flag bits were non-zero")
                    .with_byte_offset(flags_offset)
                    .with_record_index(record_index),
            ));
        }
        let metadata_value = decode_value_ref(payload, &mut offset, values, record_index)?;
        let name = strings.strings[name_string_id as usize].value.clone();
        let version = strings.strings[version_string_id as usize].value.clone();
        let invalid_name = !is_reverse_dns_extension_name(&name);
        if invalid_name && matches!(limits.extension_name_policy, ExtensionNamePolicy::Reject) {
            return Err(LybaError::MalformedExtensionPayload(
                ErrorContext::new(format!(
                    "extension name {name:?} did not match reverse-DNS or org.lyma.* policy"
                ))
                .with_record_index(record_index),
            ));
        }
        let reverse_dns_warning =
            invalid_name && matches!(limits.extension_name_policy, ExtensionNamePolicy::Warn);
        declarations.push(ExtensionDeclaration {
            name,
            version,
            flags,
            metadata_value,
            reverse_dns_warning,
        });
    }

    if offset != payload.len() {
        return Err(LybaError::MalformedExtensionPayload(
            ErrorContext::new("extension table payload had trailing bytes")
                .with_byte_offset(offset),
        ));
    }

    Ok(ExtensionTable { declarations })
}

pub(crate) fn encode_extension_table(
    table: &ExtensionTable,
    strings: &StringTable,
    values: &[Value],
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.declarations.len() as u64).encode_into(&mut bytes);

    for (record_index, declaration) in table.declarations.iter().enumerate() {
        if declaration.flags & EXTENSION_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved extension flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        let name_string_id = find_string_id(strings, &declaration.name).ok_or_else(|| {
            LybaError::MalformedExtensionPayload(
                ErrorContext::new("extension name was not present in STRS")
                    .with_record_index(record_index),
            )
        })?;
        let version_string_id = find_string_id(strings, &declaration.version).ok_or_else(|| {
            LybaError::MalformedExtensionPayload(
                ErrorContext::new("extension version was not present in STRS")
                    .with_record_index(record_index),
            )
        })?;
        UVar(name_string_id).encode_into(&mut bytes);
        UVar(version_string_id).encode_into(&mut bytes);
        UVar(declaration.flags).encode_into(&mut bytes);
        let metadata_value_ref = declaration
            .metadata_value
            .as_ref()
            .map(|metadata_value| {
                values
                    .iter()
                    .position(|value| value == metadata_value)
                    .ok_or_else(|| {
                        LybaError::InvalidValueReference(ErrorContext::new(
                            "extension metadata value was not present in the encoded VALS table",
                        )
                        .with_record_index(record_index))
                    })
                    .and_then(|index| {
                        u64::try_from(index)
                            .ok()
                            .and_then(|index| index.checked_add(1))
                            .ok_or_else(|| {
                                LybaError::invalid_section_table(
                                    "extension metadata reference overflowed u64",
                                )
                            })
                    })
            })
            .transpose()?
            .unwrap_or(0);
        UVar(metadata_value_ref).encode_into(&mut bytes);
    }

    Ok(bytes)
}

pub(crate) fn is_supported_extension(_name: &str, _version: &str) -> bool {
    false
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
    let string_count = strings.strings.len() as u64;
    if value >= string_count {
        return Err(LybaError::MalformedExtensionPayload(
            ErrorContext::new(format!(
                "extension {kind} string reference was out of range"
            ))
            .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn decode_value_ref(
    payload: &[u8],
    offset: &mut usize,
    values: Option<&[Value]>,
    record_index: usize,
) -> Result<Option<Value>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }

    let value_id = encoded - 1;
    let values = values.ok_or_else(|| {
        LybaError::MalformedExtensionPayload(
            ErrorContext::new("extension metadata value reference required VALS")
                .with_record_index(record_index),
        )
    })?;
    let value = values.get(value_id as usize).ok_or_else(|| {
        LybaError::MalformedExtensionPayload(
            ErrorContext::new("extension metadata value reference was out of range")
                .with_record_index(record_index),
        )
    })?;
    Ok(Some(value.clone()))
}

fn is_reverse_dns_extension_name(name: &str) -> bool {
    if let Some(rest) = name.strip_prefix("org.lyma.") {
        return !rest.is_empty() && rest.split('.').all(is_valid_extension_label);
    }

    let mut labels = name.split('.');
    let mut count = 0_usize;
    for label in labels.by_ref() {
        if !is_valid_extension_label(label) {
            return false;
        }
        count += 1;
    }
    count >= 3
}

fn is_valid_extension_label(label: &str) -> bool {
    let mut chars = label.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    let mut last = first;
    for ch in chars {
        if !ch.is_ascii_alphanumeric() && ch != '-' {
            return false;
        }
        last = ch;
    }
    last.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::{
        EXTENSION_FLAG_MAY_CONTAIN_CODE, EXTENSION_FLAG_REQUIRED, ExtensionDeclaration,
        ExtensionTable, decode_extension_table, encode_extension_table,
    };
    use crate::policy::{ExtensionNamePolicy, Limits};
    use crate::string_table::{StringRecord, StringTable};
    use crate::value::Value;

    #[test]
    fn extension_table_round_trips_through_strings_and_values() {
        let strings = StringTable::new()
            .with_string(StringRecord::new("com.example.feature"))
            .with_string(StringRecord::new("1.0"));
        let values = vec![Value::String(String::from("meta"))];
        let table = ExtensionTable::new().with_declaration(
            ExtensionDeclaration::new("com.example.feature", "1.0")
                .with_flags(EXTENSION_FLAG_REQUIRED | EXTENSION_FLAG_MAY_CONTAIN_CODE)
                .with_metadata_value(Some(Value::String(String::from("meta")))),
        );

        let payload =
            encode_extension_table(&table, &strings, &values).expect("EXTS should encode");
        let decoded = decode_extension_table(&payload, &Limits::public(), &strings, Some(&values))
            .expect("EXTS should decode");

        assert_eq!(decoded, table);
    }

    #[test]
    fn extension_name_policy_can_reject_non_reverse_dns_names() {
        let strings = StringTable::new()
            .with_string(StringRecord::new("bad_name"))
            .with_string(StringRecord::new("1.0"));
        let payload = vec![1, 0, 1, 0, 0];
        let mut limits = Limits::public();
        limits.extension_name_policy = ExtensionNamePolicy::Reject;

        let error = decode_extension_table(&payload, &limits, &strings, None)
            .expect_err("non reverse-dns name should fail");

        assert_eq!(error.code().as_str(), "LB0021");
    }
}
