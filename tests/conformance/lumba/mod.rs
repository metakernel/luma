use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use luma::lumba::container::{ContainerHeader, HeaderCrcMode, SECTION_ENTRY_SIZE};
use luma::lumba::primitives::Identifier;
use luma::lumba::section::SectionEntry;
use luma::lumba::verify::Verifier;
use luma::lumba::{
    self, BlobId, BlobRecord, BlobTable, CapabilityRequirement, CapabilitySetRecord,
    CapabilityTable, DependencyRecord, DependencyTable, Document, EmbeddedResourceRecord,
    EmbeddedResourceTable, Limits, LumbaError, LumbaFile, MapEntry, ReadOptions, Reader,
    SIGNATURE_ALGORITHM_SHA256, SIGNATURE_RECORD_KIND_DIGEST, SOURCE_FILE_FLAG_GENERATED,
    SOURCE_FILE_FLAG_PRIVATE, SOURCE_FILE_FLAG_VIRTUAL, SignatureRecord, SignatureTable,
    SignatureVerifier, SourceFileRecord, SourceFileTable, TAG_FLAG_KNOWN_TO_PRODUCER,
    TAG_FLAG_PORTABLE, TAG_FLAG_REQUIRES_RESOLVER, TAG_FLAG_TRUSTED_ONLY, TagDeclaration, TagTable,
    Value, WriteOptions, Writer,
};

pub const LEVEL_MANIFESTS: [(&str, &str); 12] = [
    ("level0/positive-runtime-data.json", "positive"),
    ("level0/negative-malformed-layout.json", "negative"),
    ("level1/minimal-values.json", "positive"),
    ("level1/invalid-varints.json", "negative"),
    ("level2/positive-tag-registry.json", "positive"),
    ("level2/negative-trusted-tag.json", "negative"),
    ("level3/positive-source-file.json", "positive"),
    ("level3/negative-private-source.json", "negative"),
    ("level4/positive-bundle-fixture.json", "positive"),
    ("level4/negative-invalid-embedded-ref.json", "negative"),
    ("level5/positive-capabilities.json", "positive"),
    ("level5/negative-trusted-capability.json", "negative"),
];

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lumba")
}

pub fn manifest_text(relative: &str) -> String {
    fs::read_to_string(fixture_root().join(relative))
        .unwrap_or_else(|_| panic!("missing LUMBA fixture manifest {relative}"))
}

pub fn checked_in_level1_fixture() -> &'static [u8] {
    include_bytes!("../../fixtures/lumba/level1/minimal-values.lumba")
}

pub fn level0_positive_bytes() -> Vec<u8> {
    canonical_writer()
        .write(
            &LumbaFile::new().with_document(Document::new().with_root_value(Value::Map(vec![
                MapEntry {
                    key: Value::String(String::from("name")),
                    value: Value::String(String::from("level0")),
                },
                MapEntry {
                    key: Value::String(String::from("enabled")),
                    value: Value::Bool(true),
                },
            ]))),
        )
        .expect("level0 fixture should encode")
}

pub fn assert_level0_positive(bytes: &[u8]) {
    Verifier::new()
        .verify_canonical(bytes)
        .expect("level0 bytes should be canonical");
    let decoded = Reader::new(ReadOptions::new())
        .read(bytes)
        .expect("level0 bytes should decode");
    assert_eq!(decoded.documents.len(), 1);
    assert!(matches!(
        decoded.documents[0].root_value.as_ref(),
        Some(Value::Map(entries))
            if entries.len() == 2
                && entries[0].key == Value::String(String::from("name"))
                && entries[1].value == Value::Bool(true)
    ));
}

pub fn level0_negative_error() -> LumbaError {
    let mut bytes = level0_positive_bytes();
    let header =
        ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
    let start = header.section_table_offset as usize;
    let end = start + SECTION_ENTRY_SIZE as usize;
    let mut entry = SectionEntry::decode(&bytes[start..end]).expect("section entry should decode");
    entry.payload_offset = bytes.len() as u64 + 1;
    bytes[start..end].copy_from_slice(&entry.encode().expect("section entry should reencode"));
    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("corrupt level0 bytes should fail")
}

