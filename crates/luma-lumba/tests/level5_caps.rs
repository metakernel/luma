//! Integration tests for Level 5 capability tables and inert evaluation descriptors.

use luma_lumba::primitives::Identifier;
use luma_lumba::{
    BlobId, BlobRecord, BlobTable, CAPABILITY_FLAG_DETERMINISTIC_EXPECTED,
    CAPABILITY_FLAG_MAY_READ_EXTERNAL, CAPABILITY_FLAG_MAY_WRITE_EXTERNAL,
    CAPABILITY_FLAG_PURE_EXPECTED, CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION,
    CAPABILITY_FLAG_REQUIRED_FOR_REPRODUCTION, CAPABILITY_FLAG_TRUSTED_ONLY, CapabilityRequirement,
    CapabilitySetRecord, CapabilityTable, Document, ExpressionSource, ExpressionValue, Limits,
    LuaChunkValue, LumbaError, LumbaFile, MapEntry, ReadOptions, Reader, Value, WriteOptions,
    Writer,
};

#[test]
fn capability_sets_round_trip_with_inspection_and_no_evaluation() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(
            CapabilityTable::new().with_record(
                CapabilitySetRecord::new()
                    .with_metadata_value(Some(Value::String(String::from("cap-set-meta"))))
                    .with_requirement(
                        CapabilityRequirement::new(Identifier::new("lua.eval.expr")).with_flags(
                            CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION
                                | CAPABILITY_FLAG_PURE_EXPECTED
                                | CAPABILITY_FLAG_DETERMINISTIC_EXPECTED,
                        ),
                    )
                    .with_requirement(
                        CapabilityRequirement::new(Identifier::new("host.context.read"))
                            .with_flags(
                                CAPABILITY_FLAG_REQUIRED_FOR_REPRODUCTION
                                    | CAPABILITY_FLAG_MAY_READ_EXTERNAL,
                            ),
                    )
                    .with_requirement(
                        CapabilityRequirement::new(Identifier::new("host.context.write"))
                            .with_flags(CAPABILITY_FLAG_MAY_WRITE_EXTERNAL)
                            .with_metadata_value(Some(Value::String(String::from("write-meta")))),
                    ),
            ),
        ))
        .expect("writer should encode CAPS fixture");

    let decoded = Reader::new(ReadOptions::new().with_limits(Limits::trusted()))
        .read(&bytes)
        .expect("trusted reader should keep CAPS inert");

    let capability_table = decoded.capability_table.expect("CAPS should decode");
    assert_eq!(capability_table.records.len(), 1);
    assert_eq!(decoded.documents[0].capability_set_ref, Some(0));
    let requirements = &capability_table.records[0].requirements;
    assert!(requirements[0].is_required_for_evaluation());
    assert!(requirements[0].expects_pure_behavior());
    assert!(requirements[0].expects_deterministic_behavior());
    assert!(requirements[1].is_required_for_reproduction());
    assert!(requirements[1].may_read_external());
    assert!(requirements[2].may_write_external());
    assert_eq!(
        capability_table.inspection_rows(),
        vec![String::from(
            "set#0: lua.eval.expr [eval, pure, deterministic]; host.context.read [repro, read-external]; host.context.write [write-external]"
        )]
    );

    let Value::Map(entries) = &decoded.documents[0].root_value.clone().expect("root value") else {
        panic!("fixture root should stay a map");
    };
    let expression = value_for_key(entries, "expr");
    let chunk = value_for_key(entries, "chunk");
    match expression {
        Value::ExpressionSource(expression) => {
            assert_eq!(expression.language.as_str(), "lua.expr");
            assert_eq!(expression.capability_set_ref, Some(0));
            assert_eq!(
                expression.result_value.as_deref(),
                Some(&Value::String(String::from("cached expression result")))
            );
            assert_eq!(
                expression.source,
                ExpressionSource::Text(String::from("1 + 1"))
            );
        }
        other => panic!("expected inert expression source, got {other:?}"),
    }
    match chunk {
        Value::LuaChunkSource(chunk) => {
            assert_eq!(chunk.language.as_str(), "lua.chunk");
            assert_eq!(chunk.capability_set_ref, Some(0));
            assert_eq!(chunk.source_blob_ref, BlobId(0));
            assert_eq!(chunk.result_value.as_deref(), Some(&Value::UInt(7)));
        }
        other => panic!("expected inert chunk source, got {other:?}"),
    }
}

