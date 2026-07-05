//! Decode and lex support for Luma source text.

#![forbid(unsafe_code)]

pub mod block;
pub mod decode;
pub mod directive;
pub mod document;
pub mod error;
pub mod format;
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
    FormatOptions, FormattedDocument, ParsedFormatting, format_file, format_parsed, format_str,
};
pub use indent::{IndentationFrame, IndentationState, LineIndent};
pub use lexer::{Lexed, lex_source, lex_str};
pub use luma_syntax::{
    BlockChomping, BlockKind, Diagnostic, DiagnosticCode, Document, DocumentItem, FileId, LumaFile,
    LumaNode, LumaSource, MappingBlock, MappingEntry, MappingItem, MappingKey, NumberNode,
    SequenceBlock, SequenceItem, Severity, Span, StringNode, StringStyle, Token, TokenKind,
};
pub use parser::{Parsed, parse_source, parse_str};
