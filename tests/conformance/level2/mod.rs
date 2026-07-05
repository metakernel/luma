use std::collections::BTreeMap;

#[cfg(feature = "engine-omnilua")]
use luma_engine_omnilua::OmniLuaEngine;
use luma_eval::{
    AstEvaluator, EvaluationOptions, EvaluationProfile, InMemoryModuleRegistry, InMemoryResolver,
    ModuleRegistry, ResolverPolicy, ResourceResolver, UnknownTagPolicy,
};
use luma_parser::parse_str;
use luma_runtime::{
    ConversionPolicy, Engine, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, LuaSourceText,
    RuntimeEnvironment, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule,
    RuntimeModuleFactory, RuntimeValueCodec,
};
use luma_syntax::{FileId, LumaKey, LumaMapping, LumaNull, LumaNumber, LumaSequence, LumaValue};

#[derive(Debug, Clone, PartialEq)]
struct MockValue(LumaValue);

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

    fn to_luma_value(
        &self,
        value: &Self::Value,
        _policy: &ConversionPolicy,
    ) -> Result<LumaValue, LuaRuntimeError> {
        Ok(value.0.clone())
    }

    fn from_luma_value(&self, value: &LumaValue) -> Result<Self::Value, LuaRuntimeError> {
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
                luma_runtime::RuntimeLimitKind::Instructions,
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
                luma_runtime::RuntimeLimitKind::Instructions,
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
        "level2.luma",
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
    luma: =_luma
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
        vec![(String::from("enabled"), LumaValue::Boolean(true))],
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
        .evaluate_file(&parsed.file, "level2.luma", None)
        .expect("evaluation should succeed")
        .try_into()
        .expect("single document");

    let LumaValue::Mapping(root) = value else {
        panic!()
    };
    assert!(
        root.entries
            .iter()
            .any(|entry| entry.key == LumaKey::String(String::from("base")))
    );
    assert!(
        root.entries
            .iter()
            .any(|entry| entry.key == LumaKey::String(String::from("root")))
    );

    let root_value = root
        .entries
        .iter()
        .find(|entry| entry.key == LumaKey::String(String::from("root")))
        .expect("root entry should exist");
    let LumaValue::Mapping(body) = &root_value.value else {
        panic!()
    };
    assert_eq!(
        lookup(body, "timeout"),
        &LumaValue::Number(LumaNumber::Integer(3))
    );
    assert_eq!(
        lookup(body, "imported"),
        &LumaValue::Number(LumaNumber::Integer(42))
    );
    assert_eq!(lookup(body, "module_enabled"), &LumaValue::Boolean(true));
    assert_eq!(
        lookup(body, "chunk"),
        &LumaValue::String(String::from("chunk-ok"))
    );
    assert_eq!(lookup(body, "maybe"), &LumaValue::Null(LumaNull));
    assert_eq!(
        lookup(body, "base"),
        &LumaValue::String(String::from("core"))
    );
    assert_eq!(
        lookup(body, "current_path"),
        &sequence([
            LumaValue::String(String::from("root")),
            LumaValue::String(String::from("current_path")),
        ])
    );
    let LumaValue::Sequence(steps) = lookup(body, "steps") else {
        panic!()
    };
    assert_eq!(steps.items.len(), 6);
    assert_eq!(steps.items[1], LumaValue::String(String::from("alpha")));
    assert_eq!(steps.items[5], LumaValue::String(String::from("b")));
    let LumaValue::Mapping(statuses) = lookup(body, "statuses") else {
        panic!()
    };
    assert_eq!(lookup(statuses, "ok"), &LumaValue::Boolean(true));
    assert_eq!(lookup(statuses, "warn"), &LumaValue::Boolean(false));
    let LumaValue::Mapping(nested) = lookup(body, "nested") else {
        panic!()
    };
    assert_eq!(
        lookup(nested, "local_mode"),
        &LumaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(nested, "root_base"), &LumaValue::Boolean(true));
    assert_eq!(
        lookup(nested, "parent_timeout"),
        &LumaValue::Number(LumaNumber::Integer(3))
    );
    assert_eq!(
        lookup(nested, "path"),
        &sequence([
            LumaValue::String(String::from("root")),
            LumaValue::String(String::from("nested")),
            LumaValue::String(String::from("path")),
        ])
    );
    assert_eq!(
        lookup(nested, "luma"),
        &LumaValue::String(String::from("luma"))
    );
    let LumaValue::Mapping(here) = lookup(nested, "here") else {
        panic!()
    };
    assert_eq!(
        lookup(here, "local_mode"),
        &LumaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(here, "root_base"), &LumaValue::Boolean(true));
}

#[test]
fn level2_rejects_forbidden_runtime_capabilities_as_e0019() {
    let parsed = parse_str(FileId(2), "unsafe.luma", "value: =_G\n");
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions::<MockEngine>::default(),
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "unsafe.luma", None)
        .expect_err("unsafe source should fail");
    assert_eq!(error.diagnostic.code.code(), "E0019");
}

