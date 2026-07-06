//! Level 1 golden fixture and malformed-binary tests.

use lyma_lyba::document::DOCUMENT_FLAG_HAS_VALUE_ROOT;
use lyma_lyba::primitives::UVar;
use lyma_lyba::section::SectionEntry;
use lyma_lyba::{LybaError, try_from_lyba_value_image, try_to_lyba_value_image};
use lyma_syntax::{
    FileId, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaSequence, LymaTag,
    LymaTagName, LymaTaggedValue, LymaValue, source::Span,
};
use std::{fs, path::PathBuf};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("lyba")
        .join("level1")
}

fn span() -> Span {
    Span::new(FileId(0), 0, 0)
}

fn tagged(name: &str, value: LymaValue) -> LymaValue {
    LymaValue::Tagged(LymaTaggedValue {
        tag: LymaTag {
            name: LymaTagName {
                value: name.to_owned(),
                span: span(),
            },
            span: span(),
        },
        value: Box::new(value),
        span: None,
    })
}

fn minimal_values() -> Vec<LymaValue> {
    vec![
        LymaValue::Null(LymaNull),
        LymaValue::Boolean(false),
        LymaValue::Boolean(true),
        LymaValue::Number(LymaNumber::Integer(-7)),
        LymaValue::Number(LymaNumber::Float(1.5)),
        LymaValue::String(String::from("hi")),
    ]
}

