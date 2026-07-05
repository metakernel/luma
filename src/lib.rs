//! Public library crate for the Luma workspace.

#![forbid(unsafe_code)]

pub mod prelude;
pub mod tooling;

#[cfg(feature = "syntax")]
pub use luma_syntax as syntax;

#[cfg(feature = "parser")]
pub use luma_parser as parser;

#[cfg(feature = "runtime")]
pub use luma_runtime as runtime;

#[cfg(feature = "eval")]
pub use luma_eval as eval;

#[cfg(feature = "engine-omnilua")]
pub use luma_engine_omnilua as engine_omnilua;

/// Returns the crate version at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