pub fn level2_positive_bytes() -> Vec<u8> {
    canonical_writer()
        .write(
            &LumbaFile::new()
                .with_document(
                    Document::new().with_root_value(Value::Tagged(lumba::TaggedValue {
                        tag: String::from("Duration").into(),
                        value: Box::new(Value::String(String::from("PT1H"))),
                    })),
                )
                .with_tag_table(
                    TagTable::new().with_declaration(
                        TagDeclaration::new("Duration", "urn:luma:example:duration")
                            .with_flags(
                                TAG_FLAG_KNOWN_TO_PRODUCER
                                    | TAG_FLAG_REQUIRES_RESOLVER
                                    | TAG_FLAG_PORTABLE,
                            )
                            .with_resolver_hint(Some(Value::String(String::from("hint:duration"))))
                            .with_metadata_value(Some(Value::String(String::from("iso8601")))),
                    ),
                ),
        )
        .expect("level2 fixture should encode")
}

pub fn assert_level2_positive(bytes: &[u8]) {
    let decoded = Reader::new(ReadOptions::new())
        .read(bytes)
        .expect("level2 bytes should decode");
    let tags = decoded.tag_table.expect("TAGS should decode");
    assert_eq!(tags.declarations.len(), 1);
    assert_eq!(tags.declarations[0].tag.as_str(), "Duration");
    assert!(tags.declarations[0].is_portable());
}

pub fn level2_negative_error() -> LumbaError {
    let bytes = canonical_writer()
        .write(
            &LumbaFile::new().with_tag_table(
                TagTable::new().with_declaration(
                    TagDeclaration::new("Secret", "urn:luma:example:secret")
                        .with_flags(TAG_FLAG_TRUSTED_ONLY),
                ),
            ),
        )
        .expect("trusted TAGS fixture should encode");
    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only tags")
}

pub fn level3_positive_bytes() -> Vec<u8> {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"print('hi')\n".to_vec()))
        .expect("blob should append");
    Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_blob_table(blobs)
                .with_source_file_table(
                    SourceFileTable::new().with_record(
                        SourceFileRecord::new()
                            .with_uri(Some(String::from("mem://fixture.lua")))
                            .with_display(Some(String::from("fixture.lua")))
                            .with_source_blob_ref(Some(BlobId(0)))
                            .with_flags(SOURCE_FILE_FLAG_VIRTUAL | SOURCE_FILE_FLAG_GENERATED),
                    ),
                ),
        )
        .expect("level3 fixture should encode")
}

pub fn assert_level3_positive(bytes: &[u8]) {
    let decoded = Reader::new(ReadOptions::new())
        .read(bytes)
        .expect("level3 bytes should decode");
    let source = &decoded
        .source_file_table
        .expect("SRCF should decode")
        .records[0];
    assert_eq!(source.uri.as_deref(), Some("mem://fixture.lua"));
    assert_eq!(source.source_blob_ref, Some(BlobId(0)));
}

pub fn level3_negative_error() -> LumbaError {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"secret".to_vec()))
        .expect("blob should append");
    let bytes = Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_blob_table(blobs)
                .with_source_file_table(
                    SourceFileTable::new().with_record(
                        SourceFileRecord::new()
                            .with_uri(Some(String::from("mem://private.luma")))
                            .with_source_blob_ref(Some(BlobId(0)))
                            .with_flags(SOURCE_FILE_FLAG_PRIVATE),
                    ),
                ),
        )
        .expect("private source fixture should encode");
    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject private sources")
}

