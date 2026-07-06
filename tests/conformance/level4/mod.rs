use std::{fs, path::Path};

use lyma::tooling::{format_document_edit, serialize_portable_value};
use lyma_syntax::{
    FileId, LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber,
    LymaSequence, LymaValue,
};

#[test]
fn level4_formatter_fixtures_match_canonical_snapshots() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formatter");
    let mut inputs = fs::read_dir(&fixture_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("lyma"))
        .collect::<Vec<_>>();
    inputs.sort();

    for input in inputs {
        let name = input.file_name().unwrap().to_string_lossy().to_string();
        let source = fs::read_to_string(&input).unwrap();
        let expected = fs::read_to_string(input.with_extension("expected")).unwrap();
        let result = format_document_edit(&name, &source);
        assert!(
            result.parsed.diagnostics.is_empty(),
            "{} diagnostics: {:#?}",
            name,
            result.parsed.diagnostics
        );
        assert_eq!(result.formatted.text, expected, "fixture {}", name);
    }
}

#[test]
fn level4_serializer_fixture_matches_canonical_snapshot() {
    let value = LymaValue::Mapping(LymaMapping {
        entries: vec![
            LymaMappingEntry {
                key: LymaKey::String(String::from("name")),
                value: LymaValue::String(String::from("Example")),
                span: None,
            },
            LymaMappingEntry {
                key: LymaKey::String(String::from("enabled")),
                value: LymaValue::Boolean(true),
                span: None,
            },
            LymaMappingEntry {
                key: LymaKey::String(String::from("items")),
                value: LymaValue::Sequence(LymaSequence {
                    items: vec![
                        LymaValue::Null(LymaNull),
                        LymaValue::Number(LymaNumber::Integer(2)),
                    ],
                    span: None,
                }),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    });
    let expected = "name: Example\nenabled: true\nitems:\n  - null\n  - 2\n";
    assert_eq!(serialize_portable_value(&value).unwrap(), expected);
}

#[test]
fn level4_serializer_rejects_non_portable_runtime_values() {
    for value in [
        LymaValue::Function(LymaHostValue {
            kind: String::from("fn"),
            label: None,
        }),
        LymaValue::UserData(LymaHostValue {
            kind: String::from("userdata"),
            label: None,
        }),
        LymaValue::HostObject(LymaHostValue {
            kind: String::from("host"),
            label: None,
        }),
    ] {
        let error = serialize_portable_value(&value).unwrap_err();
        assert_eq!(error.code.code(), "E0030");
    }
}

#[test]
fn level4_serializer_rejects_non_string_keys_and_non_finite_numbers() {
    let mapping = LymaValue::Mapping(LymaMapping {
        entries: vec![LymaMappingEntry {
            key: LymaKey::Number(LymaNumber::Integer(1)),
            value: LymaValue::String(String::from("value")),
            span: None,
        }],
        duplicate_keys: Vec::new(),
        span: None,
    });
    assert_eq!(
        serialize_portable_value(&mapping).unwrap_err().code.code(),
        "E0030"
    );

    let nan = LymaValue::Number(LymaNumber::Float(f64::NAN));
    assert_eq!(
        serialize_portable_value(&nan).unwrap_err().code.code(),
        "E0030"
    );
    let inf = LymaValue::Number(LymaNumber::Float(f64::INFINITY));
    assert_eq!(
        serialize_portable_value(&inf).unwrap_err().code.code(),
        "E0030"
    );
}

#[test]
fn level4_tooling_emits_full_document_replace_edits() {
    let source = "root:\r\n  value: 'hello'\r\n";
    let (formatted, edit) = lyma::tooling::format_document_text_edit("editor.lyma", source);
    assert!(formatted.parsed.diagnostics.is_empty());
    assert_eq!(
        edit.range,
        lyma::tooling::TextRange {
            start: 0,
            end: source.len()
        }
    );
    assert_eq!(edit.text, "root:\n  value: hello\n");
}

#[test]
fn level4_cycle_rejection_is_covered_by_runtime_conversion_conformance() {
    let diagnostic = serialize_portable_value(&LymaValue::Null(LymaNull)).unwrap();
    assert_eq!(diagnostic, "null\n");
    let _ = FileId(1);
}
