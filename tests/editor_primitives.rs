use luma::parser::{
    FileId, FormatRangeFallback, FormatRangeOptions, IncrementalParseInput, TextChange, TokenKind,
    format_range_edits, lex_str,
};
use luma::tooling::{
    TextRange, apply_text_edits, format_document_range_text_edits, format_document_text_edit,
    format_document_text_edits,
};
use luma::{Parser, SyntaxKind};

#[test]
fn editor_parser_syntax_index_exposes_exact_spans_and_handles() {
    let source = "service:\n  name:'api'\n  enabled:true\n";
    let parsed = Parser::new().parse_str(FileId(7), "service.luma", source);

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let index = parsed.syntax_index();
    let file_id = index.root_ids[0];
    let document_id = index.children(file_id)[0];
    let root_mapping_id = index.children(document_id)[0];

    assert_eq!(index.node(file_id).unwrap().kind, SyntaxKind::File);
    assert_eq!(index.node(document_id).unwrap().kind, SyntaxKind::Document);
    assert_eq!(
        index.node(root_mapping_id).unwrap().kind,
        SyntaxKind::Mapping
    );
    assert_eq!(index.parent(document_id), Some(file_id));
    assert_eq!(index.parent(root_mapping_id), Some(document_id));

    let service_offset = source.find("service").unwrap();
    let service_key_id = index.smallest_node_at_offset(service_offset).unwrap();
    let service_key = index.node(service_key_id).unwrap();
    assert_eq!(service_key.kind, SyntaxKind::PlainMappingKey);
    assert_eq!(&source[service_key.span.byte_range()], "service");
    assert_eq!(
        index.parent(service_key_id),
        Some(index.children(root_mapping_id)[0])
    );

    let name_offset = source.find("name").unwrap();
    let name_key_id = index.smallest_node_at_offset(name_offset).unwrap();
    let name_key = index.node(name_key_id).unwrap();
    assert_eq!(name_key.kind, SyntaxKind::PlainMappingKey);
    assert_eq!(&source[name_key.span.byte_range()], "name");

    let api_offset = source.find("'api'").unwrap() + 1;
    let api_value_id = index.smallest_node_at_offset(api_offset).unwrap();
    let api_value = index.node(api_value_id).unwrap();
    assert_eq!(api_value.kind, SyntaxKind::String);
    assert_eq!(&source[api_value.span.byte_range()], "'api'");

    let enabled_offset = source.find("true").unwrap();
    let enabled_value_id = index.smallest_node_at_offset(enabled_offset).unwrap();
    let enabled_value = index.node(enabled_value_id).unwrap();
    assert_eq!(enabled_value.kind, SyntaxKind::Boolean);
    assert_eq!(&source[enabled_value.span.byte_range()], "true");

    let covering_kinds = index
        .covering_span(enabled_value.span)
        .into_iter()
        .map(|id| index.node(id).unwrap().kind)
        .collect::<Vec<_>>();
    assert_eq!(
        covering_kinds,
        vec![
            SyntaxKind::File,
            SyntaxKind::Document,
            SyntaxKind::Mapping,
            SyntaxKind::MappingEntry,
            SyntaxKind::Mapping,
            SyntaxKind::MappingEntry,
            SyntaxKind::Boolean,
        ]
    );
}

