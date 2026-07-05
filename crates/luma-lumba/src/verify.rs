//! Verification entry points and result types.

use crate::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, LumbaFile};
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::error::{ErrorContext, LumbaError, Result};
use crate::meta::decode_metadata;
use crate::policy::{Limits, ReservedFlagPolicy};
use crate::primitives::{SVar, UVar, read_bounded_bytes, read_u64_le};
use crate::section::{
    SECTION_FLAG_RESERVED_MASK, SectionEntry, SectionId, ValidatedSection, aligned_end,
    checked_table_len_bytes, compare_canonical_section_ids, supported_section_semantics,
};
use crate::string_table::STRING_FLAG_RESERVED_MASK;
use crate::symbol::{SYMBOL_FLAG_RESERVED_MASK, decode_symbol_table};
use crate::value::{VALUE_SECTION_NAME, Value, find_duplicate_canonical_map_key};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn verify_level1_minimal_value_image_file(file: &LumbaFile) -> Result<()> {
    if file.id.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry a file identifier",
        ));
    }
    if !file.bundles.is_empty() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry bundle metadata",
        ));
    }
    if file.metadata.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry metadata",
        ));
    }
    if file.tag_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry tag declarations",
        ));
    }
    if file.schema_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry schema declarations",
        ));
    }
    if file.dependency_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry dependency declarations",
        ));
    }
    if file.embedded_resource_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry embedded resources",
        ));
    }
    if file.capability_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry capability declarations",
        ));
    }
    if file.signature_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry signatures or digests",
        ));
    }
    if file.source_file_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry source file declarations",
        ));
    }
    if file.source_span_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry source span declarations",
        ));
    }
    if file.syntax_node_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry syntax node declarations",
        ));
    }
    if file.trivia_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry trivia declarations",
        ));
    }
    if file.extension_table.is_some() {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images cannot carry extension declarations",
        ));
    }
    if file
        .sections
        .iter()
        .any(|section| section.name.as_str() != VALUE_SECTION_NAME)
    {
        return Err(LumbaError::invalid_section_table(
            "level1 minimal value images can only materialize VALS sections",
        ));
    }
    if file.documents.iter().any(|document| {
        document.root_value.is_none()
            || document.schema_ref.is_some()
            || document.capability_set_ref.is_some()
    }) {
        return Err(LumbaError::InvalidDocumentTable(
            crate::error::ErrorContext::new(
                "level1 minimal value images require every DOCS record to reference a root value and omit schema/capability-set references",
            ),
        ));
    }

    Ok(())
}

/// Verification output for a document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerificationReport {
    /// Diagnostics emitted during verification.
    pub diagnostics: Vec<Diagnostic>,
}

impl VerificationReport {
    /// Returns true when no diagnostics were emitted.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Verifier for structural and policy checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Verifier;

impl Verifier {
    /// Creates a verifier.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Verifies the provided file.
    pub fn verify(&self, file: &LumbaFile) -> Result<VerificationReport> {
        let mut report = VerificationReport::default();

        if let Some(string_table) = &file.string_table {
            let mut first_seen = BTreeMap::<&str, usize>::new();
            for (index, record) in string_table.strings.iter().enumerate() {
                if let Some(previous) = first_seen.get(record.value.as_str()) {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::NonCanonicalEncoding,
                            format!(
                                "duplicate string at STRS record {index}; first seen at record {previous}"
                            ),
                        )
                        .with_record_index(index),
                    );
                } else {
                    first_seen.insert(record.value.as_str(), index);
                }
            }
        }

