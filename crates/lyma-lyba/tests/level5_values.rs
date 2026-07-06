//! Integration tests for Level 5 inert native value tags.

use lyma_lyba::primitives::Identifier;
use lyma_lyba::{
    BlobId, BlobRecord, BlobTable, Document, ExpressionSource, ExpressionValue,
    ExtensionDeclaration, ExtensionTable, ExtensionValue, Limits, LuaChunkValue, LybaError,
    LybaFile, MapEntry, ReadOptions, Reader, RuntimeDescriptorValue, Value, WriteOptions, Writer,
};

#[test]
fn inert_level5_values_round_trip_without_lua_execution() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(Value::Map(vec![
            MapEntry {
                key: Value::String(String::from("expr")),
                value: Value::ExpressionSource(ExpressionValue {
                    language: Identifier::new("lua.expr"),
                    source: ExpressionSource::Text(String::from("this is not valid lua(")),
                    capability_set_ref: None,
                    result_value: Some(Box::new(Value::String(String::from("cached expr")))),
                }),
            },
            MapEntry {
                key: Value::String(String::from("chunk")),
                value: Value::LuaChunkSource(LuaChunkValue {
                    language: Identifier::new("lua.chunk"),
                    source_blob_ref: BlobId(0),
                    capability_set_ref: None,
                    result_value: Some(Box::new(Value::UInt(7))),
                }),
            },
            MapEntry {
                key: Value::String(String::from("runtime")),
                value: Value::RuntimeDescriptor(RuntimeDescriptorValue {
                    kind: Identifier::new("function.ref"),
                    required: false,
                    trusted_only: false,
                    capability_set_ref: None,
                    descriptor_value: Some(Box::new(Value::Map(vec![MapEntry {
                        key: Value::String(String::from("symbol")),
                        value: Value::String(String::from("demo.fn")),
                    }]))),
                    fallback_value: Some(Box::new(Value::String(String::from("fallback fn")))),
                }),
            },
            MapEntry {
                key: Value::String(String::from("extension")),
                value: Value::ExtensionValue(ExtensionValue {
                    extension_name: String::from("com.example.optional"),
                    type_name: Identifier::new("widget"),
                    payload_blob_ref: BlobId(1),
                    fallback_value: Some(Box::new(Value::Map(vec![MapEntry {
                        key: Value::String(String::from("mode")),
                        value: Value::String(String::from("safe")),
                    }]))),
                }),
            },
        ])))
        .expect("writer should encode inert value fixture");

    let decoded = Reader::new(ReadOptions::new().with_limits(Limits::public()))
        .read(&bytes)
        .expect("public reader should keep inert values as data");

    let root_value = decoded.documents[0].root_value.clone().expect("root value");
    let Value::Map(entries) = root_value else {
        panic!("fixture root should stay a map, got {root_value:?}");
    };

    match value_for_key(&entries, "expr") {
        Value::ExpressionSource(value) => {
            assert_eq!(value.language.as_str(), "lua.expr");
            assert_eq!(
                value.source,
                ExpressionSource::Text(String::from("this is not valid lua("))
            );
            assert_eq!(
                value.result_value.as_deref(),
                Some(&Value::String(String::from("cached expr")))
            );
        }
        other => panic!("expected expression source, got {other:?}"),
    }
    match value_for_key(&entries, "chunk") {
        Value::LuaChunkSource(value) => {
            assert_eq!(value.language.as_str(), "lua.chunk");
            assert_eq!(value.source_blob_ref, BlobId(0));
            assert_eq!(value.result_value.as_deref(), Some(&Value::UInt(7)));
        }
        other => panic!("expected lua chunk source, got {other:?}"),
    }
    match value_for_key(&entries, "runtime") {
        Value::RuntimeDescriptor(value) => {
            assert_eq!(value.kind.as_str(), "function.ref");
            assert!(!value.required);
            assert!(!value.trusted_only);
            assert_eq!(
                value.fallback_value.as_deref(),
                Some(&Value::String(String::from("fallback fn")))
            );
        }
        other => panic!("expected runtime descriptor, got {other:?}"),
    }
    match value_for_key(&entries, "extension") {
        Value::ExtensionValue(value) => {
            assert_eq!(value.extension_name, "com.example.optional");
            assert_eq!(value.type_name.as_str(), "widget");
            assert_eq!(value.payload_blob_ref, BlobId(1));
            assert!(matches!(
                value.fallback_value.as_deref(),
                Some(Value::Map(_))
            ));
        }
        other => panic!("expected extension value, got {other:?}"),
    }
}

#[test]
fn public_reader_rejects_required_runtime_descriptors_with_lb0020() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(Value::RuntimeDescriptor(
            RuntimeDescriptorValue {
                kind: Identifier::new("host.object.ref"),
                required: true,
                trusted_only: false,
                capability_set_ref: None,
                descriptor_value: Some(Box::new(Value::String(String::from("socket")))),
                fallback_value: Some(Box::new(Value::String(String::from("closed")))),
            },
        )))
        .expect("writer should encode required runtime descriptor");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject required runtime descriptors");

    assert!(matches!(error, LybaError::UnsafeEvaluationRequest(_)));
    assert_eq!(error.code().as_str(), "LB0020");
}

#[test]
fn public_reader_rejects_trusted_only_runtime_descriptors_with_lb0019() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file(Value::RuntimeDescriptor(
            RuntimeDescriptorValue {
                kind: Identifier::new("module.symbol"),
                required: false,
                trusted_only: true,
                capability_set_ref: None,
                descriptor_value: Some(Box::new(Value::String(String::from("internal.mod")))),
                fallback_value: Some(Box::new(Value::Null)),
            },
        )))
        .expect("writer should encode trusted-only runtime descriptor");

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only runtime descriptors");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
}

fn fixture_file(root_value: Value) -> LybaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(b"this is not valid lua either )".to_vec()))
        .expect("chunk blob should append");
    blob_table
        .push(BlobRecord::new(vec![0xde, 0xad, 0xbe, 0xef]))
        .expect("extension blob should append");

    LybaFile::new()
        .with_blob_table(blob_table)
        .with_extension_table(
            ExtensionTable::new()
                .with_declaration(ExtensionDeclaration::new("com.example.optional", "1.0")),
        )
        .with_document(Document::new().with_root_value(root_value))
}

fn value_for_key<'a>(entries: &'a [MapEntry], key: &str) -> &'a Value {
    entries
        .iter()
        .find(|entry| entry.key == Value::String(String::from(key)))
        .map(|entry| &entry.value)
        .expect("map entry should exist")
}
