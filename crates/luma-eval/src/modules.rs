//! Host-facing safe module registry contracts.

use std::collections::BTreeMap;

use luma_runtime::LuaRuntimeEngine;
use luma_syntax::{Diagnostic, DiagnosticCode, LumaValue, Severity};

use crate::resolver::{
    ResolutionContext, ResolutionError, ResolutionKind, ResolverPolicy, ResourceLocator,
};

/// Request to look up a host-approved module.
#[derive(Debug)]
pub struct ModuleLookupRequest<'a> {
    /// Raw module specifier supplied by the document.
    pub specifier: &'a str,
    /// Previously resolved resource used as a base, if any.
    pub from: Option<&'a ResourceLocator>,
    /// Mutable resolution state for cycle and depth enforcement.
    pub context: &'a mut ResolutionContext,
}

/// Stable module lookup failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleLookupError {
    /// Stable diagnostic describing the lookup failure.
    pub diagnostic: Diagnostic,
}

impl ModuleLookupError {
    /// Creates a stable module lookup error.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

impl From<ResolutionError> for ModuleLookupError {
    fn from(value: ResolutionError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

/// Host-supplied module registry.
pub trait ModuleRegistry<E: LuaRuntimeEngine> {
    /// Resolves a host-approved module into a runtime module instance.
    ///
    /// # Errors
    /// Returns [`ModuleLookupError`] when module access is disabled, violates the
    /// shared safety model, exceeds limits, or the module cannot be constructed.
    fn lookup_module(
        &self,
        engine: &E,
        request: ModuleLookupRequest<'_>,
    ) -> Result<<E as luma_runtime::RuntimeModuleFactory>::Module, ModuleLookupError>;
}

/// Default registry that rejects all module access.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllModuleRegistry;

impl<E: LuaRuntimeEngine> ModuleRegistry<E> for DenyAllModuleRegistry {
    fn lookup_module(
        &self,
        _engine: &E,
        request: ModuleLookupRequest<'_>,
    ) -> Result<<E as luma_runtime::RuntimeModuleFactory>::Module, ModuleLookupError> {
        Err(ModuleLookupError::new(
            DiagnosticCode::UnsafeOperation,
            format!(
                "module lookup requires an explicit host registry for '{}'",
                request.specifier
            ),
        ))
    }
}

/// Static in-memory registry for tests and embedding.
#[derive(Debug, Clone, Default)]
pub struct InMemoryModuleRegistry {
    policy: ResolverPolicy,
    modules: BTreeMap<String, Vec<(String, LumaValue)>>,
}

impl InMemoryModuleRegistry {
    /// Creates an in-memory registry governed by `policy`.
    #[must_use]
    pub const fn new(policy: ResolverPolicy) -> Self {
        Self {
            policy,
            modules: BTreeMap::new(),
        }
    }

    /// Registers a module export table under `specifier`.
    #[must_use]
    pub fn with_module(
        mut self,
        specifier: impl Into<String>,
        exports: Vec<(String, LumaValue)>,
    ) -> Self {
        self.modules.insert(specifier.into(), exports);
        self
    }
}

impl<E: LuaRuntimeEngine> ModuleRegistry<E> for InMemoryModuleRegistry {
    fn lookup_module(
        &self,
        engine: &E,
        request: ModuleLookupRequest<'_>,
    ) -> Result<<E as luma_runtime::RuntimeModuleFactory>::Module, ModuleLookupError> {
        if self.policy.max_depth == 0 {
            return Err(ModuleLookupError::new(
                DiagnosticCode::UnsafeOperation,
                "module lookup is disabled by policy",
            ));
        }

        if std::path::Path::new(request.specifier)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ModuleLookupError::new(
                DiagnosticCode::UnsafeOperation,
                format!(
                    "module resolver rejected parent traversal in '{}'",
                    request.specifier
                ),
            ));
        }

        let specifier = match request.from {
            Some(ResourceLocator::Virtual(base)) => match base.rsplit_once('/') {
                Some((parent, _)) if !parent.is_empty() => {
                    format!("{parent}/{}", request.specifier)
                }
                _ => request.specifier.to_owned(),
            },
            _ => request.specifier.to_owned(),
        };

        request
            .context
            .record(
                ResolutionKind::Module,
                &ResourceLocator::Virtual(specifier.clone()),
            )
            .map_err(ModuleLookupError::from)?;

        let exports = self.modules.get(&specifier).ok_or_else(|| {
            ModuleLookupError::new(
                DiagnosticCode::ImportNotFound,
                format!("module '{specifier}' was not found"),
            )
        })?;

        let mut runtime_exports = Vec::with_capacity(exports.len());
        for (name, value) in exports {
            let runtime_value = engine.from_luma_value(value).map_err(|error| {
                ModuleLookupError::new(
                    error.diagnostic.code,
                    format!(
                        "failed to decode export '{name}' for module '{specifier}': {}",
                        error.diagnostic.message
                    ),
                )
            })?;
            runtime_exports.push((name.clone(), runtime_value));
        }

        engine
            .create_module(specifier, runtime_exports)
            .map_err(|error| {
                ModuleLookupError::new(error.diagnostic.code, error.diagnostic.message.clone())
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use luma_runtime::{
        ConversionPolicy, Engine, LuaRuntimeEngine, LuaRuntimeError, LuaSourceText,
        RuntimeEnvironment, RuntimeEnvironmentFactory, RuntimeLimits, RuntimeModule,
        RuntimeModuleFactory, RuntimeValueCodec,
    };
    use luma_syntax::{LumaValue, source::Span};

    use super::{
        DenyAllModuleRegistry, InMemoryModuleRegistry, ModuleLookupRequest, ModuleRegistry,
    };
    use crate::resolver::{ResolutionContext, ResolverPolicy};

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
            Ok(MockValue(LumaValue::String(compiled.clone())))
        }
        fn evaluate_chunk(
            &self,
            compiled: &Self::CompiledChunk,
            _environment: &mut Self::Environment,
            _limits: &RuntimeLimits,
        ) -> Result<Self::Value, LuaRuntimeError> {
            Ok(MockValue(LumaValue::String(compiled.clone())))
        }
    }

    #[test]
    fn deny_all_registry_rejects_module_lookup() {
        let registry = DenyAllModuleRegistry;
        let mut context = ResolutionContext::new(1);
        let error = registry
            .lookup_module(
                &MockEngine,
                ModuleLookupRequest {
                    specifier: "safe",
                    from: None,
                    context: &mut context,
                },
            )
            .expect_err("deny-all registry should reject lookup");
        assert_eq!(
            error.diagnostic.code,
            luma_syntax::DiagnosticCode::UnsafeOperation
        );
    }

    #[test]
    fn in_memory_registry_creates_runtime_modules() {
        let registry = InMemoryModuleRegistry::new(ResolverPolicy {
            max_depth: 4,
            ..ResolverPolicy::deny_all()
        })
        .with_module(
            "safe",
            vec![(String::from("enabled"), LumaValue::Boolean(true))],
        );
        let mut context = ResolutionContext::new(4);

        let module = registry
            .lookup_module(
                &MockEngine,
                ModuleLookupRequest {
                    specifier: "safe",
                    from: None,
                    context: &mut context,
                },
            )
            .expect("module should be resolved");

        assert_eq!(module.module_name(), "safe");
        assert_eq!(
            module.exports().expect("exports should load")[0].0,
            "enabled"
        );
    }

    #[test]
    fn in_memory_registry_rejects_traversal_with_stable_diagnostic() {
        let registry = InMemoryModuleRegistry::new(ResolverPolicy {
            max_depth: 4,
            ..ResolverPolicy::deny_all()
        });
        let mut context = ResolutionContext::new(4);

        let error = registry
            .lookup_module(
                &MockEngine,
                ModuleLookupRequest {
                    specifier: "../unsafe",
                    from: None,
                    context: &mut context,
                },
            )
            .expect_err("parent traversal should be rejected");

        assert_eq!(
            error.diagnostic.code,
            luma_syntax::DiagnosticCode::UnsafeOperation
        );
        let _ = Span::new(luma_syntax::FileId(0), 0, 0);
    }
}