#[test]
fn public_reader_rejects_required_evaluation_capabilities_with_lb0020() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(
            CapabilityTable::new().with_record(
                CapabilitySetRecord::new().with_requirement(
                    CapabilityRequirement::new(Identifier::new("lua.eval.expr"))
                        .with_flags(CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION),
                ),
            ),
        ))
        .expect("writer should encode CAPS fixture");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject evaluation capability requests");

    assert!(matches!(error, LumbaError::UnsafeEvaluationRequest(_)));
    assert_eq!(error.code().as_str(), "LB0020");
}

#[test]
fn public_reader_rejects_external_effect_capabilities_with_lb0020() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(
            CapabilityTable::new().with_record(
                CapabilitySetRecord::new()
                    .with_requirement(
                        CapabilityRequirement::new(Identifier::new("host.context.read"))
                            .with_flags(CAPABILITY_FLAG_MAY_READ_EXTERNAL),
                    )
                    .with_requirement(
                        CapabilityRequirement::new(Identifier::new("host.context.write"))
                            .with_flags(CAPABILITY_FLAG_MAY_WRITE_EXTERNAL),
                    ),
            ),
        ))
        .expect("writer should encode CAPS fixture");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject external-effect capabilities");

    assert!(matches!(error, LumbaError::UnsafeEvaluationRequest(_)));
    assert_eq!(error.code().as_str(), "LB0020");
}

#[test]
fn public_reader_rejects_trusted_only_capabilities_with_lb0019() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(
            CapabilityTable::new().with_record(
                CapabilitySetRecord::new().with_requirement(
                    CapabilityRequirement::new(Identifier::new("module.resolve"))
                        .with_flags(CAPABILITY_FLAG_TRUSTED_ONLY),
                ),
            ),
        ))
        .expect("writer should encode CAPS fixture");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only capabilities");

    assert!(matches!(error, LumbaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
}

#[test]
fn writer_rejects_document_capability_refs_outside_caps_table() {
    let error = Writer::new(WriteOptions::new())
        .write(
            &fixture_file(CapabilityTable::new()).with_document(
                Document::new()
                    .with_root_value(Value::String(String::from("extra")))
                    .with_capability_set_ref(Some(0)),
            ),
        )
        .expect_err("writer should reject out-of-range document capability refs");

    assert!(matches!(error, LumbaError::InvalidValueReference(_)));
}

fn fixture_file(capability_table: CapabilityTable) -> LumbaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(b"return 7".to_vec()))
        .expect("chunk blob should append");

    LumbaFile::new()
        .with_blob_table(blob_table)
        .with_document(
            Document::new()
                .with_root_value(Value::Map(vec![
                    MapEntry {
                        key: Value::String(String::from("expr")),
                        value: Value::ExpressionSource(ExpressionValue {
                            language: Identifier::new("lua.expr"),
                            source: ExpressionSource::Text(String::from("1 + 1")),
                            capability_set_ref: Some(0),
                            result_value: Some(Box::new(Value::String(String::from(
                                "cached expression result",
                            )))),
                        }),
                    },
                    MapEntry {
                        key: Value::String(String::from("chunk")),
                        value: Value::LuaChunkSource(LuaChunkValue {
                            language: Identifier::new("lua.chunk"),
                            source_blob_ref: BlobId(0),
                            capability_set_ref: Some(0),
                            result_value: Some(Box::new(Value::UInt(7))),
                        }),
                    },
                ]))
                .with_capability_set_ref(Some(0)),
        )
        .with_capability_table(capability_table)
}

fn value_for_key<'a>(entries: &'a [MapEntry], key: &str) -> &'a Value {
    entries
        .iter()
        .find(|entry| entry.key == Value::String(String::from(key)))
        .map(|entry| &entry.value)
        .expect("map entry should exist")
}
