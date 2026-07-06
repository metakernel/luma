use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lyma::tooling::format_document_edit;
use lyma_parser::{FileId, decode_bytes, parse_str};
use lyma_syntax::DiagnosticCode;

#[cfg(feature = "eval")]
use lyma_eval::{
    FilesystemResolver, InMemoryResolver, ResolutionContext, ResolutionKind, ResolutionRequest,
    ResolverPolicy, ResourceResolver,
};

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
use lyma_engine_omnilua::OmniLuaEngine;
#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
use lyma_runtime::{
    ConversionPolicy, LuaRuntimeEngine, LuaSourceText, RuntimeEnvironment,
    RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModuleFactory, RuntimeValueCodec,
};
#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
use lyma_syntax::{FileId as EvalFileId, LymaNumber, LymaValue, source::Span};

#[test]
fn decoder_and_parser_handle_adversarial_inputs_without_panicking() {
    let invalid = decode_bytes(FileId(1), "invalid.lyma", &[0xff, 0xfe, 0x80]).unwrap_err();
    assert_eq!(invalid.diagnostics[0].code, DiagnosticCode::InvalidUtf8);

    let huge_indent = format!("{}value: true\n", " ".repeat(32_768));
    let parsed = parse_str(FileId(2), "huge-indent.lyma", &huge_indent);
    assert!(!parsed.file.documents.is_empty() || !parsed.diagnostics.is_empty());

    let mut nested = String::new();
    for depth in 0..128 {
        nested.push_str(&"  ".repeat(depth));
        nested.push_str("child:\n");
    }
    nested.push_str(&"  ".repeat(128));
    nested.push_str("leaf: true\n");

    let parsed = parse_str(FileId(3), "deep.lyma", &nested);
    let formatted = format_document_edit("deep.lyma", &nested);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert!(
        formatted.parsed.diagnostics.is_empty(),
        "{:#?}",
        formatted.parsed.diagnostics
    );
}

#[cfg(feature = "eval")]
#[test]
fn resolver_defaults_reject_traversal_bad_schemes_network_and_cycles() {
    let policy = ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    };
    let resolver = InMemoryResolver::new(policy.clone())
        .with_resource("root", "@import \"root\" as self\nvalue: true\n")
        .with_resource("include-root", "@include \"include-root\"\nvalue: true\n")
        .with_resource("https://allowed.example/test", "value: true\n");

    let mut traversal_context = ResolutionContext::new(policy.max_depth);
    let traversal = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "../secret",
            from: None,
            context: &mut traversal_context,
        })
        .unwrap_err();
    assert_eq!(traversal.diagnostic.code, DiagnosticCode::UnsafeOperation);

    let mut scheme_context = ResolutionContext::new(policy.max_depth);
    let scheme = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "ssh://example.invalid/spec",
            from: None,
            context: &mut scheme_context,
        })
        .unwrap_err();
    assert_eq!(scheme.diagnostic.code, DiagnosticCode::UnsafeOperation);

    let network_policy = ResolverPolicy {
        allowed_uri_schemes: [String::from("https")].into_iter().collect(),
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    };
    let network_resolver = InMemoryResolver::new(network_policy.clone())
        .with_resource("https://allowed.example/test", "value: true\n");
    let mut network_context = ResolutionContext::new(network_policy.max_depth);
    let network = network_resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "https://allowed.example/test",
            from: None,
            context: &mut network_context,
        })
        .unwrap_err();
    assert_eq!(network.diagnostic.code, DiagnosticCode::UnsafeOperation);
    assert!(network.diagnostic.message.contains("network access"));

    let mut cycle_context = ResolutionContext::new(policy.max_depth);
    let first = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "root",
            from: None,
            context: &mut cycle_context,
        })
        .unwrap();
    let cycle = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "root",
            from: Some(&first.locator),
            context: &mut cycle_context,
        })
        .unwrap_err();
    assert_eq!(cycle.diagnostic.code, DiagnosticCode::ImportCycle);

    let mut include_context = ResolutionContext::new(policy.max_depth);
    let first = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Include,
            specifier: "include-root",
            from: None,
            context: &mut include_context,
        })
        .unwrap();
    let cycle = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Include,
            specifier: "include-root",
            from: Some(&first.locator),
            context: &mut include_context,
        })
        .unwrap_err();
    assert_eq!(cycle.diagnostic.code, DiagnosticCode::ImportCycle);
}

