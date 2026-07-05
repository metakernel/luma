use luma_parser::parse_str;
use luma_syntax::{DiagnosticCode, FileId, LumaNode, MappingItem, SequenceItem, StringStyle};

#[test]
fn level0_parses_static_data_scalars_and_blocks() {
    let parsed = parse_str(
        FileId(1),
        "level0.luma",
        "title: \"Example\\nService\"\nlabel: 'plain'\nmissing:\ntruthy: true\nfalsey: false\nhex: 0xff\nfloat: 0x1.8p1\ndescription: |\n  line one\n  line two\nitems:\n  - alpha\n  - nil\n  - 3\n",
    );

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let LumaNode::Mapping(root) = root(&parsed) else {
        panic!()
    };
    assert_eq!(root.items.len(), 9);
    let MappingItem::Entry(entry) = &root.items[0] else {
        panic!()
    };
    let LumaNode::String(title) = &entry.value else {
        panic!()
    };
    assert_eq!(title.value, "Example\nService");
    assert_eq!(title.style, StringStyle::DoubleQuoted);

    let MappingItem::Entry(entry) = &root.items[7] else {
        panic!()
    };
    let LumaNode::String(description) = &entry.value else {
        panic!()
    };
    assert_eq!(description.value, "line one\nline two\n");

    let MappingItem::Entry(entry) = &root.items[8] else {
        panic!()
    };
    let LumaNode::Sequence(items) = &entry.value else {
        panic!()
    };
    assert_eq!(items.items.len(), 3);
    assert!(matches!(
        items.items[1],
        SequenceItem::Value(LumaNode::Null { .. })
    ));
}

#[test]
fn level0_rejects_invalid_inputs() {
    for (source, code) in [
        (
            "root:\n\tchild: value\n",
            DiagnosticCode::TabUsedForIndentation,
        ),
        (
            "root:\n    child: value\n  sibling: value\n",
            DiagnosticCode::InvalidIndentation,
        ),
        ("message: \"oops\n", DiagnosticCode::UnterminatedString),
        (": value\n", DiagnosticCode::InvalidMappingKey),
        ("null: value\n", DiagnosticCode::InvalidNullKey),
        ("name: one\nname: two\n", DiagnosticCode::DuplicateKey),
        ("num: NaN\n", DiagnosticCode::ReservedSyntax),
        ("num: infinity\n", DiagnosticCode::ReservedSyntax),
    ] {
        let parsed = parse_str(FileId(1), "invalid.luma", source);
        assert!(
            parsed.diagnostics.iter().any(|d| d.code == code),
            "missing {code:?} for {source:?}: {:#?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn level0_preserves_mapping_and_sequence_order() {
    let parsed = parse_str(
        FileId(1),
        "order.luma",
        "root:\n  c: 3\n  a: 1\n  b: 2\n  list:\n    - first\n    - second\n    - third\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let LumaNode::Mapping(root) = root(&parsed) else {
        panic!()
    };
    let MappingItem::Entry(entry) = &root.items[0] else {
        panic!()
    };
    let LumaNode::Mapping(inner) = &entry.value else {
        panic!()
    };
    assert_eq!(inner.items.len(), 4);
    assert_eq!(inner.items[0].entry_key(), "c");
    assert_eq!(inner.items[1].entry_key(), "a");
    assert_eq!(inner.items[2].entry_key(), "b");
    let MappingItem::Entry(entry) = &inner.items[3] else {
        panic!()
    };
    let LumaNode::Sequence(list) = &entry.value else {
        panic!()
    };
    assert_eq!(list.items.len(), 3);
}

fn root(parsed: &luma_parser::Parsed) -> LumaNode {
    let document = &parsed.file.documents[0];
    let luma_syntax::DocumentItem::Root(root) = &document.items[0] else {
        panic!()
    };
    root.clone()
}

trait MappingItemExt {
    fn entry_key(&self) -> &str;
}

impl MappingItemExt for MappingItem {
    fn entry_key(&self) -> &str {
        match self {
            Self::Entry(entry) => match &entry.key {
                luma_syntax::MappingKey::Plain { value, .. } => value,
                luma_syntax::MappingKey::Quoted(node) => &node.value,
                luma_syntax::MappingKey::Expression { .. } => panic!(),
            },
            _ => panic!(),
        }
    }
}
