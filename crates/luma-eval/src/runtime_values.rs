//! Runtime-value conversion helpers shared by evaluator entry points.

use luma_runtime::{ConversionPolicy, LuaRuntimeEngine};
use luma_syntax::{LumaValue, Span};

use crate::{context::EvaluationError, freeze::deep_copy_value, profile::EvaluationProfile};

/// Converts a runtime value into a detached stable value under the active profile.
///
/// # Errors
///
/// Returns an error when conversion fails or `data_only` mode rejects runtime-only values.
pub fn stabilize_runtime_value<E: LuaRuntimeEngine>(
    engine: &E,
    value: &E::Value,
    profile: &EvaluationProfile,
    origin_span: Option<Span>,
    data_only: bool,
) -> Result<LumaValue, EvaluationError> {
    let stable = engine.to_luma_value(
        value,
        &ConversionPolicy {
            allow_functions: profile.runtime_output.allow_function_values && !data_only,
            allow_userdata: profile.runtime_output.allow_user_data && !data_only,
            allow_host_objects: profile.runtime_output.allow_host_objects && !data_only,
            origin_span,
        },
    )?;
    deep_copy_value(&stable, data_only)
}

/// Produces a detached stable value from an already-converted `LumaValue`.
///
/// # Errors
///
/// Returns an error when `data_only` mode rejects runtime-only values.
pub fn stabilize_luma_value(
    value: &LumaValue,
    data_only: bool,
) -> Result<LumaValue, EvaluationError> {
    deep_copy_value(value, data_only)
}
