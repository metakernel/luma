//! Container-level document model types.

use crate::blob::BlobTable;
use crate::bundle::{BundleDescriptor, DependencyTable, EmbeddedResourceTable};
use crate::capability::CapabilityTable;
use crate::checksum::{crc32c, crc32c_footer, crc32c_header, validate_section_checksum};
use crate::diagnostic::DiagnosticTable;
use crate::document::Document;
use crate::error::{ErrorContext, LybaError, Result};
use crate::extension::ExtensionTable;
use crate::meta::Metadata;
use crate::policy::ReservedFlagPolicy;
use crate::primitives::Identifier;
use crate::primitives::{
    read_u16_le, read_u32_le, read_u64_le, write_u16_le, write_u32_le, write_u64_le,
};
use crate::schema::SchemaTable;
use crate::section::{
    Section, SectionEntry, ValidatedSection, aligned_end, checked_table_len_bytes,
    compare_canonical_section_ids, is_supported_checksum, is_supported_codec,
    supported_section_semantics, validate_zero_gap_bytes,
};
use crate::signature::SignatureTable;
use crate::source::{SourceFileTable, SourceSpanTable};
use crate::string_table::StringTable;
use crate::symbol::SymbolTable;
use crate::syntax::SyntaxNodeTable;
use crate::tag::TagTable;
use crate::trivia::TriviaTable;
use core::cmp::Ordering;

/// Exact 8-byte LYBA file magic.
pub const HEADER_MAGIC: [u8; 8] = [0x4C, 0x55, 0x4D, 0x42, 0x41, 0x0D, 0x0A, 0x1A];
/// Version 0 major value.
pub const HEADER_MAJOR_VERSION: u16 = 0;
/// Version 0.1 minor value.
pub const HEADER_MINOR_VERSION: u16 = 1;
/// Fixed version 0.1 header size.
pub const HEADER_SIZE: u16 = 64;
/// Fixed version 0.1 header length as `usize`.
pub const HEADER_LEN: usize = 64;
/// Fixed version 0.1 footer size.
pub const FOOTER_SIZE: u16 = 64;
/// Fixed version 0.1 footer length as `usize`.
pub const FOOTER_LEN: usize = 64;
/// Fixed little-endian marker.
pub const HEADER_ENDIAN_MARKER: u16 = 0x0102;
/// Fixed version 0.1 section table entry size.
pub const SECTION_ENTRY_SIZE: u32 = 64;
const FOOTER_MAGIC: [u8; 4] = *b"FOOT";
const CONTAINER_FLAG_RESERVED_MASK: u32 = !0x03ff;
const PROFILE_FLAG_RESERVED_MASK: u32 = !0x00ff;

/// Policy for optional header CRC handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HeaderCrcMode {
    /// Emit/accept a zero header CRC field.
    Disabled,
    /// Emit and validate CRC32C when present.
    #[default]
    Enabled,
}

/// Decoded optional version 0.1 container footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerFooter {
    /// Global container flags.
    pub container_flags: u32,
    /// Declared profile flags.
    pub profile_flags: u32,
    /// Absolute section table offset.
    pub section_table_offset: u64,
    /// Number of section entries.
    pub section_count: u32,
    /// Encoded section entry size.
    pub section_entry_size: u32,
    /// Declared full file length.
    pub file_length: u64,
    /// Declared root document count.
    pub root_document_count: u64,
    /// Stored optional header CRC32C copied from the header.
    pub header_crc32c: u32,
    /// Stored footer CRC32C.
    pub footer_crc32c: u32,
}

impl ContainerFooter {
    /// Builds a footer from a decoded header.
    #[must_use]
    pub fn from_header(header: &ContainerHeader) -> Self {
        Self {
            container_flags: header.container_flags,
            profile_flags: header.profile_flags,
            section_table_offset: header.section_table_offset,
            section_count: header.section_count,
            section_entry_size: header.section_entry_size,
            file_length: header.file_length,
            root_document_count: header.root_document_count,
            header_crc32c: header.header_crc32c,
            footer_crc32c: 0,
        }
    }

