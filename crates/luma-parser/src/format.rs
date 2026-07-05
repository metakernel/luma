//! Canonical formatter for parsed Luma syntax trees.

use luma_syntax::{
    BlockChomping, BlockKind, Comment, CommentKind, ConditionalBlock, Directive, Document,
    DocumentItem, ElseBranch, LetBinding, LoopBindings, LoopBlock, LumaFile, LumaNode,
    MappingBlock, MappingEntry, MappingItem, MappingKey, SequenceBlock, SequenceItem, StringNode,
    StringStyle, SyntaxKind, TextEdit, TextRange, apply_text_edits,
};

use crate::{FileId, Parsed, parse_str};

/// Formatter configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Indentation width, in spaces.
    pub indent_width: usize,
    /// Preserve explicit document terminators when present.
    pub preserve_document_terminators: bool,
}

/// Behavior when range formatting cannot be represented as edits confined to the
/// requested expanded range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatRangeFallback {
    /// Return a single whole-document replacement edit.
    WholeDocument,
    /// Reject the request with [`FormatRangeError::RequiresWholeDocument`].
    Reject,
}

/// Range-formatting configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatRangeOptions {
    /// Formatter configuration used for the canonical whole-document render.
    pub format: FormatOptions,
    /// Fallback behavior when edits would be required outside the expanded range.
    pub fallback: FormatRangeFallback,
}

impl Default for FormatRangeOptions {
    fn default() -> Self {
        Self {
            format: FormatOptions::default(),
            fallback: FormatRangeFallback::WholeDocument,
        }
    }
}

/// Range-formatting failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatRangeError {
    /// The requested range is out of bounds or has `start > end`.
    InvalidRange(TextRange),
    /// The requested range is not aligned to UTF-8 character boundaries.
    NonBoundaryRange(TextRange),
    /// Canonical formatting requires edits outside the expanded range.
    RequiresWholeDocument {
        /// Original caller-provided range.
        requested: TextRange,
        /// Line/node-expanded range used for the local formatting attempt.
        expanded: TextRange,
    },
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 2,
            preserve_document_terminators: true,
        }
    }
}

/// Result of formatting one source buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedDocument {
    /// Canonical text with normalized `\n` line endings.
    pub text: String,
    /// Whether the output differs from the provided input.
    pub changed: bool,
}

/// Formats parsed input into canonical Luma text.
#[must_use]
pub fn format_parsed(parsed: &Parsed) -> FormattedDocument {
    format_file_with_source(
        &parsed.file,
        parsed.source.as_str(),
        FormatOptions::default(),
    )
}

/// Parses and formats source text into canonical Luma text.
#[must_use]
pub fn format_str(file_id: FileId, name: &str, source: &str) -> ParsedFormatting {
    let parsed = parse_str(file_id, name, source);
    let formatted = format_parsed(&parsed);
    ParsedFormatting { parsed, formatted }
}

/// Parses and formats a source range into canonical Luma edits.
///
/// The formatter is whole-document canonical today. This helper therefore:
///
/// - validates UTF-8/source bounds for `range`
/// - expands the range to intersecting lines and then, when possible, to the
///   smallest containing syntax node span
/// - renders the entire document canonically
/// - returns minimal edits only when all intersecting changes stay within the
///   expanded range
/// - otherwise either falls back to a single whole-document replacement edit or
///   returns [`FormatRangeError::RequiresWholeDocument`], depending on `options`
///
/// # Errors
///
/// Returns [`FormatRangeError`] when `range` is out of bounds, is not aligned to
/// UTF-8 boundaries, or localized formatting would require a whole-document edit
/// and `options` rejects that fallback.
pub fn format_range_edits(
    file_id: FileId,
    name: &str,
    source: &str,
    range: TextRange,
    options: FormatRangeOptions,
) -> Result<(ParsedFormatting, Vec<TextEdit>), FormatRangeError> {
    let parsed = parse_str(file_id, name, source);
    let formatted = ParsedFormatting {
        formatted: format_file_with_source(&parsed.file, parsed.source.as_str(), options.format),
        parsed,
    };
    let edits = format_parsed_range_edits(&formatted.parsed, range, options)?;
    Ok((formatted, edits))
}

