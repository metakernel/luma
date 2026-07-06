//! Isolated runtime environment contracts.

use crate::engine::LuaRuntimeError;

/// Mutable execution environment exposed by a Lua backend.
pub trait RuntimeEnvironment {
    /// Engine-specific value representation.
    type RuntimeValue;
    /// Engine-specific module representation.
    type RuntimeModule;

    /// Creates a child environment isolated from later parent mutations.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot produce an isolated child environment.
    fn fork_isolated(&self) -> Result<Self, LuaRuntimeError>
    where
        Self: Sized;

    /// Injects a safe built-in binding.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend rejects or cannot install the binding.
    fn inject_builtin(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError>;

    /// Injects a context variable scoped to the current evaluation.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend rejects or cannot install the context value.
    fn inject_context(
        &mut self,
        name: impl Into<String>,
        value: Self::RuntimeValue,
    ) -> Result<(), LuaRuntimeError>;

    /// Injects a pre-approved safe module.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend rejects or cannot install the module.
    fn inject_module(&mut self, module: Self::RuntimeModule) -> Result<(), LuaRuntimeError>;
}

/// Creates isolated execution environments.
pub trait RuntimeEnvironmentFactory {
    /// Engine-specific value representation.
    type RuntimeValue;
    /// Engine-specific module representation.
    type RuntimeModule;
    /// Engine-specific environment representation.
    type Environment: RuntimeEnvironment<RuntimeValue = Self::RuntimeValue, RuntimeModule = Self::RuntimeModule>;

    /// Creates a new isolated environment.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot construct a fresh isolated environment.
    fn create_environment(&self) -> Result<Self::Environment, LuaRuntimeError>;
}
