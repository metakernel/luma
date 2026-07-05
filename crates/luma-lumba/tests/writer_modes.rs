//! Writer mode policy integration tests.

use luma_lumba::container::{ContainerHeader, HeaderCrcMode, SECTION_ENTRY_SIZE};
use luma_lumba::diagnostic::{DiagnosticRecord, DiagnosticTable, StoredDiagnosticSeverity};
use luma_lumba::section::SectionEntry;
use luma_lumba::source::{SourceFileRecord, SourceFileTable, SourceSpanRecord, SourceSpanTable};
use luma_lumba::syntax::{SyntaxField, SyntaxFieldValue, SyntaxNodeRecord, SyntaxNodeTable};
use luma_lumba::trivia::{TRIVIA_KIND_COMMENT, TriviaRecord, TriviaTable};
use luma_lumba::{
    BlobRecord, BlobTable, CanonicalMode, Document, LumbaFile, Value, WriteOptions, Writer,
    WriterMode,
};

const CONTAINER_FLAG_HAS_SOURCE: u32 = 1 << 2;
const CONTAINER_FLAG_HAS_SYNTAX: u32 = 1 << 3;
const CONTAINER_FLAG_HAS_VALUES: u32 = 1 << 4;
const CONTAINER_FLAG_HAS_DIAGNOSTICS: u32 = 1 << 6;

const PROFILE_FLAG_VALUE_IMAGE: u32 = 1 << 3;
const PROFILE_FLAG_SYNTAX_IMAGE: u32 = 1 << 4;

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

fn syntax_fixture(with_blob: bool) -> LumbaFile {
    let mut file = LumbaFile::new()
        .with_document(Document::new().with_root_value(Value::String(String::from("Ada"))))
        .with_source_file_table(
            SourceFileTable::new().with_record(
                SourceFileRecord::new()
                    .with_uri(Some(String::from("mem://fixture.luma")))
                    .with_display(Some(String::from("fixture.luma"))),
            ),
        )
        .with_source_span_table(
            SourceSpanTable::new().with_record(
                SourceSpanRecord::new(0, 0, 3)
                    .with_start_position(1, 1)
                    .with_end_position(1, 4),
            ),
        )
        .with_syntax_node_table(
            SyntaxNodeTable::new().with_record(
                SyntaxNodeRecord::new("document")
                    .with_primary_span_ref(Some(0))
                    .with_leading_trivia_ref(Some(0))
                    .with_field(SyntaxField::new("value", SyntaxFieldValue::ValueRef(0))),
            ),
        )
        .with_trivia_table(TriviaTable::new().with_record(
            TriviaRecord::new(TRIVIA_KIND_COMMENT, "-- fixture").with_span_ref(Some(0)),
        ))
        .with_diagnostic_table(
            DiagnosticTable::new().with_record(
                DiagnosticRecord::new(
                    StoredDiagnosticSeverity::Warning,
                    "W0001",
                    "fixture warning",
                )
                .with_primary_span_ref(Some(0)),
            ),
        );

    if with_blob {
        let mut blobs = BlobTable::new();
        blobs
            .push(BlobRecord::new(b"cached source".to_vec()))
            .expect("blob should append");
        file = file.with_blob_table(blobs);
    }

    file
}

#[test]
fn editor_cache_mode_emits_recommended_sections_and_flags() {
    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::EditorCache)
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&syntax_fixture(true))
    .expect("editor cache fixture should encode");

    let header = decode_header(&bytes);

    assert_eq!(
        header.container_flags,
        CONTAINER_FLAG_HAS_SOURCE
            | CONTAINER_FLAG_HAS_SYNTAX
            | CONTAINER_FLAG_HAS_VALUES
            | CONTAINER_FLAG_HAS_DIAGNOSTICS
    );
    assert_eq!(
        header.profile_flags,
        PROFILE_FLAG_VALUE_IMAGE | PROFILE_FLAG_SYNTAX_IMAGE
    );
    assert_eq!(
        section_ids(&bytes),
        vec![
            "META", "STRS", "SYMS", "BLOB", "VALS", "DOCS", "SRCF", "SRCS", "ASTN", "TRIV", "DIAG",
        ]
    );
}

#[test]
fn conformance_fixture_mode_emits_recommended_sections_and_flags() {
    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::ConformanceFixture)
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&syntax_fixture(false))
    .expect("conformance fixture should encode");

    let header = decode_header(&bytes);

    assert_eq!(
        header.container_flags,
        CONTAINER_FLAG_HAS_SOURCE
            | CONTAINER_FLAG_HAS_SYNTAX
            | CONTAINER_FLAG_HAS_VALUES
            | CONTAINER_FLAG_HAS_DIAGNOSTICS
    );
    assert_eq!(
        header.profile_flags,
        PROFILE_FLAG_VALUE_IMAGE | PROFILE_FLAG_SYNTAX_IMAGE
    );
    assert_eq!(
        section_ids(&bytes),
        vec![
            "META", "STRS", "SYMS", "VALS", "DOCS", "SRCF", "SRCS", "ASTN", "TRIV", "DIAG",
        ]
    );
}

#[test]
fn canonical_value_mode_omits_source_syntax_and_trivia_sections() {
    let bytes = Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::Canonical(CanonicalMode::Strict))
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&syntax_fixture(false))
    .expect("canonical value image should encode");

    assert_eq!(section_ids(&bytes), vec!["VALS", "DOCS"]);
}