        if let Some(symbol_table) = &file.symbol_table {
            let string_count = file
                .string_table
                .as_ref()
                .map(|table| table.strings.len())
                .unwrap_or(0);
            let mut first_seen = BTreeMap::<(u64, Option<u64>), usize>::new();
            for (index, record) in symbol_table.symbols.iter().enumerate() {
                if record.flags & SYMBOL_FLAG_RESERVED_MASK != 0 {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::InvalidReservedFlags,
                            "reserved symbol flag bits were non-zero",
                        )
                        .with_record_index(index),
                    );
                }
                if record.string_id >= string_count as u64 {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::InvalidValueReference,
                            "symbol string reference was out of range",
                        )
                        .with_record_index(index),
                    );
                }
                if record
                    .namespace_string_id
                    .is_some_and(|value| value >= string_count as u64)
                {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::InvalidValueReference,
                            "symbol namespace reference was out of range",
                        )
                        .with_record_index(index),
                    );
                }
                let key = (record.string_id, record.namespace_string_id);
                if let Some(previous) = first_seen.get(&key) {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::NonCanonicalEncoding,
                            format!(
                                "duplicate symbol at SYMS record {index}; first seen at record {previous}"
                            ),
                        )
                        .with_record_index(index),
                    );
                } else {
                    first_seen.insert(key, index);
                }
            }
        }

        for section in &file.sections {
            if section.name.as_str() == VALUE_SECTION_NAME {
                if let Some(record_index) = find_duplicate_canonical_map_key(&section.values) {
                    report.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::DuplicateKeyInCanonicalMap,
                            "duplicate canonical map key in VALS section",
                        )
                        .with_record_index(record_index),
                    );
                }
            }
        }

        Ok(report)
    }

    /// Verifies that the provided bytes are canonically encoded.
    pub fn verify_canonical(&self, bytes: &[u8]) -> Result<()> {
        let header = ContainerHeader::decode_with_reserved_flag_policy(
            bytes,
            HeaderCrcMode::Disabled,
            ReservedFlagPolicy::AllowFuture,
        )?;
        verify_canonical_header(&header, bytes)?;
        let sections = decode_sections_relaxed(&header, bytes)?;
        verify_canonical_sections(&header, &sections, bytes)?;
        verify_canonical_payloads(&sections)?;

        let file = crate::read::Reader::new(
            crate::read::ReadOptions::new().with_header_crc_mode(HeaderCrcMode::Disabled),
        )
        .read(bytes)?;
        let report = self.verify(&file)?;
        if let Some(diagnostic) = report.diagnostics.first() {
            return Err(diagnostic_to_error(diagnostic));
        }

        Ok(())
    }
}

fn verify_canonical_header(header: &ContainerHeader, bytes: &[u8]) -> Result<()> {
    if header.container_flags != 0 || header.profile_flags != 0 {
        return Err(noncanonical(
            "canonical headers require zero container and profile flags",
        ));
    }
    if bytes[60..64] != [0, 0, 0, 0] {
        return Err(noncanonical(
            "canonical headers require a zero reserved field",
        ));
    }
    if header.header_crc32c != 0 {
        return Err(noncanonical(
            "canonical headers omit nondeterministic header CRC metadata",
        ));
    }
    if header.section_table_offset != u64::from(HEADER_SIZE) {
        return Err(noncanonical(
            "canonical headers place the section table immediately after the fixed header",
        ));
    }

    Ok(())
}

