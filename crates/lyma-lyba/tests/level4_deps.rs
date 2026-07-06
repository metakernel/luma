//! Integration tests for Level 4 dependency table support.

use lyma_lyba::container::{ContainerHeader, HeaderCrcMode};
use lyma_lyba::primitives::Identifier;
use lyma_lyba::section::SectionEntry;
use lyma_lyba::symbol::SymbolRecord;
use lyma_lyba::{
    BlobId, BlobRecord, BlobTable, DEPENDENCY_FLAG_EMBEDDED, DEPENDENCY_FLAG_FILE_URI,
    DEPENDENCY_FLAG_HOST_MODULE, DEPENDENCY_FLAG_NETWORK_URI, DEPENDENCY_FLAG_REQUIRED,
    DEPENDENCY_FLAG_RESOLVED, DEPENDENCY_FLAG_TRUSTED_ONLY, DEPENDENCY_KIND_EXTENSION,
    DEPENDENCY_KIND_EXTERNAL_RESOURCE, DEPENDENCY_KIND_GENERATED, DEPENDENCY_KIND_IMPORT,
    DEPENDENCY_KIND_INCLUDE, DEPENDENCY_KIND_MODULE, DEPENDENCY_KIND_SCHEMA,
    DEPENDENCY_KIND_SOURCE, DependencyRecord, DependencyTable, Limits, LybaError, LybaFile,
    ReadOptions, Reader, SourceFileRecord, SourceFileTable, SourceSpanRecord, SourceSpanTable,
    StringRecord, StringTable, SymbolTable, Value, WriteOptions, Writer,
};

#[test]
fn dependency_records_round_trip_each_kind_with_inert_metadata() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new()
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_IMPORT)
                    .with_uri(Some(String::from("https://example.invalid/import.lyma")))
                    .with_alias(Some(Identifier::new("imported")))
                    .with_source_span_ref(Some(0))
                    .with_resolved_digest_blob_ref(Some(BlobId(0)))
                    .with_metadata_value(Some(Value::String(String::from("import-meta"))))
                    .with_flags(DEPENDENCY_FLAG_REQUIRED | DEPENDENCY_FLAG_RESOLVED),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_INCLUDE)
                    .with_uri(Some(String::from("file:///tmp/include.lyma")))
                    .with_flags(DEPENDENCY_FLAG_FILE_URI),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_MODULE)
                    .with_uri(Some(String::from("host:math")))
                    .with_alias(Some(Identifier::new("math")))
                    .with_flags(DEPENDENCY_FLAG_HOST_MODULE),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_SCHEMA)
                    .with_uri(Some(String::from("urn:lyma:schema:service"))),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_SOURCE)
                    .with_uri(Some(String::from("file:///workspace/main.lyma"))),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_GENERATED)
                    .with_uri(Some(String::from("mem://generated/intermediate")))
                    .with_flags(DEPENDENCY_FLAG_EMBEDDED),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                    .with_uri(Some(String::from("https://example.invalid/asset.bin")))
                    .with_flags(DEPENDENCY_FLAG_NETWORK_URI),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_EXTENSION)
                    .with_uri(Some(String::from("ext://com.example/tooling")))
                    .with_metadata_value(Some(Value::String(String::from("extension-meta")))),
            ),
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let records = &decoded
        .dependency_table
        .expect("DEPS should decode")
        .records;
    assert_eq!(records.len(), 8);
    assert_eq!(records[0].kind, DEPENDENCY_KIND_IMPORT);
    assert_eq!(
        records[0].alias.as_ref().map(Identifier::as_str),
        Some("imported")
    );
    assert_eq!(records[0].source_span_ref, Some(0));
    assert_eq!(records[0].resolved_digest_blob_ref, Some(BlobId(0)));
    assert_eq!(
        records[0].metadata_value,
        Some(Value::String(String::from("import-meta")))
    );
    assert!(records[0].is_required());
    assert!(records[0].is_resolved());
    assert!(records[0].has_digest());
    assert_eq!(records[1].kind, DEPENDENCY_KIND_INCLUDE);
    assert_eq!(records[2].kind, DEPENDENCY_KIND_MODULE);
    assert_eq!(records[3].kind, DEPENDENCY_KIND_SCHEMA);
    assert_eq!(records[4].kind, DEPENDENCY_KIND_SOURCE);
    assert_eq!(records[5].kind, DEPENDENCY_KIND_GENERATED);
    assert_eq!(records[6].kind, DEPENDENCY_KIND_EXTERNAL_RESOURCE);
    assert_eq!(records[7].kind, DEPENDENCY_KIND_EXTENSION);
}

#[test]
fn dependency_flags_round_trip_and_trusted_policy_is_enforced() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new().with_record(
            DependencyRecord::new(DEPENDENCY_KIND_MODULE)
                .with_uri(Some(String::from("host:resolver")))
                .with_flags(
                    DEPENDENCY_FLAG_NETWORK_URI
                        | DEPENDENCY_FLAG_FILE_URI
                        | DEPENDENCY_FLAG_HOST_MODULE
                        | DEPENDENCY_FLAG_TRUSTED_ONLY,
                ),
        ),
    );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only dependency");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");

    let decoded = Reader::new(ReadOptions::new().with_limits(Limits::trusted()))
        .read(&bytes)
        .expect("trusted reader should accept dependency");
    let record = &decoded
        .dependency_table
        .expect("DEPS should decode")
        .records[0];
    assert!(record.is_network_uri());
    assert!(record.is_file_uri());
    assert!(record.is_host_module());
    assert!(record.is_trusted_only());
}

