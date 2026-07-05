//! Token model shared across syntax consumers.

#![allow(missing_docs)]

use crate::{ast::BlockKind, source::Span};

/// Coarse token categories for the public lexer boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenKind {
    Identifier,
    DirectiveName,
    TagName,
    Number,
    String,
    PlainString,
    Comment,
    DocumentSeparator,
    DocumentTerminator,
    BlockHeader(BlockKind),
    Colon,
    Dash,
    Spread,
    Equals,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    KeywordLet,
    KeywordAs,
    KeywordIn,
    LineBreak,
    EndOfFile,
    Error,
}

/// Token plus source span.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    /// Token category.
    pub kind: TokenKind,
    /// Original token text.
    pub lexeme: String,
    /// Source span.
    pub span: Span,
}
