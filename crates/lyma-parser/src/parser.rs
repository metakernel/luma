//! Static parser for Level 0/1 conformance.

use std::collections::BTreeMap;

use lyma_syntax::{
    Comment, ConditionalBlock, ConditionalBranch, Diagnostic, DiagnosticCode, Directive, Document,
    DocumentItem, ElseBranch, FileId, LetBinding, LoopBindings, LoopBlock, LymaFile, LymaNode,
    MappingBlock, MappingEntry, MappingItem, SequenceBlock, SequenceItem, Span, SpreadEntry,
    SyntaxIndex, TaggedNode,
};

use crate::{
    block::parse_block_node,
    decode::{SourceText, decode_str},
    directive::{DirectiveParse, make_lua_prelude, make_meta, parse_directive_line},
    document::{parse_block_comment, parse_line_comment},
    error::{diagnostic, diagnostic_with_message},
    key::{ParsedKey, parse_mapping_key},
    lua_capture::inline_expression,
    scalar::{parse_inline_scalar, split_line_comment},
    tag::parse_tag_prefix,
};

/// Parse result for one decoded source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// Normalized source text.
    pub source: SourceText,
    /// Parsed file model.
    pub file: LymaFile,
    /// Collected parse diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    /// Builds a syntax index for this parse result on demand.
    #[must_use]
    pub fn syntax_index(&self) -> SyntaxIndex {
        SyntaxIndex::new(&self.file)
    }
}

/// Decodes and parses a UTF-8 string.
#[must_use]
pub fn parse_str(file_id: FileId, name: &str, text: &str) -> Parsed {
    match decode_str(file_id, name, text) {
        Ok(source) => parse_source(source),
        Err(diagnostic) => Parsed {
            source: SourceText {
                source: lyma_syntax::LymaSource::new(file_id, name, String::new()),
            },
            file: LymaFile {
                documents: Vec::new(),
                span: Span::new(file_id, 0, 0),
            },
            diagnostics: vec![diagnostic],
        },
    }
}

