//! Integration tests for Level 2 tag registry support.

use lyma_lyba::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE};
use lyma_lyba::primitives::UVar;
use lyma_lyba::section::{
    CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use lyma_lyba::symbol::{
    SYMBOL_FLAG_KEY, SYMBOL_FLAG_TAG, SymbolRecord, SymbolTable, encode_symbol_table,
};
use lyma_lyba::{
    Document, LybaError, LybaFile, ReadOptions, Reader, TAG_FLAG_KNOWN_TO_PRODUCER,
    TAG_FLAG_PORTABLE, TAG_FLAG_REQUIRES_RESOLVER, TAG_FLAG_TRUSTED_ONLY, TagDeclaration, TagTable,
    TaggedValue, Value, WriteOptions, Writer,
};

fn build_file(entries: &[SectionEntry], payloads: &[&[u8]], table_offset: u64) -> Vec<u8> {
    let table = entries
        .iter()
        .flat_map(|entry| entry.encode().expect("entry should encode"))
        .collect::<Vec<_>>();
    let mut file_len = usize::from(HEADER_SIZE).max((table_offset as usize) + table.len());
    for (entry, payload) in entries.iter().zip(payloads) {
        file_len = file_len.max((entry.payload_offset as usize) + payload.len());
    }

    let mut header = ContainerHeader::new();
    header.section_table_offset = table_offset;
    header.section_count = entries.len() as u32;
    header.section_entry_size = SECTION_ENTRY_SIZE;
    header.file_length = file_len as u64;

    let mut bytes = vec![0_u8; file_len];
    bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
        &header
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode"),
    );
    bytes[table_offset as usize..table_offset as usize + table.len()].copy_from_slice(&table);
    for (entry, payload) in entries.iter().zip(payloads) {
        bytes[entry.payload_offset as usize..entry.payload_offset as usize + payload.len()]
            .copy_from_slice(payload);
    }
    bytes
}

fn entry(
    section_id: SectionId,
    payload_offset: u64,
    payload_len: usize,
    item_count: u64,
) -> SectionEntry {
    SectionEntry {
        section_id,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset,
        stored_size: payload_len as u64,
        logical_size: payload_len as u64,
        item_count,
        checksum_low: 0,
        checksum_high: 0,
    }
}

fn strs_payload(values: &[&[u8]]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(values.len() as u64).encode_into(&mut payload);
    for value in values {
        UVar(0).encode_into(&mut payload);
        UVar(value.len() as u64).encode_into(&mut payload);
        payload.extend_from_slice(value);
    }
    payload
}

fn tags_payload(records: &[(u64, u64, u64, u64, u64, u64)]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(records.len() as u64).encode_into(&mut payload);
    for (tag_symbol_id, uri_string_ref, flags, schema_ref, resolver_hint_ref, metadata_ref) in
        records
    {
        UVar(*tag_symbol_id).encode_into(&mut payload);
        UVar(*uri_string_ref).encode_into(&mut payload);
        UVar(*flags).encode_into(&mut payload);
        UVar(*schema_ref).encode_into(&mut payload);
        UVar(*resolver_hint_ref).encode_into(&mut payload);
        UVar(*metadata_ref).encode_into(&mut payload);
    }
    payload
}

#[test]
fn tag_registry_round_trips_flags_metadata_and_tagged_values_without_resolution() {
    let file = LybaFile::new()
        .with_document(Document::new().with_root_value(Value::Tagged(TaggedValue {
            tag: "Duration".into(),
            value: Box::new(Value::String(String::from("PT1H"))),
        })))
        .with_tag_table(
            TagTable::new().with_declaration(
                TagDeclaration::new("Duration", "urn:lyma:example:duration")
                    .with_flags(
                        TAG_FLAG_KNOWN_TO_PRODUCER | TAG_FLAG_REQUIRES_RESOLVER | TAG_FLAG_PORTABLE,
                    )
                    .with_resolver_hint(Some(Value::String(String::from("hint:duration"))))
                    .with_metadata_value(Some(Value::String(String::from("iso8601")))),
            ),
        );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode TAGS");
    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("reader should preserve TAGS");

    assert_eq!(decoded.documents, file.documents);
    let tags = decoded.tag_table.expect("TAGS should decode");
    assert_eq!(tags.declarations.len(), 1);
    let declaration = &tags.declarations[0];
    assert_eq!(declaration.tag.as_str(), "Duration");
    assert_eq!(declaration.uri, "urn:lyma:example:duration");
    assert!(declaration.is_known_to_producer());
    assert!(declaration.requires_resolver());
    assert!(declaration.is_portable());
    assert!(!declaration.is_trusted_only());
    assert_eq!(
        declaration.resolver_hint,
        Some(Value::String(String::from("hint:duration")))
    );
    assert_eq!(
        declaration.metadata_value,
        Some(Value::String(String::from("iso8601")))
    );
    assert!(matches!(
        &decoded.documents[0].root_value,
        Some(Value::Tagged(TaggedValue { tag, value }))
            if tag.as_str() == "Duration" && **value == Value::String(String::from("PT1H"))
    ));
}

#[test]
fn trusted_only_tag_requires_trusted_reader_policy() {
    let file = LybaFile::new().with_tag_table(TagTable::new().with_declaration(
        TagDeclaration::new("Secret", "urn:lyma:example:secret").with_flags(TAG_FLAG_TRUSTED_ONLY),
    ));

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode trusted-only TAGS");
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public reader should reject trusted-only TAGS");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
    assert!(
        Reader::new(ReadOptions::new().with_limits(lyma_lyba::Limits::trusted()))
            .read(&bytes)
            .is_ok()
    );
}

#[test]
fn reader_rejects_non_tag_symbol_reference_with_lb0014() {
    let strs = strs_payload(&[b"Duration", b"urn:lyma:example:duration"]);
    let syms = encode_symbol_table(
        &SymbolTable::new().with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_KEY)),
    )
    .expect("symbol table should encode");
    let tags = tags_payload(&[(0, 1, 0, 0, 0, 0)]);

    let bytes = build_file(
        &[
            entry(SectionId::STRS, 256, strs.len(), 2),
            entry(SectionId::SYMS, 320, syms.len(), 1),
            entry(SectionId::TAGS, 384, tags.len(), 1),
        ],
        &[&strs, &syms, &tags],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("non-tag symbol ref should fail");

    assert!(matches!(error, LybaError::InvalidValueReference(_)));
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_invalid_schema_reference_with_lb0015() {
    let strs = strs_payload(&[b"Duration", b"urn:lyma:example:duration"]);
    let syms = encode_symbol_table(
        &SymbolTable::new().with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_TAG)),
    )
    .expect("symbol table should encode");
    let tags = tags_payload(&[(0, 1, 0, 1, 0, 0)]);

    let bytes = build_file(
        &[
            entry(SectionId::STRS, 256, strs.len(), 2),
            entry(SectionId::SYMS, 320, syms.len(), 1),
            entry(SectionId::TAGS, 384, tags.len(), 1),
        ],
        &[&strs, &syms, &tags],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid schema ref should fail");

    assert!(matches!(error, LybaError::InvalidSyntaxNodeReference(_)));
    assert_eq!(error.code().as_str(), "LB0015");
}
