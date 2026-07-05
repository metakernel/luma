//! Symbol-table integration tests.

use luma_lumba::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE};
use luma_lumba::primitives::UVar;
use luma_lumba::section::{
    CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use luma_lumba::string_table::{StringRecord, StringTable};
use luma_lumba::symbol::{
    SYMBOL_FLAG_KEY, SYMBOL_FLAG_RESERVED_MASK, SYMBOL_FLAG_TAG, SymbolRecord, SymbolTable,
    encode_symbol_table,
};
use luma_lumba::value::{MapEntry, TaggedValue, Value};
use luma_lumba::{
    Document, LumbaError, LumbaFile, ReadOptions, Reader, WriteOptions, Writer, WriterMode,
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

fn syms_entry(payload_len: usize, item_count: u64) -> SectionEntry {
    SectionEntry {
        section_id: SectionId::SYMS,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset: 256,
        stored_size: payload_len as u64,
        logical_size: payload_len as u64,
        item_count,
        checksum_low: 0,
        checksum_high: 0,
    }
}

fn strs_entry(payload_len: usize, item_count: u64) -> SectionEntry {
    SectionEntry {
        section_id: SectionId::STRS,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset: 192,
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

#[test]
fn writer_and_reader_round_trip_symbols_with_namespaces() {
    let file = LumbaFile::new()
        .with_string_table(
            StringTable::new()
                .with_string(StringRecord::new("name"))
                .with_string(StringRecord::new("person"))
                .with_string(StringRecord::new("luma")),
        )
        .with_symbol_table(
            SymbolTable::new()
                .with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_KEY))
                .with_symbol(
                    SymbolRecord::new(1)
                        .with_namespace_string_id(Some(2))
                        .with_flags(SYMBOL_FLAG_TAG),
                ),
        );

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("writer should encode symbols"),
        )
        .expect("reader should decode symbols");

    assert_eq!(decoded.string_table, file.string_table);
    assert_eq!(decoded.symbol_table, file.symbol_table);
}

#[test]
fn reader_rejects_invalid_symbol_string_reference_with_lb0014() {
    let strs = strs_payload(&[b"only"]);
    let syms = encode_symbol_table(
        &SymbolTable::new().with_symbol(SymbolRecord::new(1).with_flags(SYMBOL_FLAG_KEY)),
    )
    .expect("symbol table should encode");
    let bytes = build_file(
        &[strs_entry(strs.len(), 1), syms_entry(syms.len(), 1)],
        &[&strs, &syms],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid string ref should fail");

    match &error {
        LumbaError::InvalidValueReference(_) => {}
        other => panic!("expected InvalidValueReference, got {other:?}"),
    }
    assert_eq!(error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_reserved_symbol_flags_with_lb0025() {
    let strs = strs_payload(&[b"name"]);
    let syms = encode_symbol_table(
        &SymbolTable::new().with_symbol(SymbolRecord::new(0).with_flags(SYMBOL_FLAG_RESERVED_MASK)),
    )
    .expect_err("reserved flags should fail during encode");

    assert_eq!(syms.code().as_str(), "LB0025");

    let mut payload = Vec::new();
    UVar(1).encode_into(&mut payload);
    UVar(0).encode_into(&mut payload);
    UVar(0).encode_into(&mut payload);
    UVar(SYMBOL_FLAG_RESERVED_MASK).encode_into(&mut payload);
    let bytes = build_file(
        &[strs_entry(strs.len(), 1), syms_entry(payload.len(), 1)],
        &[&strs, &payload],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("reserved symbol flags should fail");

    assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}

#[test]
fn runtime_data_writer_emits_deterministic_symbol_ordering() {
    let file = LumbaFile::new().with_document(Document::new().with_root_value(Value::Map(vec![
        MapEntry {
            key: Value::String(String::from("name")),
            value: Value::Tagged(TaggedValue {
                tag: "person".into(),
                value: Box::new(Value::String(String::from("Ada"))),
            }),
        },
    ])));

    let writer = Writer::new(WriteOptions::new().with_mode(WriterMode::RuntimeData));
    let left = writer.write(&file).expect("first encode should succeed");
    let right = writer.write(&file).expect("second encode should succeed");
    assert_eq!(left, right);

    let decoded = Reader::new(ReadOptions::new())
        .read(&left)
        .expect("runtime data should decode");
    let strings = decoded.string_table.expect("STRS should exist");
    let symbols = decoded.symbol_table.expect("SYMS should exist");
    let names = symbols
        .symbols
        .iter()
        .map(|record| strings.strings[record.string_id as usize].value.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "canonical",
            "format",
            "image_kind",
            "luma_version",
            "lumba_version",
            "name",
            "person"
        ]
    );
    assert_eq!(symbols.symbols[5].flags, SYMBOL_FLAG_KEY);
    assert_eq!(symbols.symbols[6].flags, SYMBOL_FLAG_TAG);
}
