//! Static syntax tree model for parsed Luma input.

#![allow(missing_docs)]

use crate::source::{DuplicateKey, Span};

/// Root parsed file, which may contain a document stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LumaFile {
    /// Documents in source order.
    pub documents: Vec<Document>,
    /// Span of the entire parsed file.
    pub span: Span,
}

/// One Luma document within a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Top-level items in source order.
    pub items: Vec<DocumentItem>,
    /// Span of the document body.
    pub span: Span,
    /// Optional document separator span (`---`).
    pub separator_span: Option<Span>,
    /// Optional explicit document terminator span (`...`).
    pub terminator_span: Option<Span>,
}

/// Top-level document item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentItem {
    /// Top-level directive.
    Directive(Directive),
    /// Top-level lexical binding.
    Let(LetBinding),
    /// Root value.
    Root(LumaNode),
    /// Preserved comment.
    Comment(Comment),
}

/// Static node kind from the parsed model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LumaNode {
    Null { span: Span },
    Boolean { value: bool, span: Span },
    Number(NumberNode),
    String(StringNode),
    Sequence(SequenceBlock),
    Mapping(MappingBlock),
    Tagged(TaggedNode),
    LuaExpression(LuaExpression),
    LuaExpressionBlock(LuaExpression),
    LuaChunk(LuaExpression),
    LuaTableConstructor(LuaExpression),
}

/// Numeric literal node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberNode {
    /// Original source text.
    pub lexeme: String,
    /// Source span.
    pub span: Span,
}

/// String literal style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StringStyle {
    Plain,
    DoubleQuoted,
    SingleQuoted,
    Block,
}

/// Block header kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlockKind {
    Literal,
    Folded,
    LuaExpression,
    LuaChunk,
}

/// Block chomping indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlockChomping {
    Clip,
    Strip,
    Keep,
}

/// String node, including block strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringNode {
    /// Decoded string value.
    pub value: String,
    /// Original source text for the scalar body.
    pub source: String,
    /// String representation style.
    pub style: StringStyle,
    /// Optional block kind for block strings.
    pub block_kind: Option<BlockKind>,
    /// Optional chomping indicator for block strings and Lua blocks.
    pub chomping: Option<BlockChomping>,
    /// Source span.
    pub span: Span,
}

/// Lua source node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaExpression {
    /// Raw Lua source.
    pub source: String,
    /// Source span covering the full construct.
    pub span: Span,
    /// Optional header kind for block forms.
    pub block_kind: Option<BlockKind>,
    /// Optional chomping indicator for block forms.
    pub chomping: Option<BlockChomping>,
}

/// Comment kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommentKind {
    Line,
    Block,
}

/// Preserved comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Comment representation kind.
    pub kind: CommentKind,
    /// Comment text without delimiters.
    pub text: String,
    /// Source span.
    pub span: Span,
}

/// Tag name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LumaTagName {
    /// Tag text without the leading `!`.
    pub value: String,
}

/// Tag annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LumaTag {
    /// Parsed tag name.
    pub name: LumaTagName,
    /// Source span.
    pub span: Span,
}

/// Tagged value node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedNode {
    /// Tag metadata.
    pub tag: LumaTag,
    /// Tagged payload, when present.
    pub value: Option<Box<LumaNode>>,
    /// Source span.
    pub span: Span,
}

/// Mapping block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingBlock {
    /// Mapping items in source order.
    pub items: Vec<MappingItem>,
    /// Duplicate explicit-key tracking.
    pub duplicate_keys: Vec<DuplicateKey<String>>,
    /// Source span.
    pub span: Span,
}

/// Mapping item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingItem {
    Entry(MappingEntry),
    Spread(SpreadEntry),
    Directive(Directive),
    Conditional(ConditionalBlock<MappingBlock>),
    Loop(LoopBlock<MappingBlock>),
    Let(LetBinding),
    Comment(Comment),
}

