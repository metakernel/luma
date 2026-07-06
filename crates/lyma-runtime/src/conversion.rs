//! Conversion contracts between backend Lua values and `LymaValue`.

use lyma_syntax::{LymaValue, source::Span};

use crate::engine::LuaRuntimeError;

/// Policy knobs controlling Lua-to-Lyma conversion behavior.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversionPolicy {
    /// Whether runtime functions may be preserved as host placeholders.
    pub allow_functions: bool,
    /// Whether userdata values may be preserved as host placeholders.
    pub allow_userdata: bool,
    /// Whether host-object values may be preserved as host placeholders.
    pub allow_host_objects: bool,
    /// Optional span to attach to conversion diagnostics.
    pub origin_span: Option<Span>,
}

/// Converts between backend Lua values and stable `LymaValue` data.
#[allow(clippy::wrong_self_convention)]
pub trait RuntimeValueCodec {
    /// Engine-specific owned Lua value representation.
    type Value;
    /// Engine-specific immutable or detached value representation.
    type FrozenValue;

    /// Converts a backend Lua value into a stable `LymaValue`.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend value cannot be represented as a stable `LymaValue`.
    fn to_lyma_value(
        &self,
        value: &Self::Value,
        policy: &ConversionPolicy,
    ) -> Result<LymaValue, LuaRuntimeError>;

    /// Converts a stable `LymaValue` into a backend Lua value.
    ///
    /// # Errors
    ///
    /// Returns an error when the stable value cannot be materialized by the backend.
    fn from_lyma_value(&self, value: &LymaValue) -> Result<Self::Value, LuaRuntimeError>;

    /// Creates an immutable or detached snapshot of a backend Lua value.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot freeze or detach the provided value.
    fn freeze_value(&self, value: &Self::Value) -> Result<Self::FrozenValue, LuaRuntimeError>;

    /// Clones a backend Lua value within the engine boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot duplicate the provided value.
    fn clone_value(&self, value: &Self::Value) -> Result<Self::Value, LuaRuntimeError>;

    /// Re-materializes a previously frozen value.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot thaw the frozen value into an owned runtime value.
    fn thaw_value(&self, value: &Self::FrozenValue) -> Result<Self::Value, LuaRuntimeError>;
}