/// Computes canonical minimal replacement edits from `old` to `new`.
///
/// The current implementation returns either zero edits when unchanged or one
/// source-relative replacement edit. Single-line changes keep the smallest
/// replacement span; multi-line changes expand to whole line boundaries.
#[must_use]
pub fn minimal_text_edits(old: &str, new: &str) -> Vec<TextEdit> {
    if old == new {
        return Vec::new();
    }

    let prefix = common_prefix_len(old, new);
    let suffix = common_suffix_len(&old[prefix..], &new[prefix..]);

    let mut old_start = prefix;
    let mut old_end = old.len() - suffix;
    let mut new_start = prefix;
    let mut new_end = new.len() - suffix;

    let old_changed = &old[old_start..old_end];
    let new_changed = &new[new_start..new_end];
    if old_changed.contains('\n') || new_changed.contains('\n') {
        old_start = line_start(old, old_start);
        old_end = line_end(old, old_end);
        new_start = line_start(new, new_start);
        new_end = line_end(new, new_end);

        let old_suffix = &old[old_end..];
        let new_suffix = &new[new_end..];
        if let Some((old_line_ending, new_line_ending)) =
            shared_leading_line_ending_len(old_suffix, new_suffix)
        {
            old_end += old_line_ending;
            new_end += new_line_ending;
        }
    }

    let edit = TextEdit {
        range: TextRange::new(old_start, old_end),
        text: new[new_start..new_end].to_owned(),
    };

    if apply_text_edits(old, std::slice::from_ref(&edit)).as_deref() == Some(new) {
        return vec![edit];
    }

    vec![TextEdit {
        range: TextRange::new(line_start(old, prefix), old.len()),
        text: new[line_start(new, prefix)..].to_owned(),
    }]
}

/// Combined parse + format result intended for editor-style callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFormatting {
    /// Parse result.
    pub parsed: Parsed,
    /// Canonical formatting output.
    pub formatted: FormattedDocument,
}

impl ParsedFormatting {
    /// Returns canonical source-relative edits for rewriting the full document.
    #[must_use]
    pub fn text_edits_for_source(&self, source: &str) -> Vec<TextEdit> {
        minimal_text_edits(source, &self.formatted.text)
    }
}

/// Computes canonical edits for a requested source range.
///
/// # Errors
///
/// Returns [`FormatRangeError`] when `range` is invalid for the parsed source or
/// localized formatting would require a whole-document edit and `options`
/// rejects that fallback.
pub fn format_parsed_range_edits(
    parsed: &Parsed,
    range: TextRange,
    options: FormatRangeOptions,
) -> Result<Vec<TextEdit>, FormatRangeError> {
    validate_range(parsed.source.as_str(), range)?;

    let source = parsed.source.as_str();
    let expanded = expanded_format_range(parsed, range);
    let formatted = format_file_with_source(&parsed.file, source, options.format);
    let full_edits = minimal_text_edits(source, &formatted.text);

    let mut intersecting = Vec::new();
    let mut outside = false;
    for edit in &full_edits {
        if ranges_intersect(edit.range, expanded) {
            if !range_contains(expanded, edit.range) {
                outside = true;
            }
            intersecting.push(edit.clone());
        } else {
            outside = true;
        }
    }

    if intersecting.is_empty() {
        return Ok(Vec::new());
    }

    if outside {
        return match options.fallback {
            FormatRangeFallback::WholeDocument => Ok(vec![TextEdit {
                range: TextRange::new(0, source.len()),
                text: formatted.text,
            }]),
            FormatRangeFallback::Reject => Err(FormatRangeError::RequiresWholeDocument {
                requested: range,
                expanded,
            }),
        };
    }

    Ok(localized_minimal_text_edits(
        source,
        &formatted.text,
        expanded,
    ))
}

/// Formats a parsed file with explicit options.
#[must_use]
pub fn format_file(file: &LumaFile, options: FormatOptions) -> String {
    render_file(file, options)
}

fn format_file_with_source(
    file: &LumaFile,
    source: &str,
    options: FormatOptions,
) -> FormattedDocument {
    let text = render_file(file, options);
    FormattedDocument {
        changed: normalize_line_endings(source) != text,
        text,
    }
}

fn validate_range(source: &str, range: TextRange) -> Result<(), FormatRangeError> {
    if range.start > range.end || range.end > source.len() {
        return Err(FormatRangeError::InvalidRange(range));
    }
    if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
        return Err(FormatRangeError::NonBoundaryRange(range));
    }
    Ok(())
}

