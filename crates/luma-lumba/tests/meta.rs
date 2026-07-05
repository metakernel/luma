//! Metadata and runtime-data integration tests.

use luma_lumba::container::HeaderCrcMode;
use luma_lumba::meta::Metadata;
use luma_lumba::read::{ReadOptions, Reader};
use luma_lumba::value::{MapEntry, Value};
use luma_lumba::verify::Verifier;
use luma_lumba::write::{WriteOptions, Writer, WriterMode};
use luma_lumba::{Document, LumbaFile};

#[test]
fn runtime_data_fixture_validates_and_decodes_with_deterministic_metadata() {
    let fixture = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::RuntimeData)
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(
        &LumbaFile::new().with_document(Document::new().with_root_value(Value::Map(vec![
            MapEntry {
                key: Value::String(String::from("name")),
                value: Value::String(String::from("Ada")),
            },
        ]))),
    )
    .expect("runtime data fixture should encode");

    Verifier::new()
        .verify_canonical(&fixture)
        .expect("runtime data fixture should validate canonically");

    let decoded = Reader::new(ReadOptions::new())
        .read(&fixture)
        .expect("runtime data fixture should decode");

    let metadata = decoded
        .metadata
        .expect("runtime data fixture should carry META");
    assert_eq!(
        metadata.get("format"),
        Some(&Value::String(String::from("lumba")))
    );
    assert_eq!(
        metadata.get("lumba_version"),
        Some(&Value::String(String::from("0.1")))
    );
    assert_eq!(
        metadata.get("luma_version"),
        Some(&Value::String(String::from("0.1")))
    );
    assert_eq!(
        metadata.get("image_kind"),
        Some(&Value::String(String::from("value")))
    );
    assert_eq!(metadata.get("canonical"), Some(&Value::Bool(true)));
    assert_eq!(decoded.documents.len(), 1);
}

#[test]
fn metadata_map_helper_preserves_string_keys() {
    let metadata = Metadata::new()
        .with_entry("format", Value::String(String::from("lumba")))
        .with_entry("canonical", Value::Bool(true));

    let Value::Map(entries) = metadata.as_map_value() else {
        panic!("metadata should materialize as a map");
    };

    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|entry| matches!(entry.key, Value::String(_)))
    );
}
