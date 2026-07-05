//! Stable syntax, value, span, and diagnostic model types for Luma.

#![forbid(unsafe_code)]

pub mod ast;
pub mod diagnostic;
pub mod serialize;
pub mod source;
pub mod token;
pub mod value;

pub use ast::{
    BlockChomping, BlockKind, Comment, CommentKind, ConditionalBlock, ConditionalBranch, Directive,
    Document, DocumentItem, ElseBranch, ImportDirective, IncludeDirective, LetBinding,
    LoopBindings, LoopBlock, LuaExpression, LuaPreludeDirective, LumaFile, LumaNode, LumaProfile,
    LumaTag, LumaTagName, MappingBlock, MappingEntry, MappingItem, MappingKey, MetaDirective,
    NumberNode, ProfileDirective, SchemaDirective, SequenceBlock, SequenceItem, SpreadEntry,
    StringNode, StringStyle, TaggedNode, UseDirective, VersionDirective,
};
pub use diagnostic::{Diagnostic, DiagnosticCode, RelatedDiagnosticSpan, Severity};
pub use serialize::{SerializeOptions, serialize_value, serialize_value_with_options};
pub use source::{DuplicateKey, FileId, LumaSource, Offset, SourcePosition, Span, Spanned};
pub use token::{Token, TokenKind};
pub use value::{
    LumaHostValue, LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber, LumaSequence,
    LumaTaggedValue, LumaValue,
};