pub fn level4_positive_bytes() -> Vec<u8> {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"embedded bundle text\n".to_vec()))
        .expect("embedded resource blob should append");
    blobs
        .push(BlobRecord::new(vec![0xAB; 32]))
        .expect("signature blob should append");

    canonical_writer()
        .write(
            &LumbaFile::new()
                .with_document(Document::new().with_root_value(Value::Map(vec![MapEntry {
                    key: Value::String(String::from("bundle")),
                    value: Value::String(String::from("fixture")),
                }])))
                .with_blob_table(blobs)
                .with_dependency_table(
                    DependencyTable::new().with_record(
                        DependencyRecord::new(lumba::DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                            .with_uri(Some(String::from("https://127.0.0.1:9/never-contacted")))
                            .with_flags(
                                lumba::DEPENDENCY_FLAG_EMBEDDED
                                    | lumba::DEPENDENCY_FLAG_NETWORK_URI,
                            ),
                    ),
                )
                .with_embedded_resource_table(EmbeddedResourceTable::new().with_record(
                    EmbeddedResourceRecord::new(
                        0,
                        lumba::EMBEDDED_RESOURCE_KIND_LUMA_TEXT,
                        BlobId(0),
                    ),
                ))
                .with_signature_table(
                    SignatureTable::new().with_record(
                        SignatureRecord::new(SIGNATURE_RECORD_KIND_DIGEST)
                            .with_algorithm(Some(Identifier::new(SIGNATURE_ALGORITHM_SHA256)))
                            .with_covered_section_refs(vec![1, 3, 4, 5])
                            .with_payload_blob_ref(Some(BlobId(1))),
                    ),
                ),
        )
        .expect("level4 fixture should encode")
}

pub fn assert_level4_positive(bytes: &[u8]) {
    let decoded = Reader::new(ReadOptions::new().with_limits(Limits::trusted()))
        .read(bytes)
        .expect("level4 bytes should decode");
    let report = SignatureVerifier::new()
        .verify_structural_coverage(&decoded)
        .expect("signature coverage should verify structurally");
    assert_eq!(report.records.len(), 1);
    assert!(decoded.embedded_resource_table.is_some());
}

pub fn level4_negative_error() -> LumbaError {
    Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_dependency_table(
                    DependencyTable::new().with_record(
                        DependencyRecord::new(lumba::DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                            .with_uri(Some(String::from("https://example.invalid/resource"))),
                    ),
                )
                .with_embedded_resource_table(EmbeddedResourceTable::new().with_record(
                    EmbeddedResourceRecord::new(
                        1,
                        lumba::EMBEDDED_RESOURCE_KIND_LUMA_TEXT,
                        BlobId(0),
                    ),
                )),
        )
        .expect_err("writer should reject invalid embedded resource refs")
}

pub fn level5_positive_bytes() -> Vec<u8> {
    let mut blobs = BlobTable::new();
    blobs
        .push(BlobRecord::new(b"return 7".to_vec()))
        .expect("chunk blob should append");

    Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_blob_table(blobs)
                .with_document(
                    Document::new()
                        .with_root_value(Value::Map(vec![
                            MapEntry {
                                key: Value::String(String::from("expr")),
                                value: Value::ExpressionSource(lumba::ExpressionValue {
                                    language: Identifier::new("lua.expr"),
                                    source: lumba::ExpressionSource::Text(String::from("1 + 1")),
                                    capability_set_ref: Some(0),
                                    result_value: Some(Box::new(Value::String(String::from(
                                        "cached expression result",
                                    )))),
                                }),
                            },
                            MapEntry {
                                key: Value::String(String::from("chunk")),
                                value: Value::LuaChunkSource(lumba::LuaChunkValue {
                                    language: Identifier::new("lua.chunk"),
                                    source_blob_ref: BlobId(0),
                                    capability_set_ref: Some(0),
                                    result_value: Some(Box::new(Value::UInt(7))),
                                }),
                            },
                        ]))
                        .with_capability_set_ref(Some(0)),
                )
                .with_capability_table(CapabilityTable::new().with_record(
                    CapabilitySetRecord::new().with_requirement(
                        CapabilityRequirement::new(Identifier::new("lua.eval.expr")).with_flags(
                            lumba::CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION
                                | lumba::CAPABILITY_FLAG_PURE_EXPECTED
                                | lumba::CAPABILITY_FLAG_DETERMINISTIC_EXPECTED,
                        ),
                    ),
                )),
        )
        .expect("level5 fixture should encode")
}

