//! Canonical formatter for parsed Luma syntax trees.

use luma_syntax::{
    BlockChomping, BlockKind, Comment, CommentKind, ConditionalBlock, Directive, Document,
    DocumentItem, ElseBranch, LetBinding, LoopBindings, LoopBlock, LumaFile, LumaNode,
    MappingBlock, MappingEntry, MappingItem, MappingKey, SequenceBlock, SequenceItem, StringNode,
    StringStyle,
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

/// Combined parse + format result intended for editor-style callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFormatting {
    /// Parse result.
    pub parsed: Parsed,
    /// Canonical formatting output.
    pub formatted: FormattedDocument,
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
        LoopBindings::One { value } => value.clone(),
        LoopBindings::Two { key, value } => format!("{key}, {value}"),
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
    use crate::{FileId, parse_str};

    use super::format_parsed;

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
}
