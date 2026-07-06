use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "engine-omnilua")]
use lyma_engine_omnilua::OmniLuaEngine;
#[cfg(feature = "eval")]
use lyma_eval::{
    AstEvaluator, EvaluationOptions, EvaluationProfile, InMemoryModuleRegistry, InMemoryResolver,
    InMemoryTagResolver, ModuleRegistry, ResolverPolicy, ResourceResolver, UnknownTagPolicy,
};
use lyma_parser::parse_str;
#[cfg(feature = "eval")]
use lyma_runtime::{
    ConversionPolicy, Engine, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, LuaSourceText,
    RuntimeEnvironment, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule,
    RuntimeModuleFactory, RuntimeValueCodec,
};
use lyma_syntax::{
    Directive, DocumentItem, FileId, LymaFile, LymaMapping, LymaNode, LymaNumber, LymaValue,
    MappingItem, SequenceItem,
};

pub fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[allow(dead_code)]
pub fn fixtures_root() -> PathBuf {
    tests_root().join("fixtures")
}

pub fn run_level(level: &str) {
    if !filter_matches(level, &env_csv("LYMA_CONFORMANCE_LEVEL")) {
        return;
    }

    let root = tests_root();
    let fixtures = load_level_fixtures(&root, level);
    let mut report = LevelReport::default();

    for fixture in fixtures {
        if !fixture.matches_filters() {
            report.skipped += 1;
            continue;
        }
        fixture.run();
        report.passed += 1;
    }

    println!(
        "[conformance] {level}: passed={} skipped={}",
        report.passed, report.skipped
    );
}

#[derive(Default)]
struct LevelReport {
    passed: usize,
    skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Parse,
    Eval,
    Format,
    Serialize,
}

#[derive(Debug, Clone)]
struct FixtureMeta {
    title: String,
    mode: Mode,
    section: Vec<String>,
    profile: String,
    backend: String,
    features: Vec<String>,
    relaxed_limits: bool,
    max_instructions: Option<u64>,
    max_table_entries: Option<u64>,
}

#[derive(Debug, Clone)]
struct Fixture {
    level: String,
    name: String,
    source_path: PathBuf,
    meta: FixtureMeta,
    snapshot_root: PathBuf,
}

impl Fixture {
    fn matches_filters(&self) -> bool {
        filter_matches(&self.meta.backend, &env_csv("LYMA_CONFORMANCE_BACKEND"))
            && filter_matches(&self.meta.profile, &env_csv("LYMA_CONFORMANCE_PROFILE"))
            && (env_csv("LYMA_CONFORMANCE_SECTION").is_empty()
                || self.meta.section.iter().any(|section| {
                    env_csv("LYMA_CONFORMANCE_SECTION")
                        .iter()
                        .any(|wanted| section.contains(wanted) || wanted.contains(section))
                }))
            && self
                .meta
                .features
                .iter()
                .all(|feature| feature_enabled(feature))
    }

    fn run(&self) {
        let source = fs::read_to_string(&self.source_path).unwrap();
        match self.meta.mode {
            Mode::Parse => self.run_parse(&source),
            #[cfg(feature = "eval")]
            Mode::Eval => self.run_eval(&source),
            #[cfg(not(feature = "eval"))]
            Mode::Eval => panic!("eval fixtures require eval feature"),
            Mode::Format => self.run_format(&source),
            #[cfg(feature = "eval")]
            Mode::Serialize => self.run_serialize(&source),
            #[cfg(not(feature = "eval"))]
            Mode::Serialize => panic!("serialize fixtures require eval feature"),
        }
    }

    fn run_parse(&self, source: &str) {
        let parsed = parse_str(FileId(1), self.source_name(), source);
        let expected_diag = self.optional_snapshot("diag");
        if let Some(expected) = expected_diag {
            let actual = diagnostics_snapshot(&parsed.diagnostics);
            assert_diag_matches(self.label(), &expected, &actual);
            return;
        }
        assert!(
            parsed.diagnostics.is_empty(),
            "{} diagnostics: {:#?}",
            self.label(),
            parsed.diagnostics
        );
        let expected = self.required_snapshot("ast");
        let actual = ast_snapshot(&parsed.file);
        assert_eq!(actual, expected, "fixture {}", self.label());
    }

