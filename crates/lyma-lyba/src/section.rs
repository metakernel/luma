//! Section model types and section table helpers.

use crate::container::SECTION_ENTRY_SIZE;
use crate::error::{ErrorContext, LybaError, Result};
use crate::policy::ReservedFlagPolicy;
use crate::primitives::{
    Identifier, padding_to_eight, read_u16_le, read_u32_le, read_u64_le, write_u16_le,
    write_u32_le, write_u64_le,
};
use crate::value::Value;
use core::cmp::Ordering;

pub use crate::codec::{CODEC_DEFLATE, CODEC_LZ4, CODEC_NONE, CODEC_ZSTD, is_supported_codec};

/// Section entry REQUIRED bit.
pub const SECTION_FLAG_REQUIRED: u16 = 1 << 0;
/// Section entry UNIQUE bit.
pub const SECTION_FLAG_UNIQUE: u16 = 1 << 1;
/// Section entry ORDERED bit.
pub const SECTION_FLAG_ORDERED: u16 = 1 << 2;
/// Section entry CRITICAL_FOR_CANONICAL bit.
pub const SECTION_FLAG_CRITICAL_FOR_CANONICAL: u16 = 1 << 3;
/// Section entry PRIVATE bit.
pub const SECTION_FLAG_PRIVATE: u16 = 1 << 4;
/// Section entry TRUSTED_ONLY bit.
pub const SECTION_FLAG_TRUSTED_ONLY: u16 = 1 << 5;

/// Reserved bits in section entry flags.
pub const SECTION_FLAG_RESERVED_MASK: u16 = !0x003f;

/// Checksum ID for no checksum.
pub const CHECKSUM_NONE: u16 = 0;
/// Checksum ID for CRC32C.
pub const CHECKSUM_CRC32C: u16 = 1;
/// Checksum ID for XXH3-64.
pub const CHECKSUM_XXH3_64: u16 = 2;
/// Checksum ID for BLAKE3-128.
pub const CHECKSUM_BLAKE3_128: u16 = 3;

/// FourCC section identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SectionId([u8; 4]);

impl SectionId {
    /// `META`
    pub const META: Self = Self(*b"META");
    /// `EXTS`
    pub const EXTS: Self = Self(*b"EXTS");
    /// `STRS`
    pub const STRS: Self = Self(*b"STRS");
    /// `SYMS`
    pub const SYMS: Self = Self(*b"SYMS");
    /// `BLOB`
    pub const BLOB: Self = Self(*b"BLOB");
    /// `VALS`
    pub const VALS: Self = Self(*b"VALS");
    /// `DOCS`
    pub const DOCS: Self = Self(*b"DOCS");
    /// `TAGS`
    pub const TAGS: Self = Self(*b"TAGS");
    /// `SCMA`
    pub const SCMA: Self = Self(*b"SCMA");
    /// `DIAG`
    pub const DIAG: Self = Self(*b"DIAG");
    /// `SRCF`
    pub const SRCF: Self = Self(*b"SRCF");
    /// `SRCS`
    pub const SRCS: Self = Self(*b"SRCS");
    /// `ASTN`
    pub const ASTN: Self = Self(*b"ASTN");
    /// `TRIV`
    pub const TRIV: Self = Self(*b"TRIV");
    /// `DEPS`
    pub const DEPS: Self = Self(*b"DEPS");
    /// `EMBD`
    pub const EMBD: Self = Self(*b"EMBD");
    /// `CAPS`
    pub const CAPS: Self = Self(*b"CAPS");
    /// `SIGN`
    pub const SIGN: Self = Self(*b"SIGN");
    /// `FOOT`
    pub const FOOT: Self = Self(*b"FOOT");

    /// Creates a section ID from raw FourCC bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    /// Returns the raw FourCC bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 4] {
        self.0
    }

    /// Returns the FourCC as UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).expect("section IDs must be ASCII FourCC values")
    }

    /// Returns the canonical sort rank.
    #[must_use]
    pub const fn canonical_rank(self) -> Option<u8> {
        match self {
            Self::META => Some(0),
            Self::EXTS => Some(1),
            Self::STRS => Some(2),
            Self::SYMS => Some(3),
            Self::BLOB => Some(4),
            Self::VALS => Some(5),
            Self::DOCS => Some(6),
            Self::TAGS => Some(7),
            Self::SCMA => Some(8),
            Self::DIAG => Some(9),
            Self::SRCF => Some(10),
            Self::SRCS => Some(11),
            Self::ASTN => Some(12),
            Self::TRIV => Some(13),
            Self::DEPS => Some(14),
            Self::EMBD => Some(15),
            Self::CAPS => Some(16),
            Self::SIGN => Some(17),
            Self::FOOT => Some(18),
            _ => None,
        }
    }

    /// Returns whether this is a known core section.
    #[must_use]
    pub const fn is_known(self) -> bool {
        self.canonical_rank().is_some()
    }
}