/// Parses an already-decoded source buffer.
#[must_use]
pub fn parse_source(source: SourceText) -> Parsed {
    let mut parser = Parser::new(source);
    let file = parser.parse_file();
    Parsed {
        source: parser.source,
        file,
        diagnostics: parser.diagnostics,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineInfo {
    pub number: usize,
    pub start: usize,
    pub end: usize,
    pub indent: usize,
}

struct Parser {
    source: SourceText,
    lines: Vec<LineInfo>,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn new(source: SourceText) -> Self {
        let mut parser = Self {
            source,
            lines: Vec::new(),
            diagnostics: Vec::new(),
        };
        parser.index_lines();
        parser
    }

    fn parse_file(&mut self) -> LymaFile {
        let mut documents = Vec::new();
        let mut index = 0;
        while index < self.lines.len() {
            index = self.skip_blank(index);
            if index >= self.lines.len() {
                break;
            }
            let separator_span = if self.code(index) == "---" {
                let span = Some(self.line_span(index));
                index += 1;
                span
            } else {
                None
            };
            let start =
                separator_span.map_or_else(|| self.line_span(index).start, |span| span.start);
            let (items, mut next) = self.parse_document_items(index);
            let terminator_span = if next < self.lines.len() && self.code(next) == "..." {
                let span = Some(self.line_span(next));
                next += 1;
                span
            } else {
                None
            };
            let end = terminator_span.map_or_else(
                || items.last().map_or(start, document_item_end),
                |span| span.end,
            );
            documents.push(Document {
                items,
                span: Span::new(self.file_id(), start, end),
                separator_span,
                terminator_span,
            });
            index = next;
        }
        LymaFile {
            documents,
            span: self.full_span(),
        }
    }

    fn parse_document_items(&mut self, mut index: usize) -> (Vec<DocumentItem>, usize) {
        let mut items = Vec::new();
        while index < self.lines.len() {
            index = self.skip_blank(index);
            if index >= self.lines.len() || matches!(self.code(index), "---" | "...") {
                break;
            }
            let line = self.lines[index];
            if line.indent != 0 {
                self.diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidIndentation,
                    Some(self.line_span(index)),
                ));
            }
            if self.is_block_comment_start(index) {
                let file_id = self.file_id();
                let (comment, next) = parse_block_comment(
                    self.source.as_str(),
                    &self.lines,
                    index,
                    &mut self.diagnostics,
                    file_id,
                );
                items.push(DocumentItem::Comment(comment));
                index = next;
                continue;
            }
            if self.is_line_comment(index) {
                let comment_text = self.raw_line(index).trim_start();
                items.push(DocumentItem::Comment(parse_line_comment(
                    comment_text,
                    line.start + line.indent,
                    self.file_id(),
                )));
                index += 1;
                continue;
            }
            let code = self.code(index).to_owned();
            if let Some(parsed) = parse_directive_line(
                &code,
                line.start + line.indent,
                self.file_id(),
                &mut self.diagnostics,
            ) {
                match parsed {
                    DirectiveParse::Regular(directive) => {
                        items.push(DocumentItem::Directive(directive));
                    }
                    DirectiveParse::Lua => {
                        let (directive, next) = self.parse_lua_prelude(index);
                        items.push(DocumentItem::Directive(directive));
                        index = next;
                        continue;
                    }
                    DirectiveParse::Meta => {
                        let (directive, next) = self.parse_meta(index);
                        items.push(DocumentItem::Directive(directive));
                        index = next;
                        continue;
                    }
                    _ => {}
                }
                index += 1;
                continue;
            }
            if self.code(index).starts_with("let ") {
                let (binding, next) = self.parse_let(index, 0);
                items.push(DocumentItem::Let(binding));
                index = next;
                continue;
            }
            let (root, next) = self.parse_node(index, 0);
            items.push(DocumentItem::Root(root));
            index = next;
        }
        (items, index)
    }

    fn parse_node(&mut self, index: usize, indent: usize) -> (LymaNode, usize) {
        let code = self.code(index).to_owned();
        if self.lines[index].indent != indent {
            self.diagnostics.push(diagnostic(
                DiagnosticCode::InvalidIndentation,
                Some(self.line_span(index)),
            ));
        }
        if is_sequence_entry(&code) {
            self.parse_sequence(index, indent)
        } else if find_mapping_colon(&code).is_some() {
            self.parse_mapping(index, indent)
        } else {
            self.parse_value(index, indent, &code, self.lines[index].start + indent)
        }
    }

    fn parse_value(
        &mut self,
        index: usize,
        indent: usize,
        text: &str,
        start: usize,
    ) -> (LymaNode, usize) {
        if is_block_header(text) {
            return parse_block_node(
                &self.source,
                &self.lines,
                index,
                indent,
                start,
                text,
                &mut self.diagnostics,
            );
        }
        if let Some((tag, rest)) = parse_tag_prefix(text, start, self.file_id()) {
            if rest.is_empty() {
                let next = self.skip_blank(index + 1);
                let value = if next < self.lines.len() && self.lines[next].indent > indent {
                    let (node, next_index) = self.parse_nested_block(next, indent);
                    return (
                        LymaNode::Tagged(TaggedNode {
                            span: Span::new(self.file_id(), tag.span.start, node.span().end),
                            tag,
                            value: Some(Box::new(node)),
                        }),
                        next_index,
                    );
                } else {
                    None
                };
                return (
                    LymaNode::Tagged(TaggedNode {
                        span: tag.span,
                        tag,
                        value: value.map(Box::new),
                    }),
                    index + 1,
                );
            }
            let value = parse_inline_scalar(
                rest,
                start + (text.len() - rest.len()),
                self.file_id(),
                &mut self.diagnostics,
            );
            return (
                LymaNode::Tagged(TaggedNode {
                    span: Span::new(self.file_id(), tag.span.start, value.span().end),
                    tag,
                    value: Some(Box::new(value)),
                }),
                index + 1,
            );
        }
        (
            parse_inline_scalar(text, start, self.file_id(), &mut self.diagnostics),
            index + 1,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn parse_mapping(&mut self, mut index: usize, indent: usize) -> (LymaNode, usize) {
        let start = self.lines[index].start + indent;
        let mut items = Vec::new();
        let mut seen = BTreeMap::<String, (usize, Span)>::new();
        let mut duplicates = Vec::new();
        let mut last_end = start;

        while index < self.lines.len() {
            index = self.skip_blank(index);
            if index >= self.lines.len()
                || self.lines[index].indent < indent
                || matches!(self.code(index), "---" | "...")
            {
                break;
            }
            if self.lines[index].indent > indent {
                self.diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidIndentation,
                    Some(self.line_span(index)),
                ));
                index += 1;
                continue;
            }
            if self.is_block_comment_start(index) {
                let file_id = self.file_id();
                let (comment, next) = parse_block_comment(
                    self.source.as_str(),
                    &self.lines,
                    index,
                    &mut self.diagnostics,
                    file_id,
                );
                last_end = comment.span.end;
                items.push(MappingItem::Comment(comment));
                index = next;
                continue;
            }
            if self.is_line_comment(index) {
                let comment = parse_line_comment(
                    self.raw_line(index).trim_start(),
                    self.lines[index].start + indent,
                    self.file_id(),
                );
                last_end = comment.span.end;
                items.push(MappingItem::Comment(comment));
                index += 1;
                continue;
            }
            let code = self.code(index).to_owned();
            if code.starts_with("...") {
                let expr = inline_expression(
                    code.trim_start_matches("...").trim(),
                    self.lines[index].start + indent + 3,
                    self.file_id(),
                );
                let span = self.line_span(index);
                items.push(MappingItem::Spread(SpreadEntry {
                    expression: expr,
                    span,
                }));
                last_end = span.end;
                index += 1;
                continue;
            }
            if code.starts_with("let ") {
                let (binding, next) = self.parse_let(index, indent);
                last_end = binding.span.end;
                items.push(MappingItem::Let(binding));
                index = next;
                continue;
            }
            if let Some(parsed) = parse_directive_line(
                &code,
                self.lines[index].start + indent,
                self.file_id(),
                &mut self.diagnostics,
            ) {
                match parsed {
                    DirectiveParse::Regular(directive) => {
                        let end = directive_span(&directive).end;
                        items.push(MappingItem::Directive(directive));
                        last_end = end;
                        index += 1;
                    }
                    DirectiveParse::Meta => {
                        let (directive, next) = self.parse_meta(index);
                        last_end = directive_span(&directive).end;
                        items.push(MappingItem::Directive(directive));
                        index = next;
                    }
                    DirectiveParse::Lua => {
                        let (directive, next) = self.parse_lua_prelude(index);
                        last_end = directive_span(&directive).end;
                        items.push(MappingItem::Directive(directive));
                        index = next;
                    }
                    DirectiveParse::If(condition) => {
                        let (conditional, next) =
                            self.parse_mapping_conditional(index, indent, &condition);
                        last_end = conditional.span.end;
                        items.push(MappingItem::Conditional(conditional));
                        index = next;
                    }
                    DirectiveParse::For {
                        bindings,
                        bindings_span,
                        iterable,
                        iterable_start,
                    } => {
                        let (loop_block, next) = self.parse_mapping_loop(
                            index,
                            indent,
                            &bindings,
                            bindings_span,
                            &iterable,
                            iterable_start,
                        );
                        last_end = loop_block.span.end;
                        items.push(MappingItem::Loop(loop_block));
                        index = next;
                    }
                    _ => break,
                }
                continue;
            }
            let Some(colon) = find_mapping_colon(&code) else {
                break;
            };
            let key_text = &code[..colon];
            let value_text = code[colon + 1..].trim_start();
            let key_start = self.lines[index].start + indent;
            let value_start = key_start + colon + 1 + (code[colon + 1..].len() - value_text.len());
            let Some(ParsedKey {
                key,
                canonical,
                span: key_span,
            }) = parse_mapping_key(key_text, key_start, self.file_id(), &mut self.diagnostics)
            else {
                index += 1;
                continue;
            };
            let (value, next) = if value_text.is_empty() {
                let child = self.skip_blank(index + 1);
                if child < self.lines.len() && self.lines[child].indent > indent {
                    self.parse_nested_block(child, indent)
                } else {
                    (
                        LymaNode::Null {
                            span: Span::new(self.file_id(), value_start, value_start),
                        },
                        index + 1,
                    )
                }
            } else {
                self.parse_value(index, indent, value_text, value_start)
            };
            if !canonical.is_empty() {
                if let Some((first_index, first_span)) = seen.get(&canonical).copied() {
                    self.diagnostics.push(diagnostic_with_message(
                        DiagnosticCode::DuplicateKey,
                        Some(key_span),
                        format!("duplicate key `{canonical}`"),
                    ));
                    duplicates.push(lyma_syntax::DuplicateKey {
                        key: canonical.clone(),
                        first_index,
                        duplicate_index: items.len(),
                        first_span,
                        duplicate_span: key_span,
                    });
                } else {
                    seen.insert(canonical, (items.len(), key_span));
                }
            }
            let span = Span::new(
                self.file_id(),
                key_span.start,
                value.span().end.max(key_span.end),
            );
            last_end = span.end;
            items.push(MappingItem::Entry(MappingEntry { key, value, span }));
            index = next;
        }
        (
            LymaNode::Mapping(MappingBlock {
                items,
                duplicate_keys: duplicates,
                span: Span::new(self.file_id(), start, last_end),
            }),
            index,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn parse_sequence(&mut self, mut index: usize, indent: usize) -> (LymaNode, usize) {
        let start = self.lines[index].start + indent;
        let mut items = Vec::new();
        let mut last_end = start;
        while index < self.lines.len() {
            index = self.skip_blank(index);
            if index >= self.lines.len()
                || self.lines[index].indent < indent
                || matches!(self.code(index), "---" | "...")
            {
                break;
            }
            if self.lines[index].indent > indent {
                self.diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidIndentation,
                    Some(self.line_span(index)),
                ));
                index += 1;
                continue;
            }
            let code = self.code(index).to_owned();
            if self.is_block_comment_start(index) {
                let file_id = self.file_id();
                let (comment, next) = parse_block_comment(
                    self.source.as_str(),
                    &self.lines,
                    index,
                    &mut self.diagnostics,
                    file_id,
                );
                last_end = comment.span.end;
                items.push(SequenceItem::Comment(comment));
                index = next;
                continue;
            }
            if self.is_line_comment(index) {
                let comment = parse_line_comment(
                    self.raw_line(index).trim_start(),
                    self.lines[index].start + indent,
                    self.file_id(),
                );
                last_end = comment.span.end;
                items.push(SequenceItem::Comment(comment));
                index += 1;
                continue;
            }
            if let Some(parsed) = parse_directive_line(
                &code,
                self.lines[index].start + indent,
                self.file_id(),
                &mut self.diagnostics,
            ) {
                match parsed {
                    DirectiveParse::Regular(directive) => {
                        last_end = directive_span(&directive).end;
                        items.push(SequenceItem::Directive(directive));
                        index += 1;
                    }
                    DirectiveParse::If(condition) => {
                        let (conditional, next) =
                            self.parse_sequence_conditional(index, indent, &condition);
                        last_end = conditional.span.end;
                        items.push(SequenceItem::Conditional(conditional));
                        index = next;
                    }
                    DirectiveParse::For {
                        bindings,
                        bindings_span,
                        iterable,
                        iterable_start,
                    } => {
                        let (loop_block, next) = self.parse_sequence_loop(
                            index,
                            indent,
                            &bindings,
                            bindings_span,
                            &iterable,
                            iterable_start,
                        );
                        last_end = loop_block.span.end;
                        items.push(SequenceItem::Loop(loop_block));
                        index = next;
                    }
                    DirectiveParse::Lua => {
                        let (directive, next) = self.parse_lua_prelude(index);
                        last_end = directive_span(&directive).end;
                        items.push(SequenceItem::Directive(directive));
                        index = next;
                    }
                    DirectiveParse::Meta => {
                        let (directive, next) = self.parse_meta(index);
                        last_end = directive_span(&directive).end;
                        items.push(SequenceItem::Directive(directive));
                        index = next;
                    }
                    _ => break,
                }
                continue;
            }
            if !is_sequence_entry(&code) {
                break;
            }
            let rest = code[1..].trim_start();
            let value_start = self.lines[index].start + indent + 1 + (code[1..].len() - rest.len());
            if let Some(expr) = rest.strip_prefix("...") {
                let span = self.line_span(index);
                items.push(SequenceItem::Spread(SpreadEntry {
                    expression: inline_expression(expr.trim(), value_start + 3, self.file_id()),
                    span,
                }));
                last_end = span.end;
                index += 1;
                continue;
            }
            let (value, next) = if rest.is_empty() {
                let child = self.skip_blank(index + 1);
                if child < self.lines.len() && self.lines[child].indent > indent {
                    self.parse_nested_block(child, indent)
                } else {
                    (
                        LymaNode::Null {
                            span: Span::new(self.file_id(), value_start, value_start),
                        },
                        index + 1,
                    )
                }
            } else if find_mapping_colon(rest).is_some() {
                self.parse_sequence_mapping_item(index, indent, value_start, rest)
            } else {
                self.parse_value(index, indent, rest, value_start)
            };
            last_end = value.span().end;
            items.push(SequenceItem::Value(value));
            index = next;
        }
        (
            LymaNode::Sequence(SequenceBlock {
                items,
                span: Span::new(self.file_id(), start, last_end),
            }),
            index,
        )
    }

    fn parse_sequence_mapping_item(
        &mut self,
        index: usize,
        indent: usize,
        value_start: usize,
        rest: &str,
    ) -> (LymaNode, usize) {
        let Some(colon) = find_mapping_colon(rest) else {
            return self.parse_value(index, indent, rest, value_start);
        };
        let key_text = &rest[..colon];
        let value_text = rest[colon + 1..].trim_start();
        let Some(ParsedKey {
            key,
            span: key_span,
            ..
        }) = parse_mapping_key(key_text, value_start, self.file_id(), &mut self.diagnostics)
        else {
            return (
                LymaNode::Null {
                    span: Span::new(self.file_id(), value_start, value_start),
                },
                index + 1,
            );
        };
        let inline_value_start =
            value_start + colon + 1 + (rest[colon + 1..].len() - value_text.len());
        let (value, mut next) = if value_text.is_empty() {
            (
                LymaNode::Null {
                    span: Span::new(self.file_id(), inline_value_start, inline_value_start),
                },
                index + 1,
            )
        } else {
            self.parse_value(index, indent, value_text, inline_value_start)
        };
        let mut items = vec![MappingItem::Entry(MappingEntry {
            key,
            span: Span::new(
                self.file_id(),
                key_span.start,
                value.span().end.max(key_span.end),
            ),
            value,
        })];
        let child = self.skip_blank(next);
        if child < self.lines.len() && self.lines[child].indent > indent {
            let (node, consumed) = self.parse_mapping(child, self.lines[child].indent);
            if let LymaNode::Mapping(mapping) = node {
                items.extend(mapping.items);
            }
            next = consumed;
        } else {
            next = child;
        }
        let end = items.last().map_or(value_start, MappingItem::span_end);
        (
            LymaNode::Mapping(MappingBlock {
                items,
                duplicate_keys: Vec::new(),
                span: Span::new(self.file_id(), value_start, end),
            }),
            next,
        )
    }

    fn parse_let(&mut self, index: usize, indent: usize) -> (LetBinding, usize) {
        let code = self.code(index).to_owned();
        let rest = code.trim_start_matches("let ");
        let start = self.lines[index].start + indent;
        let (name, name_span, value, next) = if let Some((raw_name, value_text)) =
            rest.split_once('=')
        {
            let name = raw_name.trim();
            let name_start = start + 4 + identifier_offset(name, raw_name);
            if name.is_empty() {
                self.diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidDirectiveSyntax,
                    Some(self.line_span(index)),
                ));
            }
            let value_start =
                start + 4 + raw_name.len() + 1 + (value_text.len() - value_text.trim_start().len());
            let (value, next) = self.parse_value(index, indent, value_text.trim(), value_start);
            (
                name.to_owned(),
                Span::new(self.file_id(), name_start, name_start + name.len()),
                value,
                next,
            )
        } else if let Some((raw_name, _)) = rest.split_once(':') {
            let child = self.skip_blank(index + 1);
            let name = raw_name.trim();
            let name_start = start + 4 + identifier_offset(name, raw_name);
            if name.is_empty() {
                self.diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidDirectiveSyntax,
                    Some(self.line_span(index)),
                ));
            }
            let (value, next) = if child < self.lines.len() && self.lines[child].indent > indent {
                self.parse_nested_block(child, indent)
            } else {
                (
                    LymaNode::Null {
                        span: Span::new(self.file_id(), start, start),
                    },
                    index + 1,
                )
            };
            (
                name.to_owned(),
                Span::new(self.file_id(), name_start, name_start + name.len()),
                value,
                next,
            )
        } else {
            self.diagnostics.push(diagnostic(
                DiagnosticCode::InvalidDirectiveSyntax,
                Some(self.line_span(index)),
            ));
            (
                String::new(),
                Span::new(self.file_id(), start + 4, start + 4),
                LymaNode::Null {
                    span: self.line_span(index),
                },
                index + 1,
            )
        };
        (
            LetBinding {
                span: Span::new(self.file_id(), start, value.span().end.max(start)),
                name,
                name_span,
                value,
            },
            next,
        )
    }

    fn parse_lua_prelude(&mut self, index: usize) -> (Directive, usize) {
        let line = self.lines[index];
        let header_start = line.start + line.indent;
        let (node, next) = parse_block_node(
            &self.source,
            &self.lines,
            index,
            line.indent,
            header_start,
            "|lua",
            &mut self.diagnostics,
        );
        let LymaNode::LuaChunk(block) = node else {
            unreachable!()
        };
        (
            make_lua_prelude(
                block.source.clone(),
                Span::new(self.file_id(), header_start, block.span.end),
            ),
            next,
        )
    }

    fn parse_meta(&mut self, index: usize) -> (Directive, usize) {
        let child = self.skip_blank(index + 1);
        let start = self.lines[index].start + self.lines[index].indent;
        let (node, next) =
            if child < self.lines.len() && self.lines[child].indent > self.lines[index].indent {
                self.parse_mapping(child, self.lines[child].indent)
            } else {
                (
                    LymaNode::Mapping(MappingBlock {
                        items: Vec::new(),
                        duplicate_keys: Vec::new(),
                        span: Span::new(self.file_id(), start, start),
                    }),
                    index + 1,
                )
            };
        let LymaNode::Mapping(value) = node else {
            unreachable!()
        };
        let span = Span::new(self.file_id(), start, value.span.end);
        (make_meta(value, span), next)
    }

    fn parse_mapping_conditional(
        &mut self,
        index: usize,
        indent: usize,
        condition: &str,
    ) -> (ConditionalBlock<MappingBlock>, usize) {
        let if_span = self.line_span(index);
        let if_branch = self.parse_mapping_branch(index, indent, condition);
        let mut next = if_branch.1;
        let mut else_if_branches = Vec::new();
        let mut else_branch = None;
        while next < self.lines.len() && self.lines[next].indent == indent {
            let code = self.code(next).to_owned();
            match parse_directive_line(
                &code,
                self.lines[next].start + indent,
                self.file_id(),
                &mut self.diagnostics,
            ) {
                Some(DirectiveParse::ElseIf(condition)) => {
                    let branch = self.parse_mapping_branch(next, indent, &condition);
                    next = branch.1;
                    else_if_branches.push(branch.0);
                }
                Some(DirectiveParse::Else) => {
                    let child = self.skip_blank(next + 1);
                    let (body, consumed) = if child < self.lines.len()
                        && self.lines[child].indent > indent
                    {
                        let (node, consumed) = self.parse_mapping(child, self.lines[child].indent);
                        let LymaNode::Mapping(body) = node else {
                            unreachable!()
                        };
                        (body, consumed)
                    } else {
                        (
                            MappingBlock {
                                items: Vec::new(),
                                duplicate_keys: Vec::new(),
                                span: self.line_span(next),
                            },
                            next + 1,
                        )
                    };
                    else_branch = Some(ElseBranch {
                        span: Span::new(
                            self.file_id(),
                            self.lines[next].start + indent,
                            body.span.end,
                        ),
                        body,
                    });
                    next = consumed;
                    break;
                }
                _ => break,
            }
        }
        let end = else_branch.as_ref().map_or_else(
            || {
                else_if_branches
                    .last()
                    .map_or(if_branch.0.span.end, |b| b.span.end)
            },
            |b| b.span.end,
        );
        (
            ConditionalBlock {
                if_branch: if_branch.0,
                else_if_branches,
                else_branch,
                span: Span::new(self.file_id(), if_span.start, end),
            },
            next,
        )
    }

    fn parse_mapping_branch(
        &mut self,
        index: usize,
        indent: usize,
        condition: &str,
    ) -> (ConditionalBranch<MappingBlock>, usize) {
        let child = self.skip_blank(index + 1);
        let expr = inline_expression(
            condition,
            self.lines[index].start + indent + 3,
            self.file_id(),
        );
        let (body, next) = if child < self.lines.len() && self.lines[child].indent > indent {
            let (node, next) = self.parse_mapping(child, self.lines[child].indent);
            let LymaNode::Mapping(body) = node else {
                unreachable!()
            };
            (body, next)
        } else {
            (
                MappingBlock {
                    items: Vec::new(),
                    duplicate_keys: Vec::new(),
                    span: self.line_span(index),
                },
                index + 1,
            )
        };
        (
            ConditionalBranch {
                span: Span::new(
                    self.file_id(),
                    self.lines[index].start + indent,
                    body.span.end,
                ),
                condition: expr,
                body,
            },
            next,
        )
    }

    fn parse_sequence_conditional(
        &mut self,
        index: usize,
        indent: usize,
        condition: &str,
    ) -> (ConditionalBlock<SequenceBlock>, usize) {
        let if_branch = self.parse_sequence_branch(index, indent, condition);
        let mut next = if_branch.1;
        let mut else_if_branches = Vec::new();
        let mut else_branch = None;
        while next < self.lines.len() && self.lines[next].indent == indent {
            let code = self.code(next).to_owned();
            match parse_directive_line(
                &code,
                self.lines[next].start + indent,
                self.file_id(),
                &mut self.diagnostics,
            ) {
                Some(DirectiveParse::ElseIf(condition)) => {
                    let branch = self.parse_sequence_branch(next, indent, &condition);
                    next = branch.1;
                    else_if_branches.push(branch.0);
                }
                Some(DirectiveParse::Else) => {
                    let child = self.skip_blank(next + 1);
                    let (body, consumed) = if child < self.lines.len()
                        && self.lines[child].indent > indent
                    {
                        let (node, consumed) = self.parse_sequence(child, self.lines[child].indent);
                        let LymaNode::Sequence(body) = node else {
                            unreachable!()
                        };
                        (body, consumed)
                    } else {
                        (
                            SequenceBlock {
                                items: Vec::new(),
                                span: self.line_span(next),
                            },
                            next + 1,
                        )
                    };
                    else_branch = Some(ElseBranch {
                        body,
                        span: self.line_span(next),
                    });
                    next = consumed;
                    break;
                }
                _ => break,
            }
        }
        let end = else_branch.as_ref().map_or_else(
            || {
                else_if_branches
                    .last()
                    .map_or(if_branch.0.span.end, |b| b.span.end)
            },
            |b| b.span.end,
        );
        (
            ConditionalBlock {
                if_branch: if_branch.0,
                else_if_branches,
                else_branch,
                span: Span::new(self.file_id(), self.lines[index].start + indent, end),
            },
            next,
        )
    }

    fn parse_sequence_branch(
        &mut self,
        index: usize,
        indent: usize,
        condition: &str,
    ) -> (ConditionalBranch<SequenceBlock>, usize) {
        let child = self.skip_blank(index + 1);
        let expr = inline_expression(
            condition,
            self.lines[index].start + indent + 3,
            self.file_id(),
        );
        let (body, next) = if child < self.lines.len() && self.lines[child].indent > indent {
            let (node, next) = self.parse_sequence(child, self.lines[child].indent);
            let LymaNode::Sequence(body) = node else {
                unreachable!()
            };
            (body, next)
        } else {
            (
                SequenceBlock {
                    items: Vec::new(),
                    span: self.line_span(index),
                },
                index + 1,
            )
        };
        (
            ConditionalBranch {
                span: Span::new(
                    self.file_id(),
                    self.lines[index].start + indent,
                    body.span.end,
                ),
                condition: expr,
                body,
            },
            next,
        )
    }

    fn parse_mapping_loop(
        &mut self,
        index: usize,
        indent: usize,
        bindings: &str,
        bindings_span: Span,
        iterable: &str,
        iterable_start: usize,
    ) -> (LoopBlock<MappingBlock>, usize) {
        let child = self.skip_blank(index + 1);
        let (body, next) = if child < self.lines.len() && self.lines[child].indent > indent {
            let (node, next) = self.parse_mapping(child, self.lines[child].indent);
            let LymaNode::Mapping(body) = node else {
                unreachable!()
            };
            (body, next)
        } else {
            (
                MappingBlock {
                    items: Vec::new(),
                    duplicate_keys: Vec::new(),
                    span: self.line_span(index),
                },
                index + 1,
            )
        };
        let end = body.span.end;
        (
            LoopBlock {
                bindings: parse_loop_bindings(bindings, bindings_span),
                iterable: inline_expression(iterable, iterable_start, self.file_id()),
                body,
                span: Span::new(self.file_id(), self.lines[index].start + indent, end),
            },
            next,
        )
    }

    fn parse_sequence_loop(
        &mut self,
        index: usize,
        indent: usize,
        bindings: &str,
        bindings_span: Span,
        iterable: &str,
        iterable_start: usize,
    ) -> (LoopBlock<SequenceBlock>, usize) {
        let child = self.skip_blank(index + 1);
        let (body, next) = if child < self.lines.len() && self.lines[child].indent > indent {
            let (node, next) = self.parse_sequence(child, self.lines[child].indent);
            let LymaNode::Sequence(body) = node else {
                unreachable!()
            };
            (body, next)
        } else {
            (
                SequenceBlock {
                    items: Vec::new(),
                    span: self.line_span(index),
                },
                index + 1,
            )
        };
        let end = body.span.end;
        (
            LoopBlock {
                bindings: parse_loop_bindings(bindings, bindings_span),
                iterable: inline_expression(iterable, iterable_start, self.file_id()),
                body,
                span: Span::new(self.file_id(), self.lines[index].start + indent, end),
            },
            next,
        )
    }

    fn index_lines(&mut self) {
        let text = self.source.as_str();
        let mut start = 0;
        let mut number = 1;
        while start <= text.len() {
            let end = text[start..]
                .find('\n')
                .map_or(text.len(), |relative| start + relative);
            let line = &text[start..end];
            let mut indent = 0;
            for (offset, ch) in line.char_indices() {
                match ch {
                    ' ' => indent += 1,
                    '\t' => {
                        self.diagnostics.push(diagnostic(
                            DiagnosticCode::TabUsedForIndentation,
                            Some(Span::new(
                                self.file_id(),
                                start + offset,
                                start + offset + 1,
                            )),
                        ));
                        indent += 1;
                    }
                    _ => break,
                }
            }
            self.lines.push(LineInfo {
                number,
                start,
                end,
                indent,
            });
            if end == text.len() {
                break;
            }
            start = end + 1;
            number += 1;
        }
    }

    fn raw_line(&self, index: usize) -> &str {
        let line = self.lines[index];
        &self.source.as_str()[line.start..line.end]
    }

    fn code(&self, index: usize) -> &str {
        let raw = self.raw_line(index);
        let trimmed = raw.trim_end();
        let prefix_len = raw
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .count();
        let significant = &trimmed[prefix_len..];
        if matches!(significant, "---" | "...") || significant.starts_with("--[[") {
            significant
        } else {
            split_line_comment(significant).0.trim_end()
        }
    }

    fn is_line_comment(&self, index: usize) -> bool {
        let raw = self.raw_line(index).trim_start();
        raw.starts_with("--") && !raw.starts_with("--[[")
    }

    fn is_block_comment_start(&self, index: usize) -> bool {
        self.raw_line(index).trim_start().starts_with("--[[")
    }

    fn skip_blank(&self, mut index: usize) -> usize {
        while index < self.lines.len()
            && self.code(index).is_empty()
            && !self.is_line_comment(index)
            && !self.is_block_comment_start(index)
        {
            index += 1;
        }
        index
    }

    fn line_span(&self, index: usize) -> Span {
        let line = self.lines[index];
        Span::new(self.file_id(), line.start, line.end)
    }

    fn parse_nested_block(&mut self, index: usize, parent_indent: usize) -> (LymaNode, usize) {
        let indent = self.lines[index].indent;
        if self.block_looks_like_sequence(index, indent, parent_indent) {
            self.parse_sequence(index, indent)
        } else {
            self.parse_mapping(index, indent)
        }
    }

    fn block_looks_like_sequence(
        &self,
        mut index: usize,
        indent: usize,
        parent_indent: usize,
    ) -> bool {
        while index < self.lines.len() {
            index = self.skip_blank(index);
            if index >= self.lines.len()
                || self.lines[index].indent < indent
                || matches!(self.code(index), "---" | "...")
            {
                break;
            }
            if self.lines[index].indent > indent {
                index += 1;
                continue;
            }
            let code = self.code(index);
            if is_sequence_entry(code) {
                return true;
            }
            if find_mapping_colon(code).is_some()
                || code.starts_with("...")
                || code.starts_with("let ")
            {
                return false;
            }
            if code.starts_with('@')
                || self.is_line_comment(index)
                || self.is_block_comment_start(index)
            {
                index += 1;
                continue;
            }
            return indent > parent_indent && is_sequence_entry(code);
        }
        false
    }

    fn full_span(&self) -> Span {
        self.source.source.full_span()
    }
    const fn file_id(&self) -> FileId {
        self.source.source.id
    }
}