fn decode_sections_relaxed<'a>(
    header: &ContainerHeader,
    bytes: &'a [u8],
) -> Result<Vec<ValidatedSection<'a>>> {
    let table_len = checked_table_len_bytes(header.section_count, header.section_entry_size)?;
    let table_end = header
        .section_table_offset
        .checked_add(table_len)
        .ok_or_else(|| {
            LumbaError::offset_outside_file("section table offset plus length overflowed")
        })?;
    if table_end > bytes.len() as u64 {
        return Err(LumbaError::offset_outside_file(
            "section table extends beyond available input",
        ));
    }

    let table_start = usize::try_from(header.section_table_offset).map_err(|_| {
        LumbaError::offset_outside_file("section table offset was not representable")
    })?;
    let table_end = usize::try_from(table_end)
        .map_err(|_| LumbaError::offset_outside_file("section table end was not representable"))?;
    let table_bytes = bytes
        .get(table_start..table_end)
        .ok_or_else(|| LumbaError::offset_outside_file("section table bytes were not available"))?;

    let mut sections = Vec::with_capacity(header.section_count as usize);
    let mut spans = vec![
        (0_u64, u64::from(HEADER_SIZE), false),
        (header.section_table_offset, table_end as u64, false),
    ];

    for entry_bytes in table_bytes.chunks_exact(crate::container::SECTION_ENTRY_SIZE as usize) {
        let entry = SectionEntry::decode_with_reserved_flag_policy(
            entry_bytes,
            ReservedFlagPolicy::AllowFuture,
        )?;
        let (payload_start, payload_end) = entry.payload_range()?;
        if payload_end > bytes.len() as u64 {
            return Err(LumbaError::offset_outside_file(format!(
                "section {} payload extends beyond available input",
                entry.section_id.as_str()
            )));
        }
        let payload = entry.payload_slice(bytes)?;
        sections.push(ValidatedSection { entry, payload });
        spans.push((payload_start, payload_end, true));
    }

    spans.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    for pair in spans.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(LumbaError::OverlappingSections(ErrorContext::new(
                "section spans overlapped",
            )));
        }
    }

    Ok(sections)
}

