use clap::ValueEnum;
use luma_parser::{Diagnostic, LumaFile, LumaNode, LumaSource, Severity};
use luma_syntax::{
    ConditionalBlock, ConditionalBranch, Directive, Document, DocumentItem, LoopBindings,
    LoopBlock, LuaExpression, LumaProfile, MappingBlock, MappingItem, SequenceBlock, SequenceItem,
    Span,
};
#[cfg(feature = "eval")]
use luma_syntax::{LumaHostValue, LumaKey, LumaValue};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EmitKind {
    None,
    Ast,
    Value,
    Source,
}

pub struct CliError {
    pub message: String,
}

impl CliError {
    #[cfg(feature = "engine-omnilua")]
    pub fn from_diagnostic(diagnostic: Diagnostic) -> Self {
        Self {
            message: diagnostic.message,
        }
    }
}

pub struct CommandReport {
    pub command: &'static str,
    pub source: Option<LumaSource>,
    pub diagnostics: Vec<Diagnostic>,
    pub payload: Option<(&'static str, Value)>,
    pub human_text: Option<String>,
    pub success: Option<bool>,
}

impl CommandReport {
    #[cfg(not(feature = "engine-omnilua"))]
    pub fn diagnostic_only(
        command: &'static str,
        source: LumaSource,
        diagnostic: Diagnostic,
    ) -> Self {
        Self {
            command,
            source: Some(source),
            diagnostics: vec![diagnostic],
            payload: None,
            human_text: None,
            success: None,
        }
    }

    pub fn exit_code(&self) -> i32 {
        if let Some(success) = self.success {
            return i32::from(!success);
        }
        i32::from(
            self.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error),
        )
    }
}

#[cfg(not(feature = "engine-omnilua"))]
pub struct DiagnosticFactory;

#[cfg(not(feature = "engine-omnilua"))]
impl DiagnosticFactory {
    pub fn engine_unavailable() -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            luma_parser::DiagnosticCode::UnsafeOperation,
            Severity::Error,
        );
        diagnostic.message =
            "eval requires the luma-cli 'engine-omnilua' feature or an enabled evaluation backend"
                .to_owned();
        diagnostic
    }
}

pub fn write_report(report: &CommandReport, format: OutputFormat) {
    match format {
        OutputFormat::Human => write_human(report),
        OutputFormat::Json => write_json(report),
    }
}

fn write_human(report: &CommandReport) {
    for diagnostic in &report.diagnostics {
        if let Some(source) = &report.source {
            if let Some(span) = diagnostic.primary_span {
                let position = source.position(span.start);
                eprintln!(
                    "{} {}: {} at {}:{}:{}",
                    severity_label(diagnostic.severity),
                    diagnostic.code.code(),
                    diagnostic.message,
                    source.name,
                    position.line,
                    position.column,
                );
                continue;
            }
        }
        eprintln!(
            "{} {}: {}",
            severity_label(diagnostic.severity),
            diagnostic.code.code(),
            diagnostic.message
        );
    }

    if let Some((_, payload)) = &report.payload {
        println!(
            "{}",
            serde_json::to_string_pretty(payload).expect("json payload should serialize")
        );
    } else if let Some(text) = &report.human_text {
        print!("{text}");
    }
}

fn write_json(report: &CommandReport) {
    let mut body = serde_json::Map::new();
    body.insert(
        "command".to_owned(),
        Value::String(report.command.to_owned()),
    );
    body.insert("ok".to_owned(), Value::Bool(report.exit_code() == 0));
    body.insert(
        "diagnostics".to_owned(),
        Value::Array(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic_to_json(diagnostic, report.source.as_ref()))
                .collect(),
        ),
    );
    if let Some((label, payload)) = &report.payload {
        body.insert((*label).to_owned(), payload.clone());
    }
    if let Some(text) = &report.human_text {
        body.insert("text".to_owned(), Value::String(text.clone()));
    }
    println!("{}", Value::Object(body));
}

const fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

pub fn diagnostic_to_json(diagnostic: &Diagnostic, source: Option<&LumaSource>) -> Value {
    json!({
        "code": diagnostic.code.code(),
        "severity": severity_label(diagnostic.severity),
        "message": diagnostic.message,
        "primary_span": diagnostic.primary_span.map(|span| span_to_json(span, source)),
        "related_spans": diagnostic.related_spans.iter().map(|related| json!({
            "message": related.message,
            "span": span_to_json(related.span, source),
        })).collect::<Vec<_>>(),
        "notes": diagnostic.notes,
    })
}

pub fn ast_to_json(file: &LumaFile) -> Value {
    json!({
        "type": "file",
        "span": span_to_json(file.span, None),
        "documents": file.documents.iter().map(document_to_json).collect::<Vec<_>>(),
    })
}

