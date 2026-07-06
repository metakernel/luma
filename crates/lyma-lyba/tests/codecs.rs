//! Compression codec integration tests.

use lyma_lyba::codec::{CODEC_NONE, CODEC_ZSTD};
use lyma_lyba::container::{
    ContainerHeader, HEADER_SIZE, HeaderCrcMode, LybaFile, validate_section_table,
};
use lyma_lyba::error::LybaError;
use lyma_lyba::primitives::UVar;
use lyma_lyba::read::{ReadOptions, Reader};
use lyma_lyba::section::{
    CHECKSUM_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use lyma_lyba::string_table::{StringRecord, StringTable};
use lyma_lyba::write::{CanonicalMode, WriteOptions, Writer, WriterMode};

fn build_file(entries: &[SectionEntry], payloads: &[&[u8]], table_offset: u64) -> Vec<u8> {
    let table = entries
        .iter()
        .flat_map(|entry| entry.encode().expect("entry should encode"))
        .collect::<Vec<_>>();
    let mut file_len = usize::from(HEADER_SIZE).max((table_offset as usize) + table.len());
    for (entry, payload) in entries.iter().zip(payloads) {
        file_len = file_len.max((entry.payload_offset as usize) + payload.len());
    }

    let mut header = ContainerHeader::new();
    header.section_table_offset = table_offset;
    header.section_count = entries.len() as u32;
    header.file_length = file_len as u64;

    let mut bytes = vec![0_u8; file_len];
    bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
        &header
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode"),
    );
    bytes[table_offset as usize..table_offset as usize + table.len()].copy_from_slice(&table);
    for (entry, payload) in entries.iter().zip(payloads) {
        bytes[entry.payload_offset as usize..entry.payload_offset as usize + payload.len()]
            .copy_from_slice(payload);
    }
    bytes
}

fn strs_payload(values: &[&str]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(values.len() as u64).encode_into(&mut payload);
    for value in values {
        UVar(0).encode_into(&mut payload);
        UVar(value.len() as u64).encode_into(&mut payload);
        payload.extend_from_slice(value.as_bytes());
    }
    payload
}

#[test]
fn required_nonzero_codec_fails_with_lb0010() {
    let payload = strs_payload(&["ok"]);
    let entry = SectionEntry {
        section_id: SectionId::STRS,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_ZSTD,
        checksum_id: CHECKSUM_NONE,
        payload_offset: 128,
        stored_size: payload.len() as u64,
        logical_size: payload.len() as u64,
        item_count: 1,
        checksum_low: 0,
        checksum_high: 0,
    };
    let bytes = build_file(&[entry], &[&payload], 64);

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("required unsupported codec should fail");

    assert!(matches!(error, LybaError::UnsupportedCodec(_)));
    assert_eq!(error.code().as_str(), "LB0010");
}

#[test]
fn optional_compressed_section_is_ignored() {
    let strs_payload = strs_payload(&["ok"]);
    let diag_payload = *b"skipdiag";
    let entries = [
        SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 192,
            stored_size: strs_payload.len() as u64,
            logical_size: strs_payload.len() as u64,
            item_count: 1,
            checksum_low: 0,
            checksum_high: 0,
        },
        SectionEntry {
            section_id: SectionId::DIAG,
            section_version: 1,
            entry_flags: SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_ZSTD,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 200,
            stored_size: diag_payload.len() as u64,
            logical_size: 64,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        },
    ];
    let bytes = build_file(&entries, &[&strs_payload, &diag_payload], 64);

    let file = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("optional unsupported codec should be ignored");

    assert_eq!(
        file.string_table.expect("STRS should decode").strings.len(),
        1
    );
    assert!(file.diagnostic_table.is_none());
}

#[test]
fn canonical_writer_emits_uncompressed_sections() {
    let file = LybaFile::new()
        .with_string_table(StringTable::new().with_string(StringRecord::new("hello")));
    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::Canonical(CanonicalMode::Strict))
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&file)
    .expect("writer should encode bytes");

    let header =
        ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
    let sections = validate_section_table(&header, &bytes).expect("sections should validate");

    assert!(!sections.is_empty());
    assert!(
        sections
            .iter()
            .all(|section| section.entry.codec_id == CODEC_NONE)
    );
    assert!(
        sections
            .iter()
            .all(|section| section.entry.logical_size == section.entry.stored_size)
    );
}