fn verify_canonical_sections(
    header: &ContainerHeader,
    sections: &[ValidatedSection<'_>],
    bytes: &[u8],
) -> Result<()> {
    let mut previous: Option<SectionEntry> = None;
    let mut seen_unique = BTreeSet::new();
    let mut spans = vec![(0_u64, u64::from(HEADER_SIZE), false)];
    let table_end = header.section_table_offset
        + checked_table_len_bytes(sections.len() as u32, crate::container::SECTION_ENTRY_SIZE)?;
    spans.push((header.section_table_offset, table_end, false));

    for section in sections {
        let entry = section.entry;
        if let Some(previous_entry) = previous {
            if compare_canonical_section_ids(previous_entry.section_id, entry.section_id).is_gt() {
                return Err(noncanonical(
                    "section table entries were not in canonical order",
                ));
            }
        }
        previous = Some(entry);

        if entry.entry_flags & SECTION_FLAG_RESERVED_MASK != 0 {
            return Err(noncanonical(
                "canonical section entries require zero reserved flag bits",
            ));
        }
        if entry.payload_flags != 0 {
            return Err(noncanonical(
                "canonical section entries require zero payload flags",
            ));
        }
        if entry.codec_id != 0 {
            return Err(noncanonical(
                "canonical section entries require uncompressed payloads",
            ));
        }
        if entry.logical_size != entry.stored_size {
            return Err(noncanonical(
                "canonical section entries require logical and stored sizes to match",
            ));
        }
        if entry.payload_offset % 8 != 0 {
            return Err(noncanonical(
                "canonical section payloads must be 8-byte aligned",
            ));
        }
        if entry.checksum_id != 0 || entry.checksum_low != 0 || entry.checksum_high != 0 {
            return Err(noncanonical(
                "canonical section entries omit nondeterministic checksum metadata",
            ));
        }

        if supported_section_semantics(entry.section_id)
            .map(|semantics| semantics.unique)
            .unwrap_or(entry.is_unique())
            || entry.is_unique()
        {
            if !seen_unique.insert(entry.section_id) {
                return Err(noncanonical(
                    "canonical section tables do not duplicate unique sections",
                ));
            }
        }

        let (payload_start, payload_end) = entry.payload_range()?;
        spans.push((payload_start, payload_end, true));
    }

    spans.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    for pair in spans.windows(2) {
        let current = pair[0];
        let next = pair[1];
        let gap_end = if current.2 {
            aligned_end(current.0, current.1 - current.0)?
        } else {
            current.1
        }
        .min(next.0);
        verify_zero_gap_bytes_canonical(bytes, current.1, gap_end)?;
    }
    if let Some(last) = spans.last().copied() {
        if last.2 {
            verify_zero_gap_bytes_canonical(
                bytes,
                last.1,
                aligned_end(last.0, last.1 - last.0)?.min(bytes.len() as u64),
            )?;
        }
    }

    Ok(())
}

fn verify_canonical_payloads(sections: &[ValidatedSection<'_>]) -> Result<()> {
    let limits = Limits::public();
    let mut string_count = None;
    let mut vals_record_count = None;

    for section in sections {
        match section.entry.section_id {
            SectionId::META => verify_canonical_meta_payload(section, &limits)?,
            SectionId::STRS => {
                string_count = Some(verify_canonical_strs_payload(section, &limits)?);
            }
            SectionId::SYMS => verify_canonical_syms_payload(section, &limits, string_count)?,
            SectionId::VALS => {
                let count = verify_canonical_vals_payload(section, &limits)?;
                vals_record_count = Some(count);
            }
            SectionId::DOCS => verify_canonical_docs_payload(section, vals_record_count)?,
            _ => {}
        }
    }

    Ok(())
}

fn verify_canonical_meta_payload(section: &ValidatedSection<'_>, limits: &Limits) -> Result<()> {
    verify_canonical_vals_payload(section, limits)?;
    let metadata = decode_metadata(section.payload, limits)?;
    let Some(Value::String(_)) = metadata.get("format") else {
        return Err(LumbaError::invalid_section_table(
            "META metadata must include a string format entry",
        ));
    };
    Ok(())
}

fn verify_canonical_strs_payload(section: &ValidatedSection<'_>, limits: &Limits) -> Result<usize> {
    let mut offset = 0_usize;
    let string_count = decode_canonical_uvar(section.payload, &mut offset)?.0;
    let string_count = usize::try_from(string_count)
        .map_err(|_| LumbaError::limit_exceeded("string count exceeds configured maximum"))?;
    if string_count > limits.max_string_count {
        return Err(LumbaError::limit_exceeded(
            "string count exceeds configured maximum",
        ));
    }
    if section.entry.item_count != string_count as u64 {
        return Err(noncanonical(
            "STRS item_count did not match canonical payload record count",
        ));
    }

    for _ in 0..string_count {
        let flags = decode_canonical_uvar(section.payload, &mut offset)?.0;
        if flags & STRING_FLAG_RESERVED_MASK != 0 {
            return Err(noncanonical(
                "canonical STRS records require zero reserved flag bits",
            ));
        }
        let len = decode_canonical_uvar(section.payload, &mut offset)?.0;
        let len = usize::try_from(len)
            .map_err(|_| LumbaError::limit_exceeded("string length exceeds configured maximum"))?;
        let bytes = read_bounded_bytes(section.payload, &mut offset, len, limits.max_string_bytes)?;
        core::str::from_utf8(bytes)
            .map_err(|_| LumbaError::invalid_utf8("string bytes were not valid UTF-8"))?;
    }
    if offset != section.payload.len() {
        return Err(LumbaError::invalid_section_table(
            "string table payload had trailing bytes",
        ));
    }

    Ok(string_count)
}

fn verify_canonical_syms_payload(
    section: &ValidatedSection<'_>,
    limits: &Limits,
    string_count: Option<usize>,
) -> Result<()> {
    let mut offset = 0_usize;
    let symbol_count = decode_canonical_uvar(section.payload, &mut offset)?.0;
    let symbol_count = usize::try_from(symbol_count)
        .map_err(|_| LumbaError::limit_exceeded("symbol count exceeds configured maximum"))?;
    if symbol_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "symbol count exceeds configured maximum",
        ));
    }
    if section.entry.item_count != symbol_count as u64 {
        return Err(noncanonical(
            "SYMS item_count did not match canonical payload record count",
        ));
    }

    let string_count = string_count.unwrap_or(0);
    let table =
        decode_symbol_table(section.payload, limits, string_count).map_err(
            |error| match error {
                LumbaError::InvalidReservedFlags(_) => {
                    noncanonical("canonical SYMS records require zero reserved flag bits")
                }
                LumbaError::InvalidValueReference(_) => error,
                _ => error,
            },
        )?;

    let mut previous = None;
    for record in &table.symbols {
        let encoded_namespace = record
            .namespace_string_id
            .map(|value| value + 1)
            .unwrap_or(0);
        let key = (record.string_id, encoded_namespace, record.flags);
        if let Some(previous_key) = previous {
            if previous_key > key {
                return Err(noncanonical(
                    "SYMS records were not in canonical reference order",
                ));
            }
        }
        previous = Some(key);
        if record.flags & SYMBOL_FLAG_RESERVED_MASK != 0 {
            return Err(noncanonical(
                "canonical SYMS records require zero reserved flag bits",
            ));
        }
    }

    Ok(())
}

