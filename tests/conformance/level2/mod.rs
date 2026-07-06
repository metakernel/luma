use std::collections::BTreeMap;

#[cfg(feature = "engine-omnilua")]
use lyma_engine_omnilua::OmniLuaEngine;
use lyma_eval::{
    AstEvaluator, EvaluationOptions, EvaluationProfile, InMemoryModuleRegistry, InMemoryResolver,
    ModuleRegistry, ResolverPolicy, ResourceResolver, UnknownTagPolicy,
};
use lyma_parser::parse_str;
use lyma_runtime::{
    ConversionPolicy, Engine, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, LuaSourceText,
    RuntimeEnvironment, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule,
    RuntimeModuleFactory, RuntimeValueCodec,
};
use lyma_syntax::{FileId, LymaKey, LymaMapping, LymaNull, LymaNumber, LymaSequence, LymaValue};

#[derive(Debug, Clone, PartialEq)]
struct MockValue(LymaValue);

#[derive(Debug, Clone, PartialEq)]
struct MockModule {
    name: String,
    exports: Vec<(String, MockValue)>,
}

impl RuntimeModule for MockModule {
    type RuntimeValue = MockValue;

    fn module_name(&self) -> &str {
        &self.name
    }

    fn exports(&self) -> Result<Vec<(String, Self::RuntimeValue)>, LuaRuntimeError> {
        Ok(self.exports.clone())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct MockEnvironment {
    context: BTreeMap<String, MockValue>,
    modules: BTreeMap<String, MockModule>,
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct MockEngine;

impl Engine for MockEngine {
    fn engine_name(&self) -> &'static str {
        "mock"
    }
}

impl RuntimeEnvironmentFactory for MockEngine {
    type RuntimeValue = MockValue;
    type RuntimeModule = MockModule;
    type Environment = MockEnvironment;

    fn create_environment(&self) -> Result<Self::Environment, LuaRuntimeError> {
        Ok(MockEnvironment::default())
    }
}

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
        let source = compiled.trim();
        let source = source.strip_prefix("return ").unwrap_or(source);
        Ok(MockValue(eval_mock_expression(source, environment)?))
    }
}

#[test]
fn level2_evaluates_safe_profile_features_with_mock_engine() {
    let parsed = parse_str(
        FileId(1),
        "level2.lyma",
        r#"@import "common" as common
@include "base"
@use safe.mod as safe
let defaults:
  timeout: 3
let mode: prod
let names:
  - a
  - b
let extra_statuses:
  warn: false
root:
  base: core
  timeout: =defaults.timeout
  imported: =common.answer
  module_enabled: =safe.enabled
  chunk: |lua-
    return "chunk-ok"
  maybe: =nil
  file: =_file
  current_path: =_path
  let suffix = prod
  steps:
    - start
    - ...common.items
    @if mode == "prod":
      - release
    @else:
      - debug
    @for name in names:
      - =name
  statuses:
    ...common.statuses
    @for key, value in extra_statuses:
      [=key]: =value
  nested:
    local_mode: =suffix
    root_base: =_root.base
    parent_timeout: =_parent.timeout
    path: =_path
    here: =_here
    lyma: =_lyma
"#,
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let resolver = InMemoryResolver::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_resource(
        "common",
        "answer: 42\nitems:\n  - alpha\n  - beta\nstatuses:\n  ok: true\n",
    )
    .with_resource("base", "base: true\n");
    let modules = InMemoryModuleRegistry::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_module(
        "safe.mod",
        vec![(String::from("enabled"), LymaValue::Boolean(true))],
    );
    let profile = EvaluationProfile::restricted();
    let options = EvaluationOptions {
        profile: &profile,
        resolver: Some(&resolver as &dyn ResourceResolver),
        module_registry: Some(&modules as &dyn ModuleRegistry<MockEngine>),
        tag_resolver: None,
        schema_validator: None,
        unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
    };
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options,
    };

    let [value] = evaluator
        .evaluate_file(&parsed.file, "level2.lyma", None)
        .expect("evaluation should succeed")
        .try_into()
        .expect("single document");

    let LymaValue::Mapping(root) = value else {
        panic!()
    };
    assert!(
        root.entries
            .iter()
            .any(|entry| entry.key == LymaKey::String(String::from("base")))
    );
    assert!(
        root.entries
            .iter()
            .any(|entry| entry.key == LymaKey::String(String::from("root")))
    );

    let root_value = root
        .entries
        .iter()
        .find(|entry| entry.key == LymaKey::String(String::from("root")))
        .expect("root entry should exist");
    let LymaValue::Mapping(body) = &root_value.value else {
        panic!()
    };
    assert_eq!(
        lookup(body, "timeout"),
        &LymaValue::Number(LymaNumber::Integer(3))
    );
    assert_eq!(
        lookup(body, "imported"),
        &LymaValue::Number(LymaNumber::Integer(42))
    );
    assert_eq!(lookup(body, "module_enabled"), &LymaValue::Boolean(true));
    assert_eq!(
        lookup(body, "chunk"),
        &LymaValue::String(String::from("chunk-ok"))
    );
    assert_eq!(lookup(body, "maybe"), &LymaValue::Null(LymaNull));
    assert_eq!(
        lookup(body, "base"),
        &LymaValue::String(String::from("core"))
    );
    assert_eq!(
        lookup(body, "current_path"),
        &sequence([
            LymaValue::String(String::from("root")),
            LymaValue::String(String::from("current_path")),
        ])
    );
    let LymaValue::Sequence(steps) = lookup(body, "steps") else {
        panic!()
    };
    assert_eq!(steps.items.len(), 6);
    assert_eq!(steps.items[1], LymaValue::String(String::from("alpha")));
    assert_eq!(steps.items[5], LymaValue::String(String::from("b")));
    let LymaValue::Mapping(statuses) = lookup(body, "statuses") else {
        panic!()
    };
    assert_eq!(lookup(statuses, "ok"), &LymaValue::Boolean(true));
    assert_eq!(lookup(statuses, "warn"), &LymaValue::Boolean(false));
    let LymaValue::Mapping(nested) = lookup(body, "nested") else {
        panic!()
    };
    assert_eq!(
        lookup(nested, "local_mode"),
        &LymaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(nested, "root_base"), &LymaValue::Boolean(true));
    assert_eq!(
        lookup(nested, "parent_timeout"),
        &LymaValue::Number(LymaNumber::Integer(3))
    );
    assert_eq!(
        lookup(nested, "path"),
        &sequence([
            LymaValue::String(String::from("root")),
            LymaValue::String(String::from("nested")),
            LymaValue::String(String::from("path")),
        ])
    );
    assert_eq!(
        lookup(nested, "lyma"),
        &LymaValue::String(String::from("lyma"))
    );
    let LymaValue::Mapping(here) = lookup(nested, "here") else {
        panic!()
    };
    assert_eq!(
        lookup(here, "local_mode"),
        &LymaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(here, "root_base"), &LymaValue::Boolean(true));
}

