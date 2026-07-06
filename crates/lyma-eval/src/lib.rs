//! Evaluation-side abstractions over engine-agnostic runtime contracts.

#![forbid(unsafe_code)]

use lyma_runtime::{Engine, LuaRuntimeEngine, LuaRuntimeError, LuaSourceText, RuntimeLimits};

pub mod context;
pub mod control;
pub mod evaluator;
pub mod freeze;
pub mod imports;
pub mod metadata;
pub mod modules;
pub mod options;
pub mod profile;
pub mod resolver;
pub mod runtime_values;
pub mod schema;
pub mod schema_validator;
pub mod scope;
pub mod spread;
pub mod tags;

/// Evaluation responsibilities.
pub mod evaluate {
    use lyma_runtime::{LuaRuntimeEngine, LuaRuntimeError, LuaSourceText, RuntimeLimits};

    /// Evaluates syntax-driven operations against a runtime engine.
    pub trait Evaluator<E: LuaRuntimeEngine> {
        /// Result type produced by evaluation.
        type Output;

        /// Evaluates an expression source against the engine.
        ///
        /// # Errors
        ///
        /// Returns an error when compilation, evaluation, or limit enforcement fails.
        fn evaluate_expression(
            &self,
            engine: &E,
            source: LuaSourceText<'_>,
            limits: &RuntimeLimits,
        ) -> Result<Self::Output, LuaRuntimeError>;
    }
}

/// Coordinates resolver and evaluator components over an engine.
pub struct EvaluationPlan<E: Engine> {
    /// Engine instance used during evaluation.
    pub engine: E,
}

impl<E: LuaRuntimeEngine> EvaluationPlan<E> {
    /// Compiles and evaluates an expression in a fresh isolated environment.
    ///
    /// # Errors
    ///
    /// Returns an error when environment creation, compilation, evaluation, or limit
    /// enforcement fails.
    pub fn evaluate_expression(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<<E as lyma_runtime::RuntimeValueCodec>::Value, LuaRuntimeError> {
        let compiled = self.engine.compile_expression(source, limits)?;
        let mut environment = self.engine.create_environment()?;
        self.engine
            .evaluate_expression(&compiled, &mut environment, limits)
    }

    /// Compiles and evaluates a chunk in a fresh isolated environment.
    ///
    /// # Errors
    ///
    /// Returns an error when environment creation, compilation, evaluation, or limit
    /// enforcement fails.
    pub fn evaluate_chunk(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<<E as lyma_runtime::RuntimeValueCodec>::Value, LuaRuntimeError> {
        let compiled = self.engine.compile_chunk(source, limits)?;
        let mut environment = self.engine.create_environment()?;
        self.engine
            .evaluate_chunk(&compiled, &mut environment, limits)
    }
}

pub use context::{EvaluationError, ResourceContext};
pub use evaluate::Evaluator;
pub use evaluator::AstEvaluator;
pub use metadata::{DocumentMetadata, EvaluatedDocument};
pub use modules::{
    DenyAllModuleRegistry, InMemoryModuleRegistry, ModuleLookupError, ModuleLookupRequest,
    ModuleRegistry,
};
pub use options::{EvaluationOptions, OptionsError};
pub use profile::{
    DeterministicMode, EvaluationProfile, ProfileError, ProfilePolicy, RuntimeOutputPolicy,
};
pub use resolver::{
    DenyAllResolver, FilesystemResolver, InMemoryResolver, ResolutionContext, ResolutionError,
    ResolutionKind, ResolutionRequest, ResolvedResource, ResolverPolicy, ResourceLocator,
    ResourceResolver,
};
pub use schema::{
    DenyAllSchemaValidator, InMemorySchemaValidator, SchemaValidationError,
    SchemaValidationRequest, SchemaValidator,
};
pub use tags::{
    DenyAllTagResolver, InMemoryTagResolver, TagResolutionError, TagResolutionRequest, TagResolver,
    UnknownTagPolicy,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lyma_runtime::{
        ConversionPolicy, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, RuntimeEnvironment,
        RuntimeEnvironmentFactory, RuntimeLimitKind, RuntimeLimits, RuntimeModule,
        RuntimeModuleFactory, RuntimeValueCodec,
    };
    use lyma_syntax::{LymaValue, source::Span};

    use super::{Engine, EvaluationPlan, LuaSourceText};

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
        builtins: BTreeMap<String, MockValue>,
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
            self.builtins.insert(name.into(), value);
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
            let mut environment = MockEnvironment::default();
            environment.inject_builtin(
                "answer",
                MockValue(LymaValue::Number(lyma_syntax::LymaNumber::Integer(42))),
            )?;
            environment
                .inject_context("name", MockValue(LymaValue::String(String::from("lyma"))))?;
            environment.inject_module(MockModule {
                name: String::from("safe"),
                exports: vec![(String::from("enabled"), MockValue(LymaValue::Boolean(true)))],
            })?;
            Ok(environment)
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
            if source.text.contains("syntax_error") {
                return Err(LuaRuntimeError::syntax_error(
                    self.engine_name(),
                    LuaRuntimePhase::Compile(lyma_runtime::LuaChunkKind::Expression),
                    "mock syntax error",
                    source.span,
                ));
            }

            Ok(source.text.to_owned())
        }

        fn compile_chunk(
            &self,
            source: LuaSourceText<'_>,
            _limits: &RuntimeLimits,
        ) -> Result<Self::CompiledChunk, LuaRuntimeError> {
            Ok(source.text.to_owned())
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
                    RuntimeLimitKind::Instructions,
                    None,
                ));
            }

            if compiled == "answer" {
                return Ok(environment.builtins["answer"].clone());
            }

            Ok(MockValue(LymaValue::String(compiled.clone())))
        }

        fn evaluate_chunk(
            &self,
            compiled: &Self::CompiledChunk,
            environment: &mut Self::Environment,
            _limits: &RuntimeLimits,
        ) -> Result<Self::Value, LuaRuntimeError> {
            if compiled == "context.name" {
                return Ok(environment.context["name"].clone());
            }

            Ok(MockValue(LymaValue::String(compiled.clone())))
        }
    }

    #[test]
    fn evaluation_plan_compiles_and_runs_against_mock_engine() {
        let plan = EvaluationPlan { engine: MockEngine };
        let limits = RuntimeLimits::sandboxed();

        let expression = plan
            .evaluate_expression(LuaSourceText::new("expr", "answer"), &limits)
            .expect("expression should evaluate");
        let chunk = plan
            .evaluate_chunk(LuaSourceText::new("chunk", "context.name"), &limits)
            .expect("chunk should evaluate");

        assert_eq!(
            expression.0,
            LymaValue::Number(lyma_syntax::LymaNumber::Integer(42))
        );
        assert_eq!(chunk.0, LymaValue::String(String::from("lyma")));
    }

    #[test]
    fn mock_engine_reports_backend_neutral_diagnostics() {
        let plan = EvaluationPlan { engine: MockEngine };
        let limits = RuntimeLimits::sandboxed();
        let span = Span::new(lyma_syntax::FileId(7), 1, 13);

        let error = plan
            .evaluate_expression(
                LuaSourceText::new("expr", "syntax_error").with_span(span),
                &limits,
            )
            .expect_err("syntax errors should surface");

        assert_eq!(error.engine, "mock");
        assert_eq!(
            error.phase,
            LuaRuntimePhase::Compile(lyma_runtime::LuaChunkKind::Expression)
        );
        assert_eq!(error.diagnostic.primary_span, Some(span));
    }
}
