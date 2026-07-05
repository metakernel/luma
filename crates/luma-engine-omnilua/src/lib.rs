//! OmniLua-backed implementation of the backend-neutral Luma runtime traits.

#![forbid(unsafe_code)]

/// Value conversion helpers for the `OmniLua` backend.
pub mod convert;
mod engine;
mod env;
/// Resource-limit validation helpers for the `OmniLua` backend.
pub mod limits;

pub use crate::engine::{OmniLuaChunk, OmniLuaEngine, OmniLuaValue};
pub use crate::env::{OmniLuaEnvironment, OmniLuaModule};

/// Stable engine identifier.
#[must_use]
pub const fn engine_name() -> &'static str {
    let _ = core::mem::size_of::<omnilua::Lua>();
    "omnilua"
}
