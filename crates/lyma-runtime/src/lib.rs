//! Engine-agnostic Lua runtime contracts for the Lyma workspace.

#![forbid(unsafe_code)]

pub mod conversion;
pub mod engine;
pub mod environment;
pub mod limits;
pub mod module;

pub use conversion::{ConversionPolicy, RuntimeValueCodec};
pub use engine::{
    Engine, LuaChunkKind, LuaRuntimeEngine, LuaRuntimeError, LuaRuntimePhase, LuaSourceText,
};
pub use environment::{RuntimeEnvironment, RuntimeEnvironmentFactory};
pub use limits::{RuntimeLimitKind, RuntimeLimits};
pub use module::{RuntimeModule, RuntimeModuleFactory};
