//! Runtime scope helpers for evaluator implementations.

use lyma_runtime::{LuaRuntimeEngine, RuntimeEnvironment};
use lyma_syntax::{LymaNull, LymaNumber, LymaSequence, LymaValue};

use crate::{ResourceLocator, context::EvaluationError};

/// Runtime scope carrying a forkable environment and navigation metadata.
pub struct RuntimeScope<E: LuaRuntimeEngine> {
    /// Runtime environment for this scope.
    pub environment: E::Environment,
    /// Current path within the evaluated document.
    pub path: Vec<LymaValue>,
    /// Current file/resource locator.
    pub locator: Option<ResourceLocator>,
    /// Root file/resource locator.
    pub root_locator: Option<ResourceLocator>,
}

struct NavigationContext<'a> {
    path: &'a [LymaValue],
    locator: Option<&'a ResourceLocator>,
    root_locator: Option<&'a ResourceLocator>,
    here: &'a LymaValue,
    parent: Option<&'a LymaValue>,
    root: Option<&'a LymaValue>,
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
        segment: Option<LymaValue>,
        here: &LymaValue,
        parent: Option<&LymaValue>,
        root: Option<&LymaValue>,
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
        value: &LymaValue,
    ) -> Result<(), EvaluationError> {
        let runtime_value = engine.from_lyma_value(value)?;
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
                .unwrap_or(LymaValue::Null(LymaNull)),
        ),
        (
            "_root",
            navigation
                .root
                .cloned()
                .unwrap_or(LymaValue::Null(LymaNull)),
        ),
        (
            "_path",
            LymaValue::Sequence(LymaSequence {
                items: navigation.path.to_vec(),
                span: None,
            }),
        ),
        (
            "_file",
            LymaValue::String(
                navigation
                    .locator
                    .map_or_else(String::new, ResourceLocator::identity),
            ),
        ),
        (
            "_lyma",
            LymaValue::String(
                navigation
                    .root_locator
                    .map_or_else(|| String::from("lyma"), ResourceLocator::identity),
            ),
        ),
    ] {
        let runtime_value = engine.from_lyma_value(&value)?;
        environment.inject_context(name, runtime_value)?;
    }
    Ok(())
}

/// Builds a stable path segment from a mapping key.
#[must_use]
pub fn path_segment_from_key(key: &lyma_syntax::LymaKey) -> LymaValue {
    match key {
        lyma_syntax::LymaKey::String(value) => LymaValue::String(value.clone()),
        lyma_syntax::LymaKey::Number(value) => LymaValue::Number(value.clone()),
        lyma_syntax::LymaKey::Boolean(value) => LymaValue::Boolean(*value),
        lyma_syntax::LymaKey::Host(value) => LymaValue::HostObject(value.clone()),
    }
}

/// Builds a stable 1-based sequence path segment.
///
/// # Errors
///
/// Returns an error when the index cannot be represented as a stable 1-based
/// `i64`.
pub fn path_segment_from_index(index: usize) -> Result<LymaValue, EvaluationError> {
    let index = i64::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            EvaluationError::new(
                lyma_syntax::DiagnosticCode::SerializationError,
                "path index exceeded supported integer range",
            )
        })?;
    Ok(LymaValue::Number(LymaNumber::Integer(index)))
}