pub fn assert_level5_positive(bytes: &[u8]) {
    let decoded = Reader::new(ReadOptions::new().with_limits(Limits::trusted()))
        .read(bytes)
        .expect("level5 bytes should decode for trusted policy");
    assert_eq!(decoded.documents[0].capability_set_ref, Some(0));
    assert!(decoded.capability_table.is_some());
}

pub fn level5_negative_error() -> LumbaError {
    let bytes = canonical_writer()
        .write(
            &LumbaFile::new().with_capability_table(
                CapabilityTable::new().with_record(
                    CapabilitySetRecord::new().with_requirement(
                        CapabilityRequirement::new(Identifier::new("module.resolve"))
                            .with_flags(lumba::CAPABILITY_FLAG_TRUSTED_ONLY),
                    ),
                ),
            ),
        )
        .expect("trusted-only CAPS fixture should encode");
    Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only capability sets")
}

pub fn canonical_negative_error() -> LumbaError {
    let bytes = Writer::new(WriteOptions::new())
        .write(&LumbaFile::new().with_document(Document::new().with_root_value(Value::Int(7))))
        .expect("default writer should encode");
    Verifier::new()
        .verify_canonical(&bytes)
        .expect_err("default writer bytes should be noncanonical")
}

pub fn cli_fixture_source() -> PathBuf {
    fixture_root()
        .join("level4")
        .join("cli-fixture-source.luma")
}

pub fn run_cli_fixture_flow() {
    let temp = unique_temp_dir();
    fs::create_dir_all(&temp).expect("temp dir should create");
    let output = temp.join("fixture.lumba");

    let encode = cargo_cli([
        "run",
        "-q",
        "-p",
        "luma-cli",
        "--features",
        "lumba",
        "--",
        "--output",
        "json",
        "lumba",
        "encode",
        cli_fixture_source().to_str().unwrap(),
        output.to_str().unwrap(),
        "--mode",
        "fixture",
        "--footer",
        "--checksum",
        "crc32c",
        "--include-source",
    ]);
    assert!(
        encode.status.success(),
        "CLI fixture encode failed: {encode:?}"
    );
    assert!(String::from_utf8_lossy(&encode.stdout).contains("\"ok\":true"));

    let verify = cargo_cli([
        "run",
        "-q",
        "-p",
        "luma-cli",
        "--features",
        "lumba",
        "--",
        "--output",
        "json",
        "lumba",
        "verify",
        output.to_str().unwrap(),
    ]);
    assert!(verify.status.success(), "CLI fixture verify failed: {verify:?}");
    assert!(String::from_utf8_lossy(&verify.stdout).contains("\"ok\":true"));

    let inspect_header = cargo_cli([
        "run",
        "-q",
        "-p",
        "luma-cli",
        "--features",
        "lumba",
        "--",
        "--output",
        "json",
        "lumba",
        "inspect",
        output.to_str().unwrap(),
        "--emit",
        "header",
    ]);
    assert!(
        inspect_header.status.success(),
        "CLI fixture header inspection should still succeed: {inspect_header:?}"
    );
    assert!(String::from_utf8_lossy(&inspect_header.stdout).contains("\"header\""));

    let inspect = cargo_cli([
        "run",
        "-q",
        "-p",
        "luma-cli",
        "--features",
        "lumba",
        "--",
        "--output",
        "json",
        "lumba",
        "inspect",
        output.to_str().unwrap(),
        "--emit",
        "sections",
    ]);
    assert!(inspect.status.success(), "fixture section inspection failed: {inspect:?}");
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("\"sections\""));
}

fn canonical_writer() -> Writer {
    Writer::new(
        WriteOptions::new()
            .with_mode(lumba::WriterMode::Canonical(lumba::CanonicalMode::Strict))
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
}

fn cargo_cli<const N: usize>(args: [&str; N]) -> std::process::Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo")))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("cargo command should execute")
}

fn unique_temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "luma-lumba-conformance-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    ))
}