    /// Encodes the exact 64-byte version 0.1 footer.
    #[must_use]
    pub fn encode(&self) -> [u8; FOOTER_LEN] {
        let mut bytes = Vec::with_capacity(FOOTER_LEN);
        bytes.extend_from_slice(&FOOTER_MAGIC);
        write_u16_le(&mut bytes, HEADER_MAJOR_VERSION);
        write_u16_le(&mut bytes, HEADER_MINOR_VERSION);
        write_u16_le(&mut bytes, FOOTER_SIZE);
        write_u16_le(&mut bytes, 0);
        write_u32_le(&mut bytes, self.container_flags);
        write_u32_le(&mut bytes, self.profile_flags);
        write_u64_le(&mut bytes, self.section_table_offset);
        write_u32_le(&mut bytes, self.section_count);
        write_u32_le(&mut bytes, self.section_entry_size);
        write_u64_le(&mut bytes, self.file_length);
        write_u64_le(&mut bytes, self.root_document_count);
        write_u32_le(&mut bytes, self.header_crc32c);
        write_u32_le(&mut bytes, 0);
        write_u32_le(&mut bytes, 0);

        let crc = crc32c(&bytes[..56]);
        bytes[56..60].copy_from_slice(&crc.to_le_bytes());

        bytes
            .try_into()
            .expect("version 0.1 container footer must encode to exactly 64 bytes")
    }

    /// Decodes and validates a footer discovered at end of file.
    pub fn decode(input: &[u8]) -> Result<Self> {
        if input.len() != FOOTER_LEN {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "footer length {} did not match expected {FOOTER_LEN}",
                    input.len()
                ))
                .with_byte_offset(input.len()),
            ));
        }
        if input[..4] != FOOTER_MAGIC {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("container footer magic did not match FOOT").with_byte_offset(0),
            ));
        }

        let mut offset = 4;
        let major_version = read_u16_le(input, &mut offset)?;
        let minor_version = read_u16_le(input, &mut offset)?;
        if major_version != HEADER_MAJOR_VERSION || minor_version != HEADER_MINOR_VERSION {
            return Err(LybaError::UnsupportedVersion(
                ErrorContext::new(format!(
                    "unsupported container footer version {major_version}.{minor_version}; expected 0.1"
                ))
                .with_byte_offset(4),
            ));
        }

        let footer_size = read_u16_le(input, &mut offset)?;
        if footer_size != FOOTER_SIZE {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "invalid container footer size {footer_size}; expected {FOOTER_SIZE}"
                ))
                .with_byte_offset(8),
            ));
        }

        let reserved0 = read_u16_le(input, &mut offset)?;
        let container_flags = read_u32_le(input, &mut offset)?;
        let profile_flags = read_u32_le(input, &mut offset)?;
        let section_table_offset = read_u64_le(input, &mut offset)?;
        let section_count = read_u32_le(input, &mut offset)?;
        let section_entry_size = read_u32_le(input, &mut offset)?;
        let file_length = read_u64_le(input, &mut offset)?;
        let root_document_count = read_u64_le(input, &mut offset)?;
        let header_crc32c = read_u32_le(input, &mut offset)?;
        let footer_crc32c = read_u32_le(input, &mut offset)?;
        let reserved1 = read_u32_le(input, &mut offset)?;

        if reserved0 != 0 || reserved1 != 0 {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved footer bits were non-zero").with_byte_offset(10),
            ));
        }
        if section_entry_size != SECTION_ENTRY_SIZE {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "invalid footer section entry size {section_entry_size}; expected {SECTION_ENTRY_SIZE}"
                ))
                .with_byte_offset(28),
            ));
        }
        if footer_crc32c != crc32c_footer(input) {
            return Err(LybaError::ChecksumMismatch(
                ErrorContext::new(format!(
                    "footer CRC32C mismatch: stored 0x{footer_crc32c:08X}, computed 0x{:08X}",
                    crc32c_footer(input)
                ))
                .with_byte_offset(56),
            ));
        }

        Ok(Self {
            container_flags,
            profile_flags,
            section_table_offset,
            section_count,
            section_entry_size,
            file_length,
            root_document_count,
            header_crc32c,
            footer_crc32c,
        })
    }

    /// Ensures footer fields agree with the header.
    pub fn validate_against_header(&self, header: &ContainerHeader) -> Result<()> {
        if self.container_flags != header.container_flags
            || self.profile_flags != header.profile_flags
            || self.section_table_offset != header.section_table_offset
            || self.section_count != header.section_count
            || self.section_entry_size != header.section_entry_size
            || self.file_length != header.file_length
            || self.root_document_count != header.root_document_count
            || self.header_crc32c != header.header_crc32c
        {
            return Err(LybaError::InvalidSectionTable(ErrorContext::new(
                "container footer did not agree with the container header",
            )));
        }

        Ok(())
    }
}