    fn run_format(&self, source: &str) {
        let formatted = lyma::tooling::format_document_edit(self.source_name(), source);
        assert!(
            formatted.parsed.diagnostics.is_empty(),
            "{} diagnostics: {:#?}",
            self.label(),
            formatted.parsed.diagnostics
        );
        assert_eq!(
            formatted.formatted.text,
            self.required_snapshot("fmt"),
            "fixture {}",
            self.label()
        );
    }

    #[cfg(feature = "eval")]
    fn run_eval(&self, source: &str) {
        let parsed = parse_str(FileId(1), self.source_name(), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse diagnostics: {:#?}",
            self.label(),
            parsed.diagnostics
        );
        let expected_diag = self.optional_snapshot("diag");
        match self.meta.backend.as_str() {
            "mock" => {
                let docs = match evaluate_with_mock(
                    source,
                    self.source_name(),
                    &parsed.file,
                    &self.meta,
                ) {
                    Ok(docs) => docs,
                    Err(error) => {
                        if let Some(expected) = expected_diag.as_ref() {
                            let actual = format!("{}\n", error.diagnostic.code.code());
                            assert_diag_matches(self.label(), expected, &actual);
                            return;
                        }
                        fail_eval_error(self.label(), error);
                    }
                };
                assert!(
                    expected_diag.is_none(),
                    "fixture {} expected diagnostics but evaluation succeeded",
                    self.label()
                );
                assert_eq!(
                    documents_json(&docs),
                    self.required_snapshot("json"),
                    "fixture {}",
                    self.label()
                );
            }
            "omnilua" => {
                #[cfg(feature = "engine-omnilua")]
                {
                    let docs = match evaluate_with_omnilua(
                        source,
                        self.source_name(),
                        &parsed.file,
                        &self.meta,
                    ) {
                        Ok(docs) => docs,
                        Err(error) => {
                            if let Some(expected) = expected_diag.as_ref() {
                                let actual = format!("{}\n", error.diagnostic.code.code());
                                assert_diag_matches(self.label(), expected, &actual);
                                return;
                            }
                            fail_eval_error(self.label(), error);
                        }
                    };
                    assert!(
                        expected_diag.is_none(),
                        "fixture {} expected diagnostics but evaluation succeeded",
                        self.label()
                    );
                    assert_eq!(
                        documents_json(&docs),
                        self.required_snapshot("json"),
                        "fixture {}",
                        self.label()
                    );
                }
                #[cfg(not(feature = "engine-omnilua"))]
                panic!(
                    "omnilua fixture compiled without engine-omnilua: {}",
                    self.label()
                );
            }
            backend => panic!("unsupported eval backend {backend} for {}", self.label()),
        }
    }

    #[cfg(feature = "eval")]
    fn run_serialize(&self, source: &str) {
        let parsed = parse_str(FileId(1), self.source_name(), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse diagnostics: {:#?}",
            self.label(),
            parsed.diagnostics
        );
        let docs = evaluate_with_mock(source, self.source_name(), &parsed.file, &self.meta)
            .unwrap_or_else(|error| fail_eval_error(self.label(), error));
        let [value] = docs.try_into().expect("single document");
        let serialized = lyma::tooling::serialize_portable_value(&value).unwrap();
        assert_eq!(
            serialized,
            self.required_snapshot("fmt"),
            "fixture {}",
            self.label()
        );
    }

    fn label(&self) -> String {
        format!("{}/{} ({})", self.level, self.name, self.meta.title)
    }

    fn source_name(&self) -> &str {
        self.source_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap()
    }

    fn required_snapshot(&self, ext: &str) -> String {
        fs::read_to_string(self.snapshot_root.with_extension(ext))
            .unwrap_or_else(|_| panic!("missing snapshot {} for {}", ext, self.label()))
    }

    fn optional_snapshot(&self, ext: &str) -> Option<String> {
        fs::read_to_string(self.snapshot_root.with_extension(ext)).ok()
    }
}

fn load_level_fixtures(root: &Path, level: &str) -> Vec<Fixture> {
    let conformance_root = root.join("conformance").join(level);
    let snapshot_root = root.join("snapshots").join(level);
    let mut fixtures = Vec::new();
    visit_dir(&conformance_root, &mut |path| {
        if path.extension().and_then(|v| v.to_str()) != Some("lyma") {
            return;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let meta_path = path.with_extension("meta");
        let meta = parse_meta(
            &fs::read_to_string(&meta_path)
                .unwrap_or_else(|_| panic!("missing meta for {}", path.display())),
        );
        fixtures.push(Fixture {
            level: String::from(level),
            name: name.clone(),
            source_path: path.to_path_buf(),
            meta,
            snapshot_root: snapshot_root.join(relative_snapshot_name(
                level,
                &conformance_root,
                path,
                &name,
            )),
        });
    });
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    fixtures
}

fn relative_snapshot_name(level: &str, root: &Path, path: &Path, name: &str) -> PathBuf {
    let relative = path.parent().unwrap().strip_prefix(root).unwrap();
    if relative.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        let _ = level;
        relative.join(name)
    }
}

fn visit_dir(dir: &Path, f: &mut impl FnMut(&Path)) {
    let mut entries = fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("missing fixture directory {}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            visit_dir(&entry, f);
        } else {
            f(&entry);
        }
    }
}

fn parse_meta(input: &str) -> FixtureMeta {
    let mut map = BTreeMap::<String, String>::new();
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let (key, value) = line.split_once(':').unwrap();
        map.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    FixtureMeta {
        title: map
            .remove("title")
            .unwrap_or_else(|| String::from("fixture")),
        mode: match map.remove("mode").as_deref() {
            Some("parse") => Mode::Parse,
            Some("eval") => Mode::Eval,
            Some("format") => Mode::Format,
            Some("serialize") => Mode::Serialize,
            other => panic!("unsupported mode: {other:?}"),
        },
        section: csv_value(map.remove("section").as_deref().unwrap_or("")),
        profile: map.remove("profile").unwrap_or_else(|| String::from("any")),
        backend: map
            .remove("backend")
            .unwrap_or_else(|| String::from("parse")),
        features: csv_value(map.remove("features").as_deref().unwrap_or("")),
        relaxed_limits: map.remove("relaxed_limits").as_deref() == Some("true"),
        max_instructions: map.remove("max_instructions").and_then(|v| v.parse().ok()),
        max_table_entries: map.remove("max_table_entries").and_then(|v| v.parse().ok()),
    }
}

fn csv_value(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .collect()
}

fn env_csv(name: &str) -> Vec<String> {
    std::env::var(name).map_or_else(|_| Vec::new(), |value| csv_value(&value))
}

fn filter_matches(actual: &str, filters: &[String]) -> bool {
    filters.is_empty()
        || filters
            .iter()
            .any(|wanted| actual.contains(wanted) || wanted.contains(actual))
}

fn feature_enabled(feature: &str) -> bool {
    (feature == "eval" && cfg!(feature = "eval"))
        || (feature == "engine-omnilua" && cfg!(feature = "engine-omnilua"))
}

fn diagnostics_snapshot(diagnostics: &[lyma_syntax::Diagnostic]) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        out.push_str(diagnostic.code.code());
        out.push('\n');
    }
    out
}

