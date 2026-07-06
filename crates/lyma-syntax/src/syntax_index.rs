//! Non-owning syntax index for parsed Lyma trees.

#![allow(missing_docs)]

use crate::ast::{
    Comment, ConditionalBlock, ConditionalBranch, Directive, Document, DocumentItem, ElseBranch,
    LetBinding, LoopBlock, LuaExpression, LymaFile, LymaNode, MappingBlock, MappingEntry,
    MappingItem, MappingKey, MetaDirective, SequenceBlock, SequenceItem, SpreadEntry, StringNode,
    TaggedNode,
};
use crate::source::{Offset, Span};

/// Deterministic syntax node identifier within a single indexed parse result.
///
/// IDs are assigned in preorder traversal order and are only stable for the specific
/// [`SyntaxIndex`] they were created from. They are not cross-edit persistent identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxNodeId(pub u32);

/// Public syntax node kind for indexed AST traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxKind {
    File,
    Document,
    Comment,
    LetBinding,
    Null,
    Boolean,
    Number,
    String,
    Sequence,
    Mapping,
    Tagged,
    Tag,
    TagName,
    LuaExpression,
    LuaExpressionBlock,
    LuaChunk,
    LuaTableConstructor,
    MappingEntry,
    PlainMappingKey,
    QuotedMappingKey,
    ExpressionMappingKey,
    SpreadEntry,
    ConditionalBlock,
    ConditionalBranch,
    ElseBranch,
    LoopBlock,
    VersionDirective,
    ProfileDirective,
    SchemaDirective,
    ImportDirective,
    IncludeDirective,
    UseDirective,
    LuaPreludeDirective,
    MetaDirective,
}

/// Indexed public metadata for one syntax node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxNodeInfo {
    pub id: SyntaxNodeId,
    pub kind: SyntaxKind,
    pub span: Span,
    pub parent: Option<SyntaxNodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SyntaxNodeRecord {
    info: SyntaxNodeInfo,
    children: Vec<SyntaxNodeId>,
}

/// Non-owning index over a parsed [`LymaFile`].
///
/// The index borrows no AST data and stores only node metadata plus parent/child links.
/// Node IDs are deterministic preorder IDs for the indexed tree, but they are only stable
/// within a single parse result and must not be treated as persistent identifiers across edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxIndex {
    pub root_ids: Vec<SyntaxNodeId>,
    nodes: Vec<SyntaxNodeRecord>,
}

impl SyntaxIndex {
    /// Builds an index from a parsed file.
    #[must_use]
    pub fn new(file: &LymaFile) -> Self {
        SyntaxIndexBuilder::default().build(file)
    }

    /// Returns the parent of `id`, if it exists.
    #[must_use]
    pub fn parent(&self, id: SyntaxNodeId) -> Option<SyntaxNodeId> {
        self.node(id).and_then(|node| node.parent)
    }

    /// Returns the child IDs of `id`.
    #[must_use]
    pub fn children(&self, id: SyntaxNodeId) -> &[SyntaxNodeId] {
        self.nodes
            .get(id.0 as usize)
            .map_or(&[], |record| record.children.as_slice())
    }

    /// Returns indexed metadata for `id`.
    #[must_use]
    pub fn node(&self, id: SyntaxNodeId) -> Option<&SyntaxNodeInfo> {
        self.nodes.get(id.0 as usize).map(|record| &record.info)
    }