/// Decoded version 0.1 container header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHeader {
    /// Global container flags.
    pub container_flags: u32,
    /// Declared profile flags.
    pub profile_flags: u32,
    /// Absolute section table offset.
    pub section_table_offset: u64,
    /// Number of section entries.
    pub section_count: u32,
    /// Encoded section entry size.
    pub section_entry_size: u32,
    /// Declared full file length, or zero if omitted.
    pub file_length: u64,
    /// Declared root document count, or zero if unknown.
    pub root_document_count: u64,
    /// Stored optional header CRC32C.
    pub header_crc32c: u32,
}

impl Default for ContainerHeader {
    fn default() -> Self {
        Self {
            container_flags: 0,
            profile_flags: 0,
            section_table_offset: u64::from(HEADER_SIZE),
            section_count: 0,
            section_entry_size: SECTION_ENTRY_SIZE,
            file_length: u64::from(HEADER_SIZE),
            root_document_count: 0,
            header_crc32c: 0,
        }
    }
}

impl ContainerHeader {
    /// Creates a minimal version 0.1 header.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes and validates a version 0.1 header.
    pub fn decode(input: &[u8], header_crc_mode: HeaderCrcMode) -> Result<Self> {
        Self::decode_with_reserved_flag_policy(input, header_crc_mode, ReservedFlagPolicy::Reject)
    }

