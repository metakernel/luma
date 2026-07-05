//! Integration tests for Level 3 source file table support.

use luma_lumba::{
    BlobId, BlobRecord, BlobTable, Limits, LumbaError, LumbaFile, ReadOptions, Reader,
    SOURCE_FILE_FLAG_GENERATED, SOURCE_FILE_FLAG_PRIVATE, SOURCE_FILE_FLAG_RESERVED_MASK,
    SOURCE_FILE_FLAG_VIRTUAL, SourceFileRecord, SourceFileTable, WriteOptions, Writer,
};

#[test]
fn embedded_source_blob_round_trips_as_inert_data() {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(b"print('hi')\n".to_vec()))
        .expect("blob should append");

    let file = LumbaFile::new()
        .with_blob_table(blob_table)
        .with_source_file_table(
            SourceFileTable::new().with_record(
                SourceFileRecord::new()
                    .with_uri(Some(String::from("mem://fixture.lua")))
                    .with_display(Some(String::from("fixture.lua")))
                    .with_source_blob_ref(Some(BlobId(0)))
                    .with_flags(SOURCE_FILE_FLAG_VIRTUAL | SOURCE_FILE_FLAG_GENERATED),
            ),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let source = &decoded
        .source_file_table
        .expect("SRCF should decode")
        .records[0];
    assert_eq!(source.uri.as_deref(), Some("mem://fixture.lua"));
    assert_eq!(source.display.as_deref(), Some("fixture.lua"));
    assert_eq!(source.source_blob_ref, Some(BlobId(0)));
    assert!(source.has_uri());
    assert!(source.has_display());
    assert!(source.has_source_blob());
    assert!(source.is_virtual());
    assert!(source.is_generated());
}

#[test]
fn source_digest_blob_ref_round_trips() {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(vec![0xAA, 0xBB, 0xCC, 0xDD]))
        .expect("blob should append");

    let file = LumbaFile::new()
        .with_blob_table(blob_table)
        .with_source_file_table(
            SourceFileTable::new().with_record(
                SourceFileRecord::new()
                    .with_uri(Some(String::from("urn:luma:source:1")))
                    .with_digest_blob_ref(Some(BlobId(0))),
            ),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let source = &decoded
        .source_file_table
        .expect("SRCF should decode")
        .records[0];
    assert_eq!(source.digest_blob_ref, Some(BlobId(0)));
    assert!(source.has_digest());
}

#[test]
fn private_source_policy_uses_lb0019() {
    let file = LumbaFile::new().with_source_file_table(
        SourceFileTable::new().with_record(
            SourceFileRecord::new()
                .with_uri(Some(String::from("mem://private.luma")))
                .with_flags(SOURCE_FILE_FLAG_PRIVATE),
        ),
    );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject private sources");

    assert!(matches!(error, LumbaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
    assert!(
        Reader::new(ReadOptions::new().with_limits(Limits::trusted()))
            .read(&bytes)
            .is_ok()
    );
}

#[test]
fn writer_rejects_invalid_source_blob_refs_with_lb0014() {
    let file = LumbaFile::new().with_source_file_table(
        SourceFileTable::new().with_record(
            SourceFileRecord::new()
                .with_source_blob_ref(Some(BlobId(0)))
                .with_digest_blob_ref(Some(BlobId(1))),
        ),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("missing blob table entries should fail");

    assert!(matches!(error, LumbaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_treats_file_uri_as_inert_and_never_reads_filesystem() {
    let missing_path = std::env::temp_dir().join(format!(
        "luma-lumba-srcf-missing-{}-{}.luma",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    ));
    assert!(!missing_path.exists(), "fixture path must be absent");
    let uri = format!(
        "file:///{}",
        missing_path.to_string_lossy().replace('\\', "/")
    );

    let file = LumbaFile::new().with_source_file_table(
        SourceFileTable::new().with_record(
            SourceFileRecord::new()
                .with_uri(Some(uri.clone()))
                .with_display(Some(String::from("missing source"))),
        ),
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("reader must not attempt filesystem access for file:// URIs");

    let source = &decoded
        .source_file_table
        .expect("SRCF should decode")
        .records[0];
    assert_eq!(source.uri.as_deref(), Some(uri.as_str()));
    assert_eq!(source.display.as_deref(), Some("missing source"));
}

#[test]
fn writer_rejects_reserved_source_flags_with_lb0025() {
    let file = LumbaFile::new().with_source_file_table(SourceFileTable::new().with_record(
        SourceFileRecord::new().with_flags(SOURCE_FILE_FLAG_RESERVED_MASK & (!0x7f_u64)),
    ));

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("reserved flags should fail");

    assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}
