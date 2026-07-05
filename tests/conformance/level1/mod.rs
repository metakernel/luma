use luma_parser::parse_str;
use luma_syntax::{Directive, DocumentItem, FileId, LumaFile, LumaNode, MappingItem, SequenceItem};

#[test]
fn level1_parses_static_sections_13_to_29_into_ast_snapshots() {
    let source = r#"@luma 0.1
@profile safe
@schema "./schemas/service.schema.luma"
@import "./common.luma" as common
@include "./base.luma"
@use std.text as text
@lua:
  return {
    trim = function(x)
      return x
    end,
  }
@meta:
  title: "Example"
  generated: false
let defaults:
  retries: 3
  timeout_ms: 1000
--[[block
comment]]
service: !Service
  description: >
    This becomes a paragraph
    with folded lines.

    And a new paragraph.
  script: |lua-
    return function()
      return true
    end
  record: |expr
    make_record({ id = "example" })
  dynamic: =defaults.timeout_ms
  point: { x = 12, y = 4 }
  [=text_key]: !Date "2026-01-01"
  ...common.service
  @if environment == "production":
    debug: false
  @elseif environment == "staging":
    debug: maybe
  @else:
    debug: true
  @for name, code in status_codes:
    [=name]: =code
pipeline:
  @include "./steps.luma"
  - build
  - ...common.steps
  @if include_search:
    - search
  @else:
    - basic
  @for name in names:
    - id: =name
"#;

    let parsed = parse_str(FileId(1), "level1-static.luma", source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(snapshot(&parsed.file), expected_static_snapshot());
}

#[test]
fn level1_parses_multiple_documents_and_comments() {
    let source = r#"---
@meta:
  title: first
root: one
...
---
-- line comment
value: !Tag
  nested: two
"#;
    let parsed = parse_str(FileId(2), "stream.luma", source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(snapshot(&parsed.file), expected_stream_snapshot());
}

fn expected_static_snapshot() -> &'static str {
    "doc\n  directive version 0.1\n  directive profile safe\n  directive schema ./schemas/service.schema.luma\n  directive import ./common.luma as common\n  directive include ./base.luma\n  directive use std.text as text\n  directive lua return {\\n  trim = function(x)\\n    return x\\n  end,\\n}\\n\n  directive meta\n    entry title\n      string(\"Example\")\n    entry generated\n      bool(false)\n  let defaults\n    mapping\n      entry retries\n        number(3)\n      entry timeout_ms\n        number(1000)\n  comment block \"block\\ncomment\"\n  root\n    mapping\n      entry service\n        tagged(!Service)\n          mapping\n            entry description\n              block-folded(\"This becomes a paragraph with folded lines.\\n\\nAnd a new paragraph.\\n\")\n            entry script\n              lua-chunk(\"return function()\\n  return true\\nend\")\n            entry record\n              lua-expr-block(\"make_record({ id = \\\"example\\\" })\\n\")\n            entry dynamic\n              lua-expr(\"defaults.timeout_ms\")\n            entry point\n              lua-table(\"{ x = 12, y = 4 }\")\n            entry [expr:text_key]\n              tagged(!Date)\n                string(\"2026-01-01\")\n            spread common.service\n            conditional\n              if environment == \"production\"\n                mapping\n                  entry debug\n                    bool(false)\n              elseif environment == \"staging\"\n                mapping\n                  entry debug\n                    string(\"maybe\")\n              else\n                mapping\n                  entry debug\n                    bool(true)\n            loop key=name value=code in status_codes\n              entry [expr:name]\n                lua-expr(\"code\")\n      entry pipeline\n        sequence\n          directive include ./steps.luma\n          value\n            string(\"build\")\n          spread common.steps\n          conditional\n            if include_search\n              sequence\n                value\n                  string(\"search\")\n            else\n              sequence\n                value\n                  string(\"basic\")\n          loop value=name in names\n            value\n              mapping\n                entry id\n                  lua-expr(\"name\")\n"
}

fn expected_stream_snapshot() -> &'static str {
    "doc separator\n  directive meta\n    entry title\n      string(\"first\")\n  root\n    mapping\n      entry root\n        string(\"one\")\n  terminator\ndoc separator\n  comment line \"line comment\"\n  root\n    mapping\n      entry value\n        tagged(!Tag)\n          mapping\n            entry nested\n              string(\"two\")\n"
}

fn snapshot(file: &LumaFile) -> String {
    let mut out = String::new();
    for document in &file.documents {
        out.push_str("doc");
        if document.separator_span.is_some() {
            out.push_str(" separator");
        }
        out.push('\n');
        for item in &document.items {
            render_document_item(item, 1, &mut out);
        }
        if document.terminator_span.is_some() {
            indent(1, &mut out);
            out.push_str("terminator\n");
        }
    }
    out
}