/// Canonical ordering helper for section IDs.
#[must_use]
pub fn compare_canonical_section_ids(left: SectionId, right: SectionId) -> Ordering {
    match (left.canonical_rank(), right.canonical_rank()) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.as_bytes().cmp(&right.as_bytes()),
    }
}

/// Basic section semantics understood by this implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionSemantics {
    /// Supported section layout version.
    pub version: u16,
    /// Whether this section is unique by definition.
    pub unique: bool,
    /// Whether duplicate instances have meaningful order.
    pub ordered: bool,
}

impl SectionSemantics {
    /// Returns default semantics for supported core sections.
    #[must_use]
    pub const fn core() -> Self {
        Self {
            version: 1,
            unique: true,
            ordered: false,
        }
    }
}

/// Decoded section table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionEntry {
    /// Section FourCC.
    pub section_id: SectionId,
    /// Section layout version.
    pub section_version: u16,
    /// Entry flags.
    pub entry_flags: u16,
    /// Section-specific payload flags.
    pub payload_flags: u32,
    /// Compression codec ID.
    pub codec_id: u16,
    /// Checksum or digest algorithm ID.
    pub checksum_id: u16,
    /// Absolute payload offset.
    pub payload_offset: u64,
    /// Stored payload byte length.
    pub stored_size: u64,
    /// Logical payload byte length after decompression.
    pub logical_size: u64,
    /// Primary item count.
    pub item_count: u64,
    /// Low 64 checksum bits.
    pub checksum_low: u64,
    /// High 64 checksum bits.
    pub checksum_high: u64,
}

impl SectionEntry {
    /// Decodes one exact 64-byte section entry.
    pub fn decode(input: &[u8]) -> Result<Self> {
        Self::decode_with_reserved_flag_policy(input, ReservedFlagPolicy::Reject)
    }