fn verify_canonical_vals_payload(section: &ValidatedSection<'_>, limits: &Limits) -> Result<usize> {
    const RECORD_NULL: u64 = 0;
    const RECORD_BOOL_FALSE: u64 = 1;
    const RECORD_BOOL_TRUE: u64 = 2;
    const RECORD_INT: u64 = 3;
    const RECORD_UINT: u64 = 4;
    const RECORD_FLOAT64: u64 = 5;
    const RECORD_STRING: u64 = 6;
    const RECORD_SEQUENCE: u64 = 7;
    const RECORD_MAP: u64 = 8;
    const RECORD_TAGGED: u64 = 9;

    let mut offset = 0_usize;
    let value_count = decode_canonical_uvar(section.payload, &mut offset)?.0;
    let value_count = usize::try_from(value_count)
        .map_err(|_| LumbaError::limit_exceeded("value count exceeds configured maximum"))?;
    if value_count > limits.max_value_count {
        return Err(LumbaError::limit_exceeded(
            "value count exceeds configured maximum",
        ));
    }
    if section.entry.item_count != value_count as u64 {
        return Err(noncanonical(
            "VALS item_count did not match canonical payload record count",
        ));
    }

    let offset_table_end = offset
        .checked_add(value_count.checked_mul(8).ok_or_else(|| {
            LumbaError::invalid_section_table("value offset table length overflowed")
        })?)
        .ok_or_else(|| LumbaError::invalid_section_table("value record start overflowed"))?;
    if offset_table_end > section.payload.len() {
        return Err(LumbaError::invalid_section_table(
            "value offset table extended beyond payload",
        ));
    }

    let mut table_offset = offset;
    let mut record_offsets = Vec::with_capacity(value_count);
    for _ in 0..value_count {
        let relative = read_u64_le(section.payload, &mut table_offset)?;
        record_offsets.push(usize::try_from(relative).map_err(|_| {
            LumbaError::invalid_section_table("value record offset was not representable")
        })?);
    }
    let records_len = section.payload.len() - offset_table_end;
    for record_index in 0..value_count {
        let record_offset = record_offsets[record_index];
        let next_offset = if record_index + 1 < value_count {
            record_offsets[record_index + 1]
        } else {
            records_len
        };
        if record_offset > next_offset || next_offset > records_len {
            return Err(LumbaError::invalid_section_table(
                "value record offsets were invalid",
            ));
        }
        let record = section
            .payload
            .get(offset_table_end + record_offset..offset_table_end + next_offset)
            .ok_or_else(|| {
                LumbaError::invalid_section_table("value record bounds were outside the payload")
            })?;
        if record.is_empty() {
            return Err(LumbaError::invalid_section_table(
                "value records must not be empty",
            ));
        }
        let mut record_offset = 0_usize;
        let tag = decode_canonical_uvar(record, &mut record_offset)?.0;
        match tag {
            RECORD_NULL | RECORD_BOOL_FALSE | RECORD_BOOL_TRUE => {}
            RECORD_INT => {
                let _ = decode_canonical_svar(record, &mut record_offset)?;
            }
            RECORD_UINT => {
                let _ = decode_canonical_uvar(record, &mut record_offset)?;
            }
            RECORD_FLOAT64 => {
                let bytes = read_bounded_bytes(record, &mut record_offset, 8, 8)?;
                let value = f64::from_bits(u64::from_le_bytes(
                    bytes.try_into().expect("length checked"),
                ));
                if !value.is_finite() {
                    return Err(noncanonical("canonical values require finite floats"));
                }
            }
            RECORD_STRING => {
                let len = decode_canonical_uvar(record, &mut record_offset)?.0;
                let len = usize::try_from(len).map_err(|_| {
                    LumbaError::limit_exceeded("string length exceeds configured maximum")
                })?;
                let bytes =
                    read_bounded_bytes(record, &mut record_offset, len, limits.max_string_bytes)?;
                core::str::from_utf8(bytes)
                    .map_err(|_| LumbaError::invalid_utf8("string bytes were not valid UTF-8"))?;
            }
            RECORD_SEQUENCE => {
                let len = decode_canonical_uvar(record, &mut record_offset)?.0;
                let len = usize::try_from(len).map_err(|_| {
                    LumbaError::limit_exceeded("value count exceeds configured maximum")
                })?;
                for _ in 0..len {
                    let value_ref = decode_canonical_uvar(record, &mut record_offset)?.0;
                    if value_ref >= value_count as u64 {
                        return Err(LumbaError::InvalidValueReference(ErrorContext::new(
                            "value reference pointed outside the value arena",
                        )));
                    }
                }
            }
            RECORD_MAP => {
                let len = decode_canonical_uvar(record, &mut record_offset)?.0;
                let len = usize::try_from(len).map_err(|_| {
                    LumbaError::limit_exceeded("value count exceeds configured maximum")
                })?;
                for _ in 0..len {
                    for _ in 0..2 {
                        let value_ref = decode_canonical_uvar(record, &mut record_offset)?.0;
                        if value_ref >= value_count as u64 {
                            return Err(LumbaError::InvalidValueReference(ErrorContext::new(
                                "value reference pointed outside the value arena",
                            )));
                        }
                    }
                }
            }
            RECORD_TAGGED => {
                let len = decode_canonical_uvar(record, &mut record_offset)?.0;
                let len = usize::try_from(len).map_err(|_| {
                    LumbaError::limit_exceeded("string length exceeds configured maximum")
                })?;
                let bytes =
                    read_bounded_bytes(record, &mut record_offset, len, limits.max_string_bytes)?;
                core::str::from_utf8(bytes)
                    .map_err(|_| LumbaError::invalid_utf8("tag bytes were not valid UTF-8"))?;
                let value_ref = decode_canonical_uvar(record, &mut record_offset)?.0;
                if value_ref >= value_count as u64 {
                    return Err(LumbaError::InvalidValueReference(ErrorContext::new(
                        "value reference pointed outside the value arena",
                    )));
                }
            }
            _ => {
                return Err(LumbaError::invalid_section_table(format!(
                    "unknown value record tag {tag}"
                )));
            }
        }
        if record_offset != record.len() {
            return Err(LumbaError::invalid_section_table(
                "value record had trailing bytes",
            ));
        }
    }

    Ok(value_count)
}