fn render_document_item(item: &DocumentItem, depth: usize, out: &mut String) {
    match item {
        DocumentItem::Directive(d) => render_directive(d, depth, out),
        DocumentItem::Let(binding) => {
            indent(depth, out);
            out.push_str(&format!("let {}\n", binding.name));
            render_node(&binding.value, depth + 1, out);
        }
        DocumentItem::Root(node) => {
            indent(depth, out);
            out.push_str("root\n");
            render_node(node, depth + 1, out);
        }
        DocumentItem::Comment(comment) => {
            indent(depth, out);
            out.push_str(&format!(
                "comment {} {:?}\n",
                kind_name_comment(comment),
                comment.text
            ));
        }
    }
}

fn render_node(node: &LumaNode, depth: usize, out: &mut String) {
    match node {
        LumaNode::Null { .. } => line(depth, "null", out),
        LumaNode::Boolean { value, .. } => line(depth, &format!("bool({value})"), out),
        LumaNode::Number(number) => line(depth, &format!("number({})", number.lexeme), out),
        LumaNode::String(string) => {
            let kind = match string.block_kind {
                Some(luma_syntax::BlockKind::Folded) => "block-folded",
                Some(luma_syntax::BlockKind::Literal) => "block-literal",
                _ => "string",
            };
            line(depth, &format!("{kind}({:?})", string.value), out);
        }
        LumaNode::Sequence(sequence) => {
            line(depth, "sequence", out);
            for item in &sequence.items {
                match item {
                    SequenceItem::Value(value) => {
                        line(depth + 1, "value", out);
                        render_node(value, depth + 2, out);
                    }
                    SequenceItem::Spread(spread) => line(
                        depth + 1,
                        &format!("spread {}", spread.expression.source),
                        out,
                    ),
                    SequenceItem::Directive(d) => render_directive(d, depth + 1, out),
                    SequenceItem::Conditional(block) => {
                        render_sequence_conditional(block, depth + 1, out)
                    }
                    SequenceItem::Loop(block) => {
                        line(
                            depth + 1,
                            &format!("loop {} in {}", loop_bindings(block), block.iterable.source),
                            out,
                        );
                        render_sequence_items(&block.body.items, depth + 2, out);
                    }
                    SequenceItem::Comment(comment) => line(
                        depth + 1,
                        &format!("comment {} {:?}", kind_name_comment(comment), comment.text),
                        out,
                    ),
                }
            }
        }
        LumaNode::Mapping(mapping) => {
            line(depth, "mapping", out);
            render_mapping_items(&mapping.items, depth + 1, out);
        }
        LumaNode::Tagged(tagged) => {
            line(depth, &format!("tagged(!{})", tagged.tag.name.value), out);
            if let Some(value) = &tagged.value {
                render_node(value, depth + 1, out);
            }
        }
        LumaNode::LuaExpression(expr) => line(depth, &format!("lua-expr({:?})", expr.source), out),
        LumaNode::LuaExpressionBlock(expr) => {
            line(depth, &format!("lua-expr-block({:?})", expr.source), out)
        }
        LumaNode::LuaChunk(expr) => line(depth, &format!("lua-chunk({:?})", expr.source), out),
        LumaNode::LuaTableConstructor(expr) => {
            line(depth, &format!("lua-table({:?})", expr.source), out)
        }
    }
}

fn render_mapping_items(items: &[MappingItem], depth: usize, out: &mut String) {
    for item in items {
        match item {
            MappingItem::Entry(entry) => {
                line(depth, &format!("entry {}", key_name(&entry.key)), out);
                render_node(&entry.value, depth + 1, out);
            }
            MappingItem::Spread(spread) => {
                line(depth, &format!("spread {}", spread.expression.source), out)
            }
            MappingItem::Directive(d) => render_directive(d, depth, out),
            MappingItem::Conditional(block) => render_mapping_conditional(block, depth, out),
            MappingItem::Loop(block) => {
                line(
                    depth,
                    &format!("loop {} in {}", loop_bindings(block), block.iterable.source),
                    out,
                );
                render_mapping_items(&block.body.items, depth + 1, out);
            }
            MappingItem::Let(binding) => {
                line(depth, &format!("let {}", binding.name), out);
                render_node(&binding.value, depth + 1, out);
            }
            MappingItem::Comment(comment) => line(
                depth,
                &format!("comment {} {:?}", kind_name_comment(comment), comment.text),
                out,
            ),
        }
    }
}