fn assert_diag_matches(label: String, expected: &str, actual: &str) {
    let expected = expected.trim();
    let actual = actual.trim();
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    assert_eq!(expected_lines.len(), actual_lines.len(), "fixture {label}");
    for (expected, actual) in expected_lines.into_iter().zip(actual_lines) {
        let accepted = expected.split('|').collect::<Vec<_>>();
        assert!(
            accepted.contains(&actual),
            "fixture {label}: expected one of {accepted:?}, got {actual}"
        );
    }
}

fn ast_snapshot(file: &LymaFile) -> String {
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
            line(1, "terminator", &mut out);
        }
    }
    out
}

fn render_document_item(item: &DocumentItem, depth: usize, out: &mut String) {
    match item {
        DocumentItem::Directive(d) => render_directive(d, depth, out),
        DocumentItem::Let(binding) => {
            line(depth, &format!("let {}", binding.name), out);
            render_node(&binding.value, depth + 1, out);
        }
        DocumentItem::Root(node) => {
            line(depth, "root", out);
            render_node(node, depth + 1, out);
        }
        DocumentItem::Comment(comment) => {
            line(
                depth,
                &format!("comment {} {:?}", comment_kind(comment), comment.text),
                out,
            );
        }
    }
}

fn render_node(node: &LymaNode, depth: usize, out: &mut String) {
    match node {
        LymaNode::Null { .. } => line(depth, "null", out),
        LymaNode::Boolean { value, .. } => line(depth, &format!("bool({value})"), out),
        LymaNode::Number(number) => line(depth, &format!("number({})", number.lexeme), out),
        LymaNode::String(string) => {
            let kind = match string.block_kind {
                Some(lyma_syntax::BlockKind::Folded) => "block-folded",
                Some(lyma_syntax::BlockKind::Literal) => "block-literal",
                Some(lyma_syntax::BlockKind::LuaExpression)
                | Some(lyma_syntax::BlockKind::LuaChunk) => "string",
                None => "string",
            };
            line(depth, &format!("{kind}({:?})", string.value), out);
        }
        LymaNode::Sequence(sequence) => {
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
                    SequenceItem::Comment(comment) => {
                        line(
                            depth + 1,
                            &format!("comment {} {:?}", comment_kind(comment), comment.text),
                            out,
                        );
                    }
                }
            }
        }
        LymaNode::Mapping(mapping) => {
            line(depth, "mapping", out);
            render_mapping_items(&mapping.items, depth + 1, out);
        }
        LymaNode::Tagged(tagged) => {
            line(depth, &format!("tagged(!{})", tagged.tag.name.value), out);
            if let Some(value) = &tagged.value {
                render_node(value, depth + 1, out);
            }
        }
        LymaNode::LuaExpression(expr) => line(depth, &format!("lua-expr({:?})", expr.source), out),
        LymaNode::LuaExpressionBlock(expr) => {
            line(depth, &format!("lua-expr-block({:?})", expr.source), out)
        }
        LymaNode::LuaChunk(expr) => line(depth, &format!("lua-chunk({:?})", expr.source), out),
        LymaNode::LuaTableConstructor(expr) => {
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
                &format!("comment {} {:?}", comment_kind(comment), comment.text),
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
                &format!("comment {} {:?}", comment_kind(comment), comment.text),
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
    block: &lyma_syntax::ConditionalBlock<lyma_syntax::MappingBlock>,
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
    block: &lyma_syntax::ConditionalBlock<lyma_syntax::SequenceBlock>,
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

fn key_name(key: &lyma_syntax::MappingKey) -> String {
    match key {
        lyma_syntax::MappingKey::Plain { value, .. } => value.clone(),
        lyma_syntax::MappingKey::Quoted(node) => node.value.clone(),
        lyma_syntax::MappingKey::Expression { expression, .. } => {
            format!("[expr:{}]", expression.source)
        }
    }
}

fn comment_kind(comment: &lyma_syntax::Comment) -> &'static str {
    match comment.kind {
        lyma_syntax::CommentKind::Line => "line",
        lyma_syntax::CommentKind::Block => "block",
    }
}

