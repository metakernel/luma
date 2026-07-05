//! Integration tests for Level 3 source span support.

use luma_lumba::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE};
use luma_lumba::primitives::UVar;
use luma_lumba::section::{
    CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use luma_lumba::{
    BlobId, BlobRecord, BlobTable, LumbaError, LumbaFile, ReadOptions, Reader,
    SOURCE_SPAN_FLAG_GENERATED, SOURCE_SPAN_FLAG_RESERVED_MASK, SOURCE_SPAN_FLAG_SYNTHETIC,
    SourceFileRecord, SourceFileTable, SourceSpanRecord, SourceSpanTable, WriteOptions, Writer,
};
use luma_syntax::{FileId, SourcePosition, Span};

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
    header.section_entry_size = SECTION_ENTRY_SIZE;
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

fn entry(
    section_id: SectionId,
    payload_offset: u64,
    payload_len: usize,
    item_count: u64,
) -> SectionEntry {
    SectionEntry {
        section_id,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset,
        stored_size: payload_len as u64,
        logical_size: payload_len as u64,
        item_count,
        checksum_low: 0,
        checksum_high: 0,
    }
}

fn empty_strs_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(0).encode_into(&mut payload);
    payload
}

fn srcf_payload(record_count: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(record_count).encode_into(&mut payload);
    for _ in 0..record_count {
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
    }
    payload
}

fn srcs_payload(records: &[(u64, u64, u64, u64, u64, u64, u64, u64)]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(records.len() as u64).encode_into(&mut payload);
    for (source_file_ref, byte_offset, byte_length, sl, sc, el, ec, flags) in records {
        UVar(*source_file_ref).encode_into(&mut payload);
        UVar(*byte_offset).encode_into(&mut payload);
        UVar(*byte_length).encode_into(&mut payload);
        UVar(*sl).encode_into(&mut payload);
        UVar(*sc).encode_into(&mut payload);
        UVar(*el).encode_into(&mut payload);
        UVar(*ec).encode_into(&mut payload);
        UVar(*flags).encode_into(&mut payload);
    }
    payload
}

#[test]
fn source_spans_round_trip_and_convert_when_data_is_available() {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"alpha\nbeta\n".to_vec()))
        .expect("blob should append");

    let file = LumbaFile::new()
        .with_blob_table(blobs)
        .with_source_file_table(
            SourceFileTable::new()
                .with_record(SourceFileRecord::new().with_source_blob_ref(Some(BlobId(0)))),
        )
        .with_source_span_table(
            SourceSpanTable::new().with_record(
                SourceSpanRecord::new(0, 0, 5)
                    .with_start_position(1, 1)
                    .with_end_position(1, 6)
                    .with_flags(SOURCE_SPAN_FLAG_GENERATED | SOURCE_SPAN_FLAG_SYNTHETIC),
            ),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("writer should encode SRCS"),
        )
        .expect("reader should decode SRCS");

    let span = &decoded
        .source_span_table
        .expect("SRCS should decode")
        .records[0];
    assert_eq!(span.source_file_ref, 0);
    assert_eq!(span.byte_offset, 0);
    assert_eq!(span.byte_length, 5);
    assert!(span.is_generated());
    assert!(span.is_synthetic());
    assert_eq!(span.to_luma_syntax_span(), Some(Span::new(FileId(0), 0, 5)));
    assert_eq!(
        span.start_position_to_luma_syntax(),
        Some(SourcePosition { line: 1, column: 1 })
    );
    assert_eq!(
        span.end_position_to_luma_syntax(),
        Some(SourcePosition { line: 1, column: 6 })
    );

    let rebuilt = SourceSpanRecord::from_luma_syntax(
        Span::new(FileId(0), 0, 5),
        SourcePosition { line: 1, column: 1 },
        SourcePosition { line: 1, column: 6 },
    )
    .expect("syntax-layer span should fit");
    assert_eq!(rebuilt.source_file_ref, 0);
    assert_eq!(rebuilt.byte_offset, 0);
    assert_eq!(rebuilt.byte_length, 5);
}

#[test]
fn reader_rejects_invalid_source_file_refs_with_lb0014() {
    let strs = empty_strs_payload();
    let srcf = srcf_payload(1);
    let srcs = srcs_payload(&[(1, 0, 1, 1, 1, 1, 2, 0)]);

    let error = Reader::new(ReadOptions::new())
        .read(&build_file(
            &[
                entry(SectionId::STRS, 256, strs.len(), 0),
                entry(SectionId::SRCF, 320, srcf.len(), 1),
                entry(SectionId::SRCS, 384, srcs.len(), 1),
            ],
            &[&strs, &srcf, &srcs],
            64,
        ))
        .expect_err("invalid source-file ref should fail");

    assert!(matches!(error, LumbaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_zero_line_or_column_with_lb0022() {
    let strs = empty_strs_payload();
    let srcf = srcf_payload(1);
    let srcs = srcs_payload(&[(0, 0, 1, 0, 1, 1, 2, 0)]);

    let error = Reader::new(ReadOptions::new())
        .read(&build_file(
            &[
                entry(SectionId::STRS, 256, strs.len(), 0),
                entry(SectionId::SRCF, 320, srcf.len(), 1),
                entry(SectionId::SRCS, 384, srcs.len(), 1),
            ],
            &[&strs, &srcf, &srcs],
            64,
        ))
        .expect_err("zero line should fail");

    assert!(matches!(error, LumbaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn writer_rejects_byte_ranges_outside_embedded_source_with_lb0022() {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"short".to_vec()))
        .expect("blob should append");

    let file = LumbaFile::new()
        .with_blob_table(blobs)
        .with_source_file_table(
            SourceFileTable::new()
                .with_record(SourceFileRecord::new().with_source_blob_ref(Some(BlobId(0)))),
        )
        .with_source_span_table(
            SourceSpanTable::new().with_record(
                SourceSpanRecord::new(0, 0, 99)
                    .with_start_position(1, 1)
                    .with_end_position(1, 100),
            ),
        );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("out-of-range embedded-source span should fail");

    assert!(matches!(error, LumbaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn writer_rejects_reserved_source_span_flags_with_lb0025() {
    let file = LumbaFile::new()
        .with_source_file_table(SourceFileTable::new().with_record(SourceFileRecord::new()))
        .with_source_span_table(
            SourceSpanTable::new().with_record(
                SourceSpanRecord::new(0, 0, 0)
                    .with_start_position(1, 1)
                    .with_end_position(1, 1)
                    .with_flags(SOURCE_SPAN_FLAG_RESERVED_MASK),
            ),
        );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("reserved span flags should fail");

    assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}