    /// Decodes one exact 64-byte section entry with configurable reserved-bit handling.
    pub fn decode_with_reserved_flag_policy(
        input: &[u8],
        reserved_flag_policy: ReservedFlagPolicy,
    ) -> Result<Self> {
        if input.len() != SECTION_ENTRY_SIZE as usize {
            return Err(LybaError::InvalidSectionTable(ErrorContext::new(format!(
                "section table entry length {} did not match expected {SECTION_ENTRY_SIZE}",
                input.len()
            ))));
        }

        let section_id = SectionId::new(input[..4].try_into().expect("section id length is fixed"));
        if !section_id.as_bytes().is_ascii() {
            let [a, b, c, d] = section_id.as_bytes();
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "section id bytes 0x{a:02X}{b:02X}{c:02X}{d:02X} were not ASCII FourCC bytes"
                ))
                .with_byte_offset(0),
            ));
        }
        let mut offset = 4;
        let section_version = read_u16_le(input, &mut offset)?;
        let entry_flags = read_u16_le(input, &mut offset)?;
        let payload_flags = read_u32_le(input, &mut offset)?;
        let codec_id = read_u16_le(input, &mut offset)?;
        let checksum_id = read_u16_le(input, &mut offset)?;
        let payload_offset = read_u64_le(input, &mut offset)?;
        let stored_size = read_u64_le(input, &mut offset)?;
        let logical_size = read_u64_le(input, &mut offset)?;
        let item_count = read_u64_le(input, &mut offset)?;
        let checksum_low = read_u64_le(input, &mut offset)?;
        let checksum_high = read_u64_le(input, &mut offset)?;

        if reserved_flag_policy == ReservedFlagPolicy::Reject
            && entry_flags & SECTION_FLAG_RESERVED_MASK != 0
        {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved section entry flag bits were non-zero")
                    .with_byte_offset(6),
            ));
        }

        Ok(Self {
            section_id,
            section_version,
            entry_flags,
            payload_flags,
            codec_id,
            checksum_id,
            payload_offset,
            stored_size,
            logical_size,
            item_count,
            checksum_low,
            checksum_high,
        })
    }

    /// Encodes one exact 64-byte section entry.
    pub fn encode(&self) -> Result<[u8; SECTION_ENTRY_SIZE as usize]> {
        if self.entry_flags & SECTION_FLAG_RESERVED_MASK != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved section entry flag bits were non-zero")
                    .with_byte_offset(6),
            ));
        }

        let mut bytes = Vec::with_capacity(SECTION_ENTRY_SIZE as usize);
        bytes.extend_from_slice(&self.section_id.as_bytes());
        write_u16_le(&mut bytes, self.section_version);
        write_u16_le(&mut bytes, self.entry_flags);
        write_u32_le(&mut bytes, self.payload_flags);
        write_u16_le(&mut bytes, self.codec_id);
        write_u16_le(&mut bytes, self.checksum_id);
        write_u64_le(&mut bytes, self.payload_offset);
        write_u64_le(&mut bytes, self.stored_size);
        write_u64_le(&mut bytes, self.logical_size);
        write_u64_le(&mut bytes, self.item_count);
        write_u64_le(&mut bytes, self.checksum_low);
        write_u64_le(&mut bytes, self.checksum_high);

        Ok(bytes
            .try_into()
            .expect("section entry must encode to exactly 64 bytes"))
    }

    /// Returns whether this section is marked required.
    #[must_use]
    pub const fn is_required(self) -> bool {
        self.entry_flags & SECTION_FLAG_REQUIRED != 0
    }

    /// Returns whether this section is unique.
    #[must_use]
    pub const fn is_unique(self) -> bool {
        self.entry_flags & SECTION_FLAG_UNIQUE != 0
    }

    /// Returns whether this section is ordered.
    #[must_use]
    pub const fn is_ordered(self) -> bool {
        self.entry_flags & SECTION_FLAG_ORDERED != 0
    }

    /// Returns the occupied payload range.
    pub fn payload_range(self) -> Result<(u64, u64)> {
        let end = self
            .payload_offset
            .checked_add(self.stored_size)
            .ok_or_else(|| {
                LybaError::OffsetOutsideFile(
                    ErrorContext::new("section payload offset plus stored size overflowed")
                        .with_byte_offset(16),
                )
            })?;

        Ok((self.payload_offset, end))
    }

    /// Returns the payload bytes using checked arithmetic and checked conversion.
    pub fn payload_slice<'a>(self, input: &'a [u8]) -> Result<&'a [u8]> {
        let (start, end) = self.payload_range()?;
        let start = checked_u64_to_usize(start, usize::MAX).map_err(|_| {
            LybaError::OffsetOutsideFile(
                ErrorContext::new(
                    "section payload offset could not be represented on this platform",
                )
                .with_byte_offset(16),
            )
        })?;
        let end = checked_u64_to_usize(end, usize::MAX).map_err(|_| {
            LybaError::OffsetOutsideFile(
                ErrorContext::new("section payload end could not be represented on this platform")
                    .with_byte_offset(24),
            )
        })?;

        input.get(start..end).ok_or_else(|| {
            LybaError::OffsetOutsideFile(
                ErrorContext::new("section payload extends beyond available input")
                    .with_byte_offset(start),
            )
        })
    }
}

/// Validated section table entry with borrowed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSection<'a> {
    /// Decoded entry.
    pub entry: SectionEntry,
    /// Borrowed stored payload bytes.
    pub payload: &'a [u8],
}

/// Returns semantics for a supported section ID.
#[must_use]
pub const fn supported_section_semantics(section_id: SectionId) -> Option<SectionSemantics> {
    if section_id.is_known() {
        Some(SectionSemantics::core())
    } else {
        None
    }
}

/// Returns whether the checksum algorithm ID is understood.
#[must_use]
pub const fn is_supported_checksum(checksum_id: u16) -> bool {
    matches!(checksum_id, CHECKSUM_NONE | CHECKSUM_CRC32C)
}

pub(crate) fn checked_u64_to_usize(value: u64, max: usize) -> core::result::Result<usize, ()> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value <= max)
        .ok_or(())
}

pub(crate) fn checked_table_len_bytes(count: u32, entry_size: u32) -> Result<u64> {
    u64::from(count)
        .checked_mul(u64::from(entry_size))
        .ok_or_else(|| {
            LybaError::InvalidSectionTable(ErrorContext::new(
                "section table byte length overflowed",
            ))
        })
}

pub(crate) fn checked_table_allocation_len(entry_count: usize, entry_size: usize) -> Result<usize> {
    entry_count.checked_mul(entry_size).ok_or_else(|| {
        LybaError::InvalidSectionTable(ErrorContext::new(
            "section table allocation length overflowed",
        ))
    })
}