fn expanded_format_range(parsed: &Parsed, range: TextRange) -> TextRange {
    let source = &parsed.source.source;
    let mut expanded = source
        .expand_span_to_line_span(range.to_span(source.id))
        .map_or(range, TextRange::from_span);

    let index = parsed.syntax_index();
    let mut best = None::<(usize, usize, TextRange)>;
    for id in index.covering_span(expanded.to_span(source.id)) {
        let Some(node) = index.node(id) else {
            continue;
        };
        if matches!(node.kind, SyntaxKind::File | SyntaxKind::Document) {
            continue;
        }
        let Some(candidate) = source
            .expand_span_to_line_span(node.span)
            .map(TextRange::from_span)
        else {
            continue;
        };
        if !range_contains(candidate, expanded) {
            continue;
        }
        let score = (candidate.len(), node.span.len(), candidate);
        if best.is_none_or(|current| score < current) {
            best = Some(score);
        }
    }

    if let Some((_, _, candidate)) = best {
        expanded = candidate;
    }

    expanded
}

fn localized_minimal_text_edits(
    source: &str,
    formatted: &str,
    expanded: TextRange,
) -> Vec<TextEdit> {
    let suffix_len = source.len() - expanded.end;
    let new_end = formatted.len() - suffix_len;
    let old_slice = &source[expanded.start..expanded.end];
    let new_slice = &formatted[expanded.start..new_end];

    minimal_text_edits(old_slice, new_slice)
        .into_iter()
        .map(|edit| TextEdit {
            range: TextRange::new(
                edit.range.start + expanded.start,
                edit.range.end + expanded.start,
            ),
            text: edit.text,
        })
        .collect()
}

const fn ranges_intersect(left: TextRange, right: TextRange) -> bool {
    left.start < right.end && right.start < left.end
}

const fn range_contains(outer: TextRange, inner: TextRange) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

fn render_file(file: &LumaFile, options: FormatOptions) -> String {
    let mut out = String::new();
    let multiple_documents = file.documents.len() > 1;
    for (index, document) in file.documents.iter().enumerate() {
        if index > 0 || document.separator_span.is_some() || multiple_documents {
            out.push_str("---\n");
        }
        render_document(document, 0, &mut out, options);
        if options.preserve_document_terminators && document.terminator_span.is_some() {
            out.push_str("...\n");
        }
    }
    out
}

fn render_document(document: &Document, depth: usize, out: &mut String, options: FormatOptions) {
    for item in &document.items {
        render_document_item(item, depth, out, options);
    }
}

fn render_document_item(
    item: &DocumentItem,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    match item {
        DocumentItem::Directive(directive) => render_directive(directive, depth, out, options),
        DocumentItem::Let(binding) => render_let(binding, depth, out, options),
        DocumentItem::Root(node) => render_node(node, depth, out, options),
        DocumentItem::Comment(comment) => render_comment(comment, depth, out, options),
    }
}

fn render_node(node: &LumaNode, depth: usize, out: &mut String, options: FormatOptions) {
    match node {
        LumaNode::Null { .. } => push_line(depth, options, "null", out),
        LumaNode::Boolean { value, .. } => {
            push_line(depth, options, if *value { "true" } else { "false" }, out);
        }
        LumaNode::Number(number) => push_line(depth, options, &number.lexeme, out),
        LumaNode::String(string) => render_string_node(string, depth, out, options),
        LumaNode::Sequence(sequence) => render_sequence_block(sequence, depth, out, options),
        LumaNode::Mapping(mapping) => render_mapping_block(mapping, depth, out, options),
        LumaNode::Tagged(tagged) => {
            let tag = format!("!{}", tagged.tag.name.value);
            if let Some(value) = &tagged.value {
                if let Some(inline) = inline_node(value) {
                    push_line(depth, options, &format!("{tag} {inline}"), out);
                } else {
                    push_line(depth, options, &tag, out);
                    render_node(value, depth + 1, out, options);
                }
            } else {
                push_line(depth, options, &tag, out);
            }
        }
        LumaNode::LuaExpression(expression) => {
            push_line(
                depth,
                options,
                &format!("={}", expression.source.trim()),
                out,
            );
        }
        LumaNode::LuaExpressionBlock(expression) => {
            render_block_header(
                "|expr",
                expression.chomping.unwrap_or(BlockChomping::Clip),
                &expression.source,
                depth,
                out,
                options,
            );
        }
        LumaNode::LuaChunk(expression) => {
            render_block_header(
                "|lua",
                expression.chomping.unwrap_or(BlockChomping::Clip),
                &expression.source,
                depth,
                out,
                options,
            );
        }
        LumaNode::LuaTableConstructor(expression) => {
            push_line(depth, options, expression.source.trim(), out);
        }
    }
}

