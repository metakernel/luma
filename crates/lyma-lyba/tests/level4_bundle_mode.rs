//! Integration tests for build bundle writer mode.

use lyma_lyba::container::{ContainerHeader, HeaderCrcMode, SECTION_ENTRY_SIZE};
use lyma_lyba::extension::{ExtensionDeclaration, ExtensionTable};
use lyma_lyba::primitives::Identifier;
use lyma_lyba::schema::{SCHEMA_FLAG_REQUIRED_BY_DOCUMENT, SchemaRecord, SchemaTable};
use lyma_lyba::section::SectionEntry;
use lyma_lyba::signature::SignatureVerifier;
use lyma_lyba::value::{MapEntry, Value};
use lyma_lyba::{
    BlobId, BlobRecord, BlobTable, DEPENDENCY_FLAG_EMBEDDED, DEPENDENCY_FLAG_NETWORK_URI,
    DEPENDENCY_KIND_EXTERNAL_RESOURCE, DEPENDENCY_KIND_MODULE, DependencyRecord, DependencyTable,
    Document, EMBEDDED_RESOURCE_KIND_LYMA_TEXT, EmbeddedResourceRecord, EmbeddedResourceTable,
    LybaFile, ReadOptions, Reader, SIGNATURE_ALGORITHM_SHA256, SIGNATURE_RECORD_KIND_DIGEST,
    SignatureRecord, SignatureTable, WriteOptions, Writer, WriterMode,
};

const CONTAINER_FLAG_HAS_VALUES: u32 = 1 << 4;
const PROFILE_FLAG_VALUE_IMAGE: u32 = 1 << 3;

fn decode_header(bytes: &[u8]) -> ContainerHeader {
    ContainerHeader::decode(bytes, HeaderCrcMode::Disabled).expect("header should decode")
}

fn section_ids(bytes: &[u8]) -> Vec<String> {
    let header = decode_header(bytes);
    let table_start = header.section_table_offset as usize;
    let table_len = header.section_count as usize * SECTION_ENTRY_SIZE as usize;
    bytes[table_start..table_start + table_len]
        .chunks_exact(SECTION_ENTRY_SIZE as usize)
        .map(|entry| {
            SectionEntry::decode(entry)
                .expect("section entry should decode")
                .section_id
                .as_str()
                .to_string()
        })
        .collect()
}

#[test]
fn build_bundle_mode_emits_bundle_sections_and_keeps_dependencies_inert() {
    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::BuildBundle)
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&bundle_fixture())
    .expect("bundle fixture should encode");

    let header = decode_header(&bytes);
    assert_eq!(header.container_flags, CONTAINER_FLAG_HAS_VALUES);
    assert_eq!(header.profile_flags, PROFILE_FLAG_VALUE_IMAGE);
    assert_eq!(
        section_ids(&bytes),
        vec![
            "META", "EXTS", "STRS", "SYMS", "BLOB", "VALS", "DOCS", "SCMA", "DEPS", "EMBD", "SIGN",
        ]
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("bundle fixture should decode");

    assert_eq!(
        decoded
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("image_kind")),
        Some(&Value::String(String::from("build_bundle")))
    );
    assert_eq!(
        decoded.documents,
        vec![Document::new().with_root_value(bundle_root_value())]
    );

    let extension = &decoded
        .extension_table
        .as_ref()
        .expect("EXTS should decode")
        .declarations[0];
    assert_eq!(extension.name, "org.lyma.bundle");
    assert_eq!(
        extension.metadata_value,
        Some(Value::String(String::from("ext-meta")))
    );

    let schema = &decoded
        .schema_table
        .as_ref()
        .expect("SCMA should decode")
        .records[0];
    assert_eq!(schema.uri.as_deref(), Some("urn:lyma:schema:bundle"));
    assert!(schema.is_required_by_document());
    assert_eq!(
        schema.value,
        Some(Value::String(String::from("schema-value")))
    );

    let dependencies = &decoded
        .dependency_table
        .as_ref()
        .expect("DEPS should decode")
        .records;
    assert_eq!(dependencies.len(), 2);
    assert_eq!(
        dependencies[0].uri.as_deref(),
        Some("https://127.0.0.1:9/never-contacted")
    );
    assert!(dependencies[0].is_embedded());
    assert!(dependencies[0].is_network_uri());
    assert_eq!(
        dependencies[0].metadata_value,
        Some(Value::String(String::from("dep-meta")))
    );
    assert_eq!(dependencies[1].uri.as_deref(), Some("host:do-not-resolve"));

    let blob_table = decoded.blob_table.as_ref().expect("BLOB should decode");
    let resource = &decoded
        .embedded_resource_table
        .as_ref()
        .expect("EMBD should decode")
        .records[0];
    assert_eq!(resource.dependency_ref, 0);
    assert_eq!(resource.kind, EMBEDDED_RESOURCE_KIND_LYMA_TEXT);
    assert_eq!(
        resource
            .utf8_text(blob_table)
            .expect("resource text should decode"),
        Some("embedded bundle text\n")
    );

    let signature = &decoded
        .signature_table
        .as_ref()
        .expect("SIGN should decode")
        .records[0];
    assert!(signature.is_digest());
    assert_eq!(
        signature.algorithm.as_ref().map(Identifier::as_str),
        Some(SIGNATURE_ALGORITHM_SHA256)
    );
    assert_eq!(
        signature.metadata_value,
        Some(Value::Map(vec![MapEntry {
            key: Value::String(String::from("label")),
            value: Value::String(String::from("bundle-digest")),
        }]))
    );

    let report = SignatureVerifier::new()
        .verify_structural_coverage(&decoded)
        .expect("signature coverage should stay structural only");
    assert_eq!(report.records.len(), 1);
    assert_eq!(
        report.records[0]
            .covered_sections
            .iter()
            .map(|section| section.section_id.as_str())
            .collect::<Vec<_>>(),
        vec!["STRS", "BLOB", "VALS", "DEPS", "EMBD"]
    );
}

