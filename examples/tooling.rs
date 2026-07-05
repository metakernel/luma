//! Editor-style primitives: lexical trivia lookup, syntax lookup by offset,
//! formatting edits, and incremental parse session updates.
//!
//! Run with:
//! `cargo run --example tooling`

use luma::LumaValue;
use luma::parser::{FileId, IncrementalParseInput, ParseSession, TextChange, TokenKind, lex_str};
use luma::syntax::SyntaxKind;
use luma::tooling::{
    FormatRangeOptions, TextRange, format_document_range_text_edits, format_document_text_edit,
    serialize_portable_value,
};

fn main() {
    let source = "service:\n  name:'api'\n  enabled:true\n";
    let lexical_source = "root:\n  child:  next  -- note\n";

    let lexical = lex_str(FileId(1), "lexical.luma", lexical_source);
    let next_token = lexical
        .tokens
        .iter()
        .find(|token| token.lexeme == "next")
        .unwrap();
    let line_break = lexical
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::LineBreak)
        .unwrap();
    assert_eq!(
        &lexical_source[next_token.leading_trivia.byte_range()],
        "  "
    );
    assert_eq!(
        &lexical_source[next_token.trailing_trivia.byte_range()],
        "  "
    );
    assert_eq!(&lexical_source[line_break.span.byte_range()], "\n");
    assert_eq!(lexical.indents[1].width, 2);
    println!(
        "lexical trivia: {:?} leading={:?}, line indent span={:?}",
        next_token.kind,
        &lexical_source[next_token.leading_trivia.byte_range()],
        lexical.indents[1].span.byte_range(),
    );

    let mut session = ParseSession::new(FileId(1), "service.luma");
    let initial = session.parse(source);
    let parsed = initial.parsed();
    assert!(parsed.diagnostics.is_empty());

    let index = parsed.syntax_index();
    let name_offset = parsed.source.as_str().find("name").unwrap();
    let key_id = index.smallest_node_at_offset(name_offset).unwrap();
    let key = index.node(key_id).unwrap();
    let snippet = &parsed.source.as_str()[key.span.byte_range()];

    assert_eq!(key.kind, SyntaxKind::PlainMappingKey);
    println!(
        "hover lookup at {name_offset}: {:?} => {snippet:?}",
        key.kind
    );

    let (formatting, edit) = format_document_text_edit("service.luma", source);

    assert!(formatting.parsed.diagnostics.is_empty());
    println!("replace bytes {}..{}", edit.range.start, edit.range.end);
    println!("replacement text:\n{}", edit.text);

    let range = TextRange::new(0, source.find("enabled").unwrap_or(source.len()));
    let (_, range_edits) = format_document_range_text_edits(
        "service.luma",
        source,
        range,
        FormatRangeOptions::default(),
    )
    .expect("range formatting succeeds");
    println!("range edits: {}", range_edits.len());

    let mut session = ParseSession::new(FileId(1), "service.luma");
    let first = session.parse(source);
    let enabled_offset = first.parsed().source.as_str().find("enabled:true").unwrap();
    let update = session
        .apply(IncrementalParseInput::new(vec![TextChange::replace(
            TextRange::new(enabled_offset, enabled_offset + "enabled:true".len()),
            "enabled: false",
        )]))
        .expect("incremental shell validates and reparses");

    assert_eq!(
        update.strategy,
        luma::parser::IncrementalParseStrategy::FullReparse
    );
    assert!(!update.reused);
    println!("incremental source:\n{}", update.document.source());

    let value = LumaValue::Boolean(true);
    let serialized = serialize_portable_value(&value).expect("portable value serializes");
    println!("portable value:\n{}", serialized);
}