fn loop_bindings<T>(block: &lyma_syntax::LoopBlock<T>) -> String {
    match &block.bindings {
        lyma_syntax::LoopBindings::One { value, .. } => format!("value={value}"),
        lyma_syntax::LoopBindings::Two { key, value, .. } => format!("key={key} value={value}"),
    }
}

fn line(depth: usize, text: &str, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(text);
    out.push('\n');
}

#[cfg(feature = "eval")]
fn documents_json(docs: &[LymaValue]) -> String {
    match docs {
        [single] => {
            let mut out = json_value(single);
            out.push('\n');
            out
        }
        many => {
            let mut out = String::from("[");
            for (index, value) in many.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&json_value(value));
            }
            out.push_str("]\n");
            out
        }
    }
}

#[cfg(feature = "eval")]
fn json_value(value: &LymaValue) -> String {
    match value {
        LymaValue::Null(_) => String::from("null"),
        LymaValue::Boolean(value) => value.to_string(),
        LymaValue::Number(LymaNumber::Integer(value)) => value.to_string(),
        LymaValue::Number(LymaNumber::Float(value)) => value.to_string(),
        LymaValue::String(value) => format!("\"{}\"", escape_json(value)),
        LymaValue::Sequence(sequence) => {
            let items = sequence
                .items
                .iter()
                .map(json_value)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{items}]")
        }
        LymaValue::Mapping(mapping) => {
            let items = mapping
                .entries
                .iter()
                .map(|entry| {
                    format!(
                        "\"{}\":{}",
                        escape_json(&key_to_string(&entry.key)),
                        json_value(&entry.value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{items}}}")
        }
        LymaValue::Tagged(tagged) => format!(
            "{{\"tag\":\"{}\",\"value\":{}}}",
            escape_json(&tagged.tag.name.value),
            json_value(&tagged.value)
        ),
        LymaValue::Function(host) | LymaValue::UserData(host) | LymaValue::HostObject(host) => {
            format!(
                "\"<{}:{}>\"",
                host.kind,
                host.label.as_deref().unwrap_or("anon")
            )
        }
    }
}

#[cfg(feature = "eval")]
fn key_to_string(key: &lyma_syntax::LymaKey) -> String {
    match key {
        lyma_syntax::LymaKey::String(value) => value.clone(),
        lyma_syntax::LymaKey::Number(LymaNumber::Integer(value)) => value.to_string(),
        lyma_syntax::LymaKey::Number(LymaNumber::Float(value)) => value.to_string(),
        lyma_syntax::LymaKey::Boolean(value) => value.to_string(),
        lyma_syntax::LymaKey::Host(host) => {
            format!("{}:{}", host.kind, host.label.as_deref().unwrap_or("anon"))
        }
    }
}

#[cfg(feature = "eval")]
fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(feature = "eval")]
fn fail_eval_error(label: String, error: lyma_eval::EvaluationError) -> ! {
    panic!(
        "fixture {label}: {} {}",
        error.diagnostic.code.code(),
        error.diagnostic.message
    )
}