fn verify_canonical_docs_payload(
    section: &ValidatedSection<'_>,
    vals_record_count: Option<usize>,
) -> Result<()> {
    let mut offset = 0_usize;
    let document_count = decode_canonical_uvar(section.payload, &mut offset)?.0;
    if section.entry.item_count != document_count {
        return Err(noncanonical(
            "DOCS item_count did not match canonical payload record count",
        ));
    }
    for _ in 0..document_count {
        let flags = decode_canonical_uvar(section.payload, &mut offset)?.0;
        if flags & crate::document::DOCUMENT_FLAG_HAS_VALUE_ROOT != 0 {
            let value_ref = decode_canonical_uvar(section.payload, &mut offset)?.0;
            if let Some(value_count) = vals_record_count {
                if value_ref >= value_count as u64 {
                    return Err(LumbaError::InvalidValueReference(ErrorContext::new(
                        "document root value reference was out of range",
                    )));
                }
            }
        }
        if flags & crate::document::DOCUMENT_FLAG_HAS_SCHEMA != 0 {
            let _ = decode_canonical_uvar(section.payload, &mut offset)?;
        }
        if flags & crate::document::DOCUMENT_FLAG_HAS_CAPABILITY_SET != 0 {
            let _ = decode_canonical_uvar(section.payload, &mut offset)?;
        }
    }
    if offset != section.payload.len() {
        return Err(LumbaError::InvalidDocumentTable(ErrorContext::new(
            "document table contained trailing bytes",
        )));
    }

    Ok(())
}

