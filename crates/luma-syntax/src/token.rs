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

impl TokenKind {
    /// Returns whether this token is a preserved comment token.
    #[must_use]
    pub const fn is_comment(self) -> bool {
        matches!(self, Self::Comment)
    }

    /// Returns whether this token is structural trivia rather than content.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::Comment | Self::LineBreak)
    }
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
    /// Horizontal whitespace immediately preceding the token on the same line.
    pub leading_trivia: Span,
    /// Horizontal whitespace immediately following the token on the same line.
    pub trailing_trivia: Span,
}

impl Token {
    /// Creates a token with empty trivia spans.
    #[must_use]
    pub const fn new(kind: TokenKind, lexeme: String, span: Span) -> Self {
        Self {
            kind,
            lexeme,
            span,
            leading_trivia: Span::new(span.file_id, span.start, span.start),
            trailing_trivia: Span::new(span.file_id, span.end, span.end),
        }
    }

    /// Returns whether the token has leading horizontal whitespace.
    #[must_use]
    pub const fn has_leading_trivia(&self) -> bool {
        !self.leading_trivia.is_empty()
    }

    /// Returns whether the token has trailing horizontal whitespace.
    #[must_use]
    pub const fn has_trailing_trivia(&self) -> bool {
        !self.trailing_trivia.is_empty()
    }
}