#[cfg(feature = "eval")]
fn evaluate_with_mock(
    _source: &str,
    source_name: &str,
    file: &LymaFile,
    meta: &FixtureMeta,
) -> Result<Vec<LymaValue>, lyma_eval::EvaluationError> {
    let resolver = shared_resolver();
    let modules = shared_modules();
    let tags = shared_tags();
    let profile = mock_profile(meta);
    AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &profile,
            resolver: Some(&resolver as &dyn ResourceResolver),
            module_registry: Some(&modules as &dyn ModuleRegistry<MockEngine>),
            tag_resolver: Some(&tags),
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        },
    }
    .evaluate_file(file, source_name, None)
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
fn evaluate_with_omnilua(
    _source: &str,
    source_name: &str,
    file: &LymaFile,
    meta: &FixtureMeta,
) -> Result<Vec<LymaValue>, lyma_eval::EvaluationError> {
    let resolver = shared_resolver();
    let modules = shared_modules();
    let tags = shared_tags();
    let profile = omnilua_profile(meta);
    let engine = OmniLuaEngine::default();
    AstEvaluator {
        engine: &engine,
        options: EvaluationOptions {
            profile: &profile,
            resolver: Some(&resolver as &dyn ResourceResolver),
            module_registry: Some(&modules as &dyn ModuleRegistry<OmniLuaEngine>),
            tag_resolver: Some(&tags),
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        },
    }
    .evaluate_file(file, source_name, None)
}

#[cfg(feature = "eval")]
fn shared_resolver() -> InMemoryResolver {
    InMemoryResolver::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_resource("common", "answer: 42\nitems:\n  - alpha\n  - beta\n")
    .with_resource("base", "base: true\n")
    .with_resource(
        "schemas/example",
        "@profile data\ntype: object\nrequired:\n  id: string\n  enabled: boolean\noptional:\n  names:\n    type: array\n    items: string\n",
    )
}

#[cfg(feature = "eval")]
fn shared_modules() -> InMemoryModuleRegistry {
    InMemoryModuleRegistry::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_module(
        "safe.mod",
        vec![(String::from("enabled"), LymaValue::Boolean(true))],
    )
}

#[cfg(feature = "eval")]
fn shared_tags() -> InMemoryTagResolver {
    InMemoryTagResolver::new().with_handler("upper", |value| match value {
        LymaValue::String(value) => Ok(LymaValue::String(value.to_uppercase())),
        other => Ok(other.clone()),
    })
}

