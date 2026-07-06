//! Integration tests for the `OmniLua` backend.

use lyma_engine_omnilua::OmniLuaEngine;
use lyma_runtime::{
    ConversionPolicy, LuaRuntimeEngine, LuaSourceText, RuntimeEnvironment,
    RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModuleFactory, RuntimeValueCodec,
};
use lyma_syntax::{
    FileId, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaValue, source::Span,
};

fn eval_expr(
    engine: &OmniLuaEngine,
    environment: &mut <OmniLuaEngine as RuntimeEnvironmentFactory>::Environment,
    source: &str,
    span: Span,
    limits: &RuntimeLimits,
) -> Result<LymaValue, lyma_runtime::LuaRuntimeError> {
    let compiled =
        engine.compile_expression(LuaSourceText::new("expr", source).with_span(span), limits)?;
    let value = engine.evaluate_expression(&compiled, environment, limits)?;
    engine.to_lyma_value(
        &value,
        &ConversionPolicy {
            allow_functions: true,
            allow_userdata: true,
            allow_host_objects: true,
            origin_span: Some(span),
        },
    )
}

fn eval_chunk(
    engine: &OmniLuaEngine,
    environment: &mut <OmniLuaEngine as RuntimeEnvironmentFactory>::Environment,
    source: &str,
    span: Span,
    limits: &RuntimeLimits,
) -> Result<LymaValue, lyma_runtime::LuaRuntimeError> {
    let compiled =
        engine.compile_chunk(LuaSourceText::new("chunk", source).with_span(span), limits)?;
    let value = engine.evaluate_chunk(&compiled, environment, limits)?;
    engine.to_lyma_value(
        &value,
        &ConversionPolicy {
            origin_span: Some(span),
            ..ConversionPolicy::default()
        },
    )
}

#[test]
fn expression_and_chunk_diagnostics_keep_original_spans() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let compile_span = Span::new(FileId(7), 3, 11);
    let compile_error = engine
        .compile_expression(
            LuaSourceText::new("expr", "1 +").with_span(compile_span),
            &RuntimeLimits::unbounded(),
        )
        .unwrap_err();
    assert_eq!(compile_error.diagnostic.code.code(), "E0012");
    assert_eq!(compile_error.diagnostic.primary_span, Some(compile_span));

    let runtime_span = Span::new(FileId(8), 5, 17);
    let runtime_error = eval_chunk(
        &engine,
        &mut environment,
        "math.abs = nil",
        runtime_span,
        &RuntimeLimits::unbounded(),
    )
    .unwrap_err();
    assert_eq!(runtime_error.diagnostic.code.code(), "E0013");
    assert_eq!(runtime_error.diagnostic.primary_span, Some(runtime_span));
}

#[test]
fn safe_environment_hides_unsafe_globals_and_escape_hatches() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let limits = RuntimeLimits::unbounded();
    let span = Span::new(FileId(1), 0, 1);

    assert_eq!(
        eval_expr(&engine, &mut environment, "io", span, &limits).unwrap(),
        LymaValue::Null(LymaNull)
    );
    assert_eq!(
        eval_expr(&engine, &mut environment, "os", span, &limits).unwrap(),
        LymaValue::Null(LymaNull)
    );
    assert_eq!(
        eval_expr(&engine, &mut environment, "debug", span, &limits).unwrap(),
        LymaValue::Null(LymaNull)
    );
    assert_eq!(
        eval_expr(&engine, &mut environment, "package", span, &limits).unwrap(),
        LymaValue::Null(LymaNull)
    );

    for script in [
        "math.abs = nil",
        "return _G.math",
        "return getmetatable(math)",
        "return setmetatable({}, {})",
        "return load('return 1')",
        "return os.getenv('SECRET')",
        "return io.open('x')",
        "return os.execute('whoami')",
        "return math.random()",
        "return math.randomseed(1)",
        "return math['random']()",
        "return math['randomseed'](1)",
    ] {
        let error = eval_chunk(&engine, &mut environment, script, span, &limits).unwrap_err();
        assert_eq!(error.diagnostic.code.code(), "E0013", "script: {script}");
        assert_eq!(
            error.diagnostic.primary_span,
            Some(span),
            "script: {script}"
        );
    }
}