#[test]
fn dependency_uris_are_inert_and_do_not_trigger_resolution() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new()
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_IMPORT)
                    .with_uri(Some(String::from("file:///definitely/not/read.lyma"))),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                    .with_uri(Some(String::from("https://127.0.0.1:9/never-contacted")))
                    .with_flags(DEPENDENCY_FLAG_NETWORK_URI),
            )
            .with_record(
                DependencyRecord::new(DEPENDENCY_KIND_MODULE)
                    .with_uri(Some(String::from("host:do-not-resolve")))
                    .with_flags(DEPENDENCY_FLAG_HOST_MODULE),
            ),
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should treat dependencies as inert metadata");

    let records = &decoded
        .dependency_table
        .expect("DEPS should decode")
        .records;
    assert_eq!(
        records[0].uri.as_deref(),
        Some("file:///definitely/not/read.lyma")
    );
    assert_eq!(
        records[1].uri.as_deref(),
        Some("https://127.0.0.1:9/never-contacted")
    );
    assert_eq!(records[2].uri.as_deref(), Some("host:do-not-resolve"));
}

#[test]
fn writer_rejects_invalid_dependency_source_span_ref_with_lb0022() {
    let file =
        fixture_file().with_dependency_table(DependencyTable::new().with_record(
            DependencyRecord::new(DEPENDENCY_KIND_IMPORT).with_source_span_ref(Some(1)),
        ));

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("invalid source span ref should fail");

    assert!(matches!(error, LybaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn writer_rejects_invalid_dependency_digest_ref_with_lb0014() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new().with_record(
            DependencyRecord::new(DEPENDENCY_KIND_IMPORT)
                .with_resolved_digest_blob_ref(Some(BlobId(9))),
        ),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("invalid digest ref should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_invalid_dependency_uri_and_alias_refs_with_lb0014() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new().with_record(DependencyRecord::new(DEPENDENCY_KIND_IMPORT)),
    );

    let mut bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");
    patch_deps_payload(&mut bytes, &[1, 0, 99, 0, 0, 0, 0, 0]);
    let uri_error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid dependency URI ref should fail");
    assert!(matches!(uri_error, LybaError::InvalidValueReference(_)));
    assert_eq!(uri_error.code().as_str(), "LB0014");

    let mut bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");
    patch_deps_payload(&mut bytes, &[1, 0, 0, 99, 0, 0, 0, 0]);
    let alias_error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid dependency alias ref should fail");
    assert!(matches!(alias_error, LybaError::InvalidValueReference(_)));
    assert_eq!(alias_error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_invalid_dependency_source_span_ref_with_lb0022() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new().with_record(DependencyRecord::new(DEPENDENCY_KIND_IMPORT)),
    );
    let mut bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");

    patch_deps_payload(&mut bytes, &[1, 0, 0, 0, 0, 99, 0, 0]);
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid dependency source span ref should fail");

    assert!(matches!(error, LybaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn writer_rejects_reserved_dependency_flags_with_lb0025() {
    let file = fixture_file().with_dependency_table(
        DependencyTable::new()
            .with_record(DependencyRecord::new(DEPENDENCY_KIND_IMPORT).with_flags(1 << 8)),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("reserved dependency flags should fail");

    assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}

fn fixture_file() -> LybaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(vec![0xAA, 0xBB, 0xCC]))
        .expect("blob should append");
    LybaFile::new()
        .with_string_table(
            StringTable::new()
                .with_string(StringRecord::new("imported"))
                .with_string(StringRecord::new("math")),
        )
        .with_symbol_table(
            SymbolTable::new()
                .with_symbol(SymbolRecord::new(0))
                .with_symbol(SymbolRecord::new(1)),
        )
        .with_blob_table(blob_table)
        .with_source_file_table(
            SourceFileTable::new()
                .with_record(SourceFileRecord::new().with_uri(Some(String::from("mem://fixture")))),
        )
        .with_source_span_table(SourceSpanTable::new().with_record(SourceSpanRecord::new(0, 0, 0)))
}

fn patch_deps_payload(bytes: &mut [u8], payload: &[u8]) {
    let header =
        ContainerHeader::decode(bytes, HeaderCrcMode::Enabled).expect("header should decode");
    for index in 0..header.section_count as usize {
        let start = 64 + index * 64;
        let end = start + 64;
        let entry = SectionEntry::decode(&bytes[start..end]).expect("entry should decode");
        if entry.section_id == lyma_lyba::section::SectionId::DEPS {
            let payload_offset = entry.payload_offset as usize;
            let payload_end = payload_offset + payload.len();
            bytes[payload_offset..payload_end].copy_from_slice(payload);
            return;
        }
    }
    panic!("DEPS section not found");
}