fn nested_values() -> Vec<LymaValue> {
    vec![LymaValue::Mapping(LymaMapping {
        entries: vec![
            LymaMappingEntry {
                key: LymaKey::String(String::from("items")),
                value: LymaValue::Sequence(LymaSequence {
                    items: vec![
                        LymaValue::Number(LymaNumber::Integer(1)),
                        LymaValue::Mapping(LymaMapping {
                            entries: vec![LymaMappingEntry {
                                key: LymaKey::String(String::from("deep")),
                                value: LymaValue::Boolean(true),
                                span: None,
                            }],
                            duplicate_keys: Vec::new(),
                            span: None,
                        }),
                    ],
                    span: None,
                }),
                span: None,
            },
            LymaMappingEntry {
                key: LymaKey::String(String::from("note")),
                value: LymaValue::String(String::from("nested")),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    })]
}

fn multiple_documents() -> Vec<LymaValue> {
    vec![
        LymaValue::String(String::from("first")),
        LymaValue::Sequence(LymaSequence {
            items: vec![LymaValue::Boolean(true), LymaValue::Boolean(false)],
            span: None,
        }),
        LymaValue::Mapping(LymaMapping {
            entries: vec![LymaMappingEntry {
                key: LymaKey::String(String::from("third")),
                value: LymaValue::Null(LymaNull),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        }),
    ]
}

fn tagged_values() -> Vec<LymaValue> {
    vec![
        tagged("Thing", LymaValue::String(String::from("alpha"))),
        tagged(
            "Wrap",
            LymaValue::Mapping(LymaMapping {
                entries: vec![LymaMappingEntry {
                    key: LymaKey::String(String::from("inner")),
                    value: tagged("Leaf", LymaValue::Number(LymaNumber::Integer(2))),
                    span: None,
                }],
                duplicate_keys: Vec::new(),
                span: None,
            }),
        ),
    ]
}

fn duplicate_key_values() -> Vec<LymaValue> {
    vec![LymaValue::Mapping(LymaMapping {
        entries: vec![
            LymaMappingEntry {
                key: LymaKey::String(String::from("dup")),
                value: LymaValue::Number(LymaNumber::Integer(1)),
                span: None,
            },
            LymaMappingEntry {
                key: LymaKey::String(String::from("dup")),
                value: LymaValue::Number(LymaNumber::Integer(2)),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    })]
}

fn valid_cases() -> [(&'static str, Vec<LymaValue>, &'static [u8]); 4] {
    [
        (
            "minimal-values",
            minimal_values(),
            include_bytes!("../../../tests/fixtures/lyba/level1/minimal-values.lyba"),
        ),
        (
            "nested-values",
            nested_values(),
            include_bytes!("../../../tests/fixtures/lyba/level1/nested-values.lyba"),
        ),
        (
            "multiple-documents",
            multiple_documents(),
            include_bytes!("../../../tests/fixtures/lyba/level1/multiple-documents.lyba"),
        ),
        (
            "tags",
            tagged_values(),
            include_bytes!("../../../tests/fixtures/lyba/level1/tags.lyba"),
        ),
    ]
}

fn docs_payload(count: u64, flags: u64, root_ref: Option<u64>) -> Vec<u8> {
    let mut bytes = Vec::new();
    UVar(count).encode_into(&mut bytes);
    UVar(flags).encode_into(&mut bytes);
    if let Some(root_ref) = root_ref {
        UVar(root_ref).encode_into(&mut bytes);
    }
    bytes
}

fn invalid_refs_bytes() -> Vec<u8> {
    let mut bytes = try_to_lyba_value_image(&[LymaValue::Null(LymaNull)])
        .expect("single-null image should encode");
    let docs_entry = SectionEntry::decode(&bytes[192..256]).expect("DOCS entry should decode");
    let payload = docs_payload(1, DOCUMENT_FLAG_HAS_VALUE_ROOT, Some(1));
    let offset = docs_entry.payload_offset as usize;
    bytes[offset..offset + payload.len()].copy_from_slice(&payload);
    bytes
}

fn invalid_varints_bytes() -> Vec<u8> {
    let mut bytes = try_to_lyba_value_image(&[LymaValue::Null(LymaNull)])
        .expect("single-null image should encode");
    let docs_entry = SectionEntry::decode(&bytes[192..256]).expect("DOCS entry should decode");
    bytes[docs_entry.payload_offset as usize] = 0x80;
    bytes
}

fn bad_section_layouts_bytes() -> Vec<u8> {
    let mut bytes = try_to_lyba_value_image(&[LymaValue::Null(LymaNull)])
        .expect("single-null image should encode");
    let mut entry = SectionEntry::decode(&bytes[64..128]).expect("STRS entry should decode");
    entry.payload_offset = bytes.len() as u64;
    bytes[64..128].copy_from_slice(&entry.encode().expect("STRS entry should reencode"));
    bytes
}

#[test]
fn checked_in_valid_fixtures_match_deterministic_level1_output() {
    for (name, values, expected_bytes) in valid_cases() {
        let encoded = try_to_lyba_value_image(&values).expect("valid fixture should encode");
        assert_eq!(encoded.as_slice(), expected_bytes, "fixture {name} drifted");

        let decoded = try_from_lyba_value_image(expected_bytes).expect("fixture should decode");
        assert_eq!(decoded, values, "fixture {name} decoded unexpectedly");
    }
}

#[test]
fn checked_in_duplicate_key_fixture_is_rejected_during_encoding() {
    let error = try_to_lyba_value_image(&duplicate_key_values())
        .expect_err("duplicate-key fixture should fail encoding");

    assert!(matches!(error, LybaError::DuplicateKeyInCanonicalMap(_)));
    assert_eq!(error.code().as_str(), "LB0016");
}

#[test]
fn checked_in_malformed_binary_fixtures_report_expected_codes() {
    let cases = [
        (
            "invalid-refs",
            include_bytes!("../../../tests/fixtures/lyba/level1/invalid-refs.lyba").as_slice(),
            "LB0014",
        ),
        (
            "invalid-varints",
            include_bytes!("../../../tests/fixtures/lyba/level1/invalid-varints.lyba").as_slice(),
            "LB0012",
        ),
        (
            "bad-section-layouts",
            include_bytes!("../../../tests/fixtures/lyba/level1/bad-section-layouts.lyba")
                .as_slice(),
            "LB0007",
        ),
    ];

    for (name, bytes, expected_code) in cases {
        let error = try_from_lyba_value_image(bytes).expect_err("malformed fixture should fail");
        assert_eq!(
            error.code().as_str(),
            expected_code,
            "fixture {name} failed with the wrong code"
        );
    }
}

#[test]
fn fixture_manifests_are_checked_in() {
    let manifests = [
        include_str!("../../../tests/fixtures/lyba/level1/minimal-values.json"),
        include_str!("../../../tests/fixtures/lyba/level1/nested-values.json"),
        include_str!("../../../tests/fixtures/lyba/level1/multiple-documents.json"),
        include_str!("../../../tests/fixtures/lyba/level1/tags.json"),
        include_str!("../../../tests/fixtures/lyba/level1/duplicate-key-rejection.json"),
        include_str!("../../../tests/fixtures/lyba/level1/invalid-refs.json"),
        include_str!("../../../tests/fixtures/lyba/level1/invalid-varints.json"),
        include_str!("../../../tests/fixtures/lyba/level1/bad-section-layouts.json"),
    ];

    for manifest in manifests {
        assert!(manifest.contains("\"name\""));
        assert!(manifest.contains("\"kind\""));
    }
}

#[test]
#[ignore = "fixture regeneration is opt-in and not required for normal test runs"]
fn regenerate_level1_fixtures() {
    let dir = fixture_dir();
    fs::write(
        dir.join("minimal-values.lyba"),
        try_to_lyba_value_image(&minimal_values()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("nested-values.lyba"),
        try_to_lyba_value_image(&nested_values()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("multiple-documents.lyba"),
        try_to_lyba_value_image(&multiple_documents()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("tags.lyba"),
        try_to_lyba_value_image(&tagged_values()).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("invalid-refs.lyba"), invalid_refs_bytes()).unwrap();
    fs::write(dir.join("invalid-varints.lyba"), invalid_varints_bytes()).unwrap();
    fs::write(
        dir.join("bad-section-layouts.lyba"),
        bad_section_layouts_bytes(),
    )
    .unwrap();

    assert_eq!(
        fs::read(dir.join("duplicate-key-rejection.json")).is_ok(),
        true,
        "duplicate-key rejection stays source-only because canonical encoding must fail",
    );
}

#[test]
fn regeneration_helpers_match_checked_in_binary_fixtures() {
    assert_eq!(
        invalid_refs_bytes(),
        include_bytes!("../../../tests/fixtures/lyba/level1/invalid-refs.lyba"),
    );
    assert_eq!(
        invalid_varints_bytes(),
        include_bytes!("../../../tests/fixtures/lyba/level1/invalid-varints.lyba"),
    );
    assert_eq!(
        bad_section_layouts_bytes(),
        include_bytes!("../../../tests/fixtures/lyba/level1/bad-section-layouts.lyba"),
    );
}
