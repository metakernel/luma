//! Integration tests for Level 4 signature and digest support.

use lyma_lyba::container::{ContainerHeader, HeaderCrcMode};
use lyma_lyba::primitives::{Identifier, UVar};
use lyma_lyba::section::{SectionEntry, SectionId};
use lyma_lyba::value::{MapEntry, Value};
use lyma_lyba::{
    BlobId, BlobRecord, BlobTable, LybaError, LybaFile, ReadOptions, Reader,
    SIGNATURE_ALGORITHM_SHA256, SIGNATURE_COVERED_RANGE_KIND_EXPLICIT_SECTIONS,
    SIGNATURE_RECORD_KIND_CERTIFICATE_CHAIN, SIGNATURE_RECORD_KIND_DIGEST,
    SIGNATURE_RECORD_KIND_EXTENSION, SIGNATURE_RECORD_KIND_SIGNATURE,
    SIGNATURE_RECORD_KIND_TRANSPARENCY_RECORD, SignatureRecord, SignatureTable, SignatureVerifier,
    WriteOptions, Writer,
};

#[test]
fn sign_records_round_trip_and_structural_verification_is_inert() {
    let file = fixture_file();

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let records = &decoded
        .signature_table
        .as_ref()
        .expect("SIGN should decode")
        .records;

    assert_eq!(records.len(), 5);
    assert!(records[0].is_digest());
    assert!(records[1].is_signature());
    assert_eq!(records[2].kind, SIGNATURE_RECORD_KIND_CERTIFICATE_CHAIN);
    assert_eq!(records[3].kind, SIGNATURE_RECORD_KIND_TRANSPARENCY_RECORD);
    assert_eq!(records[4].kind, SIGNATURE_RECORD_KIND_EXTENSION);
    assert_eq!(
        records[0].algorithm.as_ref().map(Identifier::as_str),
        Some(SIGNATURE_ALGORITHM_SHA256)
    );
    assert_eq!(
        records[1].algorithm.as_ref().map(Identifier::as_str),
        Some("com.example.unknown-signature")
    );
    assert_eq!(records[0].payload_blob_ref, Some(BlobId(0)));
    assert_eq!(
        records[0].metadata_value,
        Some(Value::Map(vec![MapEntry {
            key: Value::String(String::from("label")),
            value: Value::String(String::from("digest")),
        }]))
    );

    let report = SignatureVerifier::new()
        .verify_structural_coverage(&decoded)
        .expect("structural verification should succeed");
    assert_eq!(report.records.len(), 5);
    assert_eq!(
        report.records[0]
            .covered_sections
            .iter()
            .map(|section| section.section_id)
            .collect::<Vec<_>>(),
        vec![SectionId::STRS, SectionId::BLOB, SectionId::VALS]
    );
    assert_eq!(
        report.records[1].algorithm.as_ref().map(Identifier::as_str),
        Some("com.example.unknown-signature")
    );
}

#[test]
fn writer_rejects_invalid_signature_covered_section_refs_with_lb0014() {
    let error = Writer::new(WriteOptions::new())
        .write(
            &fixture_file().with_signature_table(
                SignatureTable::new().with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_DIGEST)
                        .with_covered_section_refs(vec![99])
                        .with_payload_blob_ref(Some(BlobId(0))),
                ),
            ),
        )
        .expect_err("invalid covered section ref should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_invalid_signature_covered_section_refs_with_lb0014() {
    let mut bytes = Writer::new(WriteOptions::new())
        .write(&single_record_fixture())
        .expect("write should succeed");
    let mut payload = Vec::new();
    UVar(1).encode_into(&mut payload);
    UVar(SIGNATURE_RECORD_KIND_DIGEST).encode_into(&mut payload);
    UVar(0).encode_into(&mut payload);
    UVar(SIGNATURE_COVERED_RANGE_KIND_EXPLICIT_SECTIONS).encode_into(&mut payload);
    UVar(1).encode_into(&mut payload);
    UVar(99).encode_into(&mut payload);
    UVar(1).encode_into(&mut payload);
    UVar(0).encode_into(&mut payload);
    patch_sign_payload(&mut bytes, &payload);

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid covered section ref should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

fn fixture_file() -> LybaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(vec![0xAA; 32]))
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(vec![0xBB; 64]))
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(b"leaf-cert\nintermediate-cert".to_vec()))
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(b"transparency-entry".to_vec()))
        .expect("blob append should succeed");

    LybaFile::new()
        .with_blob_table(blob_table)
        .with_signature_table(
            SignatureTable::new()
                .with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_DIGEST)
                        .with_algorithm(Some(Identifier::new(SIGNATURE_ALGORITHM_SHA256)))
                        .with_covered_section_refs(vec![0, 2, 3])
                        .with_payload_blob_ref(Some(BlobId(0)))
                        .with_metadata_value(Some(Value::Map(vec![MapEntry {
                            key: Value::String(String::from("label")),
                            value: Value::String(String::from("digest")),
                        }]))),
                )
                .with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_SIGNATURE)
                        .with_algorithm(Some(Identifier::new("com.example.unknown-signature")))
                        .with_covered_section_refs(vec![0, 1, 2, 3])
                        .with_payload_blob_ref(Some(BlobId(1))),
                )
                .with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_CERTIFICATE_CHAIN)
                        .with_algorithm(Some(Identifier::new("x509")))
                        .with_covered_section_refs(vec![0, 1, 2, 3])
                        .with_payload_blob_ref(Some(BlobId(2))),
                )
                .with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_TRANSPARENCY_RECORD)
                        .with_algorithm(Some(Identifier::new("rfc9162")))
                        .with_covered_section_refs(vec![0, 1, 2, 3])
                        .with_payload_blob_ref(Some(BlobId(3))),
                )
                .with_record(
                    SignatureRecord::new(SIGNATURE_RECORD_KIND_EXTENSION)
                        .with_algorithm(Some(Identifier::new("com.example.extension.integrity")))
                        .with_covered_section_refs(vec![0, 1, 2, 3, 4]),
                ),
        )
}

fn single_record_fixture() -> LybaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(vec![0xAA; 32]))
        .expect("blob append should succeed");
    LybaFile::new()
        .with_blob_table(blob_table)
        .with_signature_table(
            SignatureTable::new().with_record(
                SignatureRecord::new(SIGNATURE_RECORD_KIND_DIGEST)
                    .with_covered_section_refs(vec![0])
                    .with_payload_blob_ref(Some(BlobId(0))),
            ),
        )
}

fn patch_sign_payload(bytes: &mut [u8], payload: &[u8]) {
    let header =
        ContainerHeader::decode(bytes, HeaderCrcMode::Enabled).expect("header should decode");
    for index in 0..header.section_count as usize {
        let start = 64 + index * 64;
        let end = start + 64;
        let entry = SectionEntry::decode(&bytes[start..end]).expect("entry should decode");
        if entry.section_id == SectionId::SIGN {
            let payload_offset = entry.payload_offset as usize;
            let payload_end = payload_offset + payload.len();
            bytes[payload_offset..payload_end].copy_from_slice(payload);
            return;
        }
    }
    panic!("SIGN section not found");
}
