//! Capability table helpers for inert evaluation requirements.

use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::Limits;
use crate::primitives::{Identifier, UVar};
use crate::string_table::StringTable;
use crate::symbol::SymbolTable;
use crate::value::Value;

/// `CAPS`
pub const CAPABILITY_SECTION_NAME: &str = "CAPS";

/// Capability is required to evaluate the referenced inert source.
pub const CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION: u64 = 1 << 0;
/// Capability is required to reproduce the same resulting value image.
pub const CAPABILITY_FLAG_REQUIRED_FOR_REPRODUCTION: u64 = 1 << 1;
/// Producer expects pure behavior.
pub const CAPABILITY_FLAG_PURE_EXPECTED: u64 = 1 << 2;
/// Producer expects deterministic behavior.
pub const CAPABILITY_FLAG_DETERMINISTIC_EXPECTED: u64 = 1 << 3;
/// Capability requires trusted policy.
pub const CAPABILITY_FLAG_TRUSTED_ONLY: u64 = 1 << 4;
/// Capability may read external data.
pub const CAPABILITY_FLAG_MAY_READ_EXTERNAL: u64 = 1 << 5;
/// Capability may mutate external state.
pub const CAPABILITY_FLAG_MAY_WRITE_EXTERNAL: u64 = 1 << 6;
/// Reserved capability flag bits.
pub const CAPABILITY_FLAG_RESERVED_MASK: u64 = !0x7f;

/// One capability requirement inside a set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CapabilityRequirement {
    /// Capability symbol name.
    pub capability: Identifier,
    /// Raw requirement flags.
    pub flags: u64,
    /// Optional inert metadata value stored through `VALS`.
    pub metadata_value: Option<Value>,
}

impl CapabilityRequirement {
    /// Creates a new requirement.
    #[must_use]
    pub fn new(capability: Identifier) -> Self {
        Self {
            capability,
            ..Self::default()
        }
    }

    /// Sets explicit flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets optional metadata.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Returns whether the capability is required for evaluation.
    #[must_use]
    pub const fn is_required_for_evaluation(&self) -> bool {
        self.flags & CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION != 0
    }

    /// Returns whether the capability is required for reproducible output.
    #[must_use]
    pub const fn is_required_for_reproduction(&self) -> bool {
        self.flags & CAPABILITY_FLAG_REQUIRED_FOR_REPRODUCTION != 0
    }

    /// Returns whether the producer expects pure behavior.
    #[must_use]
    pub const fn expects_pure_behavior(&self) -> bool {
        self.flags & CAPABILITY_FLAG_PURE_EXPECTED != 0
    }

    /// Returns whether the producer expects deterministic behavior.
    #[must_use]
    pub const fn expects_deterministic_behavior(&self) -> bool {
        self.flags & CAPABILITY_FLAG_DETERMINISTIC_EXPECTED != 0
    }

    /// Returns whether trusted policy is required.
    #[must_use]
    pub const fn is_trusted_only(&self) -> bool {
        self.flags & CAPABILITY_FLAG_TRUSTED_ONLY != 0
    }

    /// Returns whether the capability may read external data.
    #[must_use]
    pub const fn may_read_external(&self) -> bool {
        self.flags & CAPABILITY_FLAG_MAY_READ_EXTERNAL != 0
    }

    /// Returns whether the capability may mutate external state.
    #[must_use]
    pub const fn may_write_external(&self) -> bool {
        self.flags & CAPABILITY_FLAG_MAY_WRITE_EXTERNAL != 0
    }

    /// Returns a compact inspection label.
    #[must_use]
    pub fn inspection_label(&self) -> String {
        let mut suffixes = Vec::new();
        if self.is_required_for_evaluation() {
            suffixes.push("eval");
        }
        if self.is_required_for_reproduction() {
            suffixes.push("repro");
        }
        if self.expects_pure_behavior() {
            suffixes.push("pure");
        }
        if self.expects_deterministic_behavior() {
            suffixes.push("deterministic");
        }
        if self.is_trusted_only() {
            suffixes.push("trusted");
        }
        if self.may_read_external() {
            suffixes.push("read-external");
        }
        if self.may_write_external() {
            suffixes.push("write-external");
        }
        if suffixes.is_empty() {
            self.capability.as_str().to_owned()
        } else {
            format!("{} [{}]", self.capability.as_str(), suffixes.join(", "))
        }
    }
}

/// One decoded `CAPS` capability set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CapabilitySetRecord {
    /// Reserved for future set-level policy bits; must currently be zero.
    pub flags: u64,
    /// Optional inert metadata value stored through `VALS`.
    pub metadata_value: Option<Value>,
    /// Ordered capability requirements.
    pub requirements: Vec<CapabilityRequirement>,
}

impl CapabilitySetRecord {
    /// Creates an empty capability set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets set-level flags.
    #[must_use]
    pub fn with_flags(mut self, flags: u64) -> Self {
        self.flags = flags;
        self
    }

    /// Sets optional metadata.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Appends a requirement.
    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirement) -> Self {
        self.requirements.push(requirement);
        self
    }

    /// Returns a human-oriented inspection summary.
    #[must_use]
    pub fn inspection_summary(&self) -> String {
        self.requirements
            .iter()
            .map(CapabilityRequirement::inspection_label)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// In-memory `CAPS` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CapabilityTable {
    /// Ordered capability sets.
    pub records: Vec<CapabilitySetRecord>,
}

impl CapabilityTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: CapabilitySetRecord) -> Self {
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

    /// Returns a capability set by table index.
    #[must_use]
    pub fn get(&self, index: u64) -> Option<&CapabilitySetRecord> {
        self.records.get(index as usize)
    }