#[test]
fn level2_surfaces_runtime_limit_failures_as_e0020() {
    let parsed = parse_str(FileId(3), "limits.luma", "value: =answer\n");
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
        .evaluate_file(&parsed.file, "limits.luma", None)
        .expect_err("limit failure should surface");
    assert_eq!(error.diagnostic.code.code(), "E0020");
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level2_evaluates_safe_profile_features_with_omnilua_engine() {
    let parsed = parse_str(
        FileId(4),
        "level2-omnilua.luma",
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
        vec![(String::from("enabled"), LumaValue::Boolean(true))],
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
        .evaluate_file(&parsed.file, "level2-omnilua.luma", None)
        .expect("evaluation should succeed")
        .try_into()
        .expect("single document");

    let LumaValue::Mapping(root) = value else {
        panic!()
    };
    let root_value = root
        .entries
        .iter()
        .find(|entry| entry.key == LumaKey::String(String::from("root")))
        .expect("root entry should exist");
    let LumaValue::Mapping(body) = &root_value.value else {
        panic!()
    };
    assert_eq!(
        lookup(body, "imported"),
        &LumaValue::Number(LumaNumber::Integer(42))
    );
    assert_eq!(lookup(body, "module_enabled"), &LumaValue::Boolean(true));
    assert_eq!(
        lookup(body, "chunk"),
        &LumaValue::String(String::from("CHUNK-OK"))
    );
    assert_eq!(
        lookup(body, "current_path"),
        &sequence([
            LumaValue::String(String::from("root")),
            LumaValue::String(String::from("current_path")),
        ])
    );
    let LumaValue::Sequence(steps) = lookup(body, "steps") else {
        panic!()
    };
    assert_eq!(steps.items[4], LumaValue::String(String::from("A")));
    assert_eq!(steps.items[5], LumaValue::String(String::from("B")));
    let LumaValue::Mapping(nested) = lookup(body, "nested") else {
        panic!()
    };
    assert_eq!(
        lookup(nested, "local_mode"),
        &LumaValue::String(String::from("prod"))
    );
    assert_eq!(lookup(nested, "root_base"), &LumaValue::Boolean(true));
    assert_eq!(
        lookup(nested, "parent_timeout"),
        &LumaValue::Number(LumaNumber::Integer(3))
    );
    assert_eq!(
        lookup(nested, "path_joined"),
        &LumaValue::String(String::from("root/nested/path_joined"))
    );
    assert_eq!(
        lookup(nested, "path_tail"),
        &LumaValue::String(String::from("path_tail"))
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
        let parsed = parse_str(FileId(5), "unsafe-omnilua.luma", source);
        let evaluator = AstEvaluator {
            engine: &engine,
            options: EvaluationOptions::<OmniLuaEngine>::default(),
        };
        let error = evaluator
            .evaluate_file(&parsed.file, "unsafe-omnilua.luma", None)
            .expect_err("unsafe source should fail");
        assert_eq!(error.diagnostic.code.code(), "E0019", "source: {source}");
    }
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level2_omnilua_surfaces_resource_limit_failures_as_e0020() {
    let parsed = parse_str(
        FileId(6),
        "limits-omnilua.luma",
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
        .evaluate_file(&parsed.file, "limits-omnilua.luma", None)
        .expect_err("limit failure should surface");
    assert_eq!(error.diagnostic.code.code(), "E0020");
}

fn lookup<'a>(mapping: &'a LumaMapping, key: &str) -> &'a LumaValue {
    &mapping
        .entries
        .iter()
        .find(|entry| entry.key == LumaKey::String(String::from(key)))
        .unwrap()
        .value
}

fn sequence<const N: usize>(items: [LumaValue; N]) -> LumaValue {
    LumaValue::Sequence(LumaSequence {
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
) -> Result<LumaValue, LuaRuntimeError> {
    let trimmed = source.trim();
    if trimmed == "nil" {
        return Ok(LumaValue::Null(LumaNull));
    }
    if trimmed == "true" {
        return Ok(LumaValue::Boolean(true));
    }
    if trimmed == "false" {
        return Ok(LumaValue::Boolean(false));
    }
    if let Some(value) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Ok(LumaValue::String(String::from(value)));
    }
    if let Some((left, right)) = trimmed.split_once("==") {
        return Ok(LumaValue::Boolean(
            eval_mock_expression(left.trim(), environment)?
                == eval_mock_expression(right.trim(), environment)?,
        ));
    }
    if let Ok(number) = trimmed.parse::<i64>() {
        return Ok(LumaValue::Number(LumaNumber::Integer(number)));
    }

    let mut parts = trimmed.split('.');
    let first = parts.next().unwrap();
    let mut value = environment
        .context
        .get(first)
        .ok_or_else(|| {
            LuaRuntimeError::runtime_error(
                "mock",
                LuaRuntimePhase::Evaluate(luma_runtime::LuaChunkKind::Expression),
                format!("unknown symbol: {first}"),
                None,
            )
        })?
        .0
        .clone();
    for part in parts {
        value = match value {
            LumaValue::Mapping(mapping) => lookup(&mapping, part).clone(),
            _ => {
                return Err(LuaRuntimeError::runtime_error(
                    "mock",
                    LuaRuntimePhase::Evaluate(luma_runtime::LuaChunkKind::Expression),
                    format!("cannot index {part}"),
                    None,
                ));
            }
        };
    }
    Ok(value)
}
