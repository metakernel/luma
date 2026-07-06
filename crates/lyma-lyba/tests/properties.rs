#![allow(missing_docs)]

use lyma_lyba::container::{ContainerHeader, HeaderCrcMode, SECTION_ENTRY_SIZE};
use lyma_lyba::primitives::{Identifier, SVar, UVar};
use lyma_lyba::section::{CHECKSUM_NONE, CODEC_NONE, Section, SectionEntry, SectionId};
use lyma_lyba::{
    BlobRecord, BlobTable, Document, FiniteFloat, Limits, MapEntry, ReadOptions, Reader,
    TaggedValue, Value, WriteOptions, Writer, WriterMode,
};
use lyma_syntax::{
    FileId, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaSequence, LymaTag,
    LymaTagName, LymaTaggedValue, LymaValue, source::Span,
};
use proptest::prelude::*;

const MAX_DEPTH: u32 = 3;

fn empty_span() -> Span {
    Span::new(FileId(0), 0, 0)
}

fn arb_string(max_len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..=max_len).prop_map(|chars| chars.into_iter().collect())
}

fn arb_identifier() -> impl Strategy<Value = Identifier> {
    arb_string(8).prop_map(Identifier::new)
}

fn arb_finite_float() -> impl Strategy<Value = FiniteFloat> {
    any::<u64>()
        .prop_map(f64::from_bits)
        .prop_filter("finite float", |value| value.is_finite())
        .prop_map(|value| FiniteFloat::new(value).expect("finite float should construct"))
}

fn arb_lyma_number() -> impl Strategy<Value = LymaNumber> {
    prop_oneof![
        any::<i64>().prop_map(LymaNumber::Integer),
        any::<u64>()
            .prop_map(f64::from_bits)
            .prop_filter("finite float", |value| value.is_finite())
            .prop_map(LymaNumber::Float),
    ]
}

fn arb_lyma_value() -> BoxedStrategy<LymaValue> {
    let leaf = prop_oneof![
        Just(LymaValue::Null(LymaNull)),
        any::<bool>().prop_map(LymaValue::Boolean),
        arb_lyma_number().prop_map(LymaValue::Number),
        arb_string(16).prop_map(LymaValue::String),
    ];

    leaf.prop_recursive(MAX_DEPTH, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..=4)
                .prop_map(|items| { LymaValue::Sequence(LymaSequence { items, span: None }) }),
            prop::collection::btree_map(arb_string(8), inner.clone(), 0..=4).prop_map(|entries| {
                LymaValue::Mapping(LymaMapping {
                    entries: entries
                        .into_iter()
                        .map(|(key, value)| LymaMappingEntry {
                            key: LymaKey::String(key),
                            value,
                            span: None,
                        })
                        .collect(),
                    duplicate_keys: Vec::new(),
                    span: None,
                })
            }),
            (arb_string(8), inner).prop_map(|(tag, value)| {
                LymaValue::Tagged(LymaTaggedValue {
                    tag: LymaTag {
                        name: LymaTagName {
                            value: tag,
                            span: empty_span(),
                        },
                        span: empty_span(),
                    },
                    value: Box::new(value),
                    span: None,
                })
            }),
        ]
    })
    .boxed()
}

fn arb_native_value() -> BoxedStrategy<Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        any::<u64>().prop_map(Value::UInt),
        arb_finite_float().prop_map(Value::Float),
        arb_string(16).prop_map(Value::String),
        prop::collection::vec(any::<u8>(), 0..=16).prop_map(Value::BytesInline),
        (0_u64..=1).prop_map(|id| Value::BytesBlob(lyma_lyba::BlobId(id))),
    ];

    leaf.prop_recursive(MAX_DEPTH, 128, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..=4).prop_map(Value::Sequence),
            prop::collection::btree_map(arb_string(8), inner.clone(), 0..=4).prop_map(|entries| {
                Value::Map(
                    entries
                        .into_iter()
                        .map(|(key, value)| MapEntry {
                            key: Value::String(key),
                            value,
                        })
                        .collect(),
                )
            }),
            (arb_identifier(), inner.clone()).prop_map(|(tag, value)| Value::Tagged(TaggedValue {
                tag,
                value: Box::new(value),
            })),
        ]
    })
    .boxed()
}

fn arb_section_id() -> impl Strategy<Value = SectionId> {
    prop::array::uniform4(prop::sample::select((b'A'..=b'Z').collect::<Vec<_>>()))
        .prop_map(SectionId::new)
}