    /// Returns compact inspection rows.
    #[must_use]
    pub fn inspection_rows(&self) -> Vec<String> {
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| format!("set#{index}: {}", record.inspection_summary()))
            .collect()
    }
}

pub(crate) fn decode_capability_table(
    payload: &[u8],
    limits: &Limits,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: Option<&[Value]>,
) -> Result<CapabilityTable> {
    let mut offset = 0_usize;
    let set_count = usize::try_from(UVar::decode(payload, &mut offset)?.0).map_err(|_| {
        LybaError::limit_exceeded("capability set count exceeds configured maximum")
    })?;
    if set_count > limits.max_table_record_count {
        return Err(LybaError::limit_exceeded(
            "capability set count exceeds configured maximum",
        ));
    }

    let mut records = Vec::with_capacity(set_count);
    for record_index in 0..set_count {
        let flags_offset = offset;
        let flags = UVar::decode(payload, &mut offset)?.0;
        if flags != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved capability-set flag bits were non-zero")
                    .with_record_index(record_index)
                    .with_byte_offset(flags_offset),
            ));
        }
        let metadata_value = decode_value_ref(
            payload,
            &mut offset,
            values,
            record_index,
            "capability-set metadata",
        )?;
        let requirement_count =
            usize::try_from(UVar::decode(payload, &mut offset)?.0).map_err(|_| {
                LybaError::limit_exceeded("capability count exceeds configured maximum")
            })?;
        if requirement_count > limits.max_table_record_count {
            return Err(LybaError::limit_exceeded(
                "capability count exceeds configured maximum",
            ));
        }
        let mut requirements = Vec::with_capacity(requirement_count);
        for requirement_index in 0..requirement_count {
            let capability = decode_required_symbol_string_ref(
                payload,
                &mut offset,
                strings,
                symbols,
                record_index,
                requirement_index,
                "capability symbol",
            )?;
            let requirement_flags_offset = offset;
            let requirement_flags = UVar::decode(payload, &mut offset)?.0;
            if requirement_flags & CAPABILITY_FLAG_RESERVED_MASK != 0 {
                return Err(LybaError::InvalidReservedFlags(
                    ErrorContext::new("reserved capability flag bits were non-zero")
                        .with_record_index(record_index)
                        .with_byte_offset(requirement_flags_offset),
                ));
            }
            let metadata_value = decode_value_ref(
                payload,
                &mut offset,
                values,
                record_index,
                "capability metadata",
            )?;
            let _ = requirement_index;
            requirements.push(CapabilityRequirement {
                capability,
                flags: requirement_flags,
                metadata_value,
            });
        }
        records.push(CapabilitySetRecord {
            flags,
            metadata_value,
            requirements,
        });
    }

    if offset != payload.len() {
        return Err(LybaError::InvalidSectionTable(
            ErrorContext::new("capability table payload had trailing bytes")
                .with_byte_offset(offset),
        ));
    }

    Ok(CapabilityTable { records })
}

pub(crate) fn encode_capability_table(
    table: &CapabilityTable,
    strings: &StringTable,
    symbols: &SymbolTable,
    values: &[Value],
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);
    for (record_index, record) in table.records.iter().enumerate() {
        if record.flags != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved capability-set flag bits were non-zero")
                    .with_record_index(record_index),
            ));
        }
        UVar(record.flags).encode_into(&mut bytes);
        UVar(encode_value_ref(
            record.metadata_value.as_ref(),
            values,
            record_index,
            "capability-set metadata",
        )?)
        .encode_into(&mut bytes);
        UVar(record.requirements.len() as u64).encode_into(&mut bytes);
        for requirement in &record.requirements {
            if requirement.flags & CAPABILITY_FLAG_RESERVED_MASK != 0 {
                return Err(LybaError::InvalidReservedFlags(
                    ErrorContext::new("reserved capability flag bits were non-zero")
                        .with_record_index(record_index),
                ));
            }
            UVar(encode_required_symbol_string_ref(
                &requirement.capability,
                strings,
                symbols,
                record_index,
                "capability symbol",
            )?)
            .encode_into(&mut bytes);
            UVar(requirement.flags).encode_into(&mut bytes);
            UVar(encode_value_ref(
                requirement.metadata_value.as_ref(),
                values,
                record_index,
                "capability metadata",
            )?)
            .encode_into(&mut bytes);
        }
    }
    Ok(bytes)
}

fn decode_required_symbol_string_ref(
    payload: &[u8],
    offset: &mut usize,
    strings: &StringTable,
    symbols: &SymbolTable,
    record_index: usize,
    _requirement_index: usize,
    kind: &str,
) -> Result<Identifier> {
    let symbol_ref = UVar::decode(payload, offset)?.0;
    let symbol = symbols.symbols.get(symbol_ref as usize).ok_or_else(|| {
        LybaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        )
    })?;
    let string = strings
        .strings
        .get(symbol.string_id as usize)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string reference was out of range"))
                    .with_record_index(record_index),
            )
        })?;
    Ok(Identifier::new(string.value.clone()))
}

fn encode_required_symbol_string_ref(
    value: &Identifier,
    strings: &StringTable,
    symbols: &SymbolTable,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let string_id = strings
        .strings
        .iter()
        .position(|record| record.value == value.as_str())
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string was not present in STRS"))
                    .with_record_index(record_index),
            )
        })? as u64;
    symbols
        .symbols
        .iter()
        .position(|record| record.string_id == string_id)
        .map(|index| index as u64)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} symbol was not present in SYMS"))
                    .with_record_index(record_index),
            )
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
    let index = values
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| {
            LybaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} was not present in the encoded VALS table"))
                    .with_record_index(record_index),
            )
        })? as u64;
    index.checked_add(1).ok_or_else(|| {
        LybaError::invalid_section_table("capability value reference overflowed u64")
    })
}