#[cfg(feature = "eval")]
pub fn evaluated_document_to_json(document: &luma_eval::EvaluatedDocument) -> Value {
    json!({
        "value": value_to_json(&document.value),
        "metadata": {
            "version": document.metadata.version,
            "profile": document.metadata.profile.as_ref().map(profile_to_json),
            "schema": document.metadata.schema,
            "value": document.metadata.value.as_ref().map(value_to_json),
        }
    })
}

fn document_to_json(document: &Document) -> Value {
    json!({
        "type": "document",
        "span": span_to_json(document.span, None),
        "separator_span": document.separator_span.map(|span| span_to_json(span, None)),
        "terminator_span": document.terminator_span.map(|span| span_to_json(span, None)),
        "items": document.items.iter().map(document_item_to_json).collect::<Vec<_>>(),
    })
}

fn document_item_to_json(item: &DocumentItem) -> Value {
    match item {
        DocumentItem::Directive(directive) => {
            json!({"type":"directive","value": directive_to_json(directive)})
        }
        DocumentItem::Let(binding) => {
            json!({"type":"let","name": binding.name, "value": node_to_json(&binding.value), "span": span_to_json(binding.span, None)})
        }
        DocumentItem::Root(node) => json!({"type":"root","value": node_to_json(node)}),
        DocumentItem::Comment(comment) => {
            json!({"type":"comment","kind": format!("{:?}", comment.kind).to_lowercase(), "text": comment.text, "span": span_to_json(comment.span, None)})
        }
    }
}

fn directive_to_json(directive: &Directive) -> Value {
    match directive {
        Directive::Version(value) => {
            json!({"kind":"version","version":value.version,"span": span_to_json(value.span, None)})
        }
        Directive::Profile(value) => {
            json!({"kind":"profile","profile": profile_to_json(&value.profile),"span": span_to_json(value.span, None)})
        }
        Directive::Schema(value) => {
            json!({"kind":"schema","location": string_node_to_json(&value.location),"span": span_to_json(value.span, None)})
        }
        Directive::Import(value) => {
            json!({"kind":"import","location": string_node_to_json(&value.location),"alias": value.alias,"span": span_to_json(value.span, None)})
        }
        Directive::Include(value) => {
            json!({"kind":"include","location": string_node_to_json(&value.location),"span": span_to_json(value.span, None)})
        }
        Directive::Use(value) => {
            json!({"kind":"use","module": value.module,"alias": value.alias,"span": span_to_json(value.span, None)})
        }
        Directive::LuaPrelude(value) => {
            json!({"kind":"lua_prelude","block": lua_expression_to_json(&value.block),"span": span_to_json(value.span, None)})
        }
        Directive::Meta(value) => {
            json!({"kind":"meta","value": mapping_to_json(&value.value),"span": span_to_json(value.span, None)})
        }
    }
}

fn node_to_json(node: &LumaNode) -> Value {
    match node {
        LumaNode::Null { span } => json!({"type":"null","span": span_to_json(*span, None)}),
        LumaNode::Boolean { value, span } => {
            json!({"type":"boolean","value":value,"span": span_to_json(*span, None)})
        }
        LumaNode::Number(number) => {
            json!({"type":"number","lexeme":number.lexeme,"span": span_to_json(number.span, None)})
        }
        LumaNode::String(string) => string_node_to_json(string),
        LumaNode::Sequence(sequence) => sequence_to_json(sequence),
        LumaNode::Mapping(mapping) => mapping_to_json(mapping),
        LumaNode::Tagged(tagged) => {
            json!({"type":"tagged","tag": {"name": tagged.tag.name.value, "span": span_to_json(tagged.tag.span, None)}, "value": tagged.value.as_deref().map(node_to_json), "span": span_to_json(tagged.span, None)})
        }
        LumaNode::LuaExpression(expr) => {
            json!({"type":"lua_expression","value": lua_expression_to_json(expr)})
        }
        LumaNode::LuaExpressionBlock(expr) => {
            json!({"type":"lua_expression_block","value": lua_expression_to_json(expr)})
        }
        LumaNode::LuaChunk(expr) => {
            json!({"type":"lua_chunk","value": lua_expression_to_json(expr)})
        }
        LumaNode::LuaTableConstructor(expr) => {
            json!({"type":"lua_table_constructor","value": lua_expression_to_json(expr)})
        }
    }
}

fn string_node_to_json(string: &luma_parser::StringNode) -> Value {
    json!({
        "type": "string",
        "value": string.value,
        "source": string.source,
        "style": format!("{:?}", string.style).to_lowercase(),
        "block_kind": string.block_kind.map(|kind| format!("{kind:?}").to_lowercase()),
        "chomping": string.chomping.map(|kind| format!("{kind:?}").to_lowercase()),
        "span": span_to_json(string.span, None),
    })
}

