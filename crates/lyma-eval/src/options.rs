//! Concrete evaluator-facing option bundle.

use lyma_runtime::LuaRuntimeEngine;
use lyma_syntax::{Diagnostic, DiagnosticCode, Severity};

use crate::{
    modules::ModuleRegistry,
    profile::{ProfilePolicy, RESTRICTED_EVALUATION_PROFILE},
    resolver::ResourceResolver,
    schema::SchemaValidator,
    tags::{TagResolver, UnknownTagPolicy},
};

/// Stable option validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError {
    /// Stable diagnostic describing the missing capability.
    pub diagnostic: Diagnostic,
}

impl OptionsError {
    /// Creates a stable options validation error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::UnsafeOperation, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

/// Host-supplied evaluator options. Every capability is opt-in.
pub struct EvaluationOptions<'a, E: LuaRuntimeEngine> {
    /// Active evaluation profile policy.
    pub profile: &'a dyn ProfilePolicy,
    /// Shared resource resolver for imports/includes and similar loads.
    pub resolver: Option<&'a dyn ResourceResolver>,
    /// Host-supplied module registry.
    pub module_registry: Option<&'a dyn ModuleRegistry<E>>,
    /// Host-supplied tag resolver.
    pub tag_resolver: Option<&'a dyn TagResolver>,
    /// Host-supplied schema validator.
    pub schema_validator: Option<&'a dyn SchemaValidator>,
    /// Policy to apply when a tag is unknown to the host resolver.
    pub unknown_tag_policy: UnknownTagPolicy,
}

impl<E: LuaRuntimeEngine> Default for EvaluationOptions<'_, E> {
    fn default() -> Self {
        Self {
            profile: &RESTRICTED_EVALUATION_PROFILE,
            resolver: None,
            module_registry: None,
            tag_resolver: None,
            schema_validator: None,
            unknown_tag_policy: UnknownTagPolicy::RejectForSchemaValidatedDocuments,
        }
    }
}

impl<'a, E: LuaRuntimeEngine> EvaluationOptions<'a, E> {
    /// Returns the configured resource resolver for `capability`.
    ///
    /// # Errors
    /// Returns [`OptionsError`] when no host resource resolver was supplied.
    pub fn require_resolver(
        &self,
        capability: &'static str,
    ) -> Result<&'a dyn ResourceResolver, OptionsError> {
        self.resolver.ok_or_else(|| {
            OptionsError::new(format!(
                "{capability} requires an explicit host resource resolver"
            ))
        })
    }

    /// Returns the configured module registry.
    ///
    /// # Errors
    /// Returns [`OptionsError`] when no host module registry was supplied.
    pub fn require_module_registry(&self) -> Result<&'a dyn ModuleRegistry<E>, OptionsError> {
        self.module_registry.ok_or_else(|| {
            OptionsError::new("module lookup requires an explicit host module registry")
        })
    }

    /// Returns the configured tag resolver.
    ///
    /// # Errors
    /// Returns [`OptionsError`] when no host tag resolver was supplied.
    pub fn require_tag_resolver(&self) -> Result<&'a dyn TagResolver, OptionsError> {
        self.tag_resolver.ok_or_else(|| {
            OptionsError::new("tag construction requires an explicit host tag resolver")
        })
    }

    /// Returns the configured schema validator.
    ///
    /// # Errors
    /// Returns [`OptionsError`] when no host schema validator was supplied.
    pub fn require_schema_validator(&self) -> Result<&'a dyn SchemaValidator, OptionsError> {
        self.schema_validator.ok_or_else(|| {
            OptionsError::new("schema validation requires an explicit host schema validator")
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lyma_runtime::{
        ConversionPolicy, Engine, LuaRuntimeEngine, LuaRuntimeError, LuaSourceText,
        RuntimeEnvironment, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule,
        RuntimeModuleFactory, RuntimeValueCodec,
    };
    use lyma_syntax::LymaValue;

    use super::EvaluationOptions;

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
            _environment: &mut Self::Environment,
            _limits: &RuntimeLimits,
        ) -> Result<Self::Value, LuaRuntimeError> {
            Ok(MockValue(LymaValue::String(compiled.clone())))
        }
        fn evaluate_chunk(
            &self,
            compiled: &Self::CompiledChunk,
            _environment: &mut Self::Environment,
            _limits: &RuntimeLimits,
        ) -> Result<Self::Value, LuaRuntimeError> {
            Ok(MockValue(LymaValue::String(compiled.clone())))
        }
    }

    #[test]
    fn default_options_require_explicit_capabilities() {
        let options = EvaluationOptions::<MockEngine>::default();
        assert!(options.require_resolver("imports").is_err());
        assert!(options.require_module_registry().is_err());
        assert!(options.require_tag_resolver().is_err());
        assert!(options.require_schema_validator().is_err());
    }
}
