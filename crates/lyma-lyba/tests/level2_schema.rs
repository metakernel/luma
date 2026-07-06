//! Integration tests for Level 2 schema table support.

use lyma_lyba::symbol::{SYMBOL_FLAG_TAG, SymbolRecord, SymbolTable};
use lyma_lyba::{
    BlobId, BlobRecord, BlobTable, Document, LybaError, LybaFile, ReadOptions, Reader,
    SCHEMA_FLAG_REQUIRED_BY_DOCUMENT, SCHEMA_FLAG_TRUSTED_VALIDATOR_REQUIRED,
    SCHEMA_FLAG_VALIDATED_BY_PRODUCER, SchemaRecord, SchemaTable, StringRecord, StringTable,
    TagDeclaration, TagTable, Value, WriteOptions, Writer,
};

#[test]
fn schema_uri_only_round_trips_as_inert_data() {
    let file = LybaFile::new().with_schema_table(
        SchemaTable::new().with_record(
            SchemaRecord::new()
                .with_uri(Some(String::from("urn:lyma:schema:service")))
                .with_flags(SCHEMA_FLAG_VALIDATED_BY_PRODUCER),
        ),
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let schema = &decoded.schema_table.expect("SCMA should decode").records[0];
    assert_eq!(schema.uri.as_deref(), Some("urn:lyma:schema:service"));
    assert!(schema.has_uri());
    assert!(!schema.has_value());
    assert!(!schema.has_digest());
    assert!(schema.is_validated_by_producer());
}

#[test]
fn schema_embedded_value_round_trips_and_document_schema_ref_is_preserved() {
    let file = LybaFile::new()
        .with_document(
            Document::new()
                .with_root_value(Value::Int(7))
                .with_schema_ref(Some(0)),
        )
        .with_schema_table(
            SchemaTable::new().with_record(
                SchemaRecord::new()
                    .with_value(Some(Value::String(String::from("type: integer"))))
                    .with_flags(SCHEMA_FLAG_REQUIRED_BY_DOCUMENT),
            ),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    assert_eq!(decoded.documents[0].schema_ref, Some(0));
    let schema = &decoded.schema_table.expect("SCMA should decode").records[0];
    assert_eq!(
        schema.value,
        Some(Value::String(String::from("type: integer")))
    );
    assert!(schema.has_value());
    assert!(schema.is_required_by_document());
}

#[test]
fn schema_digest_blob_ref_round_trips() {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(vec![0xAA, 0xBB, 0xCC]))
        .expect("blob should append");
    let file = LybaFile::new()
        .with_blob_table(blob_table)
        .with_schema_table(
            SchemaTable::new()
                .with_record(SchemaRecord::new().with_digest_blob_ref(Some(BlobId(0)))),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let schema = &decoded.schema_table.expect("SCMA should decode").records[0];
    assert_eq!(schema.digest_blob_ref, Some(BlobId(0)));
    assert!(schema.has_digest());
}

#[test]
fn tag_schema_refs_round_trip_against_real_scma_count() {
    let file = LybaFile::new()
        .with_string_table(
            StringTable::new()
                .with_string(StringRecord::new("Duration"))
                .with_string(StringRecord::new("urn:lyma:tag:duration"))
                .with_string(StringRecord::new("urn:lyma:schema:duration")),
        )
        .with_symbol_table(
            SymbolTable::new().with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_TAG)),
        )
        .with_schema_table(SchemaTable::new().with_record(
            SchemaRecord::new().with_uri(Some(String::from("urn:lyma:schema:duration"))),
        ))
        .with_tag_table(TagTable::new().with_declaration(
            TagDeclaration::new("Duration", "urn:lyma:tag:duration").with_schema_ref(Some(0)),
        ));

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    assert_eq!(
        decoded.tag_table.expect("TAGS should decode").declarations[0].schema_ref,
        Some(0)
    );
}

#[test]
fn invalid_document_schema_ref_uses_existing_lb0015() {
    let file = LybaFile::new()
        .with_schema_table(SchemaTable::new().with_record(
            SchemaRecord::new().with_uri(Some(String::from("urn:lyma:schema:duration"))),
        ))
        .with_document(Document::new().with_schema_ref(Some(1)));

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("document schema ref 1 should be out of range");

    assert!(matches!(error, LybaError::InvalidSyntaxNodeReference(_)));
    assert_eq!(error.code().as_str(), "LB0015");
}

#[test]
fn trusted_validator_schema_is_rejected_under_public_policy() {
    let file = LybaFile::new().with_schema_table(
        SchemaTable::new()
            .with_record(SchemaRecord::new().with_flags(SCHEMA_FLAG_TRUSTED_VALIDATOR_REQUIRED)),
    );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("write should succeed");
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted validator schema");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
    assert!(
        Reader::new(ReadOptions::new().with_limits(lyma_lyba::Limits::trusted()))
            .read(&bytes)
            .is_ok()
    );
}

#[test]
fn schema_data_is_not_used_to_validate_documents() {
    let file = LybaFile::new()
        .with_document(
            Document::new()
                .with_root_value(Value::Int(42))
                .with_schema_ref(Some(0)),
        )
        .with_schema_table(
            SchemaTable::new().with_record(SchemaRecord::new().with_value(Some(Value::String(
                String::from("pretend-callback: reject-all-values"),
            )))),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should treat schemas as inert data");

    assert_eq!(decoded.documents[0].root_value, Some(Value::Int(42)));
    assert_eq!(decoded.documents[0].schema_ref, Some(0));
}

#[test]
fn writer_rejects_invalid_schema_digest_ref_with_lb0014() {
    let file = LybaFile::new().with_schema_table(
        SchemaTable::new().with_record(SchemaRecord::new().with_digest_blob_ref(Some(BlobId(0)))),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("missing blob table entry should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}