fn lua_expression_to_json(expr: &LuaExpression) -> Value {
    json!({
        "source": expr.source,
        "block_kind": expr.block_kind.map(|kind| format!("{kind:?}").to_lowercase()),
        "chomping": expr.chomping.map(|kind| format!("{kind:?}").to_lowercase()),
        "span": span_to_json(expr.span, None),
    })
}

fn mapping_to_json(mapping: &MappingBlock) -> Value {
    json!({
        "type": "mapping",
        "span": span_to_json(mapping.span, None),
        "items": mapping.items.iter().map(mapping_item_to_json).collect::<Vec<_>>(),
        "duplicate_keys": mapping.duplicate_keys.iter().map(|duplicate| json!({
            "key": duplicate.key,
            "first_index": duplicate.first_index,
            "duplicate_index": duplicate.duplicate_index,
            "first_span": span_to_json(duplicate.first_span, None),
            "duplicate_span": span_to_json(duplicate.duplicate_span, None),
        })).collect::<Vec<_>>(),
    })
}

fn mapping_item_to_json(item: &MappingItem) -> Value {
    match item {
        MappingItem::Entry(entry) => {
            json!({"type":"entry","key": mapping_key_to_json(&entry.key),"value": node_to_json(&entry.value),"span": span_to_json(entry.span, None)})
        }
        MappingItem::Spread(spread) => {
            json!({"type":"spread","expression": lua_expression_to_json(&spread.expression),"span": span_to_json(spread.span, None)})
        }
        MappingItem::Directive(directive) => {
            json!({"type":"directive","value": directive_to_json(directive)})
        }
        MappingItem::Conditional(block) => {
            json!({"type":"conditional","value": conditional_mapping_to_json(block)})
        }
        MappingItem::Loop(block) => json!({"type":"loop","value": loop_mapping_to_json(block)}),
        MappingItem::Let(binding) => {
            json!({"type":"let","name": binding.name, "value": node_to_json(&binding.value), "span": span_to_json(binding.span, None)})
        }
        MappingItem::Comment(comment) => {
            json!({"type":"comment","text": comment.text,"span": span_to_json(comment.span, None)})
        }
    }
}

fn sequence_to_json(sequence: &SequenceBlock) -> Value {
    json!({
        "type": "sequence",
        "span": span_to_json(sequence.span, None),
        "items": sequence.items.iter().map(sequence_item_to_json).collect::<Vec<_>>(),
    })
}

fn sequence_item_to_json(item: &SequenceItem) -> Value {
    match item {
        SequenceItem::Value(value) => json!({"type":"value","value": node_to_json(value)}),
        SequenceItem::Spread(spread) => {
            json!({"type":"spread","expression": lua_expression_to_json(&spread.expression),"span": span_to_json(spread.span, None)})
        }
        SequenceItem::Directive(directive) => {
            json!({"type":"directive","value": directive_to_json(directive)})
        }
        SequenceItem::Conditional(block) => {
            json!({"type":"conditional","value": conditional_sequence_to_json(block)})
        }
        SequenceItem::Loop(block) => json!({"type":"loop","value": loop_sequence_to_json(block)}),
        SequenceItem::Comment(comment) => {
            json!({"type":"comment","text": comment.text,"span": span_to_json(comment.span, None)})
        }
    }
}

fn mapping_key_to_json(key: &luma_parser::MappingKey) -> Value {
    match key {
        luma_parser::MappingKey::Plain { value, span } => {
            json!({"type":"plain","value":value,"span": span_to_json(*span, None)})
        }
        luma_parser::MappingKey::Quoted(value) => {
            json!({"type":"quoted","value": string_node_to_json(value)})
        }
        luma_parser::MappingKey::Expression { expression, span } => {
            json!({"type":"expression","expression": lua_expression_to_json(expression),"span": span_to_json(*span, None)})
        }
    }
}

fn conditional_mapping_to_json(block: &ConditionalBlock<MappingBlock>) -> Value {
    json!({
        "if_branch": conditional_mapping_branch_to_json(&block.if_branch),
        "else_if_branches": block.else_if_branches.iter().map(conditional_mapping_branch_to_json).collect::<Vec<_>>(),
        "else_branch": block.else_branch.as_ref().map(|branch| json!({"body": mapping_to_json(&branch.body), "span": span_to_json(branch.span, None)})),
        "span": span_to_json(block.span, None),
    })
}

fn conditional_mapping_branch_to_json(branch: &ConditionalBranch<MappingBlock>) -> Value {
    json!({"condition": lua_expression_to_json(&branch.condition), "body": mapping_to_json(&branch.body), "span": span_to_json(branch.span, None)})
}

