//! Level 1 golden fixture and malformed-binary tests.

use luma_lumba::document::DOCUMENT_FLAG_HAS_VALUE_ROOT;
use luma_lumba::primitives::UVar;
use luma_lumba::section::SectionEntry;
use luma_lumba::{LumbaError, try_from_lumba_value_image, try_to_lumba_value_image};
use luma_syntax::{
    FileId, LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber, LumaSequence, LumaTag,
    LumaTagName, LumaTaggedValue, LumaValue, source::Span,
};
use std::{fs, path::PathBuf};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("lumba")
        .join("level1")
}

fn span() -> Span {
    Span::new(FileId(0), 0, 0)
}

fn tagged(name: &str, value: LumaValue) -> LumaValue {
    LumaValue::Tagged(LumaTaggedValue {
        tag: LumaTag {
            name: LumaTagName {
                value: name.to_owned(),
                span: span(),
            },
            span: span(),
        },
        value: Box::new(value),
        span: None,
    })
}

fn minimal_values() -> Vec<LumaValue> {
    vec![
        LumaValue::Null(LumaNull),
        LumaValue::Boolean(false),
        LumaValue::Boolean(true),
        LumaValue::Number(LumaNumber::Integer(-7)),
        LumaValue::Number(LumaNumber::Float(1.5)),
        LumaValue::String(String::from("hi")),
    ]
}

