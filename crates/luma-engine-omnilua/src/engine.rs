//! `OmniLua` engine adapter types.

use std::cell::RefCell;

use luma_runtime::{
    ConversionPolicy, Engine, LuaChunkKind, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase,
    LuaSourceText, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModuleFactory,
    RuntimeValueCodec,
};
use luma_syntax::LumaValue;
use omnilua::{Lua, LuaError, Value};

use crate::{
    convert::{freeze_runtime_value, to_luma_value},
    engine_name,
    env::{OmniLuaEnvironment, OmniLuaModule},
    limits::{max_table_entries, validate_limits_for_phase},
};

/// Engine-local runtime value representation.
#[derive(Debug, Clone)]
pub enum OmniLuaValue {
    /// Detached stable value snapshot.
    Frozen(LumaValue),
    /// Live `OmniLua` value rooted in an `OmniLua` state.
    Live(Value),
}

/// Validated compiled source descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmniLuaChunk {
    pub(crate) kind: LuaChunkKind,
    pub(crate) source_name: String,
    pub(crate) source_text: String,
    pub(crate) prepared_source: String,
    pub(crate) span: Option<luma_syntax::source::Span>,
}

/// Luma runtime engine backed by `omnilua`.
#[derive(Debug, Default)]
pub struct OmniLuaEngine {
    conversion_limits: RefCell<Option<RuntimeLimits>>,
}

impl Engine for OmniLuaEngine {
    fn engine_name(&self) -> &'static str {
        engine_name()
    }
}

impl RuntimeEnvironmentFactory for OmniLuaEngine {
    type RuntimeValue = OmniLuaValue;
    type RuntimeModule = OmniLuaModule;
    type Environment = OmniLuaEnvironment;

    fn create_environment(&self) -> Result<Self::Environment, LuaRuntimeError> {
        Ok(OmniLuaEnvironment::default())
    }
}

impl RuntimeModuleFactory for OmniLuaEngine {
    type RuntimeValue = OmniLuaValue;
    type Module = OmniLuaModule;