fn conditional_sequence_to_json(block: &ConditionalBlock<SequenceBlock>) -> Value {
    json!({
        "if_branch": conditional_sequence_branch_to_json(&block.if_branch),
        "else_if_branches": block.else_if_branches.iter().map(conditional_sequence_branch_to_json).collect::<Vec<_>>(),
        "else_branch": block.else_branch.as_ref().map(|branch| json!({"body": sequence_to_json(&branch.body), "span": span_to_json(branch.span, None)})),
        "span": span_to_json(block.span, None),
    })
}

fn conditional_sequence_branch_to_json(branch: &ConditionalBranch<SequenceBlock>) -> Value {
    json!({"condition": lua_expression_to_json(&branch.condition), "body": sequence_to_json(&branch.body), "span": span_to_json(branch.span, None)})
}

fn loop_mapping_to_json(block: &LoopBlock<MappingBlock>) -> Value {
    json!({"bindings": loop_bindings_to_json(&block.bindings), "iterable": lua_expression_to_json(&block.iterable), "body": mapping_to_json(&block.body), "span": span_to_json(block.span, None)})
}

fn loop_sequence_to_json(block: &LoopBlock<SequenceBlock>) -> Value {
    json!({"bindings": loop_bindings_to_json(&block.bindings), "iterable": lua_expression_to_json(&block.iterable), "body": sequence_to_json(&block.body), "span": span_to_json(block.span, None)})
}

fn loop_bindings_to_json(bindings: &LoopBindings) -> Value {
    match bindings {
        LoopBindings::One { value } => json!({"type":"one","value": value}),
        LoopBindings::Two { key, value } => json!({"type":"two","key": key, "value": value}),
    }
}

fn profile_to_json(profile: &LumaProfile) -> Value {
    match profile {
        LumaProfile::Data => Value::String("data".to_owned()),
        LumaProfile::Safe => Value::String("safe".to_owned()),
        LumaProfile::Trusted => Value::String("trusted".to_owned()),
        LumaProfile::Custom(value) => Value::String(value.clone()),
    }
}

#[cfg(feature = "eval")]
pub fn value_to_json(value: &LumaValue) -> Value {
    match value {
        LumaValue::Null(_) => Value::Null,
        LumaValue::Boolean(value) => Value::Bool(*value),
        LumaValue::Number(number) => match number {
            luma_syntax::LumaNumber::Integer(value) => json!({"type":"integer","value": value}),
            luma_syntax::LumaNumber::Float(value) => json!({"type":"float","value": value}),
        },
        LumaValue::String(value) => Value::String(value.clone()),
        LumaValue::Sequence(sequence) => {
            json!({"type":"sequence","items": sequence.items.iter().map(value_to_json).collect::<Vec<_>>() })
        }
        LumaValue::Mapping(mapping) => {
            json!({"type":"mapping","entries": mapping.entries.iter().map(|entry| json!({"key": key_to_json(&entry.key), "value": value_to_json(&entry.value)})).collect::<Vec<_>>() })
        }
        LumaValue::Tagged(tagged) => {
            json!({"type":"tagged","tag": tagged.tag.name.value, "value": value_to_json(&tagged.value)})
        }
        LumaValue::Function(value) => host_value_json("function", value),
        LumaValue::UserData(value) => host_value_json("userdata", value),
        LumaValue::HostObject(value) => host_value_json("host_object", value),
    }
}

#[cfg(feature = "eval")]
fn key_to_json(key: &LumaKey) -> Value {
    match key {
        LumaKey::String(value) => json!({"type":"string","value": value}),
        LumaKey::Number(number) => {
            json!({"type":"number","value": value_to_json(&LumaValue::Number(number.clone()))})
        }
        LumaKey::Boolean(value) => json!({"type":"boolean","value": value}),
        LumaKey::Host(value) => host_value_json("host", value),
    }
}

#[cfg(feature = "eval")]
fn host_value_json(kind: &str, value: &LumaHostValue) -> Value {
    json!({"type": kind, "kind": value.kind, "label": value.label})
}

pub fn span_to_json(span: Span, source: Option<&LumaSource>) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("file_id".to_owned(), json!(span.file_id.0));
    value.insert("start".to_owned(), json!(span.start));
    value.insert("end".to_owned(), json!(span.end));
    if let Some(source) = source {
        let start = source.position(span.start);
        let end = source.position(span.end);
        value.insert(
            "source".to_owned(),
            json!({
                "name": &*source.name,
                "start": {"line": start.line, "column": start.column},
                "end": {"line": end.line, "column": end.column},
            }),
        );
    }
    Value::Object(value)
}