fn nested_values() -> Vec<LumaValue> {
    vec![LumaValue::Mapping(LumaMapping {
        entries: vec![
            LumaMappingEntry {
                key: LumaKey::String(String::from("items")),
                value: LumaValue::Sequence(LumaSequence {
                    items: vec![
                        LumaValue::Number(LumaNumber::Integer(1)),
                        LumaValue::Mapping(LumaMapping {
                            entries: vec![LumaMappingEntry {
                                key: LumaKey::String(String::from("deep")),
                                value: LumaValue::Boolean(true),
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
            LumaMappingEntry {
                key: LumaKey::String(String::from("note")),
                value: LumaValue::String(String::from("nested")),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    })]
}

fn multiple_documents() -> Vec<LumaValue> {
    vec![
        LumaValue::String(String::from("first")),
        LumaValue::Sequence(LumaSequence {
            items: vec![LumaValue::Boolean(true), LumaValue::Boolean(false)],
            span: None,
        }),
        LumaValue::Mapping(LumaMapping {
            entries: vec![LumaMappingEntry {
                key: LumaKey::String(String::from("third")),
                value: LumaValue::Null(LumaNull),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        }),
    ]
}

fn tagged_values() -> Vec<LumaValue> {
    vec![
        tagged("Thing", LumaValue::String(String::from("alpha"))),
        tagged(
            "Wrap",
            LumaValue::Mapping(LumaMapping {
                entries: vec![LumaMappingEntry {
                    key: LumaKey::String(String::from("inner")),
                    value: tagged("Leaf", LumaValue::Number(LumaNumber::Integer(2))),
                    span: None,
                }],
                duplicate_keys: Vec::new(),
                span: None,
            }),
        ),
    ]
}

fn duplicate_key_values() -> Vec<LumaValue> {
    vec![LumaValue::Mapping(LumaMapping {
        entries: vec![
            LumaMappingEntry {
                key: LumaKey::String(String::from("dup")),
                value: LumaValue::Number(LumaNumber::Integer(1)),
                span: None,
            },
            LumaMappingEntry {
                key: LumaKey::String(String::from("dup")),
                value: LumaValue::Number(LumaNumber::Integer(2)),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    })]
}

fn valid_cases() -> [(&'static str, Vec<LumaValue>, &'static [u8]); 4] {
    [
        (
            "minimal-values",
            minimal_values(),
            include_bytes!("../../../tests/fixtures/lumba/level1/minimal-values.lumba"),
        ),
        (
            "nested-values",
            nested_values(),
            include_bytes!("../../../tests/fixtures/lumba/level1/nested-values.lumba"),
        ),
        (
            "multiple-documents",
            multiple_documents(),
            include_bytes!("../../../tests/fixtures/lumba/level1/multiple-documents.lumba"),
        ),
        (
            "tags",
            tagged_values(),
            include_bytes!("../../../tests/fixtures/lumba/level1/tags.lumba"),
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
    let mut bytes = try_to_lumba_value_image(&[LumaValue::Null(LumaNull)])
        .expect("single-null image should encode");
    let docs_entry = SectionEntry::decode(&bytes[192..256]).expect("DOCS entry should decode");
    let payload = docs_payload(1, DOCUMENT_FLAG_HAS_VALUE_ROOT, Some(1));
    let offset = docs_entry.payload_offset as usize;
    bytes[offset..offset + payload.len()].copy_from_slice(&payload);
    bytes
}

fn invalid_varints_bytes() -> Vec<u8> {
    let mut bytes = try_to_lumba_value_image(&[LumaValue::Null(LumaNull)])
        .expect("single-null image should encode");
    let docs_entry = SectionEntry::decode(&bytes[192..256]).expect("DOCS entry should decode");
    bytes[docs_entry.payload_offset as usize] = 0x80;
    bytes
}

fn bad_section_layouts_bytes() -> Vec<u8> {
    let mut bytes = try_to_lumba_value_image(&[LumaValue::Null(LumaNull)])
        .expect("single-null image should encode");
    let mut entry = SectionEntry::decode(&bytes[64..128]).expect("STRS entry should decode");
    entry.payload_offset = bytes.len() as u64;
    bytes[64..128].copy_from_slice(&entry.encode().expect("STRS entry should reencode"));
    bytes
}

#[test]
fn checked_in_valid_fixtures_match_deterministic_level1_output() {
    for (name, values, expected_bytes) in valid_cases() {
        let encoded = try_to_lumba_value_image(&values).expect("valid fixture should encode");
        assert_eq!(encoded.as_slice(), expected_bytes, "fixture {name} drifted");

        let decoded = try_from_lumba_value_image(expected_bytes).expect("fixture should decode");
        assert_eq!(decoded, values, "fixture {name} decoded unexpectedly");
    }
}

#[test]
fn checked_in_duplicate_key_fixture_is_rejected_during_encoding() {
    let error = try_to_lumba_value_image(&duplicate_key_values())
        .expect_err("duplicate-key fixture should fail encoding");

    assert!(matches!(error, LumbaError::DuplicateKeyInCanonicalMap(_)));
    assert_eq!(error.code().as_str(), "LB0016");
}

#[test]
fn checked_in_malformed_binary_fixtures_report_expected_codes() {
    let cases = [
        (
            "invalid-refs",
            include_bytes!("../../../tests/fixtures/lumba/level1/invalid-refs.lumba").as_slice(),
            "LB0014",
        ),
        (
            "invalid-varints",
            include_bytes!("../../../tests/fixtures/lumba/level1/invalid-varints.lumba").as_slice(),
            "LB0012",
        ),
        (
            "bad-section-layouts",
            include_bytes!("../../../tests/fixtures/lumba/level1/bad-section-layouts.lumba")
                .as_slice(),
            "LB0007",
        ),
    ];

    for (name, bytes, expected_code) in cases {
        let error = try_from_lumba_value_image(bytes).expect_err("malformed fixture should fail");
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
        include_str!("../../../tests/fixtures/lumba/level1/minimal-values.json"),
        include_str!("../../../tests/fixtures/lumba/level1/nested-values.json"),
        include_str!("../../../tests/fixtures/lumba/level1/multiple-documents.json"),
        include_str!("../../../tests/fixtures/lumba/level1/tags.json"),
        include_str!("../../../tests/fixtures/lumba/level1/duplicate-key-rejection.json"),
        include_str!("../../../tests/fixtures/lumba/level1/invalid-refs.json"),
        include_str!("../../../tests/fixtures/lumba/level1/invalid-varints.json"),
        include_str!("../../../tests/fixtures/lumba/level1/bad-section-layouts.json"),
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
        dir.join("minimal-values.lumba"),
        try_to_lumba_value_image(&minimal_values()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("nested-values.lumba"),
        try_to_lumba_value_image(&nested_values()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("multiple-documents.lumba"),
        try_to_lumba_value_image(&multiple_documents()).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("tags.lumba"),
        try_to_lumba_value_image(&tagged_values()).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("invalid-refs.lumba"), invalid_refs_bytes()).unwrap();
    fs::write(dir.join("invalid-varints.lumba"), invalid_varints_bytes()).unwrap();
    fs::write(
        dir.join("bad-section-layouts.lumba"),
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
        include_bytes!("../../../tests/fixtures/lumba/level1/invalid-refs.lumba"),
    );
    assert_eq!(
        invalid_varints_bytes(),
        include_bytes!("../../../tests/fixtures/lumba/level1/invalid-varints.lumba"),
    );
    assert_eq!(
        bad_section_layouts_bytes(),
        include_bytes!("../../../tests/fixtures/lumba/level1/bad-section-layouts.lumba"),
    );
}