#[test]
fn modules_are_copied_read_only_and_environment_forks_are_isolated() {
    let engine = OmniLuaEngine::default();
    let mut root = engine.create_environment().unwrap();
    root.inject_context(
        "answer",
        engine
            .from_lyma_value(&LymaValue::Number(LymaNumber::Integer(41)))
            .unwrap(),
    )
    .unwrap();
    let module = engine
        .create_module(
            "mod",
            vec![(
                String::from("answer"),
                engine
                    .from_lyma_value(&LymaValue::Number(LymaNumber::Integer(42)))
                    .unwrap(),
            )],
        )
        .unwrap();
    root.inject_module(module).unwrap();

    let mut child = root.fork_isolated().unwrap();
    child
        .inject_context(
            "answer",
            engine
                .from_lyma_value(&LymaValue::Number(LymaNumber::Integer(99)))
                .unwrap(),
        )
        .unwrap();

    let span = Span::new(FileId(2), 0, 10);
    let limits = RuntimeLimits::unbounded();
    assert_eq!(
        eval_expr(&engine, &mut root, "answer", span, &limits).unwrap(),
        LymaValue::Number(LymaNumber::Integer(41))
    );
    assert_eq!(
        eval_expr(&engine, &mut child, "answer", span, &limits).unwrap(),
        LymaValue::Number(LymaNumber::Integer(99))
    );
    assert_eq!(
        eval_expr(&engine, &mut root, "mod.answer", span, &limits).unwrap(),
        LymaValue::Number(LymaNumber::Integer(42))
    );

    let error = eval_chunk(&engine, &mut root, "mod.answer = 0", span, &limits).unwrap_err();
    assert_eq!(error.diagnostic.code.code(), "E0013");
}

#[test]
fn null_and_ordered_tables_round_trip() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let mapping = LymaValue::Mapping(LymaMapping {
        entries: vec![
            LymaMappingEntry {
                key: LymaKey::String(String::from("first")),
                value: LymaValue::Number(LymaNumber::Integer(1)),
                span: None,
            },
            LymaMappingEntry {
                key: LymaKey::String(String::from("second")),
                value: LymaValue::Null(LymaNull),
                span: None,
            },
        ],
        duplicate_keys: Vec::new(),
        span: None,
    });
    environment
        .inject_context("ctx", engine.from_lyma_value(&mapping).unwrap())
        .unwrap();
    let span = Span::new(FileId(3), 0, 3);
    let value = eval_expr(
        &engine,
        &mut environment,
        "ctx",
        span,
        &RuntimeLimits::unbounded(),
    )
    .unwrap();
    assert_eq!(value, mapping);
}

#[test]
fn unsupported_safe_mode_limits_fail_closed_as_e0020() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let span = Span::new(FileId(4), 9, 15);
    let error = eval_expr(
        &engine,
        &mut environment,
        "1",
        span,
        &RuntimeLimits::sandboxed(),
    )
    .unwrap_err();
    assert_eq!(error.diagnostic.code.code(), "E0020");
    assert_eq!(error.diagnostic.primary_span, Some(span));
}

#[test]
fn table_entry_limit_is_enforced_during_conversion() {
    let engine = OmniLuaEngine::default();
    let mut environment = engine.create_environment().unwrap();
    let span = Span::new(FileId(5), 0, 20);
    let limits = RuntimeLimits {
        max_table_entries: Some(2),
        ..RuntimeLimits::unbounded()
    };
    let compiled = engine
        .compile_expression(
            LuaSourceText::new("expr", "{ alpha = 1, beta = 2, gamma = 3 }").with_span(span),
            &limits,
        )
        .unwrap();
    let value = engine
        .evaluate_expression(&compiled, &mut environment, &limits)
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
    assert_eq!(error.diagnostic.code.code(), "E0020");
    assert_eq!(error.diagnostic.primary_span, Some(span));
}
