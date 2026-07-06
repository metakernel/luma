//! Integration tests for Level 3 trivia support.

use lyma_lyba::container::{
    ContainerHeader, HeaderCrcMode, validate_section_table_with_reserved_flag_policy,
};
use lyma_lyba::policy::ReservedFlagPolicy;
use lyma_lyba::section::SectionId;
use lyma_lyba::{
    LybaError, LybaFile, ReadOptions, Reader, SourceFileRecord, SourceFileTable,
    SourceSpanRecord, SourceSpanTable, SyntaxField, SyntaxFieldValue, SyntaxNodeRecord,
    SyntaxNodeTable, TRIVIA_KIND_BLANK_LINE, TRIVIA_KIND_COMMENT, TRIVIA_KIND_EXTENSION,
    TRIVIA_KIND_INDENTATION, TRIVIA_KIND_MALFORMED, TRIVIA_KIND_NEWLINE, TRIVIA_KIND_PUNCTUATION,
    TRIVIA_KIND_WHITESPACE, TriviaRecord, TriviaTable, WriteOptions, Writer, to_lyba_value_image,
};
use lyma_syntax::LymaValue;

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

fn span_record(source: &str, start: usize, len: usize) -> SourceSpanRecord {
    let (start_line, start_column) = line_column(source, start);
    let (end_line, end_column) = line_column(source, start + len);
    SourceSpanRecord::new(0, start as u64, len as u64)
        .with_start_position(start_line, start_column)
        .with_end_position(end_line, end_column)
}

#[test]
fn trivia_round_trips_editor_cache_style_records_and_astn_refs() {
    let source = "root:  value, -- note\n\n  next\n";
    let value_start = source.find("value").expect("value token should exist");
    let whitespace_start = source.find("  value").expect("whitespace should exist");
    let comma_start = source.find(',').expect("comma should exist");
    let comment_start = source.find("-- note").expect("comment should exist");
    let blank_line_start = source.find("\n\n").expect("blank line should exist");
    let indent_start = source.rfind("  next").expect("indent should exist");
    let final_newline_start = source.len() - 1;

    let spans = SourceSpanTable::new()
        .with_record(span_record(source, value_start, "value".len()))
        .with_record(span_record(source, whitespace_start, 2))
        .with_record(span_record(source, comma_start, 1))
        .with_record(span_record(source, comment_start, "-- note".len()))
        .with_record(span_record(source, blank_line_start, 2))
        .with_record(span_record(source, indent_start, 2))
        .with_record(span_record(source, final_newline_start, 1));

    let trivia = TriviaTable::new()
        .with_record(TriviaRecord::new(TRIVIA_KIND_WHITESPACE, "  ").with_span_ref(Some(1)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_PUNCTUATION, ",").with_span_ref(Some(2)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_COMMENT, "-- note").with_span_ref(Some(3)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_BLANK_LINE, "\n\n").with_span_ref(Some(4)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_INDENTATION, "  ").with_span_ref(Some(5)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_NEWLINE, "\n").with_span_ref(Some(6)))
        .with_record(TriviaRecord::new(TRIVIA_KIND_MALFORMED, "<<bad>>"))
        .with_record(TriviaRecord::new(TRIVIA_KIND_EXTENSION, "@@ext"));

    let syntax = SyntaxNodeTable::new().with_record(
        SyntaxNodeRecord::new("value_token")
            .with_primary_span_ref(Some(0))
            .with_leading_trivia_ref(Some(0))
            .with_trailing_trivia_ref(Some(2))
            .with_field(SyntaxField::new(
                "text",
                SyntaxFieldValue::TokenText(String::from("value")),
            )),
    );

    let file = LybaFile::new()
        .with_source_file_table(SourceFileTable::new().with_record(SourceFileRecord::new()))
        .with_source_span_table(spans)
        .with_syntax_node_table(syntax.clone())
        .with_trivia_table(trivia.clone());

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("TRIV should encode"),
        )
        .expect("TRIV should decode");

    assert_eq!(decoded.trivia_table, Some(trivia));
    assert_eq!(decoded.syntax_node_table, Some(syntax));
}

#[test]
fn trivia_source_order_validation_uses_lb0022() {
    let source = "root: value -- note\n";
    let spans = SourceSpanTable::new()
        .with_record(span_record(source, source.find("-- note").unwrap(), 7))
        .with_record(span_record(source, source.find(" ").unwrap(), 1));

    let file = LybaFile::new()
        .with_source_file_table(SourceFileTable::new().with_record(SourceFileRecord::new()))
        .with_source_span_table(spans)
        .with_trivia_table(
            TriviaTable::new()
                .with_record(
                    TriviaRecord::new(TRIVIA_KIND_COMMENT, "-- note").with_span_ref(Some(0)),
                )
                .with_record(TriviaRecord::new(TRIVIA_KIND_WHITESPACE, " ").with_span_ref(Some(1))),
        );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("out-of-order trivia should fail");

    assert!(matches!(error, LybaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn canonical_value_images_omit_triv_section() {
    let bytes = to_lyba_value_image(&[LymaValue::String(String::from("portable"))]);
    let header = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
        .expect("level1 image header should decode");
    let sections = validate_section_table_with_reserved_flag_policy(
        &header,
        &bytes,
        ReservedFlagPolicy::Reject,
    )
    .expect("section table should validate");

    assert!(
        sections
            .iter()
            .all(|section| section.entry.section_id != SectionId::TRIV)
    );
}
