//! Module-boundary traits for exposing safe Lua APIs.

use crate::engine::LuaRuntimeError;

/// Backend-specific safe module descriptor.
pub trait RuntimeModule {
    /// Engine-specific value representation exported by the module.
    type RuntimeValue;

    /// Stable public module name.
    fn module_name(&self) -> &str;

    /// Returns exported named values for installation into a runtime.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot enumerate or clone the module exports.
    fn exports(&self) -> Result<Vec<(String, Self::RuntimeValue)>, LuaRuntimeError>;
}

/// Factory for creating backend-specific safe modules.
pub trait RuntimeModuleFactory {
    /// Engine-specific value representation.
    type RuntimeValue;
    /// Engine-specific module representation.
    type Module: RuntimeModule<RuntimeValue = Self::RuntimeValue>;

    /// Creates a module from explicit exports.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot construct the safe module wrapper.
    fn create_module(
        &self,
        name: impl Into<String>,
        exports: Vec<(String, Self::RuntimeValue)>,
    ) -> Result<Self::Module, LuaRuntimeError>;
}
