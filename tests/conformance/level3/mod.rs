use std::collections::BTreeMap;

#[cfg(feature = "engine-omnilua")]
use lyma_engine_omnilua::OmniLuaEngine;
use lyma_eval::{
    AstEvaluator, EvaluationOptions, EvaluationProfile, InMemoryResolver, InMemoryTagResolver,
    ModuleRegistry, ResolverPolicy, ResourceResolver, UnknownTagPolicy,
};
use lyma_parser::parse_str;
use lyma_runtime::{
    Engine, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, LuaSourceText, RuntimeEnvironment,
    RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule, RuntimeModuleFactory,
    RuntimeValueCodec,
};
use lyma_syntax::{FileId, LymaKey, LymaMapping, LymaNull, LymaNumber, LymaValue};

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
        _policy: &lyma_runtime::ConversionPolicy,
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
        _limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        Ok(MockValue(eval_mock_expression(compiled, environment)?))
    }

    fn evaluate_chunk(
        &self,
        compiled: &Self::CompiledChunk,
        environment: &mut Self::Environment,
        _limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        let source = compiled.trim();
        let source = source.strip_prefix("return ").unwrap_or(source);
        Ok(MockValue(eval_mock_expression(source, environment)?))
    }
}

#[test]
fn level3_extracts_metadata_and_runs_tag_resolvers() {
    let parsed = parse_str(
        FileId(10),
        "level3-meta.lyma",
        r#"@lyma 0.1
@profile safe
@meta:
  title: Example
  generated: false
!upper hello
"#,
    );
    let tag_resolver = InMemoryTagResolver::new().with_handler("upper", |value| match value {
        LymaValue::String(value) => Ok(LymaValue::String(value.to_uppercase())),
        _ => unreachable!(),
    });
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &EvaluationProfile::restricted(),
            resolver: None,
            module_registry: None::<&dyn ModuleRegistry<MockEngine>>,
            tag_resolver: Some(&tag_resolver),
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::Preserve,
        },
    };

    let [document] = evaluator
        .evaluate_file_with_metadata(&parsed.file, "level3-meta.lyma", None)
        .expect("evaluation should succeed")
        .try_into()
        .expect("single document");

    assert_eq!(document.value, LymaValue::String(String::from("HELLO")));
    assert_eq!(document.metadata.version, Some(String::from("0.1")));
    assert!(matches!(
        document.metadata.profile,
        Some(lyma_syntax::LymaProfile::Safe)
    ));
    let LymaValue::Mapping(meta) = document.metadata.value.expect("meta should exist") else {
        panic!()
    };
    assert_eq!(
        lookup(&meta, "title"),
        &LymaValue::String(String::from("Example"))
    );
}

#[test]
fn level3_loads_basic_schemas_and_rejects_unknown_tags_by_default() {
    let parsed = parse_str(
        FileId(11),
        "level3-schema.lyma",
        r#"@profile data
@schema "schemas/example"
id: demo
enabled: !mystery true
"#,
    );
    let resolver = InMemoryResolver::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_resource(
        "schemas/example",
        "@profile data\ntype: object\nrequired:\n  id: string\n  enabled: boolean\n",
    );
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &EvaluationProfile::restricted(),
            resolver: Some(&resolver as &dyn ResourceResolver),
            module_registry: None::<&dyn ModuleRegistry<MockEngine>>,
            tag_resolver: None,
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        },
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "level3-schema.lyma", None)
        .expect_err("unknown tags should be rejected for schema documents");
    assert_eq!(error.diagnostic.code.code(), "E0011");
}

#[test]
fn level3_data_outputs_reject_runtime_only_values_as_e0025() {
    let parsed = parse_str(
        FileId(12),
        "level3-data.lyma",
        "@profile data\nroot: =make_userdata\n",
    );
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &EvaluationProfile::restricted(),
            resolver: None,
            module_registry: None::<&dyn ModuleRegistry<MockEngine>>,
            tag_resolver: None,
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::Preserve,
        },
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "level3-data.lyma", None)
        .expect_err("data profile should reject runtime-only values");
    assert_eq!(error.diagnostic.code.code(), "E0025");
}

#[test]
fn level3_loaded_schema_validates_values() {
    let parsed = parse_str(
        FileId(13),
        "level3-schema-pass.lyma",
        r#"@profile data
@schema "schemas/example"
id: demo
enabled: true
names:
  - a
  - b
"#,
    );
    let resolver = InMemoryResolver::new(ResolverPolicy {
        max_depth: 8,
        ..ResolverPolicy::deny_all()
    })
    .with_resource(
        "schemas/example",
        "@profile data\ntype: object\nrequired:\n  id: string\n  enabled: boolean\noptional:\n  names:\n    type: array\n    items: string\n",
    );
    let evaluator = AstEvaluator {
        engine: &MockEngine,
        options: EvaluationOptions {
            profile: &EvaluationProfile::restricted(),
            resolver: Some(&resolver as &dyn ResourceResolver),
            module_registry: None::<&dyn ModuleRegistry<MockEngine>>,
            tag_resolver: None,
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        },
    };

    let [value] = evaluator
        .evaluate_file(&parsed.file, "level3-schema-pass.lyma", None)
        .expect("schema should validate")
        .try_into()
        .expect("single document");
    let LymaValue::Mapping(mapping) = value else {
        panic!()
    };
    assert_eq!(
        lookup(&mapping, "id"),
        &LymaValue::String(String::from("demo"))
    );
}

#[cfg(feature = "engine-omnilua")]
#[test]
fn level3_omnilua_rejects_cyclic_chunk_outputs() {
    let parsed = parse_str(
        FileId(14),
        "level3-cycle.lyma",
        "root: |lua\n  local t = {}\n  t.self = t\n  return t\n",
    );
    let engine = OmniLuaEngine::default();
    let mut profile = EvaluationProfile::restricted();
    profile.runtime_limits.max_instructions = None;
    profile.runtime_limits.max_call_depth = None;
    profile.runtime_limits.max_memory_bytes = None;
    profile.runtime_limits.max_runtime_millis = None;
    let evaluator = AstEvaluator {
        engine: &engine,
        options: EvaluationOptions {
            profile: &profile,
            ..EvaluationOptions::<OmniLuaEngine>::default()
        },
    };

    let error = evaluator
        .evaluate_file(&parsed.file, "level3-cycle.lyma", None)
        .expect_err("cycles should be rejected");
    assert!(
        matches!(error.diagnostic.code.code(), "E0013" | "E0030"),
        "unexpected diagnostic: {:?}",
        error.diagnostic
    );
}

fn lookup<'a>(mapping: &'a LymaMapping, key: &str) -> &'a LymaValue {
    &mapping
        .entries
        .iter()
        .find(|entry| entry.key == LymaKey::String(String::from(key)))
        .unwrap()
        .value
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