#[cfg(feature = "eval")]
#[test]
fn filesystem_resolver_blocks_symlink_root_escape() {
    let root = TempTree::new("lyma-security-root");
    let outside = TempTree::new("lyma-security-outside");
    fs::write(outside.path().join("secret.lyma"), "value: secret\n").unwrap();

    let link = root.path().join("escape.lyma");
    if let Err(error) = create_symlink_or_junction(&outside.path().join("secret.lyma"), &link) {
        if is_symlink_permission_error(&error) {
            eprintln!("skipping symlink escape test: {error}");
            return;
        }
        panic!("failed to create symlink fixture: {error}");
    }

    let policy = ResolverPolicy::filesystem_only(vec![root.path().to_path_buf()]);
    let resolver = FilesystemResolver::new(policy.clone());
    let mut context = ResolutionContext::new(policy.max_depth);
    let error = resolver
        .resolve(ResolutionRequest {
            kind: ResolutionKind::Import,
            specifier: "escape.lyma",
            from: None,
            context: &mut context,
        })
        .unwrap_err();
    assert_eq!(error.diagnostic.code, DiagnosticCode::UnsafeOperation);
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
#[test]
fn omnilua_safe_defaults_block_escape_hatches_and_secret_access() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let limits = RuntimeLimits::unbounded();
    let span = Span::new(EvalFileId(7), 0, 1);

    for expr in ["io", "os", "debug", "package"] {
        let value = eval_expr(&engine, &mut environment, expr, span, &limits).unwrap();
        assert_eq!(
            value,
            LymaValue::Null(lyma_syntax::LymaNull),
            "expr: {expr}"
        );
    }

    for script in [
        "return require('socket')",
        "return load('return 1')",
        "return loadfile('x')",
        "return dofile('x')",
        "return package.loadlib('x', 'y')",
        "return os.getenv('TOKEN')",
        "return io.open('secret')",
        "collectgarbage()",
        "return math.random()",
        "return math.randomseed(1)",
        "return math['random']()",
        "return math['randomseed'](1)",
        "math.abs = nil",
    ] {
        let error = eval_chunk(&engine, &mut environment, script, span, &limits).unwrap_err();
        assert_eq!(error.diagnostic.code.code(), "E0013", "script: {script}");
    }
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
#[test]
fn omnilua_rejects_hostile_resource_limit_configurations_with_diagnostics() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let span = Span::new(EvalFileId(8), 5, 25);

    for (limits, needle) in [
        (
            RuntimeLimits {
                max_instructions: Some(1),
                ..RuntimeLimits::unbounded()
            },
            "Instructions",
        ),
        (
            RuntimeLimits {
                max_call_depth: Some(8),
                ..RuntimeLimits::unbounded()
            },
            "CallDepth",
        ),
        (
            RuntimeLimits {
                max_memory_bytes: Some(1_024),
                ..RuntimeLimits::unbounded()
            },
            "Memory",
        ),
        (
            RuntimeLimits {
                max_runtime_millis: Some(1),
                ..RuntimeLimits::unbounded()
            },
            "Runtime",
        ),
    ] {
        let error = eval_chunk(
            &engine,
            &mut environment,
            "while true do end",
            span,
            &limits,
        )
        .unwrap_err();
        assert_eq!(error.diagnostic.code.code(), "E0020");
        assert!(error.diagnostic.message.contains(needle), "{error:#?}");
    }
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
#[test]
fn omnilua_reports_cyclic_tables_and_read_only_modules() {
    let engine = OmniLuaEngine::default();
    let span = Span::new(EvalFileId(9), 0, 12);
    let limits = RuntimeLimits::unbounded();

    let mut environment = engine.create_environment().unwrap();
    let compiled = engine
        .compile_chunk(
            LuaSourceText::new("cycle.lua", "local t = {}; t.self = t; return t").with_span(span),
            &limits,
        )
        .unwrap();
    let value = engine
        .evaluate_chunk(&compiled, &mut environment, &limits)
        .unwrap();
    let error = engine
        .to_lyma_value(
            &value,
            &ConversionPolicy {
                origin_span: Some(span),
                ..ConversionPolicy::default()
            },
        )
        .unwrap_err();
    assert_eq!(error.diagnostic.code.code(), "E0030");
    assert!(error.diagnostic.message.contains("cyclic Lua tables"));

    let module = engine
        .create_module(
            "safe",
            vec![(
                String::from("answer"),
                engine
                    .from_lyma_value(&LymaValue::Number(LymaNumber::Integer(42)))
                    .unwrap(),
            )],
        )
        .unwrap();
    environment.inject_module(module).unwrap();
    let error =
        eval_chunk(&engine, &mut environment, "safe.answer = 0", span, &limits).unwrap_err();
    assert_eq!(error.diagnostic.code.code(), "E0013");
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
fn eval_chunk(
    engine: &OmniLuaEngine,
    environment: &mut <OmniLuaEngine as RuntimeEnvironmentFactory>::Environment,
    source: &str,
    span: Span,
    limits: &RuntimeLimits,
) -> Result<LymaValue, lyma_runtime::LuaRuntimeError> {
    let compiled = engine.compile_chunk(
        LuaSourceText::new("security", source).with_span(span),
        limits,
    )?;
    let value = engine.evaluate_chunk(&compiled, environment, limits)?;
    engine.to_lyma_value(
        &value,
        &ConversionPolicy {
            origin_span: Some(span),
            ..ConversionPolicy::default()
        },
    )
}

#[cfg(all(feature = "eval", feature = "engine-omnilua"))]
fn eval_expr(
    engine: &OmniLuaEngine,
    environment: &mut <OmniLuaEngine as RuntimeEnvironmentFactory>::Environment,
    source: &str,
    span: Span,
    limits: &RuntimeLimits,
) -> Result<LymaValue, lyma_runtime::LuaRuntimeError> {
    let compiled = engine.compile_expression(
        LuaSourceText::new("security-expr", source).with_span(span),
        limits,
    )?;
    let value = engine.evaluate_expression(&compiled, environment, limits)?;
    engine.to_lyma_value(
        &value,
        &ConversionPolicy {
            origin_span: Some(span),
            ..ConversionPolicy::default()
        },
    )
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    path: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("{label}-{timestamp}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(unix)]
fn create_symlink_or_junction(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink_or_junction(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

fn is_symlink_permission_error(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::PermissionDenied)
        || cfg!(windows) && error.raw_os_error() == Some(1314)
}