#[cfg(feature = "eval")]
fn mock_profile(meta: &FixtureMeta) -> EvaluationProfile {
    let mut profile = EvaluationProfile::restricted();
    if meta.relaxed_limits {
        profile.runtime_limits.max_instructions = None;
        profile.runtime_limits.max_call_depth = None;
        profile.runtime_limits.max_memory_bytes = None;
        profile.runtime_limits.max_runtime_millis = None;
    }
    if let Some(value) = meta.max_instructions {
        profile.runtime_limits.max_instructions = Some(value);
    }
    if let Some(value) = meta.max_table_entries {
        profile.runtime_limits.max_table_entries = Some(usize::try_from(value).unwrap());
    }
    profile
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
fn omnilua_profile(meta: &FixtureMeta) -> EvaluationProfile {
    let mut profile = EvaluationProfile::restricted();
    if meta.relaxed_limits {
        profile.runtime_limits.max_instructions = None;
        profile.runtime_limits.max_call_depth = None;
        profile.runtime_limits.max_memory_bytes = None;
        profile.runtime_limits.max_runtime_millis = None;
    }
    if let Some(value) = meta.max_instructions {
        profile.runtime_limits.max_instructions = Some(value);
    }
    if let Some(value) = meta.max_table_entries {
        profile.runtime_limits.max_table_entries = Some(usize::try_from(value).unwrap());
    }
    profile
}

#[cfg(feature = "eval")]
#[derive(Debug, Clone, PartialEq)]
struct MockValue(LymaValue);

#[cfg(feature = "eval")]
#[derive(Debug, Clone, PartialEq)]
struct MockModule {
    name: String,
    exports: Vec<(String, MockValue)>,
}

#[cfg(feature = "eval")]
impl RuntimeModule for MockModule {
    type RuntimeValue = MockValue;

    fn module_name(&self) -> &str {
        &self.name
    }

    fn exports(&self) -> Result<Vec<(String, Self::RuntimeValue)>, LuaRuntimeError> {
        Ok(self.exports.clone())
    }
}

#[cfg(feature = "eval")]
#[derive(Debug, Clone, Default, PartialEq)]
struct MockEnvironment {
    context: BTreeMap<String, MockValue>,
    modules: BTreeMap<String, MockModule>,
}

#[cfg(feature = "eval")]
impl RuntimeEnvironment for MockEnvironment {
    type RuntimeValue = MockValue;
    type RuntimeModule = MockModule;

    fn fork_isolated(&self) -> Result<Self, LuaRuntimeError> {
        Ok(self.clone())
    }

    fn inject_builtin(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError> {
        self.context.insert(name.into(), value);
        Ok(())
    }

    fn inject_context(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError> {
        self.context.insert(name.into(), value);
        Ok(())
    }

    fn inject_module(&mut self, module: Self::RuntimeModule) -> Result<(), LuaRuntimeError> {
        self.modules.insert(module.name.clone(), module);
        Ok(())
    }
}

#[cfg(feature = "eval")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct MockEngine;

#[cfg(feature = "eval")]
impl Engine for MockEngine {
    fn engine_name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(feature = "eval")]
impl RuntimeEnvironmentFactory for MockEngine {
    type RuntimeValue = MockValue;
    type RuntimeModule = MockModule;
    type Environment = MockEnvironment;

    fn create_environment(&self) -> Result<Self::Environment, LuaRuntimeError> {
        Ok(MockEnvironment::default())
    }
}

#[cfg(feature = "eval")]
impl RuntimeModuleFactory for MockEngine {
    type RuntimeValue = MockValue;
    type Module = MockModule;

    fn create_module(
        &self,
        name: impl Into<String>,
        exports: Vec<(String, Self::RuntimeValue)>,
    ) -> Result<Self::Module, LuaRuntimeError> {
        Ok(MockModule {
            name: name.into(),
            exports,
        })
    }
}

#[cfg(feature = "eval")]
impl RuntimeValueCodec for MockEngine {
    type Value = MockValue;
    type FrozenValue = MockValue;

    fn to_lyma_value(
        &self,
        value: &Self::Value,
        _policy: &ConversionPolicy,
    ) -> Result<LymaValue, LuaRuntimeError> {
        Ok(value.0.clone())
    }

    fn from_lyma_value(&self, value: &LymaValue) -> Result<Self::Value, LuaRuntimeError> {
        Ok(MockValue(value.clone()))
    }

    fn freeze_value(&self, value: &Self::Value) -> Result<Self::FrozenValue, LuaRuntimeError> {
        Ok(value.clone())
    }

    fn clone_value(&self, value: &Self::Value) -> Result<Self::Value, LuaRuntimeError> {
        Ok(value.clone())
    }

    fn thaw_value(&self, value: &Self::FrozenValue) -> Result<Self::Value, LuaRuntimeError> {
        Ok(value.clone())
    }
}

#[cfg(feature = "eval")]
impl LuaRuntimeEngine for MockEngine {
    type CompiledExpression = String;
    type CompiledChunk = String;

    fn compile_expression(
        &self,
        source: LuaSourceText<'_>,
        _limits: &RuntimeLimits,
    ) -> Result<Self::CompiledExpression, LuaRuntimeError> {
        Ok(source.text.trim().to_owned())
    }

    fn compile_chunk(
        &self,
        source: LuaSourceText<'_>,
        _limits: &RuntimeLimits,
    ) -> Result<Self::CompiledChunk, LuaRuntimeError> {
        Ok(source.text.trim().to_owned())
    }

    fn evaluate_expression(
        &self,
        compiled: &Self::CompiledExpression,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        if limits.max_instructions == Some(0) {
            return Err(LuaRuntimeError::limit_exceeded(
                self.engine_name(),
                lyma_runtime::RuntimeLimitKind::Instructions,
                None,
            ));
        }
        Ok(MockValue(eval_mock_expression(compiled, environment)?))
    }

    fn evaluate_chunk(
        &self,
        compiled: &Self::CompiledChunk,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        if limits.max_instructions == Some(0) {
            return Err(LuaRuntimeError::limit_exceeded(
                self.engine_name(),
                lyma_runtime::RuntimeLimitKind::Instructions,
                None,
            ));
        }
        let source = compiled
            .trim()
            .strip_prefix("return ")
            .unwrap_or(compiled.trim());
        Ok(MockValue(eval_mock_expression(source, environment)?))
    }
}

#[cfg(feature = "eval")]
fn eval_mock_expression(
    source: &str,
    environment: &MockEnvironment,
) -> Result<LymaValue, LuaRuntimeError> {
    let trimmed = source.trim();
    if trimmed == "nil" {
        return Ok(LymaValue::Null(lyma_syntax::LymaNull));
    }
    if trimmed == "true" {
        return Ok(LymaValue::Boolean(true));
    }
    if trimmed == "false" {
        return Ok(LymaValue::Boolean(false));
    }
    if trimmed == "make_userdata" {
        return Ok(LymaValue::UserData(lyma_syntax::LymaHostValue {
            kind: String::from("mock_userdata"),
            label: Some(String::from("userdata")),
        }));
    }
    if let Some(value) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Ok(LymaValue::String(String::from(value)));
    }
    if let Some((left, right)) = trimmed.split_once("==") {
        return Ok(LymaValue::Boolean(
            eval_mock_expression(left.trim(), environment)?
                == eval_mock_expression(right.trim(), environment)?,
        ));
    }
    if let Ok(number) = trimmed.parse::<i64>() {
        return Ok(LymaValue::Number(LymaNumber::Integer(number)));
    }
    let mut parts = trimmed.split('.');
    let first = parts.next().unwrap();
    let mut value = environment
        .context
        .get(first)
        .ok_or_else(|| {
            LuaRuntimeError::runtime_error(
                "mock",
                LuaRuntimePhase::Evaluate(lyma_runtime::LuaChunkKind::Expression),
                format!("unknown symbol: {first}"),
                None,
            )
        })?
        .0
        .clone();
    for part in parts {
        value = match value {
            LymaValue::Mapping(mapping) => lookup(&mapping, part).clone(),
            _ => {
                return Err(LuaRuntimeError::runtime_error(
                    "mock",
                    LuaRuntimePhase::Evaluate(lyma_runtime::LuaChunkKind::Expression),
                    format!("cannot index {part}"),
                    None,
                ));
            }
        };
    }
    Ok(value)
}

#[cfg(feature = "eval")]
fn lookup<'a>(mapping: &'a LymaMapping, key: &str) -> &'a LymaValue {
    &mapping
        .entries
        .iter()
        .find(|entry| matches!(&entry.key, lyma_syntax::LymaKey::String(value) if value == key))
        .unwrap()
        .value
}