fn verify_zero_gap_bytes_canonical(bytes: &[u8], gap_start: u64, gap_end: u64) -> Result<()> {
    if gap_start >= gap_end {
        return Ok(());
    }
    let start = usize::try_from(gap_start)
        .map_err(|_| LumbaError::offset_outside_file("gap start was not representable"))?;
    let end = usize::try_from(gap_end)
        .map_err(|_| LumbaError::offset_outside_file("gap end was not representable"))?;
    let gap = bytes.get(start..end).ok_or_else(|| {
        LumbaError::offset_outside_file("gap bytes extended beyond available input")
    })?;
    if gap.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(noncanonical("canonical padding bytes must be zero"))
    }
}

fn decode_canonical_uvar(input: &[u8], offset: &mut usize) -> Result<UVar> {
    UVar::decode_canonical(input, offset)
}

fn decode_canonical_svar(input: &[u8], offset: &mut usize) -> Result<SVar> {
    SVar::decode_canonical(input, offset)
}

fn noncanonical(message: impl Into<String>) -> LumbaError {
    LumbaError::non_canonical_encoding(message)
}

fn diagnostic_to_error(diagnostic: &Diagnostic) -> LumbaError {
    let mut context = ErrorContext::new(diagnostic.message.clone());
    context.record_index = diagnostic.record_index;
    match diagnostic.code {
        DiagnosticCode::DuplicateKeyInCanonicalMap => {
            LumbaError::DuplicateKeyInCanonicalMap(context)
        }
        _ => LumbaError::NonCanonicalEncoding(context),
    }
}

#[cfg(test)]
mod tests {
    use super::Verifier;
    use crate::container::LumbaFile;
    use crate::meta::Metadata;
    use crate::primitives::Identifier;
    use crate::section::Section;
    use crate::string_table::{StringRecord, StringTable};
    use crate::value::{MapEntry, Value};

    #[test]
    fn verifier_rejects_duplicate_strings_as_non_canonical_lb0017() {
        let file = LumbaFile::new().with_string_table(
            StringTable::new()
                .with_string(StringRecord::new("dup"))
                .with_string(StringRecord::new("dup")),
        );

        let report = Verifier::new()
            .verify(&file)
            .expect("verification should succeed");

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code.as_str(), "LB0017");
        assert_eq!(report.diagnostics[0].record_index, Some(1));
    }

    #[test]
    fn verifier_rejects_duplicate_canonical_map_keys_as_lb0016() {
        let file = LumbaFile::new().with_section(Section {
            name: Identifier::new("VALS"),
            values: vec![Value::Map(vec![
                MapEntry {
                    key: Value::String(String::from("dup")),
                    value: Value::Int(1),
                },
                MapEntry {
                    key: Value::String(String::from("dup")),
                    value: Value::Int(2),
                },
            ])],
        });

        let report = Verifier::new()
            .verify(&file)
            .expect("verification should succeed");

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code.as_str(), "LB0016");
        assert_eq!(report.diagnostics[0].record_index, Some(0));
    }

    #[test]
    fn level1_minimal_verifier_rejects_metadata() {
        let error = super::verify_level1_minimal_value_image_file(&LumbaFile::new().with_metadata(
            Metadata::new().with_entry("format", Value::String(String::from("lumba"))),
        ))
        .expect_err("metadata should be rejected");

        assert_eq!(error.code().as_str(), "LB0005");
    }
}