fn render_sequence_items(items: &[SequenceItem], depth: usize, out: &mut String) {
    for item in items {
        match item {
            SequenceItem::Value(value) => {
                line(depth, "value", out);
                render_node(value, depth + 1, out);
            }
            SequenceItem::Spread(spread) => {
                line(depth, &format!("spread {}", spread.expression.source), out)
            }
            SequenceItem::Directive(d) => render_directive(d, depth, out),
            SequenceItem::Conditional(block) => render_sequence_conditional(block, depth, out),
            SequenceItem::Loop(block) => {
                line(
                    depth,
                    &format!("loop {} in {}", loop_bindings(block), block.iterable.source),
                    out,
                );
                render_sequence_items(&block.body.items, depth + 1, out);
            }
            SequenceItem::Comment(comment) => line(
                depth,
                &format!("comment {} {:?}", kind_name_comment(comment), comment.text),
                out,
            ),
        }
    }
}

fn render_directive(directive: &Directive, depth: usize, out: &mut String) {
    match directive {
        Directive::Version(v) => line(depth, &format!("directive version {}", v.version), out),
        Directive::Profile(v) => line(
            depth,
            &format!("directive profile {:?}", v.profile).to_lowercase(),
            out,
        ),
        Directive::Schema(v) => line(
            depth,
            &format!("directive schema {}", v.location.value),
            out,
        ),
        Directive::Import(v) => line(
            depth,
            &format!("directive import {} as {}", v.location.value, v.alias),
            out,
        ),
        Directive::Include(v) => line(
            depth,
            &format!("directive include {}", v.location.value),
            out,
        ),
        Directive::Use(v) => line(
            depth,
            &format!("directive use {} as {}", v.module, v.alias),
            out,
        ),
        Directive::LuaPrelude(v) => line(
            depth,
            &format!("directive lua {}", v.block.source.replace('\n', "\\n")),
            out,
        ),
        Directive::Meta(v) => {
            line(depth, "directive meta", out);
            render_mapping_items(&v.value.items, depth + 1, out);
        }
    }
}

fn render_mapping_conditional(
    block: &luma_syntax::ConditionalBlock<luma_syntax::MappingBlock>,
    depth: usize,
    out: &mut String,
) {
    line(depth, "conditional", out);
    line(
        depth + 1,
        &format!("if {}", block.if_branch.condition.source),
        out,
    );
    line(depth + 2, "mapping", out);
    render_mapping_items(&block.if_branch.body.items, depth + 3, out);
    for branch in &block.else_if_branches {
        line(
            depth + 1,
            &format!("elseif {}", branch.condition.source),
            out,
        );
        line(depth + 2, "mapping", out);
        render_mapping_items(&branch.body.items, depth + 3, out);
    }
    if let Some(branch) = &block.else_branch {
        line(depth + 1, "else", out);
        line(depth + 2, "mapping", out);
        render_mapping_items(&branch.body.items, depth + 3, out);
    }
}

fn render_sequence_conditional(
    block: &luma_syntax::ConditionalBlock<luma_syntax::SequenceBlock>,
    depth: usize,
    out: &mut String,
) {
    line(depth, "conditional", out);
    line(
        depth + 1,
        &format!("if {}", block.if_branch.condition.source),
        out,
    );
    line(depth + 2, "sequence", out);
    render_sequence_items(&block.if_branch.body.items, depth + 3, out);
    for branch in &block.else_if_branches {
        line(
            depth + 1,
            &format!("elseif {}", branch.condition.source),
            out,
        );
        line(depth + 2, "sequence", out);
        render_sequence_items(&branch.body.items, depth + 3, out);
    }
    if let Some(branch) = &block.else_branch {
        line(depth + 1, "else", out);
        line(depth + 2, "sequence", out);
        render_sequence_items(&branch.body.items, depth + 3, out);
    }
}

fn key_name(key: &luma_syntax::MappingKey) -> String {
    match key {
        luma_syntax::MappingKey::Plain { value, .. } => value.clone(),
        luma_syntax::MappingKey::Quoted(node) => node.value.clone(),
        luma_syntax::MappingKey::Expression { expression, .. } => {
            format!("[expr:{}]", expression.source)
        }
    }
}

fn kind_name_comment(comment: &luma_syntax::Comment) -> &'static str {
    match comment.kind {
        luma_syntax::CommentKind::Line => "line",
        luma_syntax::CommentKind::Block => "block",
    }
}

fn loop_bindings<T>(block: &luma_syntax::LoopBlock<T>) -> String {
    match &block.bindings {
        luma_syntax::LoopBindings::One { value, .. } => format!("value={value}"),
        luma_syntax::LoopBindings::Two { key, value, .. } => format!("key={key} value={value}"),
    }
}

fn indent(depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
}

fn line(depth: usize, text: &str, out: &mut String) {
    indent(depth, out);
    out.push_str(text);
    out.push('\n');
}