    /// Returns ancestor IDs from the immediate parent outward.
    pub fn ancestors(&self, id: SyntaxNodeId) -> impl Iterator<Item = SyntaxNodeId> + '_ {
        std::iter::successors(self.parent(id), |current| self.parent(*current))
    }

    /// Returns all node IDs whose spans fully cover `span`, in preorder.
    #[must_use]
    pub fn covering_span(&self, span: Span) -> Vec<SyntaxNodeId> {
        self.nodes
            .iter()
            .filter_map(|record| {
                record
                    .info
                    .span
                    .contains_span(span)
                    .then_some(record.info.id)
            })
            .collect()
    }

    /// Returns the smallest indexed node whose span contains `offset`.
    #[must_use]
    pub fn smallest_node_at_offset(&self, offset: Offset) -> Option<SyntaxNodeId> {
        let mut best: Option<(SyntaxNodeId, Offset)> = None;
        for record in &self.nodes {
            if record.info.span.contains_offset(offset) {
                let len = record.info.span.len();
                match best {
                    None => best = Some((record.info.id, len)),
                    Some((_best_id, best_len)) if len < best_len => {
                        best = Some((record.info.id, len));
                    }
                    Some((best_id, best_len))
                        if len == best_len && record.info.id.0 > best_id.0 =>
                    {
                        best = Some((record.info.id, len));
                    }
                    _ => {}
                }
            }
        }
        best.map(|(id, _)| id)
    }
}

impl From<&LymaFile> for SyntaxIndex {
    fn from(value: &LymaFile) -> Self {
        Self::new(value)
    }
}

#[derive(Default)]
struct SyntaxIndexBuilder {
    root_ids: Vec<SyntaxNodeId>,
    nodes: Vec<SyntaxNodeRecord>,
}

impl SyntaxIndexBuilder {
    fn build(mut self, file: &LymaFile) -> SyntaxIndex {
        self.push_file(file);
        SyntaxIndex {
            root_ids: self.root_ids,
            nodes: self.nodes,
        }
    }

    fn push(&mut self, kind: SyntaxKind, span: Span, parent: Option<SyntaxNodeId>) -> SyntaxNodeId {
        let id = SyntaxNodeId(
            u32::try_from(self.nodes.len()).expect("syntax index node count exceeded u32::MAX"),
        );
        self.nodes.push(SyntaxNodeRecord {
            info: SyntaxNodeInfo {
                id,
                kind,
                span,
                parent,
            },
            children: Vec::new(),
        });
        if let Some(parent_id) = parent {
            self.nodes[parent_id.0 as usize].children.push(id);
        } else {
            self.root_ids.push(id);
        }
        id
    }

    fn push_file(&mut self, file: &LymaFile) {
        let file_id = self.push(SyntaxKind::File, file.span, None);
        for document in &file.documents {
            self.push_document(document, file_id);
        }
    }