#[test]
fn editor_parse_session_updates_public_handles_after_text_change() {
    let source = "service:\n  name:'api'\n  enabled:true\n";
    let mut session = Parser::new().session(FileId(9), "service.luma");
    let _initial = session.parse(source);
    let replace_range = TextRange::new(
        source.find("enabled:true").unwrap(),
        source.find("enabled:true").unwrap() + "enabled:true".len(),
    );

    let updated = session
        .apply(IncrementalParseInput::new(vec![TextChange::replace(
            replace_range,
            "enabled: false",
        )]))
        .unwrap();

    let expected_source = apply_text_edits(
        source,
        &[luma::tooling::TextEdit {
            range: replace_range,
            text: String::from("enabled: false"),
        }],
    )
    .unwrap();
    assert_eq!(updated.document().source(), expected_source);
    assert_eq!(session.current().unwrap().source(), expected_source);
    assert!(updated.parsed().diagnostics.is_empty());

    let false_offset = updated.document().source().find("false").unwrap();
    let index = updated.parsed().syntax_index();
    let false_id = index.smallest_node_at_offset(false_offset).unwrap();
    let false_node = index.node(false_id).unwrap();
    assert_eq!(false_node.kind, SyntaxKind::Boolean);
    assert_eq!(
        &updated.document().source()[false_node.span.byte_range()],
        "false"
    );

    let entry_id = index.parent(false_id).unwrap();
    let entry = index.node(entry_id).unwrap();
    assert_eq!(entry.kind, SyntaxKind::MappingEntry);
    assert_eq!(
        &updated.document().source()[entry.span.byte_range()],
        "enabled: false"
    );
}

#[test]
fn editor_lexical_primitives_expose_token_trivia_and_line_indents() {
    let source = "root:\n  child:  next  -- note\n";
    let lexed = lex_str(FileId(11), "service.luma", source);

    assert!(lexed.diagnostics.is_empty(), "{:#?}", lexed.diagnostics);

    let child = lexed
        .tokens
        .iter()
        .find(|token| token.lexeme == "child")
        .unwrap();
    let colon = lexed
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::Colon && token.span.start > child.span.start)
        .unwrap();
    let next = lexed
        .tokens
        .iter()
        .find(|token| token.lexeme == "next")
        .unwrap();
    let comment = lexed
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::Comment)
        .unwrap();
    let line_break = lexed
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::LineBreak)
        .unwrap();

    assert!(!child.has_leading_trivia());
    assert_eq!(&source[colon.trailing_trivia.byte_range()], "  ");
    assert_eq!(&source[next.leading_trivia.byte_range()], "  ");
    assert_eq!(&source[next.trailing_trivia.byte_range()], "  ");
    assert_eq!(&source[comment.leading_trivia.byte_range()], "  ");
    assert_eq!(&source[line_break.span.byte_range()], "\n");
    assert!(comment.kind.is_comment());
    assert!(line_break.kind.is_trivia());

    assert_eq!(lexed.indents.len(), 2);
    assert_eq!(&source[lexed.indents[0].span.byte_range()], "");
    assert_eq!(lexed.indents[0].width, 0);
    assert!(!lexed.indents[0].is_ignorable);
    assert_eq!(&source[lexed.indents[1].span.byte_range()], "  ");
    assert_eq!(lexed.indents[1].width, 2);
    assert!(!lexed.indents[1].is_ignorable);
}

#[test]
fn editor_formatting_helpers_apply_back_to_canonical_output() {
    let source = "service:\r\n  name:'api'\r\n  enabled:true\r\n";

    let (formatted, whole_edit) = format_document_text_edit("service.luma", source);
    assert!(formatted.parsed.diagnostics.is_empty());
    assert_eq!(whole_edit.range, TextRange::new(0, source.len()));
    assert_eq!(
        apply_text_edits(source, &[whole_edit.clone()]),
        Some(formatted.formatted.text.clone())
    );

    let (_, minimal_edits) = format_document_text_edits("service.luma", source);
    assert_eq!(
        apply_text_edits(source, &minimal_edits),
        Some(formatted.formatted.text.clone())
    );

    let normalized_source = formatted.parsed.source.as_str();
    let full_range = TextRange::new(0, normalized_source.len());
    let options = FormatRangeOptions {
        fallback: FormatRangeFallback::Reject,
        ..FormatRangeOptions::default()
    };
    let (_, tooling_range_edits) =
        format_document_range_text_edits("service.luma", normalized_source, full_range, options)
            .unwrap();
    let (_, parser_range_edits) = format_range_edits(
        FileId(1),
        "service.luma",
        normalized_source,
        full_range,
        options,
    )
    .unwrap();

    assert_eq!(tooling_range_edits, parser_range_edits);
    assert_eq!(
        apply_text_edits(normalized_source, &tooling_range_edits),
        Some(formatted.formatted.text)
    );
}