    /// Decodes and validates a version 0.1 header with configurable reserved-bit handling.
    pub fn decode_with_reserved_flag_policy(
        input: &[u8],
        header_crc_mode: HeaderCrcMode,
        reserved_flag_policy: ReservedFlagPolicy,
    ) -> Result<Self> {
        if input.len() < usize::from(HEADER_SIZE) {
            return Err(LybaError::OffsetOutsideFile(
                ErrorContext::new("input shorter than 64-byte container header")
                    .with_byte_offset(input.len()),
            ));
        }

        if input[..HEADER_MAGIC.len()] != HEADER_MAGIC {
            return Err(LybaError::InvalidMagic(
                ErrorContext::new("container header magic did not match LYBA signature")
                    .with_byte_offset(0),
            ));
        }

        let mut offset = HEADER_MAGIC.len();
        let major_version = read_u16_le(input, &mut offset)?;
        let minor_version = read_u16_le(input, &mut offset)?;
        if major_version != HEADER_MAJOR_VERSION || minor_version != HEADER_MINOR_VERSION {
            return Err(LybaError::UnsupportedVersion(
                ErrorContext::new(format!(
                    "unsupported container version {major_version}.{minor_version}; expected 0.1"
                ))
                .with_byte_offset(8),
            ));
        }

        let header_size = read_u16_le(input, &mut offset)?;
        if header_size != HEADER_SIZE {
            return Err(LybaError::InvalidHeaderSize(
                ErrorContext::new(format!(
                    "invalid container header size {header_size}; expected {HEADER_SIZE}"
                ))
                .with_byte_offset(12),
            ));
        }

        let endian_marker = read_u16_le(input, &mut offset)?;
        if endian_marker != HEADER_ENDIAN_MARKER {
            return Err(LybaError::InvalidEndianMarker(
                ErrorContext::new(format!(
                    "invalid endian marker 0x{endian_marker:04X}; expected 0x{HEADER_ENDIAN_MARKER:04X}"
                ))
                .with_byte_offset(14),
            ));
        }

        let container_flags = read_u32_le(input, &mut offset)?;
        let profile_flags = read_u32_le(input, &mut offset)?;
        let section_table_offset = read_u64_le(input, &mut offset)?;
        let section_count = read_u32_le(input, &mut offset)?;
        let section_entry_size = read_u32_le(input, &mut offset)?;
        let file_length = read_u64_le(input, &mut offset)?;
        let root_document_count = read_u64_le(input, &mut offset)?;
        let header_crc32c = read_u32_le(input, &mut offset)?;
        let reserved = read_u32_le(input, &mut offset)?;

        if section_entry_size != SECTION_ENTRY_SIZE {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "invalid section entry size {section_entry_size}; expected {SECTION_ENTRY_SIZE}"
                ))
                .with_byte_offset(36),
            ));
        }

        if reserved_flag_policy == ReservedFlagPolicy::Reject
            && (container_flags & CONTAINER_FLAG_RESERVED_MASK != 0
                || profile_flags & PROFILE_FLAG_RESERVED_MASK != 0
                || reserved != 0)
        {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved header flag or field bits were non-zero")
                    .with_byte_offset(16),
            ));
        }

        if section_table_offset < u64::from(HEADER_SIZE) {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("section table offset overlapped the 64-byte header")
                    .with_byte_offset(24),
            ));
        }

        if file_length != 0 && file_length != input.len() as u64 {
            return Err(LybaError::OffsetOutsideFile(
                ErrorContext::new(format!(
                    "declared file length {file_length} did not match available input length {}",
                    input.len()
                ))
                .with_byte_offset(40),
            ));
        }

        if header_crc_mode == HeaderCrcMode::Enabled && header_crc32c != 0 {
            let expected = crc32c_header(input);
            if header_crc32c != expected {
                return Err(LybaError::ChecksumMismatch(
                    ErrorContext::new(format!(
                        "header CRC32C mismatch: stored 0x{header_crc32c:08X}, computed 0x{expected:08X}"
                    ))
                    .with_byte_offset(56),
                ));
            }
        }

        Ok(Self {
            container_flags,
            profile_flags,
            section_table_offset,
            section_count,
            section_entry_size,
            file_length,
            root_document_count,
            header_crc32c,
        })
    }

    /// Encodes the exact 64-byte version 0.1 header.
    pub fn encode(&self, header_crc_mode: HeaderCrcMode) -> Result<[u8; HEADER_LEN]> {
        if self.container_flags & CONTAINER_FLAG_RESERVED_MASK != 0
            || self.profile_flags & PROFILE_FLAG_RESERVED_MASK != 0
        {
            return Err(LybaError::InvalidReservedFlags(
                ErrorContext::new("reserved header flag bits were non-zero").with_byte_offset(16),
            ));
        }

        if self.section_entry_size != SECTION_ENTRY_SIZE {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new(format!(
                    "invalid section entry size {}; expected {SECTION_ENTRY_SIZE}",
                    self.section_entry_size
                ))
                .with_byte_offset(36),
            ));
        }

        if self.section_table_offset < u64::from(HEADER_SIZE) {
            return Err(LybaError::InvalidSectionTable(
                ErrorContext::new("section table offset overlapped the 64-byte header")
                    .with_byte_offset(24),
            ));
        }

        let mut bytes = Vec::with_capacity(usize::from(HEADER_SIZE));
        bytes.extend_from_slice(&HEADER_MAGIC);
        write_u16_le(&mut bytes, HEADER_MAJOR_VERSION);
        write_u16_le(&mut bytes, HEADER_MINOR_VERSION);
        write_u16_le(&mut bytes, HEADER_SIZE);
        write_u16_le(&mut bytes, HEADER_ENDIAN_MARKER);
        write_u32_le(&mut bytes, self.container_flags);
        write_u32_le(&mut bytes, self.profile_flags);
        write_u64_le(&mut bytes, self.section_table_offset);
        write_u32_le(&mut bytes, self.section_count);
        write_u32_le(&mut bytes, self.section_entry_size);
        write_u64_le(&mut bytes, self.file_length);
        write_u64_le(&mut bytes, self.root_document_count);
        write_u32_le(&mut bytes, 0);
        write_u32_le(&mut bytes, 0);

        let crc = if header_crc_mode == HeaderCrcMode::Enabled {
            crc32c(&bytes[..56])
        } else {
            0
        };
        bytes[56..60].copy_from_slice(&crc.to_le_bytes());

        let header: [u8; HEADER_LEN] = bytes
            .try_into()
            .expect("version 0.1 container header must encode to exactly 64 bytes");
        Ok(header)
    }
}

