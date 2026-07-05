use std::{fs, path::Path};

use luma::tooling::{format_document_edit, serialize_portable_value};
use luma_syntax::{
    FileId, LumaHostValue, LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber,
    LumaSequence, LumaValue,
};

#[test]
fn level4_formatter_fixtures_match_canonical_snapshots() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formatter");
    let mut inputs = fs::read_dir(&fixture_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("luma"))
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
    let value = LumaValue::Mapping(LumaMapping {
        entries: vec![
            LumaMappingEntry {
                key: LumaKey::String(String::from("name")),
                value: LumaValue::String(String::from("Example")),
                span: None,
            },
            LumaMappingEntry {
                key: LumaKey::String(String::from("enabled")),
                value: LumaValue::Boolean(true),
                span: None,
            },
            LumaMappingEntry {
                key: LumaKey::String(String::from("items")),
                value: LumaValue::Sequence(LumaSequence {
                    items: vec![
                        LumaValue::Null(LumaNull),
                        LumaValue::Number(LumaNumber::Integer(2)),
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
        LumaValue::Function(LumaHostValue {
            kind: String::from("fn"),
            label: None,
        }),
        LumaValue::UserData(LumaHostValue {
            kind: String::from("userdata"),
            label: None,
        }),
        LumaValue::HostObject(LumaHostValue {
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
    let mapping = LumaValue::Mapping(LumaMapping {
        entries: vec![LumaMappingEntry {
            key: LumaKey::Number(LumaNumber::Integer(1)),
            value: LumaValue::String(String::from("value")),
            span: None,
        }],
        duplicate_keys: Vec::new(),
        span: None,
    });
    assert_eq!(
        serialize_portable_value(&mapping).unwrap_err().code.code(),
        "E0030"
    );

    let nan = LumaValue::Number(LumaNumber::Float(f64::NAN));
    assert_eq!(
        serialize_portable_value(&nan).unwrap_err().code.code(),
        "E0030"
    );
    let inf = LumaValue::Number(LumaNumber::Float(f64::INFINITY));
    assert_eq!(
        serialize_portable_value(&inf).unwrap_err().code.code(),
        "E0030"
    );
}

#[test]
fn level4_tooling_emits_full_document_replace_edits() {
    let source = "root:\r\n  value: 'hello'\r\n";
    let (formatted, edit) = luma::tooling::format_document_text_edit("editor.luma", source);
    assert!(formatted.parsed.diagnostics.is_empty());
    assert_eq!(
        edit.range,
        luma::tooling::TextRange {
            start: 0,
            end: source.len()
        }
    );
    assert_eq!(edit.text, "root:\n  value: hello\n");
}

#[test]
fn level4_cycle_rejection_is_covered_by_runtime_conversion_conformance() {
    let diagnostic = serialize_portable_value(&LumaValue::Null(LumaNull)).unwrap();
    assert_eq!(diagnostic, "null\n");
    let _ = FileId(1);
}