    fn create_module(
        &self,
        name: impl Into<String>,
        exports: Vec<(String, Self::RuntimeValue)>,
    ) -> Result<Self::Module, LuaRuntimeError> {
        Ok(OmniLuaModule {
            name: name.into(),
            exports: exports
                .into_iter()
                .map(|(name, value)| Ok((name, freeze_runtime_value(value)?)))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl RuntimeValueCodec for OmniLuaEngine {
    type Value = OmniLuaValue;
    type FrozenValue = LumaValue;

    fn to_luma_value(
        &self,
        value: &Self::Value,
        policy: &ConversionPolicy,
    ) -> Result<LumaValue, LuaRuntimeError> {
        to_luma_value(value, policy, self.active_max_table_entries())
    }

    fn from_luma_value(&self, value: &LumaValue) -> Result<Self::Value, LuaRuntimeError> {
        Ok(OmniLuaValue::Frozen(value.clone()))
    }

    fn freeze_value(&self, value: &Self::Value) -> Result<Self::FrozenValue, LuaRuntimeError> {
        match value {
            OmniLuaValue::Frozen(value) => Ok(value.clone()),
            OmniLuaValue::Live(value) => to_luma_value(
                &OmniLuaValue::Live(value.clone()),
                &ConversionPolicy::default(),
                self.active_max_table_entries(),
            ),
        }
    }

    fn clone_value(&self, value: &Self::Value) -> Result<Self::Value, LuaRuntimeError> {
        Ok(value.clone())
    }

    fn thaw_value(&self, value: &Self::FrozenValue) -> Result<Self::Value, LuaRuntimeError> {
        Ok(OmniLuaValue::Frozen(value.clone()))
    }
}

impl LuaRuntimeEngine for OmniLuaEngine {
    type CompiledExpression = OmniLuaChunk;
    type CompiledChunk = OmniLuaChunk;

    fn compile_expression(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<Self::CompiledExpression, LuaRuntimeError> {
        validate_limits_for_phase(
            limits,
            LuaRuntimePhase::Compile(LuaChunkKind::Expression),
            source.span,
        )?;
        Self::compile(LuaChunkKind::Expression, &source)
    }

    fn compile_chunk(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<Self::CompiledChunk, LuaRuntimeError> {
        validate_limits_for_phase(
            limits,
            LuaRuntimePhase::Compile(LuaChunkKind::Chunk),
            source.span,
        )?;
        Self::compile(LuaChunkKind::Chunk, &source)
    }

    fn evaluate_expression(
        &self,
        compiled: &Self::CompiledExpression,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        self.evaluate(compiled, environment, limits)
    }

    fn evaluate_chunk(
        &self,
        compiled: &Self::CompiledChunk,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<Self::Value, LuaRuntimeError> {
        self.evaluate(compiled, environment, limits)
    }
}

impl OmniLuaEngine {
    fn compile(
        kind: LuaChunkKind,
        source: &LuaSourceText<'_>,
    ) -> Result<OmniLuaChunk, LuaRuntimeError> {
        let prepared_source = prepare_source(kind, source.text);
        let chunk = OmniLuaChunk {
            kind,
            source_name: String::from(source.name),
            source_text: String::from(source.text),
            prepared_source,
            span: source.span,
        };
        validate_source_syntax(&chunk)?;
        Ok(chunk)
    }

    fn evaluate(
        &self,
        compiled: &OmniLuaChunk,
        environment: &OmniLuaEnvironment,
        limits: &RuntimeLimits,
    ) -> Result<OmniLuaValue, LuaRuntimeError> {
        validate_limits_for_phase(
            limits,
            LuaRuntimePhase::Evaluate(compiled.kind),
            compiled.span,
        )?;
        self.conversion_limits.replace(Some(limits.clone()));
        let lua = environment.materialize()?;
        let result = evaluate_prepared(&lua, compiled)?;
        Ok(OmniLuaValue::Live(result))
    }

    fn active_max_table_entries(&self) -> Option<usize> {
        self.conversion_limits
            .borrow()
            .as_ref()
            .and_then(max_table_entries)
    }
}

fn validate_source_syntax(compiled: &OmniLuaChunk) -> Result<(), LuaRuntimeError> {
    let lua = Lua::try_new().map_err(|error| {
        LuaRuntimeError::runtime_error(
            engine_name(),
            LuaRuntimePhase::Compile(compiled.kind),
            format!("failed to create compiler state: {error}"),
            compiled.span,
        )
    })?;
    lua.load(compiled.prepared_source.as_bytes())
        .set_name(compiled.source_name.as_bytes())
        .into_function()
        .map(|_| ())
        .map_err(|error| {
            LuaRuntimeError::syntax_error(
                engine_name(),
                LuaRuntimePhase::Compile(compiled.kind),
                error.to_string(),
                compiled.span,
            )
        })
}

fn evaluate_prepared(lua: &Lua, compiled: &OmniLuaChunk) -> Result<Value, LuaRuntimeError> {
    lua.load(compiled.prepared_source.as_bytes())
        .set_name(compiled.source_name.as_bytes())
        .eval::<Value>()
        .map_err(|error| {
            map_omnilua_error(
                &error,
                LuaRuntimePhase::Evaluate(compiled.kind),
                compiled.span,
            )
        })
}

fn prepare_source(kind: LuaChunkKind, source: &str) -> String {
    match kind {
        LuaChunkKind::Expression => format!("return ({source})"),
        LuaChunkKind::Chunk => String::from(source),
    }
}

fn map_omnilua_error(
    error: &omnilua::Error,
    phase: LuaRuntimePhase,
    span: Option<luma_syntax::source::Span>,
) -> LuaRuntimeError {
    match error.kind() {
        LuaError::Syntax(_) => {
            LuaRuntimeError::syntax_error(engine_name(), phase, error.to_string(), span)
        }
        _ => LuaRuntimeError::runtime_error(engine_name(), phase, error.to_string(), span),
    }
}