/// Decodes, validates, and slices the section table referenced by the header.
pub fn validate_section_table<'a>(
    header: &ContainerHeader,
    input: &'a [u8],
) -> Result<Vec<ValidatedSection<'a>>> {
    validate_section_table_with_reserved_flag_policy(header, input, ReservedFlagPolicy::Reject)
}

/// Decodes, validates, and slices the section table referenced by the header.
pub fn validate_section_table_with_reserved_flag_policy<'a>(
    header: &ContainerHeader,
    input: &'a [u8],
    reserved_flag_policy: ReservedFlagPolicy,
) -> Result<Vec<ValidatedSection<'a>>> {
    let table_len = checked_table_len_bytes(header.section_count, header.section_entry_size)?;
    let table_end = header
        .section_table_offset
        .checked_add(table_len)
        .ok_or_else(|| {
            LybaError::OffsetOutsideFile(
                ErrorContext::new("section table offset plus length overflowed")
                    .with_byte_offset(24),
            )
        })?;

    if table_end > input.len() as u64 {
        return Err(LybaError::OffsetOutsideFile(
            ErrorContext::new("section table extends beyond available input").with_byte_offset(24),
        ));
    }

    let table_start = usize::try_from(header.section_table_offset).map_err(|_| {
        LybaError::OffsetOutsideFile(
            ErrorContext::new("section table offset could not be represented on this platform")
                .with_byte_offset(24),
        )
    })?;
    let table_end_usize = usize::try_from(table_end).map_err(|_| {
        LybaError::OffsetOutsideFile(
            ErrorContext::new("section table end could not be represented on this platform")
                .with_byte_offset(24),
        )
    })?;
    let table_bytes = input.get(table_start..table_end_usize).ok_or_else(|| {
        LybaError::OffsetOutsideFile(
            ErrorContext::new("section table bytes were not available")
                .with_byte_offset(table_start),
        )
    })?;

    let mut validated = Vec::with_capacity(header.section_count as usize);
    let mut seen_unique = Vec::<SectionIdRecord>::new();
    let mut spans = Vec::with_capacity(header.section_count as usize + 2);
    spans.push(SpanRecord::new(0, u64::from(HEADER_SIZE), false, None));
    spans.push(SpanRecord::new(
        header.section_table_offset,
        table_end,
        false,
        Some(SpanLabel::Static("section table")),
    ));

    let mut previous: Option<SectionEntry> = None;
    for (index, entry_bytes) in table_bytes
        .chunks_exact(SECTION_ENTRY_SIZE as usize)
        .enumerate()
    {
        let entry =
            SectionEntry::decode_with_reserved_flag_policy(entry_bytes, reserved_flag_policy)?;

        if let Some(previous_entry) = previous {
            let ordering =
                compare_canonical_section_ids(previous_entry.section_id, entry.section_id);
            if ordering == Ordering::Greater {
                return Err(LybaError::NonCanonicalEncoding(
                    ErrorContext::new(format!(
                        "section table entry {} with {} appeared after {} in non-canonical order",
                        index,
                        entry.section_id.as_str(),
                        previous_entry.section_id.as_str()
                    ))
                    .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize),
                ));
            }
        }
        previous = Some(entry);

        match supported_section_semantics(entry.section_id) {
            Some(semantics) => {
                if entry.section_version != semantics.version {
                    return Err(LybaError::UnsupportedRequiredSection(
                        ErrorContext::new(format!(
                            "section {} version {} is not supported; expected {}",
                            entry.section_id.as_str(),
                            entry.section_version,
                            semantics.version
                        ))
                        .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize + 4),
                    ));
                }

                if semantics.unique || entry.is_unique() {
                    if seen_unique
                        .iter()
                        .any(|record| record.section_id == entry.section_id)
                    {
                        return Err(LybaError::InvalidSectionTable(
                            ErrorContext::new(format!(
                                "duplicate unique section {}",
                                entry.section_id.as_str()
                            ))
                            .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize),
                        ));
                    }
                    seen_unique.push(SectionIdRecord {
                        section_id: entry.section_id,
                    });
                }
            }
            None if entry.is_required() => {
                return Err(LybaError::UnsupportedRequiredSection(
                    ErrorContext::new(format!(
                        "required section {} is not supported",
                        entry.section_id.as_str()
                    ))
                    .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize),
                ));
            }
            None => {}
        }

        if !is_supported_codec(entry.codec_id) {
            if entry.is_required() {
                return Err(LybaError::UnsupportedCodec(
                    ErrorContext::new(format!(
                        "required section {} uses unsupported codec {}",
                        entry.section_id.as_str(),
                        entry.codec_id
                    ))
                    .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize + 12),
                ));
            }
        }

        if !is_supported_checksum(entry.checksum_id) && entry.is_required() {
            return Err(LybaError::UnsupportedRequiredSection(
                ErrorContext::new(format!(
                    "required section {} uses unsupported checksum {}",
                    entry.section_id.as_str(),
                    entry.checksum_id
                ))
                .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize + 14),
            ));
        }

        let (payload_start, payload_end) = entry.payload_range()?;
        if payload_end > input.len() as u64 {
            return Err(LybaError::OffsetOutsideFile(
                ErrorContext::new(format!(
                    "section {} payload extends beyond available input",
                    entry.section_id.as_str()
                ))
                .with_byte_offset(table_start + index * SECTION_ENTRY_SIZE as usize + 16),
            ));
        }

        let _ = aligned_end(payload_start, entry.stored_size)?;
        let payload = entry.payload_slice(input)?;
        validate_section_checksum(entry, payload)?;
        spans.push(SpanRecord::new(
            payload_start,
            payload_end,
            true,
            Some(SpanLabel::Section(entry.section_id)),
        ));
        validated.push(ValidatedSection { entry, payload });
    }

    spans.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
    for pair in spans.windows(2) {
        let current = pair[0];
        let next = pair[1];

        if current.end > next.start {
            return Err(LybaError::OverlappingSections(
                ErrorContext::new(match (current.label, next.label) {
                    (Some(left), Some(right)) => {
                        format!("{} overlapped {}", left.display(), right.display())
                    }
                    _ => "section spans overlapped".to_owned(),
                })
                .with_byte_offset(usize::try_from(next.start).unwrap_or(usize::MAX)),
            ));
        }

        let gap_end = if current.is_payload {
            aligned_end(current.start, current.end - current.start)?
        } else {
            current.end
        };
        let gap_end = gap_end.min(next.start);
        validate_zero_gap_bytes(input, current.end, gap_end)?;
    }

    if let Some(last) = spans.last().copied() {
        if last.is_payload {
            let trailing_padding_end =
                aligned_end(last.start, last.end - last.start)?.min(input.len() as u64);
            validate_zero_gap_bytes(input, last.end, trailing_padding_end)?;
        }
    }

    Ok(validated)
}

