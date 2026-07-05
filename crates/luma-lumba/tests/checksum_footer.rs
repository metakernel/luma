//! Integration tests for section checksum and footer handling.

use luma_lumba::LumbaFile;
use luma_lumba::container::{
    ContainerFooter, ContainerHeader, FOOTER_LEN, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE,
};
use luma_lumba::document::Document;
use luma_lumba::error::LumbaError;
use luma_lumba::read::{ReadOptions, Reader};
use luma_lumba::section::{CHECKSUM_CRC32C, CHECKSUM_NONE, SectionEntry};
use luma_lumba::value::Value;
use luma_lumba::write::{WriteOptions, Writer};

fn sample_file() -> LumbaFile {
    LumbaFile::new()
        .with_document(Document::new().with_root_value(Value::String(String::from("hello"))))
}

fn decode_first_entry(bytes: &[u8]) -> SectionEntry {
    SectionEntry::decode(
        &bytes[usize::from(HEADER_SIZE)..usize::from(HEADER_SIZE) + SECTION_ENTRY_SIZE as usize],
    )
    .expect("section entry should decode")
}

#[test]
fn valid_crc32c_section_checksum_round_trips() {
    let bytes = Writer::new(WriteOptions::new().with_section_checksum_id(CHECKSUM_CRC32C))
        .write(&sample_file())
        .expect("writer should encode crc32c checksums");

    let entry = decode_first_entry(&bytes);
    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should validate crc32c checksums");

    assert_eq!(entry.checksum_id, CHECKSUM_CRC32C);
    assert_ne!(entry.checksum_low, 0);
    assert_eq!(entry.checksum_high, 0);
    assert_eq!(decoded.documents, sample_file().documents);
}

#[test]
fn checksum_mismatch_returns_lb0011() {
    let mut bytes = Writer::new(WriteOptions::new().with_section_checksum_id(CHECKSUM_CRC32C))
        .write(&sample_file())
        .expect("writer should encode crc32c checksums");
    let entry = decode_first_entry(&bytes);
    bytes[entry.payload_offset as usize] ^= 0x01;

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("reader should reject mismatched checksum");

    assert!(matches!(error, LumbaError::ChecksumMismatch(_)));
    assert_eq!(error.code().as_str(), "LB0011");
}

#[test]
fn absent_checksum_round_trips_with_zero_metadata() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&sample_file())
        .expect("writer should encode without checksums");

    let entry = decode_first_entry(&bytes);
    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should accept absent checksums");

    assert_eq!(entry.checksum_id, CHECKSUM_NONE);
    assert_eq!(entry.checksum_low, 0);
    assert_eq!(entry.checksum_high, 0);
}

#[test]
fn valid_footer_is_written_and_read() {
    let bytes = Writer::new(WriteOptions::new().with_footer(true))
        .write(&sample_file())
        .expect("writer should encode footer");

    let header =
        ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");
    let footer =
        ContainerFooter::decode(&bytes[bytes.len() - FOOTER_LEN..]).expect("footer should decode");
    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should accept valid footer");

    footer
        .validate_against_header(&header)
        .expect("footer should agree with header");
    assert_eq!(header.file_length, bytes.len() as u64);
    assert_eq!(decoded.documents.len(), 1);
}

#[test]
fn footer_header_mismatch_is_rejected() {
    let bytes = Writer::new(WriteOptions::new().with_footer(true))
        .write(&sample_file())
        .expect("writer should encode footer");
    let mut corrupted = bytes.clone();
    let footer_start = corrupted.len() - FOOTER_LEN;
    let mut footer =
        ContainerFooter::decode(&corrupted[footer_start..]).expect("footer should decode");
    footer.section_count += 1;
    corrupted[footer_start..].copy_from_slice(&footer.encode());

    let error = Reader::new(ReadOptions::new())
        .read(&corrupted)
        .expect_err("reader should reject mismatched footer");

    assert!(matches!(error, LumbaError::InvalidSectionTable(_)));
}

#[test]
fn footer_discovery_works_for_header_only_files() {
    let bytes = Writer::new(WriteOptions::new().with_footer(true))
        .write(&LumbaFile::new())
        .expect("writer should encode header-only file with footer");

    let header =
        ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");
    let footer =
        ContainerFooter::decode(&bytes[bytes.len() - FOOTER_LEN..]).expect("footer should decode");
    let file = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should discover and validate footer");

    assert_eq!(bytes.len(), usize::from(HEADER_SIZE) + FOOTER_LEN);
    footer
        .validate_against_header(&header)
        .expect("discovered footer should agree with header");
    assert_eq!(file.documents.len(), 0);
}
