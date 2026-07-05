//! Runtime scope helpers for evaluator implementations.

use luma_runtime::{LuaRuntimeEngine, RuntimeEnvironment};
use luma_syntax::{LumaNull, LumaNumber, LumaSequence, LumaValue};

use crate::{ResourceLocator, context::EvaluationError};

/// Runtime scope carrying a forkable environment and navigation metadata.
pub struct RuntimeScope<E: LuaRuntimeEngine> {
    /// Runtime environment for this scope.
    pub environment: E::Environment,
    /// Current path within the evaluated document.
    pub path: Vec<LumaValue>,
    /// Current file/resource locator.
    pub locator: Option<ResourceLocator>,
    /// Root file/resource locator.
    pub root_locator: Option<ResourceLocator>,
}

struct NavigationContext<'a> {
    path: &'a [LumaValue],
    locator: Option<&'a ResourceLocator>,
    root_locator: Option<&'a ResourceLocator>,
    here: &'a LumaValue,
    parent: Option<&'a LumaValue>,
    root: Option<&'a LumaValue>,
}

impl<E: LuaRuntimeEngine> RuntimeScope<E> {
    /// Creates a root scope.
    #[must_use]
    pub const fn new(
        environment: E::Environment,
        locator: Option<ResourceLocator>,
        root_locator: Option<ResourceLocator>,
    ) -> Self {
        Self {
            environment,
            path: Vec::new(),
            locator,
            root_locator,
        }
    }

    /// Forks the scope and injects navigation bindings.
    ///
    /// # Errors
    ///
    /// Returns an error when environment forking or runtime context injection
    /// fails.
    pub fn child(
        &self,
        engine: &E,
        segment: Option<LumaValue>,
        here: &LumaValue,
        parent: Option<&LumaValue>,
        root: Option<&LumaValue>,
    ) -> Result<Self, EvaluationError> {
        let mut environment = self.environment.fork_isolated()?;
        let mut path = self.path.clone();
        if let Some(segment) = segment {
            path.push(segment);
        }
        inject_navigation(
            engine,
            &mut environment,
            &NavigationContext {
                path: &path,
                locator: self.locator.as_ref(),
                root_locator: self.root_locator.as_ref(),
                here,
                parent,
                root,
            },
        )?;
        Ok(Self {
            environment,
            path,
            locator: self.locator.clone(),
            root_locator: self.root_locator.clone(),
        })
    }

    /// Binds a stable value into the current runtime scope.
    ///
    /// # Errors
    ///
    /// Returns an error when value conversion or runtime context injection
    /// fails.
    pub fn bind_value(
        &mut self,
        engine: &E,
        name: &str,
        value: &LumaValue,
    ) -> Result<(), EvaluationError> {
        let runtime_value = engine.from_luma_value(value)?;
        self.environment.inject_context(name, runtime_value)?;
        Ok(())
    }

    /// Forks the current lexical scope while preserving path metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying runtime environment cannot be
    /// forked.
    pub fn fork(&self) -> Result<Self, EvaluationError> {
        Ok(Self {
            environment: self.environment.fork_isolated()?,
            path: self.path.clone(),
            locator: self.locator.clone(),
            root_locator: self.root_locator.clone(),
        })
    }
}

fn inject_navigation<E: LuaRuntimeEngine>(
    engine: &E,
    environment: &mut E::Environment,
    navigation: &NavigationContext<'_>,
) -> Result<(), EvaluationError> {
    for (name, value) in [
        ("_here", navigation.here.clone()),
        (
            "_parent",
            navigation
                .parent
                .cloned()
                .unwrap_or(LumaValue::Null(LumaNull)),
        ),
        (
            "_root",
            navigation
                .root
                .cloned()
                .unwrap_or(LumaValue::Null(LumaNull)),
        ),
        (
            "_path",
            LumaValue::Sequence(LumaSequence {
                items: navigation.path.to_vec(),
                span: None,
            }),
        ),
        (
            "_file",
            LumaValue::String(
                navigation
                    .locator
                    .map_or_else(String::new, ResourceLocator::identity),
            ),
        ),
        (
            "_luma",
            LumaValue::String(
                navigation
                    .root_locator
                    .map_or_else(|| String::from("luma"), ResourceLocator::identity),
            ),
        ),
    ] {
        let runtime_value = engine.from_luma_value(&value)?;
        environment.inject_context(name, runtime_value)?;
    }
    Ok(())
}

/// Builds a stable path segment from a mapping key.
#[must_use]
pub fn path_segment_from_key(key: &luma_syntax::LumaKey) -> LumaValue {
    match key {
        luma_syntax::LumaKey::String(value) => LumaValue::String(value.clone()),
        luma_syntax::LumaKey::Number(value) => LumaValue::Number(value.clone()),
        luma_syntax::LumaKey::Boolean(value) => LumaValue::Boolean(*value),
        luma_syntax::LumaKey::Host(value) => LumaValue::HostObject(value.clone()),
    }
}

/// Builds a stable 1-based sequence path segment.
///
/// # Errors
///
/// Returns an error when the index cannot be represented as a stable 1-based
/// `i64`.
pub fn path_segment_from_index(index: usize) -> Result<LumaValue, EvaluationError> {
    let index = i64::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            EvaluationError::new(
                luma_syntax::DiagnosticCode::SerializationError,
                "path index exceeded supported integer range",
            )
        })?;
    Ok(LumaValue::Number(LumaNumber::Integer(index)))
}