fn fuzz_limits() -> Limits {
    Limits::strict()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn uvar_round_trips_canonically(value in any::<u64>()) {
        let encoded = UVar(value).encode();
        let mut offset = 0;
        let decoded = UVar::decode_canonical(&encoded, &mut offset)?;

        prop_assert_eq!(decoded, UVar(value));
        prop_assert_eq!(offset, encoded.len());
        prop_assert_eq!(encoded, decoded.encode());
    }

    #[test]
    fn svar_round_trips_canonically(value in any::<i64>()) {
        let encoded = SVar(value).encode();
        let mut offset = 0;
        let decoded = SVar::decode_canonical(&encoded, &mut offset)?;

        prop_assert_eq!(decoded, SVar(value));
        prop_assert_eq!(offset, encoded.len());
        prop_assert_eq!(encoded, decoded.encode());
    }

    #[test]
    fn container_headers_round_trip(
        container_flags in 0_u32..0x0400,
        profile_flags in 0_u32..0x0100,
        root_document_count in 0_u64..=8,
    ) {
        let header = ContainerHeader {
            container_flags,
            profile_flags,
            section_table_offset: 64,
            section_count: 0,
            section_entry_size: SECTION_ENTRY_SIZE,
            file_length: 64,
            root_document_count,
            header_crc32c: 0,
        };

        let encoded = header.encode(HeaderCrcMode::Enabled)?;
        let decoded = ContainerHeader::decode(&encoded, HeaderCrcMode::Enabled)?;

        prop_assert_eq!(decoded.container_flags, container_flags);
        prop_assert_eq!(decoded.profile_flags, profile_flags);
        prop_assert_eq!(decoded.section_table_offset, 64);
        prop_assert_eq!(decoded.section_count, 0);
        prop_assert_eq!(decoded.section_entry_size, SECTION_ENTRY_SIZE);
        prop_assert_eq!(decoded.file_length, 64);
        prop_assert_eq!(decoded.root_document_count, root_document_count);
        prop_assert_ne!(decoded.header_crc32c, 0);
        prop_assert_eq!(decoded.encode(HeaderCrcMode::Enabled)?, encoded);
    }

    #[test]
    fn section_entries_round_trip(
        section_id in arb_section_id(),
        section_version in any::<u16>(),
        entry_flags in 0_u16..0x0040,
        payload_flags in any::<u32>(),
        payload_offset in any::<u64>(),
        stored_size in any::<u64>(),
        logical_size in any::<u64>(),
        item_count in any::<u64>(),
        checksum_low in any::<u64>(),
        checksum_high in any::<u64>(),
    ) {
        let entry = SectionEntry {
            section_id,
            section_version,
            entry_flags,
            payload_flags,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset,
            stored_size,
            logical_size,
            item_count,
            checksum_low,
            checksum_high,
        };

        let encoded = entry.encode()?;
        let decoded = SectionEntry::decode(&encoded)?;

        prop_assert_eq!(decoded, entry);
    }

    #[test]
    fn portable_value_images_round_trip(values in prop::collection::vec(arb_lyma_value(), 0..=6)) {
        let encoded = lyma_lyba::try_to_lyba_value_image(&values)?;
        let decoded = lyma_lyba::try_from_lyba_value_image(&encoded)?;
        let reencoded = lyma_lyba::try_to_lyba_value_image(&decoded)?;

        prop_assert_eq!(decoded, values);
        prop_assert_eq!(reencoded, encoded);
    }

    #[test]
    fn native_value_containers_round_trip(values in prop::collection::vec(arb_native_value(), 0..=4)) {
        let mut blob_table = BlobTable::new();
        let _ = blob_table.push(BlobRecord::new(vec![0, 1, 2, 3]))?;
        let _ = blob_table.push(BlobRecord::new(b"payload".to_vec()))?;

        let mut file = lyma_lyba::LybaFile::new().with_blob_table(blob_table);
        for value in &values {
            file = file.with_document(Document::new().with_root_value(value.clone()));
        }
        file = file.with_section(Section {
            name: Identifier::new("VALS"),
            values: values.clone(),
        });

        let writer = Writer::new(
            WriteOptions::new()
                .with_mode(WriterMode::RuntimeData)
                .with_limits(fuzz_limits())
                .with_header_crc_mode(HeaderCrcMode::Disabled),
        );
        let reader = Reader::new(ReadOptions::new().with_limits(fuzz_limits()));

        let encoded = writer.write(&file)?;
        let decoded = reader.read(&encoded)?;
        let reencoded = writer.write(&decoded)?;

        let decoded_roots = decoded
            .documents
            .iter()
            .filter_map(|document| document.root_value.clone())
            .collect::<Vec<_>>();

        prop_assert_eq!(decoded_roots, values);
        prop_assert_eq!(reencoded, encoded);
    }
}