fn render_mapping_block(
    mapping: &MappingBlock,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    for item in &mapping.items {
        match item {
            MappingItem::Entry(entry) => render_mapping_entry(entry, depth, out, options),
            MappingItem::Spread(spread) => push_line(
                depth,
                options,
                &format!("...{}", spread.expression.source.trim()),
                out,
            ),
            MappingItem::Directive(directive) => render_directive(directive, depth, out, options),
            MappingItem::Conditional(block) => {
                render_mapping_conditional(block, depth, out, options);
            }
            MappingItem::Loop(block) => render_mapping_loop(block, depth, out, options),
            MappingItem::Let(binding) => render_let(binding, depth, out, options),
            MappingItem::Comment(comment) => render_comment(comment, depth, out, options),
        }
    }
}

fn render_mapping_entry(
    entry: &MappingEntry,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    let key = render_key(&entry.key);
    if let Some(inline) = inline_node(&entry.value) {
        push_line(depth, options, &format!("{key}: {inline}"), out);
    } else if render_prefixed_node(&format!("{key}:"), &entry.value, depth, out, options) {
        return;
    } else {
        push_line(depth, options, &format!("{key}:"), out);
        render_node(&entry.value, depth + 1, out, options);
    }
}

fn render_sequence_block(
    sequence: &SequenceBlock,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    for item in &sequence.items {
        match item {
            SequenceItem::Value(value) => {
                if let Some(inline) = inline_node(value) {
                    push_line(depth, options, &format!("- {inline}"), out);
                } else {
                    push_line(depth, options, "-", out);
                    render_node(value, depth + 1, out, options);
                }
            }
            SequenceItem::Spread(spread) => push_line(
                depth,
                options,
                &format!("- ...{}", spread.expression.source.trim()),
                out,
            ),
            SequenceItem::Directive(directive) => render_directive(directive, depth, out, options),
            SequenceItem::Conditional(block) => {
                render_sequence_conditional(block, depth, out, options);
            }
            SequenceItem::Loop(block) => render_sequence_loop(block, depth, out, options),
            SequenceItem::Comment(comment) => render_comment(comment, depth, out, options),
        }
    }
}

fn render_directive(directive: &Directive, depth: usize, out: &mut String, options: FormatOptions) {
    match directive {
        Directive::Version(version) => {
            push_line(depth, options, &format!("@luma {}", version.version), out);
        }
        Directive::Profile(profile) => push_line(
            depth,
            options,
            &format!("@profile {}", profile_name(&profile.profile)),
            out,
        ),
        Directive::Schema(schema) => push_line(
            depth,
            options,
            &format!("@schema {}", format_string_scalar(&schema.location.value)),
            out,
        ),
        Directive::Import(import) => push_line(
            depth,
            options,
            &format!(
                "@import {} as {}",
                format_string_scalar(&import.location.value),
                import.alias
            ),
            out,
        ),
        Directive::Include(include) => push_line(
            depth,
            options,
            &format!("@include {}", format_string_scalar(&include.location.value)),
            out,
        ),
        Directive::Use(use_directive) => push_line(
            depth,
            options,
            &format!("@use {} as {}", use_directive.module, use_directive.alias),
            out,
        ),
        Directive::LuaPrelude(prelude) => render_block_header(
            "@lua:",
            prelude.block.chomping.unwrap_or(BlockChomping::Clip),
            &prelude.block.source,
            depth,
            out,
            options,
        ),
        Directive::Meta(meta) => {
            push_line(depth, options, "@meta:", out);
            render_mapping_block(&meta.value, depth + 1, out, options);
        }
    }
}

fn render_let(binding: &LetBinding, depth: usize, out: &mut String, options: FormatOptions) {
    if let Some(inline) = inline_node(&binding.value) {
        push_line(
            depth,
            options,
            &format!("let {} = {inline}", binding.name),
            out,
        );
    } else if render_prefixed_node(
        &format!("let {}:", binding.name),
        &binding.value,
        depth,
        out,
        options,
    ) {
        return;
    } else {
        push_line(depth, options, &format!("let {}:", binding.name), out);
        render_node(&binding.value, depth + 1, out, options);
    }
}

