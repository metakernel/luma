//! Decode and lex support for Lyma source text.

#![forbid(unsafe_code)]

pub mod block;
pub mod decode;
pub mod directive;
pub mod document;
pub mod error;
pub mod format;
pub mod incremental;
pub mod indent;
pub mod key;
pub mod lexer;
pub mod lua_capture;
pub mod parser;
pub mod scalar;
pub mod tag;

pub use decode::{DecodeError, SourceText, decode_bytes, decode_str};
pub use error::{diagnostic, diagnostic_with_message};
pub use format::{
    FormatOptions, FormatRangeError, FormatRangeFallback, FormatRangeOptions, FormattedDocument,
    ParsedFormatting, format_file, format_parsed, format_parsed_range_edits, format_range_edits,
    format_str, minimal_text_edits,
};
pub use incremental::{
    IncrementalParseError, IncrementalParseInput, IncrementalParseResult, IncrementalParseStrategy,
    ParseSession, ParsedDocument, TextChange,
};
pub use indent::{IndentationFrame, IndentationState, LineIndent};
pub use lexer::{Lexed, lex_source, lex_str};
pub use lyma_syntax::{
    BlockChomping, BlockKind, Diagnostic, DiagnosticCode, Document, DocumentItem, FileId, LymaFile,
    LymaNode, LymaSource, MappingBlock, MappingEntry, MappingItem, MappingKey, NumberNode,
    SequenceBlock, SequenceItem, Severity, Span, StringNode, StringStyle, SyntaxIndex, SyntaxKind,
    SyntaxNodeId, SyntaxNodeInfo, TextEdit, TextRange, Token, TokenKind, apply_text_edits,
};
pub use parser::{Parsed, parse_source, parse_str};

#[cfg(test)]
mod syntax_index_tests {
    use lyma_syntax::{FileId, SyntaxKind, SyntaxNodeId};

    use crate::parse_str;

    #[test]
    fn syntax_index_uses_deterministic_preorder_ids_and_navigation() {
        let source = concat!(
            "-- note\n",
            "@schema \"schema.lyma\"\n",
            "items:\n",
            "  - alpha\n",
            "  - beta\n",
            "value: 42\n",
        );

        let parsed = parse_str(FileId(1), "index.lyma", source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let index = parsed.syntax_index();

        assert_eq!(index.root_ids, vec![SyntaxNodeId(0)]);

        let kinds: Vec<_> = (0..14)
            .map(SyntaxNodeId)
            .map(|id| index.node(id).map(|node| node.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                Some(SyntaxKind::File),
                Some(SyntaxKind::Document),
                Some(SyntaxKind::Comment),
                Some(SyntaxKind::SchemaDirective),
                Some(SyntaxKind::String),
                Some(SyntaxKind::Mapping),
                Some(SyntaxKind::MappingEntry),
                Some(SyntaxKind::PlainMappingKey),
                Some(SyntaxKind::Sequence),
                Some(SyntaxKind::String),
                Some(SyntaxKind::String),
                Some(SyntaxKind::MappingEntry),
                Some(SyntaxKind::PlainMappingKey),
                Some(SyntaxKind::Number),
            ]
        );

        assert_eq!(index.children(SyntaxNodeId(0)), &[SyntaxNodeId(1)]);
        assert_eq!(
            index.children(SyntaxNodeId(1)),
            &[SyntaxNodeId(2), SyntaxNodeId(3), SyntaxNodeId(5)]
        );
        assert_eq!(
            index.children(SyntaxNodeId(5)),
            &[SyntaxNodeId(6), SyntaxNodeId(11)]
        );
        assert_eq!(
            index.children(SyntaxNodeId(6)),
            &[SyntaxNodeId(7), SyntaxNodeId(8)]
        );
        assert_eq!(
            index.children(SyntaxNodeId(8)),
            &[SyntaxNodeId(9), SyntaxNodeId(10)]
        );
        assert_eq!(
            index.children(SyntaxNodeId(11)),
            &[SyntaxNodeId(12), SyntaxNodeId(13)]
        );

        assert_eq!(index.parent(SyntaxNodeId(13)), Some(SyntaxNodeId(11)));
        assert_eq!(
            index.ancestors(SyntaxNodeId(13)).collect::<Vec<_>>(),
            vec![
                SyntaxNodeId(11),
                SyntaxNodeId(5),
                SyntaxNodeId(1),
                SyntaxNodeId(0),
            ]
        );
    }

    #[test]
    fn syntax_index_finds_smallest_nodes_for_key_value_and_comment_offsets() {
        let source = concat!(
            "-- note\n",
            "@schema \"schema.lyma\"\n",
            "items:\n",
            "  - alpha\n",
            "  - beta\n",
            "value: 42\n",
        );

        let parsed = parse_str(FileId(1), "index.lyma", source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let index = parsed.syntax_index();

        let key_offset = source.find("value").unwrap();
        let value_offset = source.find("42").unwrap();
        let comment_offset = source.find("-- note").unwrap() + 3;

        let key_id = index.smallest_node_at_offset(key_offset).unwrap();
        let value_id = index.smallest_node_at_offset(value_offset).unwrap();
        let comment_id = index.smallest_node_at_offset(comment_offset).unwrap();

        assert_eq!(
            index.node(key_id).unwrap().kind,
            SyntaxKind::PlainMappingKey
        );
        assert_eq!(index.node(value_id).unwrap().kind, SyntaxKind::Number);
        assert_eq!(index.node(comment_id).unwrap().kind, SyntaxKind::Comment);

        let covering_kinds: Vec<_> = index
            .covering_span(index.node(comment_id).unwrap().span)
            .into_iter()
            .map(|id| index.node(id).unwrap().kind)
            .collect();
        assert_eq!(
            covering_kinds,
            vec![SyntaxKind::File, SyntaxKind::Document, SyntaxKind::Comment,]
        );
    }
}
