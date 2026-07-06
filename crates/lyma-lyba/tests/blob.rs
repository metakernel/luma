//! Blob section integration tests.

use lyma_lyba::{
    BLOB_FLAG_GENERATED, BLOB_FLAG_LUA_SOURCE, BLOB_FLAG_RESERVED_MASK, BLOB_FLAG_SOURCE_TEXT,
    BLOB_FLAG_UTF8_TEXT, BlobId, BlobRecord, BlobTable, CanonicalMode, LybaError, LybaFile,
    ReadOptions, Reader, Value, WriteOptions, Writer, WriterMode,
};

fn value_section(values: Vec<Value>) -> lyma_lyba::section::Section {
    let mut section = lyma_lyba::section::Section::new("VALS");
    for value in values {
        section = section.with_value(value);
    }
    section
}

fn write_then_read(file: &LybaFile, mode: WriterMode) -> lyma_lyba::Result<LybaFile> {
    let bytes = Writer::new(WriteOptions::new().with_mode(mode)).write(file)?;
    Reader::new(ReadOptions::new()).read(&bytes)
}

#[test]
fn canonical_writer_keeps_small_bytes_inline() {
    let file = LybaFile::new().with_section(value_section(vec![Value::BytesInline(vec![7; 64])]));

    let decoded = write_then_read(&file, WriterMode::Canonical(CanonicalMode::Strict))
        .expect("canonical round trip should succeed");

    assert!(decoded.blob_table.is_none());
    assert_eq!(
        decoded.sections[0].values,
        vec![Value::BytesInline(vec![7; 64])]
    );
}

#[test]
fn canonical_writer_promotes_large_inline_bytes_into_blob_section() {
    let file = LybaFile::new().with_section(value_section(vec![Value::BytesInline(vec![9; 65])]));

    let decoded = write_then_read(&file, WriterMode::Canonical(CanonicalMode::Strict))
        .expect("canonical round trip should succeed");

    let blob_table = decoded.blob_table.expect("BLOB should be present");
    assert_eq!(blob_table.records.len(), 1);
    assert_eq!(blob_table.records[0].as_bytes(), vec![9; 65].as_slice());
    assert_eq!(
        decoded.sections[0].values,
        vec![Value::BytesBlob(BlobId(0))]
    );
}

#[test]
fn writer_rejects_invalid_blob_refs_with_lb0014() {
    let file = LybaFile::new().with_section(value_section(vec![Value::BytesBlob(BlobId(0))]));

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("invalid blob ref should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn blob_flags_round_trip_for_utf8_source_lua_generated() {
    let file = LybaFile::new()
        .with_blob_table(BlobTable {
            records: vec![BlobRecord::new(b"print('hello')".to_vec()).with_flags(
                BLOB_FLAG_UTF8_TEXT
                    | BLOB_FLAG_SOURCE_TEXT
                    | BLOB_FLAG_LUA_SOURCE
                    | BLOB_FLAG_GENERATED,
            )],
        })
        .with_section(value_section(vec![Value::BytesBlob(BlobId(0))]));

    let decoded = write_then_read(&file, WriterMode::Pretty).expect("round trip should succeed");
    let blob = &decoded.blob_table.expect("BLOB should decode").records[0];

    assert!(blob.is_utf8_text());
    assert!(blob.is_source_text());
    assert!(blob.is_lua_source());
    assert!(blob.is_generated());
}

#[test]
fn reserved_blob_flags_are_rejected_with_lb0025() {
    let file = LybaFile::new()
        .with_blob_table(BlobTable {
            records: vec![BlobRecord::new(Vec::new()).with_flags(BLOB_FLAG_RESERVED_MASK)],
        })
        .with_section(value_section(vec![Value::BytesBlob(BlobId(0))]));

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("reserved flags should fail");

    assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}

#[test]
fn reader_keeps_large_blob_bytes_accessible() {
    let file = LybaFile::new()
        .with_blob_table(BlobTable {
            records: vec![BlobRecord::new(vec![3; 1024])],
        })
        .with_section(value_section(vec![Value::BytesBlob(BlobId(0))]));

    let decoded = write_then_read(&file, WriterMode::Pretty).expect("round trip should succeed");
    let blob = &decoded.blob_table.expect("BLOB should decode").records[0];

    assert_eq!(blob.len(), 1024);
    assert_eq!(blob.as_bytes()[0], 3);
    assert_eq!(blob.as_bytes()[1023], 3);
}