fn render_comment(comment: &Comment, depth: usize, out: &mut String, options: FormatOptions) {
    match comment.kind {
        CommentKind::Line => push_line(depth, options, &format!("-- {}", comment.text.trim()), out),
        CommentKind::Block => {
            let body = comment.text.replace("\r\n", "\n").replace('\r', "\n");
            let indent = indent(depth, options);
            out.push_str(&indent);
            out.push_str("--[[");
            out.push_str(&body);
            out.push_str("]]\n");
        }
    }
}

fn render_mapping_conditional(
    block: &ConditionalBlock<MappingBlock>,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    push_line(
        depth,
        options,
        &format!("@if {}:", block.if_branch.condition.source.trim()),
        out,
    );
    render_mapping_block(&block.if_branch.body, depth + 1, out, options);
    for branch in &block.else_if_branches {
        push_line(
            depth,
            options,
            &format!("@elseif {}:", branch.condition.source.trim()),
            out,
        );
        render_mapping_block(&branch.body, depth + 1, out, options);
    }
    if let Some(ElseBranch { body, .. }) = &block.else_branch {
        push_line(depth, options, "@else:", out);
        render_mapping_block(body, depth + 1, out, options);
    }
}

fn render_sequence_conditional(
    block: &ConditionalBlock<SequenceBlock>,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    push_line(
        depth,
        options,
        &format!("@if {}:", block.if_branch.condition.source.trim()),
        out,
    );
    render_sequence_block(&block.if_branch.body, depth + 1, out, options);
    for branch in &block.else_if_branches {
        push_line(
            depth,
            options,
            &format!("@elseif {}:", branch.condition.source.trim()),
            out,
        );
        render_sequence_block(&branch.body, depth + 1, out, options);
    }
    if let Some(ElseBranch { body, .. }) = &block.else_branch {
        push_line(depth, options, "@else:", out);
        render_sequence_block(body, depth + 1, out, options);
    }
}

fn render_mapping_loop(
    block: &LoopBlock<MappingBlock>,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    push_line(
        depth,
        options,
        &format!(
            "@for {} in {}:",
            render_loop_bindings(&block.bindings),
            block.iterable.source.trim()
        ),
        out,
    );
    render_mapping_block(&block.body, depth + 1, out, options);
}

fn render_sequence_loop(
    block: &LoopBlock<SequenceBlock>,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    push_line(
        depth,
        options,
        &format!(
            "@for {} in {}:",
            render_loop_bindings(&block.bindings),
            block.iterable.source.trim()
        ),
        out,
    );
    render_sequence_block(&block.body, depth + 1, out, options);
}

fn render_loop_bindings(bindings: &LoopBindings) -> String {
    match bindings {
        LoopBindings::One { value, .. } => value.clone(),
        LoopBindings::Two { key, value, .. } => format!("{key}, {value}"),
    }
}

fn render_string_node(string: &StringNode, depth: usize, out: &mut String, options: FormatOptions) {
    if string.style == StringStyle::Block || string.value.contains('\n') {
        let header = match string.block_kind.unwrap_or(BlockKind::Literal) {
            BlockKind::Literal => "|",
            BlockKind::Folded => ">",
            BlockKind::LuaExpression => "|expr",
            BlockKind::LuaChunk => "|lua",
        };
        render_block_header(
            header,
            string.chomping.unwrap_or(BlockChomping::Clip),
            &string.value,
            depth,
            out,
            options,
        );
        return;
    }
    push_line(depth, options, &format_string_scalar(&string.value), out);
}