#[derive(Debug, Clone, Copy)]
struct SpanRecord {
    start: u64,
    end: u64,
    is_payload: bool,
    label: Option<SpanLabel>,
}

impl SpanRecord {
    const fn new(start: u64, end: u64, is_payload: bool, label: Option<SpanLabel>) -> Self {
        Self {
            start,
            end,
            is_payload,
            label,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SpanLabel {
    Static(&'static str),
    Section(crate::section::SectionId),
}

impl SpanLabel {
    fn display(self) -> String {
        match self {
            Self::Static(value) => value.to_owned(),
            Self::Section(section_id) => section_id.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SectionIdRecord {
    section_id: crate::section::SectionId,
}

/// Discovers an optional footer located at the end of the file.
pub fn discover_footer(input: &[u8]) -> Result<Option<ContainerFooter>> {
    if input.len() < FOOTER_LEN {
        return Ok(None);
    }

    let footer_start = input.len() - FOOTER_LEN;
    let Some(candidate) = input.get(footer_start..) else {
        return Ok(None);
    };
    if candidate[..4] != FOOTER_MAGIC {
        return Ok(None);
    }

    ContainerFooter::decode(candidate).map(Some)
}

/// In-memory representation of a LYBA file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LybaFile {
    /// Optional identifier for the file.
    pub id: Option<Identifier>,
    /// Optional deterministic metadata map from the `META` section.
    pub metadata: Option<Metadata>,
    /// Optional interned string table.
    pub string_table: Option<StringTable>,
    /// Optional extension declaration table.
    pub extension_table: Option<ExtensionTable>,
    /// Optional interned symbol table.
    pub symbol_table: Option<SymbolTable>,
    /// Optional blob table.
    pub blob_table: Option<BlobTable>,
    /// Optional tag registry.
    pub tag_table: Option<TagTable>,
    /// Optional schema table.
    pub schema_table: Option<SchemaTable>,
    /// Optional stored diagnostics from the `DIAG` section.
    pub diagnostic_table: Option<DiagnosticTable>,
    /// Optional dependency table from the `DEPS` section.
    pub dependency_table: Option<DependencyTable>,
    /// Optional embedded resource table from the `EMBD` section.
    pub embedded_resource_table: Option<EmbeddedResourceTable>,
    /// Optional capability table from the `CAPS` section.
    pub capability_table: Option<CapabilityTable>,
    /// Optional signature table from the `SIGN` section.
    pub signature_table: Option<SignatureTable>,
    /// Optional source-file table from the `SRCF` section.
    pub source_file_table: Option<SourceFileTable>,
    /// Optional source-span table from the `SRCS` section.
    pub source_span_table: Option<SourceSpanTable>,
    /// Optional syntax-node table from the `ASTN` section.
    pub syntax_node_table: Option<SyntaxNodeTable>,
    /// Optional trivia table from the `TRIV` section.
    pub trivia_table: Option<TriviaTable>,
    /// Ordered sections contained in the file.
    pub sections: Vec<Section>,
    /// Root document records materialized from DOCS and/or VALS.
    pub documents: Vec<Document>,
    /// Optional bundle descriptors attached to the file.
    pub bundles: Vec<BundleDescriptor>,
}

impl LybaFile {
    /// Creates an empty file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a section and returns the updated file.
    #[must_use]
    pub fn with_section(mut self, section: Section) -> Self {
        self.sections.push(section);
        self
    }

    /// Appends a root document and returns the updated file.
    #[must_use]
    pub fn with_document(mut self, document: Document) -> Self {
        self.documents.push(document);
        self
    }

    /// Sets the level-1 string table.
    #[must_use]
    pub fn with_string_table(mut self, string_table: StringTable) -> Self {
        self.string_table = Some(string_table);
        self
    }

    /// Sets the extension declaration table.
    #[must_use]
    pub fn with_extension_table(mut self, extension_table: ExtensionTable) -> Self {
        self.extension_table = Some(extension_table);
        self
    }

    /// Sets the level-1 symbol table.
    #[must_use]
    pub fn with_symbol_table(mut self, symbol_table: SymbolTable) -> Self {
        self.symbol_table = Some(symbol_table);
        self
    }

    /// Sets the level-1 blob table.
    #[must_use]
    pub fn with_blob_table(mut self, blob_table: BlobTable) -> Self {
        self.blob_table = Some(blob_table);
        self
    }

    /// Sets the tag registry carried by the `TAGS` section.
    #[must_use]
    pub fn with_tag_table(mut self, tag_table: TagTable) -> Self {
        self.tag_table = Some(tag_table);
        self
    }

    /// Sets the schema table carried by the `SCMA` section.
    #[must_use]
    pub fn with_schema_table(mut self, schema_table: SchemaTable) -> Self {
        self.schema_table = Some(schema_table);
        self
    }

    /// Sets the stored diagnostics carried by the `DIAG` section.
    #[must_use]
    pub fn with_diagnostic_table(mut self, diagnostic_table: DiagnosticTable) -> Self {
        self.diagnostic_table = Some(diagnostic_table);
        self
    }

    /// Sets the dependency table carried by the `DEPS` section.
    #[must_use]
    pub fn with_dependency_table(mut self, dependency_table: DependencyTable) -> Self {
        self.dependency_table = Some(dependency_table);
        self
    }

    /// Sets the embedded resource table carried by the `EMBD` section.
    #[must_use]
    pub fn with_embedded_resource_table(
        mut self,
        embedded_resource_table: EmbeddedResourceTable,
    ) -> Self {
        self.embedded_resource_table = Some(embedded_resource_table);
        self
    }

    /// Sets the capability table carried by the `CAPS` section.
    #[must_use]
    pub fn with_capability_table(mut self, capability_table: CapabilityTable) -> Self {
        self.capability_table = Some(capability_table);
        self
    }

    /// Sets the signature table carried by the `SIGN` section.
    #[must_use]
    pub fn with_signature_table(mut self, signature_table: SignatureTable) -> Self {
        self.signature_table = Some(signature_table);
        self
    }

    /// Sets the source-file table carried by the `SRCF` section.
    #[must_use]
    pub fn with_source_file_table(mut self, source_file_table: SourceFileTable) -> Self {
        self.source_file_table = Some(source_file_table);
        self
    }

    /// Sets the source-span table carried by the `SRCS` section.
    #[must_use]
    pub fn with_source_span_table(mut self, source_span_table: SourceSpanTable) -> Self {
        self.source_span_table = Some(source_span_table);
        self
    }

    /// Sets the syntax-node table carried by the `ASTN` section.
    #[must_use]
    pub fn with_syntax_node_table(mut self, syntax_node_table: SyntaxNodeTable) -> Self {
        self.syntax_node_table = Some(syntax_node_table);
        self
    }

    /// Sets the trivia table carried by the `TRIV` section.
    #[must_use]
    pub fn with_trivia_table(mut self, trivia_table: TriviaTable) -> Self {
        self.trivia_table = Some(trivia_table);
        self
    }

    /// Sets metadata carried by the `META` section.
    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Original bytes and associated decoded model information for a document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentImage {
    /// Source bytes captured during reading.
    pub bytes: Vec<u8>,
    /// Optional nominal source name.
    pub source_name: Option<String>,
}

impl DocumentImage {
    /// Creates a new document image from bytes.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            source_name: None,
        }
    }

    /// Sets a source name for the image.
    #[must_use]
    pub fn with_source_name(mut self, source_name: impl Into<String>) -> Self {
        self.source_name = Some(source_name.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainerHeader, HEADER_LEN, HEADER_MAGIC, HeaderCrcMode};
    use crate::error::LybaError;

    fn valid_header_bytes() -> [u8; HEADER_LEN] {
        ContainerHeader::new()
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode")
    }

    #[test]
    fn decodes_and_reencodes_valid_header_with_crc() {
        let bytes = valid_header_bytes();

        let header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");

        assert_eq!(header.file_length, 64);
        assert_eq!(header.section_table_offset, 64);
        assert_ne!(header.header_crc32c, 0);
        assert_eq!(
            header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should reencode"),
            bytes
        );
    }

    #[test]
    fn rejects_invalid_magic_with_lb0001() {
        let mut bytes = valid_header_bytes();
        bytes[..HEADER_MAGIC.len()].copy_from_slice(b"LB0001\r\n");

        let error =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect_err("magic should fail");

        assert!(matches!(error, LybaError::InvalidMagic(_)));
        assert_eq!(error.code().as_str(), "LB0001");
    }

    #[test]
    fn rejects_unsupported_version_with_lb0002() {
        let mut bytes = valid_header_bytes();
        bytes[8..10].copy_from_slice(&9_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&9_u16.to_le_bytes());

        let error = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect_err("version should fail");

        assert!(matches!(error, LybaError::UnsupportedVersion(_)));
        assert_eq!(error.code().as_str(), "LB0002");
    }

    #[test]
    fn rejects_invalid_endian_marker_with_lb0003() {
        let mut bytes = valid_header_bytes();
        bytes[14..16].copy_from_slice(&0x0201_u16.to_le_bytes());

        let error = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect_err("endian marker should fail");

        assert!(matches!(error, LybaError::InvalidEndianMarker(_)));
        assert_eq!(error.code().as_str(), "LB0003");
    }

    #[test]
    fn rejects_invalid_header_size_with_lb0004() {
        let mut bytes = valid_header_bytes();
        bytes[12..14].copy_from_slice(&63_u16.to_le_bytes());

        let error = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect_err("header size should fail");

        assert!(matches!(error, LybaError::InvalidHeaderSize(_)));
        assert_eq!(error.code().as_str(), "LB0004");
    }

    #[test]
    fn rejects_reserved_header_bits_with_lb0025() {
        let mut bytes = valid_header_bytes();
        bytes[16..20].copy_from_slice(&(1_u32 << 10).to_le_bytes());

        let error = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect_err("reserved flags should fail");

        assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }
}