/// Sequence block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceBlock {
    /// Sequence items in source order.
    pub items: Vec<SequenceItem>,
    /// Source span.
    pub span: Span,
}

/// Sequence item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceItem {
    Value(LumaNode),
    Spread(SpreadEntry),
    Directive(Directive),
    Conditional(ConditionalBlock<SequenceBlock>),
    Loop(LoopBlock<SequenceBlock>),
    Comment(Comment),
}

/// Mapping entry key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingKey {
    Plain {
        value: String,
        span: Span,
    },
    Quoted(StringNode),
    Expression {
        expression: LuaExpression,
        span: Span,
    },
}

/// Explicit mapping entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingEntry {
    /// Entry key.
    pub key: MappingKey,
    /// Entry value.
    pub value: LumaNode,
    /// Source span.
    pub span: Span,
}

/// Spread entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadEntry {
    /// Lua expression supplying the spread value.
    pub expression: LuaExpression,
    /// Source span.
    pub span: Span,
}

/// Lexical binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetBinding {
    /// Binding name.
    pub name: String,
    /// Bound value.
    pub value: LumaNode,
    /// Source span.
    pub span: Span,
}

/// Conditional block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalBlock<T> {
    /// Initial `@if` branch.
    pub if_branch: ConditionalBranch<T>,
    /// Zero or more `@elseif` branches.
    pub else_if_branches: Vec<ConditionalBranch<T>>,
    /// Optional trailing `@else` branch.
    pub else_branch: Option<ElseBranch<T>>,
    /// Source span.
    pub span: Span,
}

/// Conditional branch with an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalBranch<T> {
    /// Condition expression.
    pub condition: LuaExpression,
    /// Branch body.
    pub body: T,
    /// Source span.
    pub span: Span,
}

/// `@else` branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElseBranch<T> {
    /// Branch body.
    pub body: T,
    /// Source span.
    pub span: Span,
}

/// Loop binding shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopBindings {
    One { value: String },
    Two { key: String, value: String },
}

/// Loop block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopBlock<T> {
    /// Loop variable bindings.
    pub bindings: LoopBindings,
    /// Iterable expression.
    pub iterable: LuaExpression,
    /// Loop body.
    pub body: T,
    /// Source span.
    pub span: Span,
}

/// Public directive model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    Version(VersionDirective),
    Profile(ProfileDirective),
    Schema(SchemaDirective),
    Import(ImportDirective),
    Include(IncludeDirective),
    Use(UseDirective),
    LuaPrelude(LuaPreludeDirective),
    Meta(MetaDirective),
}

/// `@luma` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDirective {
    /// Declared version text.
    pub version: String,
    /// Source span.
    pub span: Span,
}

/// Standard Luma profile names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LumaProfile {
    Data,
    Safe,
    Trusted,
    Custom(String),
}

/// `@profile` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileDirective {
    /// Declared profile.
    pub profile: LumaProfile,
    /// Source span.
    pub span: Span,
}

/// `@schema` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDirective {
    /// Schema location.
    pub location: StringNode,
    /// Source span.
    pub span: Span,
}

/// `@import` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDirective {
    /// Imported URI or host reference.
    pub location: StringNode,
    /// Imported binding alias.
    pub alias: String,
    /// Source span.
    pub span: Span,
}

/// `@include` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDirective {
    /// Included URI or host reference.
    pub location: StringNode,
    /// Source span.
    pub span: Span,
}

/// `@use` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDirective {
    /// Host module name.
    pub module: String,
    /// Module alias.
    pub alias: String,
    /// Source span.
    pub span: Span,
}

/// `@lua:` prelude directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaPreludeDirective {
    /// Prelude Lua block.
    pub block: LuaExpression,
    /// Source span.
    pub span: Span,
}

/// `@meta:` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDirective {
    /// Metadata mapping payload.
    pub value: MappingBlock,
    /// Source span.
    pub span: Span,
}