fn render_prefixed_node(
    prefix: &str,
    node: &LumaNode,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) -> bool {
    match node {
        LumaNode::String(string)
            if string.style == StringStyle::Block || string.value.contains('\n') =>
        {
            let header = match string.block_kind.unwrap_or(BlockKind::Literal) {
                BlockKind::Literal => "|",
                BlockKind::Folded => ">",
                BlockKind::LuaExpression => "|expr",
                BlockKind::LuaChunk => "|lua",
            };
            let suffix = match string.chomping.unwrap_or(BlockChomping::Clip) {
                BlockChomping::Clip => "",
                BlockChomping::Strip => "-",
                BlockChomping::Keep => "+",
            };
            push_line(depth, options, &format!("{prefix} {header}{suffix}"), out);
            write_block_body(
                &string.value,
                string.chomping.unwrap_or(BlockChomping::Clip),
                depth,
                out,
                options,
            );
            true
        }
        LumaNode::LuaExpressionBlock(expression) => {
            let suffix = match expression.chomping.unwrap_or(BlockChomping::Clip) {
                BlockChomping::Clip => "",
                BlockChomping::Strip => "-",
                BlockChomping::Keep => "+",
            };
            push_line(depth, options, &format!("{prefix} |expr{suffix}"), out);
            write_block_body(
                &expression.source,
                expression.chomping.unwrap_or(BlockChomping::Clip),
                depth,
                out,
                options,
            );
            true
        }
        LumaNode::LuaChunk(expression) => {
            let suffix = match expression.chomping.unwrap_or(BlockChomping::Clip) {
                BlockChomping::Clip => "",
                BlockChomping::Strip => "-",
                BlockChomping::Keep => "+",
            };
            push_line(depth, options, &format!("{prefix} |lua{suffix}"), out);
            write_block_body(
                &expression.source,
                expression.chomping.unwrap_or(BlockChomping::Clip),
                depth,
                out,
                options,
            );
            true
        }
        LumaNode::Tagged(tagged) => {
            let tag = format!("!{}", tagged.tag.name.value);
            if let Some(value) = &tagged.value {
                if let Some(inline) = inline_node(value) {
                    push_line(depth, options, &format!("{prefix} {tag} {inline}"), out);
                } else {
                    push_line(depth, options, &format!("{prefix} {tag}"), out);
                    render_node(value, depth + 1, out, options);
                }
            } else {
                push_line(depth, options, &format!("{prefix} {tag}"), out);
            }
            true
        }
        _ => false,
    }
}

fn render_block_header(
    prefix: &str,
    chomping: BlockChomping,
    body: &str,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    let suffix = match chomping {
        BlockChomping::Clip => "",
        BlockChomping::Strip => "-",
        BlockChomping::Keep => "+",
    };
    push_line(depth, options, &format!("{prefix}{suffix}"), out);
    write_block_body(body, chomping, depth, out, options);
}

fn write_block_body(
    body: &str,
    chomping: BlockChomping,
    depth: usize,
    out: &mut String,
    options: FormatOptions,
) {
    let normalized = normalize_line_endings(body);
    let content = match chomping {
        BlockChomping::Clip => normalized.strip_suffix('\n').unwrap_or(&normalized),
        BlockChomping::Strip | BlockChomping::Keep => normalized.trim_end_matches('\n'),
    };
    if content.is_empty() {
        return;
    }
    for line in content.split('\n') {
        let indent = indent(depth + 1, options);
        out.push_str(&indent);
        out.push_str(line);
        out.push('\n');
    }
}

fn render_key(key: &MappingKey) -> String {
    match key {
        MappingKey::Plain { value, .. } if is_plain_key(value) => value.clone(),
        MappingKey::Plain { value, .. } => format_string_scalar(value),
        MappingKey::Quoted(node) => format_string_scalar(&node.value),
        MappingKey::Expression { expression, .. } => format!("[={}]", expression.source.trim()),
    }
}

fn inline_node(node: &LumaNode) -> Option<String> {
    match node {
        LumaNode::Null { .. } => Some(String::from("null")),
        LumaNode::Boolean { value, .. } => Some(if *value {
            String::from("true")
        } else {
            String::from("false")
        }),
        LumaNode::Number(number) => Some(number.lexeme.clone()),
        LumaNode::String(string)
            if string.style != StringStyle::Block && !string.value.contains('\n') =>
        {
            Some(format_string_scalar(&string.value))
        }
        LumaNode::Tagged(tagged) => tagged
            .value
            .as_deref()
            .and_then(inline_node)
            .map(|value| format!("!{} {value}", tagged.tag.name.value))
            .or_else(|| Some(format!("!{}", tagged.tag.name.value))),
        LumaNode::LuaExpression(expression) => Some(format!("={}", expression.source.trim())),
        LumaNode::LuaTableConstructor(expression) => Some(expression.source.trim().to_owned()),
        _ => None,
    }
}

fn format_string_scalar(value: &str) -> String {
    if is_plain_scalar(value) {
        value.to_owned()
    } else {
        quote_double(value)
    }
}

