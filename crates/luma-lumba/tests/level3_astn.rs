//! Integration tests for Level 3 syntax node support.

use luma_lumba::{
    LumbaError, LumbaFile, ReadOptions, Reader, SourceFileRecord, SourceFileTable,
    SourceSpanRecord, SourceSpanTable, SyntaxField, SyntaxFieldValue, SyntaxNodeRecord,
    SyntaxNodeTable, WriteOptions, Writer,
};
use luma_parser::parse_str;
use luma_syntax::{FileId, SyntaxKind, SyntaxNodeId};

fn line_column(source: &str, offset: usize) -> (u64, u64) {
    let mut line = 1_u64;
    let mut column = 1_u64;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn source_spans_for(parsed: &luma_parser::Parsed) -> SourceSpanTable {
    let index = parsed.syntax_index();
    let source = parsed.source.as_str();
    let mut table = SourceSpanTable::new();
    let mut next = 0_u32;
    loop {
        let Some(node) = index.node(SyntaxNodeId(next)) else {
            break;
        };
        let (start_line, start_column) = line_column(source, node.span.start);
        let (end_line, end_column) = line_column(source, node.span.end);
        table = table.with_record(
            SourceSpanRecord::new(0, node.span.start as u64, node.span.len() as u64)
                .with_start_position(start_line, start_column)
                .with_end_position(end_line, end_column),
        );
        next += 1;
    }
    table
}

#[test]
fn astn_round_trips_parsed_documents_and_preserves_preorder() {
    let source = concat!(
        "@luma 0.1\n",
        "@profile safe\n",
        "@schema \"schema.luma\"\n",
        "@include \"shared.luma\"\n",
        "@use host.module as host\n",
        "let defaults:\n",
        "  replicas: 3\n",
        "service:\n",
        "  env: !env \"prod\"\n",
        "  replicas: =defaults.replicas\n",
        "  items:\n",
        "    - one\n",
        "    - two\n",
    );
    let parsed = parse_str(FileId(0), "fixture.luma", source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let source_spans = source_spans_for(&parsed);
    let mut syntax_nodes = SyntaxNodeTable::from_luma_file(&parsed.file, Some(&source_spans));
    syntax_nodes.records.push(
        SyntaxNodeRecord::new("error_node").with_field(SyntaxField::new(
            "text",
            SyntaxFieldValue::TokenText(String::from("unexpected token")),
        )),
    );

    let file = LumbaFile::new()
        .with_source_file_table(SourceFileTable::new().with_record(SourceFileRecord::new()))
        .with_source_span_table(source_spans)
        .with_syntax_node_table(syntax_nodes.clone());

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("ASTN should encode"),
        )
        .expect("ASTN should decode");

    assert_eq!(decoded.syntax_node_table, Some(syntax_nodes.clone()));

    let kinds: Vec<_> = syntax_nodes
        .records
        .iter()
        .take(8)
        .map(|record| record.kind.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "file",
            "document",
            "version_directive",
            "profile_directive",
            "schema_directive",
            "quoted_scalar",
            "include_directive",
            "quoted_scalar",
        ]
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "mapping")
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "sequence")
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "tagged_value")
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "let_binding")
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "lua_expression")
    );
    assert!(
        syntax_nodes
            .records
            .iter()
            .any(|record| record.kind == "error_node")
    );

    let syntax_index = parsed.syntax_index();
    assert_eq!(
        syntax_index.node(SyntaxNodeId(0)).map(|node| node.kind),
        Some(SyntaxKind::File)
    );
}

#[test]
fn astn_invalid_node_refs_use_lb0015() {
    let file = LumbaFile::new().with_syntax_node_table(
        SyntaxNodeTable::new().with_record(
            SyntaxNodeRecord::new("document")
                .with_field(SyntaxField::new("parent", SyntaxFieldValue::Absent))
                .with_field(SyntaxField::new(
                    "children",
                    SyntaxFieldValue::NodeList(vec![9]),
                )),
        ),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("invalid ASTN node ref should fail");

    assert!(matches!(error, LumbaError::InvalidSyntaxNodeReference(_)));
    assert_eq!(error.code().as_str(), "LB0015");
}