fn bundle_fixture() -> LybaFile {
    let mut blob_table = BlobTable::new();
    blob_table
        .push(BlobRecord::new(b"embedded bundle text\n".to_vec()))
        .expect("embedded resource blob should append");
    blob_table
        .push(BlobRecord::new(vec![0xAB; 32]))
        .expect("signature blob should append");

    LybaFile::new()
        .with_document(Document::new().with_root_value(bundle_root_value()))
        .with_extension_table(
            ExtensionTable::new().with_declaration(
                ExtensionDeclaration::new("org.lyma.bundle", "1.0.0")
                    .with_metadata_value(Some(Value::String(String::from("ext-meta")))),
            ),
        )
        .with_blob_table(blob_table)
        .with_schema_table(
            SchemaTable::new().with_record(
                SchemaRecord::new()
                    .with_flags(SCHEMA_FLAG_REQUIRED_BY_DOCUMENT)
                    .with_uri(Some(String::from("urn:lyma:schema:bundle")))
                    .with_value(Some(Value::String(String::from("schema-value"))))
                    .with_metadata_value(Some(Value::String(String::from("schema-meta")))),
            ),
        )
        .with_dependency_table(
            DependencyTable::new()
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                        .with_uri(Some(String::from("https://127.0.0.1:9/never-contacted")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED | DEPENDENCY_FLAG_NETWORK_URI)
                        .with_metadata_value(Some(Value::String(String::from("dep-meta")))),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_MODULE)
                        .with_uri(Some(String::from("host:do-not-resolve")))
                        .with_alias(Some(Identifier::new("tooling"))),
                ),
        )
        .with_embedded_resource_table(EmbeddedResourceTable::new().with_record(
            EmbeddedResourceRecord::new(0, EMBEDDED_RESOURCE_KIND_LYMA_TEXT, BlobId(0)),
        ))
        .with_signature_table(
            SignatureTable::new().with_record(
                SignatureRecord::new(SIGNATURE_RECORD_KIND_DIGEST)
                    .with_algorithm(Some(Identifier::new(SIGNATURE_ALGORITHM_SHA256)))
                    .with_covered_section_refs(vec![2, 4, 5, 8, 9])
                    .with_payload_blob_ref(Some(BlobId(1)))
                    .with_metadata_value(Some(Value::Map(vec![MapEntry {
                        key: Value::String(String::from("label")),
                        value: Value::String(String::from("bundle-digest")),
                    }]))),
            ),
        )
}

fn bundle_root_value() -> Value {
    Value::Map(vec![MapEntry {
        key: Value::String(String::from("bundle")),
        value: Value::String(String::from("fixture")),
    }])
}