fn is_sequence_entry(code: &str) -> bool {
    code == "-" || code.starts_with("- ") || code.starts_with("-\t")
}

fn is_block_header(text: &str) -> bool {
    matches!(
        text.trim(),
        "|" | "|-"
            | "|+"
            | ">"
            | ">-"
            | ">+"
            | "|expr"
            | "|expr-"
            | "|expr+"
            | "|lua"
            | "|lua-"
            | "|lua+"
    )
}

fn find_mapping_colon(text: &str) -> Option<usize> {
    let mut quote = None;
    let mut escape = false;
    let mut bracket_depth = 0usize;
    for (index, ch) in text.char_indices() {
        if let Some(active) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ':' if bracket_depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_loop_bindings(text: &str, span: Span) -> LoopBindings {
    if let Some((key, value)) = text.split_once(',') {
        let key = key.trim();
        let value = value.trim();
        let key_start =
            span.start + identifier_offset(key, text.split_once(',').map_or("", |(raw, _)| raw));
        let value_start = span.start + text.find(value).unwrap_or(text.len());
        LoopBindings::Two {
            key: key.to_owned(),
            key_span: Span::new(span.file_id, key_start, key_start + key.len()),
            value: value.to_owned(),
            value_span: Span::new(span.file_id, value_start, value_start + value.len()),
        }
    } else {
        let value = text.trim();
        let value_start = span.start + identifier_offset(value, text);
        LoopBindings::One {
            value: value.to_owned(),
            value_span: Span::new(span.file_id, value_start, value_start + value.len()),
        }
    }
}

fn identifier_offset(trimmed: &str, raw: &str) -> usize {
    raw.find(trimmed).unwrap_or(0)
}

fn document_item_end(item: &DocumentItem) -> usize {
    match item {
        DocumentItem::Directive(d) => directive_span(d).end,
        DocumentItem::Let(binding) => binding.span.end,
        DocumentItem::Root(node) => node.span().end,
        DocumentItem::Comment(comment) => comment.span.end,
    }
}

#[allow(clippy::missing_const_for_fn)]
fn directive_span(directive: &Directive) -> Span {
    match directive {
        Directive::Version(v) => v.span,
        Directive::Profile(v) => v.span,
        Directive::Schema(v) => v.span,
        Directive::Import(v) => v.span,
        Directive::Include(v) => v.span,
        Directive::Use(v) => v.span,
        Directive::LuaPrelude(v) => v.span,
        Directive::Meta(v) => v.span,
    }
}

trait NodeSpan {
    fn span(&self) -> Span;
}

impl NodeSpan for LymaNode {
    fn span(&self) -> Span {
        match self {
            Self::Null { span } | Self::Boolean { span, .. } => *span,
            Self::Number(number) => number.span,
            Self::String(string) => string.span,
            Self::Sequence(sequence) => sequence.span,
            Self::Mapping(mapping) => mapping.span,
            Self::Tagged(tagged) => tagged.span,
            Self::LuaExpression(expression)
            | Self::LuaExpressionBlock(expression)
            | Self::LuaChunk(expression)
            | Self::LuaTableConstructor(expression) => expression.span,
        }
    }
}

trait MappingItemExt {
    fn span_end(&self) -> usize;
}

impl MappingItemExt for MappingItem {
    fn span_end(&self) -> usize {
        match self {
            Self::Entry(entry) => entry.span.end,
            Self::Spread(entry) => entry.span.end,
            Self::Directive(d) => directive_span(d).end,
            Self::Conditional(v) => v.span.end,
            Self::Loop(v) => v.span.end,
            Self::Let(v) => v.span.end,
            Self::Comment(Comment { span, .. }) => span.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use lyma_syntax::{
        DiagnosticCode, Directive, DocumentItem, FileId, LoopBindings, LymaNode, MappingItem,
        MappingKey, StringStyle, SyntaxKind,
    };

    use super::parse_str;

    #[test]
    fn parses_mapping_sequence_and_scalars() {
        let parsed = parse_str(
            FileId(1),
            "level0.lyma",
            "name: Example\nenabled: true\ncount: 0xff\nitems:\n  - alpha\n  - 2\n",
        );
        assert!(parsed.diagnostics.is_empty());
        let document = &parsed.file.documents[0];
        let DocumentItem::Root(LymaNode::Mapping(mapping)) = &document.items[0] else {
            panic!()
        };
        assert_eq!(mapping.items.len(), 4);
        let MappingItem::Entry(name) = &mapping.items[0] else {
            panic!()
        };
        let LymaNode::String(value) = &name.value else {
            panic!()
        };
        assert_eq!(value.value, "Example");
        assert_eq!(value.style, StringStyle::Plain);
    }

    #[test]
    fn rejects_duplicate_keys() {
        let parsed = parse_str(FileId(1), "dup.lyma", "name: one\nname: two\n");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateKey)
        );
    }

    #[test]
    fn parsed_builds_syntax_index_on_demand() {
        let source = "root:\n  child: 42\n";
        let parsed = parse_str(FileId(1), "example.lyma", source);

        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let index = parsed.syntax_index();
        let child_offset = source.find("child").unwrap();
        let child_id = index.smallest_node_at_offset(child_offset).unwrap();
        let entry_id = index.parent(child_id).unwrap();

        assert_eq!(
            index.node(child_id).unwrap().kind,
            SyntaxKind::PlainMappingKey
        );
        assert_eq!(index.node(entry_id).unwrap().kind, SyntaxKind::MappingEntry);
    }

    #[test]
    fn parses_exact_identifier_subspans() {
        let source = concat!(
            "let foo = 1\n",
            "@import \"x\" as alias\n",
            "@use module as alias\n",
            "container:\n",
            "  @for key, value in expr:\n",
            "    plain_key: !tag\n",
        );

        let parsed = parse_str(FileId(1), "spans.lyma", source);

        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let DocumentItem::Let(binding) = &parsed.file.documents[0].items[0] else {
            panic!()
        };
        assert_eq!(binding.name, "foo");
        assert_eq!(binding.name_span, lyma_syntax::Span::new(FileId(1), 4, 7));

        let DocumentItem::Directive(Directive::Import(import)) = &parsed.file.documents[0].items[1]
        else {
            panic!()
        };
        assert_eq!(import.alias, "alias");
        assert_eq!(import.name_span, lyma_syntax::Span::new(FileId(1), 13, 19));
        assert_eq!(import.alias_span, lyma_syntax::Span::new(FileId(1), 27, 32));

        let DocumentItem::Directive(Directive::Use(usage)) = &parsed.file.documents[0].items[2]
        else {
            panic!()
        };
        assert_eq!(usage.module, "module");
        assert_eq!(usage.module_span, lyma_syntax::Span::new(FileId(1), 38, 44));
        assert_eq!(usage.alias_span, lyma_syntax::Span::new(FileId(1), 48, 53));

        let DocumentItem::Root(LymaNode::Mapping(mapping)) = &parsed.file.documents[0].items[3]
        else {
            panic!()
        };
        let MappingItem::Entry(container) = &mapping.items[0] else {
            panic!()
        };
        let LymaNode::Mapping(container_body) = &container.value else {
            panic!()
        };
        let MappingItem::Loop(loop_block) = &container_body.items[0] else {
            panic!()
        };
        assert_eq!(
            loop_block.iterable.span,
            lyma_syntax::Span::new(FileId(1), 86, 90)
        );
        if let LoopBindings::Two {
            key,
            key_span,
            value,
            value_span,
        } = &loop_block.bindings
        {
            assert_eq!(key, "key");
            assert_eq!(*key_span, lyma_syntax::Span::new(FileId(1), 72, 75));
            assert_eq!(value, "value");
            assert_eq!(*value_span, lyma_syntax::Span::new(FileId(1), 77, 82));
        } else {
            panic!();
        }

        let MappingItem::Entry(entry) = &loop_block.body.items[0] else {
            panic!()
        };
        let MappingKey::Plain { value_span, .. } = &entry.key else {
            panic!()
        };
        assert_eq!(*value_span, lyma_syntax::Span::new(FileId(1), 96, 105));

        let LymaNode::Tagged(tagged) = &entry.value else {
            panic!()
        };
        assert_eq!(tagged.tag.span, lyma_syntax::Span::new(FileId(1), 107, 111));
        assert_eq!(
            tagged.tag.name.span,
            lyma_syntax::Span::new(FileId(1), 108, 111)
        );
    }

    #[test]
    fn malformed_let_keeps_broad_binding_span() {
        let parsed = parse_str(FileId(1), "bad-let.lyma", "let = 1\n");

        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidDirectiveSyntax)
        );

        let DocumentItem::Let(binding) = &parsed.file.documents[0].items[0] else {
            panic!()
        };
        assert_eq!(binding.name, "");
        assert_eq!(binding.name_span, lyma_syntax::Span::new(FileId(1), 4, 4));
        assert_eq!(binding.span, lyma_syntax::Span::new(FileId(1), 0, 7));
    }
}