#[test]
fn level2_rejects_forbidden_runtime_capabilities_as_e0019() {
    let parsed = parse_str(FileId(2), "unsafe.lyma", "value: =_G\n");
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions::<MockEngine>::default(),
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "unsafe.lyma", None)
        .expect_err("unsafe source should fail");
    assert_eq!(error.diagnostic.code.code(), "E0019");
}

#[test]
fn level2_surfaces_runtime_limit_failures_as_e0020() {
    let parsed = parse_str(FileId(3), "limits.lyma", "value: =answer\n");
    let mut profile = EvaluationProfile::restricted();
    profile.runtime_limits.max_instructions = Some(0);
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &profile,
            ..EvaluationOptions::<MockEngine>::default()
        },
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "limits.lyma", None)
        .expect_err("limit failure should surface");
    assert_eq!(error.diagnostic.code.code(), "E0020");
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level2_evaluates_safe_profile_features_with_omnilua_engine() {
    let parsed = parse_str(
        FileId(4),
        "level2-omnilua.lyma",
        r#"@import "common" as common
@include "base"
@use safe.mod as safe
let defaults:
  timeout: 3
let mode: prod
let names:
  - a
  - b
let extra_statuses:
  warn: false
root:
  base: core
  timeout: =defaults.timeout
  imported: =common.answer
  module_enabled: =safe.enabled
  chunk: |lua-
    return string.upper("chunk-ok")
  maybe: =nil
  file: =_file
  current_path: =_path
  let suffix = prod
  steps:
    - start
    - ...common.items
    @if mode == "prod":
      - release
    @else:
      - debug
    @for _, name in names:
      - =string.upper(name)
  statuses:
    ...common.statuses
    @for key, value in extra_statuses:
      [=key]: =value
  nested:
    local_mode: =suffix
    root_base: =_root.base
    parent_timeout: =_parent.timeout
    path_joined: =table.concat(_path, "/")
    path_tail: =_path[#_path]
"#,
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let resolver = InMemoryResolver::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_resource(
        "common",
        "answer: 42\nitems:\n  - alpha\n  - beta\nstatuses:\n  ok: true\n",
    )
    .with_resource("base", "base: true\n");
    let modules = InMemoryModuleRegistry::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_module(
        "safe.mod",
        vec![(String::from("enabled"), LymaValue::Boolean(true))],
    );
    let profile = omnilua_safe_profile();
    let engine = OmniLuaEngine::default();
    let evaluator = AstEvaluator {
        engine: &engine,
        options: EvaluationOptions {
            profile: &profile,
            resolver: Some(&resolver as &dyn ResourceResolver),
            module_registry: Some(&modules as &dyn ModuleRegistry<OmniLuaEngine>),
            tag_resolver: None,
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        },
    };

    let [value] = evaluator
        .evaluate_file(&parsed.file, "level2-omnilua.lyma", None)
        .expect("evaluation should succeed")
        .try_into()
        .expect("single document");

    let LymaValue::Mapping(root) = value else {
        panic!()
    };
    let root_value = root
        .entries
        .iter()
        .find(|entry| entry.key == LymaKey::String(String::from("root")))
        .expect("root entry should exist");
    let LymaValue::Mapping(body) = &root_value.value else {
        panic!()
    };
    assert_eq!(
        lookup(body, "imported"),
        &LymaValue::Number(LymaNumber::Integer(42))
    );
    assert_eq!(lookup(body, "module_enabled"), &LymaValue::Boolean(true));
    assert_eq!(
        lookup(body, "chunk"),
        &LymaValue::String(String::from("CHUNK-OK"))
    );
    assert_eq!(
        lookup(body, "current_path"),
        &sequence([
            LymaValue::String(String::from("root")),
            LymaValue::String(String::from("current_path")),
        ])
    );
    let LymaValue::Sequence(steps) = lookup(body, "steps") else {
        panic!()
    };
    assert_eq!(steps.items[4], LymaValue::String(String::from("A")));
    assert_eq!(steps.items[5], LymaValue::String(String::from("B")));
    let LymaValue::Mapping(nested) = lookup(body, "nested") else {
        panic!()
    };
    assert_eq!(
        lookup(nested, "local_mode"),
        &LymaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(nested, "root_base"), &LymaValue::Boolean(true));
    assert_eq!(
        lookup(nested, "parent_timeout"),
        &LymaValue::Number(LymaNumber::Integer(3))
    );
    assert_eq!(
        lookup(nested, "path_joined"),
        &LymaValue::String(String::from("root/nested/path_joined"))
    );
    assert_eq!(
        lookup(nested, "path_tail"),
        &LymaValue::String(String::from("path_tail"))
    );
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level2_omnilua_rejects_forbidden_capabilities_and_escape_attempts() {
    let engine = OmniLuaEngine::default();

    for source in [
        "value: =_ENV\n",
        "value: |lua-\n  return getmetatable(math)\n",
        "value: =math.random()\n",
    ] {
        let parsed = parse_str(FileId(5), "unsafe-omnilua.lyma", source);
        let evaluator = AstEvaluator {
            engine: &engine,
            options: EvaluationOptions::<OmniLuaEngine>::default(),
        };
        let error = evaluator
            .evaluate_file(&parsed.file, "unsafe-omnilua.lyma", None)
            .expect_err("unsafe source should fail");
        assert_eq!(error.diagnostic.code.code(), "E0019", "source: {source}");
    }
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level2_omnilua_surfaces_resource_limit_failures_as_e0020() {
    let parsed = parse_str(
        FileId(6),
        "limits-omnilua.lyma",
        "value: ={ alpha = 1, beta = 2, gamma = 3 }\n",
    );
    let mut profile = omnilua_safe_profile();
    profile.runtime_limits.max_table_entries = Some(2);
    let engine = OmniLuaEngine::default();
    let evaluator = AstEvaluator {
        engine: &engine,
        options: EvaluationOptions {
            profile: &profile,
            ..EvaluationOptions::<OmniLuaEngine>::default()
        },
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "limits-omnilua.lyma", None)
        .expect_err("limit failure should surface");
    assert_eq!(error.diagnostic.code.code(), "E0020");
}

fn lookup<'a>(mapping: &'a LymaMapping, key: &str) -> &'a LymaValue {
    &mapping
        .entries
        .iter()
        .find(|entry| entry.key == LymaKey::String(String::from(key)))
        .unwrap()
        .value
}

fn sequence<const N: usize>(items: [LymaValue; N]) -> LymaValue {
    LymaValue::Sequence(LymaSequence {
        items: items.into_iter().collect(),
        span: None,
    })
}

#[cfg(feature = "engine-omnilua")]
fn omnilua_safe_profile() -> EvaluationProfile {
    let mut profile = EvaluationProfile::restricted();
    profile.runtime_limits.max_instructions = None;
    profile.runtime_limits.max_call_depth = None;
    profile.runtime_limits.max_memory_bytes = None;
    profile.runtime_limits.max_runtime_millis = None;
    profile
}

fn eval_mock_expression(
    source: &str,
    environment: &MockEnvironment,
) -> Result<LymaValue, LuaRuntimeError> {
    let trimmed = source.trim();
    if trimmed == "nil" {
        return Ok(LymaValue::Null(LymaNull));
    }
    if trimmed == "true" {
        return Ok(LymaValue::Boolean(true));
    }
    if trimmed == "false" {
        return Ok(LymaValue::Boolean(false));
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