fn quote_double(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn is_plain_key(value: &str) -> bool {
    is_plain_scalar(value)
        && !value.contains(':')
        && !value.starts_with(['[', '{', '}', ',', '|', '>', '='])
}

fn is_plain_scalar(value: &str) -> bool {
    if value.is_empty()
        || value != value.trim()
        || value.contains('\n')
        || value.starts_with(['-', '@', '!', '#'])
        || value.contains("--")
        || value.contains(':')
        || matches!(value, "null" | "nil" | "true" | "false")
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
    {
        return false;
    }
    true
}

fn profile_name(profile: &luma_syntax::LumaProfile) -> &str {
    match profile {
        luma_syntax::LumaProfile::Data => "data",
        luma_syntax::LumaProfile::Safe => "safe",
        luma_syntax::LumaProfile::Trusted => "trusted",
        luma_syntax::LumaProfile::Custom(value) => value,
    }
}

fn normalize_line_endings(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    let max = left.len().min(right.len());
    let mut matched = 0;

    while matched < max && left.as_bytes()[matched] == right.as_bytes()[matched] {
        matched += 1;
    }

    while matched > 0 && (!left.is_char_boundary(matched) || !right.is_char_boundary(matched)) {
        matched -= 1;
    }

    matched
}

fn common_suffix_len(left: &str, right: &str) -> usize {
    let max = left.len().min(right.len());
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut matched = 0;

    while matched < max {
        let left_index = left.len() - matched - 1;
        let right_index = right.len() - matched - 1;
        if left_bytes[left_index] != right_bytes[right_index] {
            break;
        }
        matched += 1;
    }

    while matched > 0
        && (!left.is_char_boundary(left.len() - matched)
            || !right.is_char_boundary(right.len() - matched))
    {
        matched -= 1;
    }

    matched
}

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn line_end(text: &str, offset: usize) -> usize {
    text[offset.min(text.len())..]
        .find('\n')
        .map_or(text.len(), |index| offset.min(text.len()) + index + 1)
}

fn shared_leading_line_ending_len(old: &str, new: &str) -> Option<(usize, usize)> {
    if old.starts_with("\r\n") && new.starts_with('\n') {
        Some((2, 1))
    } else if old.starts_with(['\n', '\r']) && new.starts_with('\n') {
        Some((1, 1))
    } else {
        None
    }
}

fn indent(depth: usize, options: FormatOptions) -> String {
    " ".repeat(depth.saturating_mul(options.indent_width))
}

fn push_line(depth: usize, options: FormatOptions, text: &str, out: &mut String) {
    out.push_str(&indent(depth, options));
    out.push_str(text);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use crate::{FileId, apply_text_edits, parse_str};

    use super::{
        FormatRangeError, FormatRangeFallback, FormatRangeOptions, TextRange, format_parsed,
        format_parsed_range_edits, format_range_edits, format_str, minimal_text_edits,
    };

    #[test]
    fn formatter_normalizes_line_endings_and_comments() {
        let parsed = parse_str(
            FileId(1),
            "fmt.luma",
            "root:\r\n  -- note\r\n  value: 'hello'\r\n",
        );
        let formatted = format_parsed(&parsed);
        assert_eq!(formatted.text, "root:\n  -- note\n  value: hello\n");
        assert!(formatted.changed);
    }

    #[test]
    fn unchanged_formatting_returns_no_text_edits() {
        let source = "root:\n  value: hello\n";
        let formatted = format_str(FileId(1), "stable.luma", source);

        assert!(!formatted.formatted.changed);
        assert!(formatted.text_edits_for_source(source).is_empty());
    }

    #[test]
    fn one_line_changes_produce_small_replacements() {
        let source = "root:\n  value: 'hello'\n";
        let formatted = format_str(FileId(1), "single-line.luma", source);
        let edits = formatted.text_edits_for_source(source);

        assert_eq!(edits.len(), 1);
        let quote_start = source.find("'hello'").unwrap();
        assert_eq!(edits[0].range, TextRange::new(quote_start, quote_start + 7));
        assert_eq!(edits[0].text, "hello");
        assert_eq!(
            apply_text_edits(source, &edits),
            Some(formatted.formatted.text)
        );
    }

    #[test]
    fn multi_line_changes_expand_to_line_boundaries() {
        let source = "root:\n    alpha: 1\r\n    beta: 'two'\n";
        let formatted = format_str(FileId(1), "multi-line.luma", source);
        let edits = formatted.text_edits_for_source(source);

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start, 6);
        assert!(source[..edits[0].range.start].ends_with('\n'));
        assert!(
            edits[0].range.end == source.len() || source[edits[0].range.end..].starts_with('\n')
        );
        assert_eq!(
            &source[edits[0].range.start..edits[0].range.end],
            "    alpha: 1\r\n    beta: 'two'\n"
        );
        assert_eq!(edits[0].text, "  alpha: 1\n  beta: two\n");
        assert_eq!(
            apply_text_edits(source, &edits),
            Some(formatted.formatted.text)
        );
    }

    #[test]
    fn direct_minimal_text_edits_apply_to_target_text() {
        let old = "a\n  b\n";
        let new = "a\n  c\n";
        let edits = minimal_text_edits(old, new);

        assert_eq!(apply_text_edits(old, &edits), Some(String::from(new)));
    }

    #[test]
    fn range_formatting_formats_dirty_scalar_line() {
        let source = "root:\n  value: 'hello'\n  stable: ok\n";
        let line_start = source.find("  value").unwrap();
        let range = TextRange::new(line_start + 10, line_start + 15);

        let (formatted, edits) = format_range_edits(
            FileId(1),
            "range-scalar.luma",
            source,
            range,
            FormatRangeOptions::default(),
        )
        .unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(&source[edits[0].range.start..edits[0].range.end], "'hello'");
        assert_eq!(edits[0].text, "hello");
        let applied = apply_text_edits(source, &edits).unwrap();
        assert_eq!(applied, "root:\n  value: hello\n  stable: ok\n");
        assert_eq!(formatted.formatted.text, applied);
    }

    #[test]
    fn range_formatting_expands_nested_block_ranges() {
        let source = "root:\n  child:\n      alpha: 1\n      beta: 'two'\n  stable: ok\n";
        let start = source.find("alpha").unwrap();
        let end = source.find("'two'").unwrap() + 5;
        let range = TextRange::new(start, end);

        let parsed = parse_str(FileId(1), "range-block.luma", source);
        let edits =
            format_parsed_range_edits(&parsed, range, FormatRangeOptions::default()).unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(
            apply_text_edits(source, &edits),
            Some(String::from(
                "root:\n  child:\n    alpha: 1\n    beta: two\n  stable: ok\n"
            ))
        );
    }

    #[test]
    fn unchanged_range_returns_no_edits_even_when_document_is_dirty_elsewhere() {
        let source = "root:\n  dirty: 'hello'\n  stable: ok\n";
        let line_start = source.find("  stable").unwrap();
        let range = TextRange::new(line_start, line_start + "  stable: ok".len());

        let edits = format_range_edits(
            FileId(1),
            "range-stable.luma",
            source,
            range,
            FormatRangeOptions::default(),
        )
        .unwrap()
        .1;

        assert!(edits.is_empty());
    }

    #[test]
    fn invalid_range_returns_typed_error() {
        let source = "name: demo\n";
        let err = format_range_edits(
            FileId(1),
            "invalid-range.luma",
            source,
            TextRange::new(5, source.len() + 1),
            FormatRangeOptions::default(),
        )
        .unwrap_err();

        assert_eq!(
            err,
            FormatRangeError::InvalidRange(TextRange::new(5, source.len() + 1))
        );
    }

    #[test]
    fn non_boundary_range_returns_typed_error() {
        let source = "name: café\n";
        let accent = source.find('é').unwrap();
        let err = format_range_edits(
            FileId(1),
            "non-boundary-range.luma",
            source,
            TextRange::new(accent, accent + 1),
            FormatRangeOptions::default(),
        )
        .unwrap_err();

        assert_eq!(
            err,
            FormatRangeError::NonBoundaryRange(TextRange::new(accent, accent + 1))
        );
    }

    #[test]
    fn range_formatting_falls_back_to_whole_document_edit() {
        let source = "---\nfirst: 'one'\n---\nsecond: 'two'\n";
        let start = source.find("'one'").unwrap();
        let end = start + 5;
        let range = TextRange::new(start, end);

        let edits = format_range_edits(
            FileId(1),
            "range-fallback.luma",
            source,
            range,
            FormatRangeOptions::default(),
        )
        .unwrap()
        .1;

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range, TextRange::new(0, source.len()));
        assert_eq!(edits[0].text, "---\nfirst: one\n---\nsecond: two\n");
        assert_eq!(
            apply_text_edits(source, &edits),
            Some(edits[0].text.clone())
        );
    }

    #[test]
    fn reject_fallback_reports_requires_whole_document() {
        let source = "---\nfirst: 'one'\n---\nsecond: 'two'\n";
        let start = source.find("'one'").unwrap();
        let end = start + 5;
        let range = TextRange::new(start, end);

        let err = format_range_edits(
            FileId(1),
            "range-fallback-reject.luma",
            source,
            range,
            FormatRangeOptions {
                fallback: FormatRangeFallback::Reject,
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(matches!(
            err,
            FormatRangeError::RequiresWholeDocument {
                requested,
                expanded: _
            } if requested == range
        ));
    }
}