pub(crate) fn validate_zero_gap_bytes(input: &[u8], gap_start: u64, gap_end: u64) -> Result<()> {
    if gap_start >= gap_end {
        return Ok(());
    }

    let start = checked_u64_to_usize(gap_start, usize::MAX).map_err(|_| {
        LybaError::OffsetOutsideFile(ErrorContext::new(
            "gap start could not be represented on this platform",
        ))
    })?;
    let end = checked_u64_to_usize(gap_end, usize::MAX).map_err(|_| {
        LybaError::OffsetOutsideFile(ErrorContext::new(
            "gap end could not be represented on this platform",
        ))
    })?;
    let bytes = input.get(start..end).ok_or_else(|| {
        LybaError::OffsetOutsideFile(
            ErrorContext::new("gap bytes extended beyond available input").with_byte_offset(start),
        )
    })?;

    if bytes.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(LybaError::InvalidReservedFlags(
            ErrorContext::new("padding bytes must be zero").with_byte_offset(start),
        ))
    }
}

pub(crate) fn aligned_end(offset: u64, size: u64) -> Result<u64> {
    let end = offset.checked_add(size).ok_or_else(|| {
        LybaError::OffsetOutsideFile(ErrorContext::new(
            "payload range overflowed while computing aligned end",
        ))
    })?;
    let size_usize = checked_u64_to_usize(size, usize::MAX).map_err(|_| {
        LybaError::OffsetOutsideFile(ErrorContext::new(
            "payload size could not be represented on this platform",
        ))
    })?;
    end.checked_add(padding_to_eight(size_usize) as u64)
        .ok_or_else(|| {
            LybaError::OffsetOutsideFile(ErrorContext::new("aligned payload end overflowed"))
        })
}

/// Logical section within a LYBA document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    /// Section name or tag.
    pub name: Identifier,
    /// Section values in source order.
    pub values: Vec<Value>,
}

impl Section {
    /// Creates a new section with the provided name.
    #[must_use]
    pub fn new(name: impl Into<Identifier>) -> Self {
        Self {
            name: name.into(),
            values: Vec::new(),
        }
    }

    /// Appends a value and returns the updated section.
    #[must_use]
    pub fn with_value(mut self, value: Value) -> Self {
        self.values.push(value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_RESERVED_MASK,
        SECTION_FLAG_UNIQUE, SectionEntry, SectionId, aligned_end, checked_table_allocation_len,
        checked_table_len_bytes, checked_u64_to_usize, compare_canonical_section_ids,
    };
    use crate::error::LybaError;

    #[test]
    fn canonical_order_matches_spec_core_section_order() {
        assert!(compare_canonical_section_ids(SectionId::META, SectionId::DOCS).is_lt());
        assert!(compare_canonical_section_ids(SectionId::DOCS, SectionId::FOOT).is_lt());
        assert!(compare_canonical_section_ids(SectionId::new(*b"ZZZZ"), SectionId::FOOT).is_gt());
    }

    #[test]
    fn section_entry_round_trips_exact_binary_layout() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 7,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 32,
            logical_size: 32,
            item_count: 2,
            checksum_low: 11,
            checksum_high: 0,
        };

        let decoded = SectionEntry::decode(&entry.encode().expect("entry should encode"))
            .expect("entry should decode");

        assert_eq!(decoded, entry);
    }

    #[test]
    fn section_entry_rejects_reserved_bits_with_lb0025() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_RESERVED_MASK,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 64,
            stored_size: 0,
            logical_size: 0,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };

        let error = entry.encode().expect_err("reserved bits should fail");

        assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }

    #[test]
    fn section_entry_rejects_non_ascii_fourcc_bytes() {
        let mut bytes = [0_u8; 64];
        bytes[..4].copy_from_slice(&[0xFF, b'A', b'B', b'C']);

        let error = SectionEntry::decode(&bytes).expect_err("non-ascii FourCC should fail");

        assert!(matches!(error, LybaError::InvalidSectionTable(_)));
    }

    #[test]
    fn checked_math_helpers_reject_overflow_before_use() {
        assert_eq!(
            checked_table_len_bytes(u32::MAX, u32::MAX).unwrap(),
            18_446_744_065_119_617_025
        );

        let error =
            checked_table_allocation_len(usize::MAX, 2).expect_err("alloc mul should overflow");
        assert!(matches!(error, LybaError::InvalidSectionTable(_)));

        assert!(checked_u64_to_usize(9, 8).is_err());

        let error = aligned_end(u64::MAX - 1, 8).expect_err("aligned end should overflow");
        assert!(matches!(error, LybaError::OffsetOutsideFile(_)));
    }
}