    fn push_document(&mut self, document: &Document, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::Document, document.span, Some(parent));
        for item in &document.items {
            self.push_document_item(item, id);
        }
    }

    fn push_document_item(&mut self, item: &DocumentItem, parent: SyntaxNodeId) {
        match item {
            DocumentItem::Directive(directive) => self.push_directive(directive, parent),
            DocumentItem::Let(binding) => self.push_let_binding(binding, parent),
            DocumentItem::Root(node) => self.push_lyma_node(node, parent),
            DocumentItem::Comment(comment) => self.push_comment(comment, parent),
        }
    }

    fn push_lyma_node(&mut self, node: &LymaNode, parent: SyntaxNodeId) {
        match node {
            LymaNode::Null { span } => {
                self.push(SyntaxKind::Null, *span, Some(parent));
            }
            LymaNode::Boolean { span, .. } => {
                self.push(SyntaxKind::Boolean, *span, Some(parent));
            }
            LymaNode::Number(number) => {
                self.push(SyntaxKind::Number, number.span, Some(parent));
            }
            LymaNode::String(string) => {
                self.push(SyntaxKind::String, string.span, Some(parent));
            }
            LymaNode::Sequence(sequence) => self.push_sequence_block(sequence, parent),
            LymaNode::Mapping(mapping) => self.push_mapping_block(mapping, parent),
            LymaNode::Tagged(tagged) => self.push_tagged_node(tagged, parent),
            LymaNode::LuaExpression(expression) => {
                self.push_lua_expression(SyntaxKind::LuaExpression, expression, parent);
            }
            LymaNode::LuaExpressionBlock(expression) => {
                self.push_lua_expression(SyntaxKind::LuaExpressionBlock, expression, parent);
            }
            LymaNode::LuaChunk(expression) => {
                self.push_lua_expression(SyntaxKind::LuaChunk, expression, parent);
            }
            LymaNode::LuaTableConstructor(expression) => {
                self.push_lua_expression(SyntaxKind::LuaTableConstructor, expression, parent);
            }
        }
    }

    fn push_mapping_block(&mut self, mapping: &MappingBlock, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::Mapping, mapping.span, Some(parent));
        for item in &mapping.items {
            self.push_mapping_item(item, id);
        }
    }

    fn push_sequence_block(&mut self, sequence: &SequenceBlock, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::Sequence, sequence.span, Some(parent));
        for item in &sequence.items {
            self.push_sequence_item(item, id);
        }
    }

    fn push_mapping_item(&mut self, item: &MappingItem, parent: SyntaxNodeId) {
        match item {
            MappingItem::Entry(entry) => self.push_mapping_entry(entry, parent),
            MappingItem::Spread(spread) => self.push_spread_entry(spread, parent),
            MappingItem::Directive(directive) => self.push_directive(directive, parent),
            MappingItem::Conditional(block) => self.push_mapping_conditional(block, parent),
            MappingItem::Loop(block) => self.push_mapping_loop(block, parent),
            MappingItem::Let(binding) => self.push_let_binding(binding, parent),
            MappingItem::Comment(comment) => self.push_comment(comment, parent),
        }
    }

    fn push_sequence_item(&mut self, item: &SequenceItem, parent: SyntaxNodeId) {
        match item {
            SequenceItem::Value(node) => self.push_lyma_node(node, parent),
            SequenceItem::Spread(spread) => self.push_spread_entry(spread, parent),
            SequenceItem::Directive(directive) => self.push_directive(directive, parent),
            SequenceItem::Conditional(block) => self.push_sequence_conditional(block, parent),
            SequenceItem::Loop(block) => self.push_sequence_loop(block, parent),
            SequenceItem::Comment(comment) => self.push_comment(comment, parent),
        }
    }

    fn push_mapping_entry(&mut self, entry: &MappingEntry, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::MappingEntry, entry.span, Some(parent));
        self.push_mapping_key(&entry.key, id);
        self.push_lyma_node(&entry.value, id);
    }

    fn push_mapping_key(&mut self, key: &MappingKey, parent: SyntaxNodeId) {
        match key {
            MappingKey::Plain { span, .. } => {
                self.push(SyntaxKind::PlainMappingKey, *span, Some(parent));
            }
            MappingKey::Quoted(StringNode { span, .. }) => {
                self.push(SyntaxKind::QuotedMappingKey, *span, Some(parent));
            }
            MappingKey::Expression { span, .. } => {
                self.push(SyntaxKind::ExpressionMappingKey, *span, Some(parent));
            }
        }
    }

    fn push_tagged_node(&mut self, tagged: &TaggedNode, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::Tagged, tagged.span, Some(parent));
        let tag_id = self.push(SyntaxKind::Tag, tagged.tag.span, Some(id));
        self.push(SyntaxKind::TagName, tagged.tag.name.span, Some(tag_id));
        if let Some(value) = &tagged.value {
            self.push_lyma_node(value, id);
        }
    }

    fn push_lua_expression(
        &mut self,
        kind: SyntaxKind,
        expression: &LuaExpression,
        parent: SyntaxNodeId,
    ) {
        self.push(kind, expression.span, Some(parent));
    }

    fn push_spread_entry(&mut self, spread: &SpreadEntry, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::SpreadEntry, spread.span, Some(parent));
        self.push_lua_expression(SyntaxKind::LuaExpression, &spread.expression, id);
    }

    fn push_let_binding(&mut self, binding: &LetBinding, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::LetBinding, binding.span, Some(parent));
        self.push_lyma_node(&binding.value, id);
    }

    fn push_comment(&mut self, comment: &Comment, parent: SyntaxNodeId) {
        self.push(SyntaxKind::Comment, comment.span, Some(parent));
    }

    fn push_mapping_conditional(
        &mut self,
        conditional: &ConditionalBlock<MappingBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ConditionalBlock, conditional.span, Some(parent));
        self.push_mapping_branch(&conditional.if_branch, id);
        for branch in &conditional.else_if_branches {
            self.push_mapping_branch(branch, id);
        }
        if let Some(branch) = &conditional.else_branch {
            self.push_mapping_else_branch(branch, id);
        }
    }

    fn push_sequence_conditional(
        &mut self,
        conditional: &ConditionalBlock<SequenceBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ConditionalBlock, conditional.span, Some(parent));
        self.push_sequence_branch(&conditional.if_branch, id);
        for branch in &conditional.else_if_branches {
            self.push_sequence_branch(branch, id);
        }
        if let Some(branch) = &conditional.else_branch {
            self.push_sequence_else_branch(branch, id);
        }
    }

    fn push_mapping_branch(
        &mut self,
        branch: &ConditionalBranch<MappingBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ConditionalBranch, branch.span, Some(parent));
        self.push_lua_expression(SyntaxKind::LuaExpression, &branch.condition, id);
        self.push_mapping_block(&branch.body, id);
    }

    fn push_sequence_branch(
        &mut self,
        branch: &ConditionalBranch<SequenceBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ConditionalBranch, branch.span, Some(parent));
        self.push_lua_expression(SyntaxKind::LuaExpression, &branch.condition, id);
        self.push_sequence_block(&branch.body, id);
    }

    fn push_mapping_else_branch(
        &mut self,
        branch: &ElseBranch<MappingBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ElseBranch, branch.span, Some(parent));
        self.push_mapping_block(&branch.body, id);
    }

    fn push_sequence_else_branch(
        &mut self,
        branch: &ElseBranch<SequenceBlock>,
        parent: SyntaxNodeId,
    ) {
        let id = self.push(SyntaxKind::ElseBranch, branch.span, Some(parent));
        self.push_sequence_block(&branch.body, id);
    }

    fn push_mapping_loop(&mut self, block: &LoopBlock<MappingBlock>, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::LoopBlock, block.span, Some(parent));
        self.push_lua_expression(SyntaxKind::LuaExpression, &block.iterable, id);
        self.push_mapping_block(&block.body, id);
    }

    fn push_sequence_loop(&mut self, block: &LoopBlock<SequenceBlock>, parent: SyntaxNodeId) {
        let id = self.push(SyntaxKind::LoopBlock, block.span, Some(parent));
        self.push_lua_expression(SyntaxKind::LuaExpression, &block.iterable, id);
        self.push_sequence_block(&block.body, id);
    }

    fn push_directive(&mut self, directive: &Directive, parent: SyntaxNodeId) {
        match directive {
            Directive::Version(directive) => {
                self.push(SyntaxKind::VersionDirective, directive.span, Some(parent));
            }
            Directive::Profile(directive) => {
                self.push(SyntaxKind::ProfileDirective, directive.span, Some(parent));
            }
            Directive::Schema(directive) => {
                let id = self.push(SyntaxKind::SchemaDirective, directive.span, Some(parent));
                self.push(SyntaxKind::String, directive.location.span, Some(id));
            }
            Directive::Import(directive) => {
                let id = self.push(SyntaxKind::ImportDirective, directive.span, Some(parent));
                self.push(SyntaxKind::String, directive.location.span, Some(id));
            }
            Directive::Include(directive) => {
                let id = self.push(SyntaxKind::IncludeDirective, directive.span, Some(parent));
                self.push(SyntaxKind::String, directive.location.span, Some(id));
            }
            Directive::Use(directive) => {
                self.push(SyntaxKind::UseDirective, directive.span, Some(parent));
            }
            Directive::LuaPrelude(directive) => {
                let id = self.push(
                    SyntaxKind::LuaPreludeDirective,
                    directive.span,
                    Some(parent),
                );
                self.push_lua_expression(SyntaxKind::LuaExpression, &directive.block, id);
            }
            Directive::Meta(MetaDirective { value, span, .. }) => {
                let id = self.push(SyntaxKind::MetaDirective, *span, Some(parent));
                self.push_mapping_block(value, id);
            }
        }
    }
}
