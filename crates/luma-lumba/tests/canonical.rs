//! Canonical verification integration tests.

use luma_lumba::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode};
use luma_lumba::primitives::UVar;
use luma_lumba::read::{ReadOptions, Reader};
use luma_lumba::section::{
    CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use luma_lumba::string_table::{StringRecord, StringTable};
use luma_lumba::value::Value;
use luma_lumba::verify::Verifier;
use luma_lumba::write::{CanonicalMode, WriteOptions, Writer, WriterMode};
use luma_lumba::{Document, LumbaFile};

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

fn strs_entry(payload_len: usize, item_count: u64) -> SectionEntry {
    SectionEntry {
        section_id: SectionId::STRS,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset: 128,
        stored_size: payload_len as u64,
        logical_size: payload_len as u64,
        item_count,
        checksum_low: 0,
        checksum_high: 0,
    }
}

#[test]
fn canonical_verifier_accepts_strict_canonical_writer_output() {
    let file = LumbaFile::new()
        .with_string_table(StringTable::new().with_string(StringRecord::new("hello")))
        .with_document(Document::new().with_root_value(Value::String(String::from("hello"))));

    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::Canonical(CanonicalMode::Strict))
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&file)
    .expect("writer should encode canonical bytes");

    Verifier::new()
        .verify_canonical(&bytes)
        .expect("canonical bytes should verify");
}

#[test]
fn canonical_verifier_rejects_nonzero_header_crc_with_lb0017_while_reader_can_read() {
    let file = LumbaFile::new().with_document(Document::new().with_root_value(Value::Int(7)));
    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("default writer should encode bytes");

    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should still read noncanonical bytes");
    let error = Verifier::new()
        .verify_canonical(&bytes)
        .expect_err("header CRC metadata should be noncanonical");

    assert_eq!(error.code().as_str(), "LB0017");
}

#[test]
fn canonical_verifier_rejects_duplicate_strings_with_lb0017_while_reader_can_read() {
    let mut payload = Vec::new();
    UVar(2).encode_into(&mut payload);
    for value in [b"dup".as_slice(), b"dup".as_slice()] {
        UVar(0).encode_into(&mut payload);
        UVar(value.len() as u64).encode_into(&mut payload);
        payload.extend_from_slice(value);
    }
    let entry = strs_entry(payload.len(), 2);
    let bytes = build_file(&[entry], &[&payload], 64);

    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should accept duplicate strings");
    let error = Verifier::new()
        .verify_canonical(&bytes)
        .expect_err("duplicate strings should be noncanonical");

    assert_eq!(error.code().as_str(), "LB0017");
}

#[test]
fn canonical_verifier_rejects_nonminimal_varints_with_lb0017_while_reader_can_read() {
    let payload = vec![0x81, 0x00, 0x00, 0x00];
    let entry = strs_entry(payload.len(), 1);
    let bytes = build_file(&[entry], &[&payload], 64);

    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should accept relaxed varints");
    let error = Verifier::new()
        .verify_canonical(&bytes)
        .expect_err("non-minimal varints should be noncanonical");

    assert_eq!(error.code().as_str(), "LB0017");
}
