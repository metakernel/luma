//! Evaluation context and shared error types.

use lyma_runtime::LuaRuntimeError;
use lyma_syntax::{Diagnostic, DiagnosticCode, Severity};

use crate::{
    ModuleLookupError, OptionsError, ResolutionError, ResourceLocator, SchemaValidationError,
    TagResolutionError,
};

/// Stable evaluator failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationError {
    /// Stable diagnostic payload.
    pub diagnostic: Diagnostic,
}

impl EvaluationError {
    /// Creates a new evaluator error.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

impl From<LuaRuntimeError> for EvaluationError {
    fn from(value: LuaRuntimeError) -> Self {
        Self {
            diagnostic: (*value.diagnostic).clone(),
        }
    }
}

impl From<ResolutionError> for EvaluationError {
    fn from(value: ResolutionError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

impl From<ModuleLookupError> for EvaluationError {
    fn from(value: ModuleLookupError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

impl From<SchemaValidationError> for EvaluationError {
    fn from(value: SchemaValidationError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

impl From<TagResolutionError> for EvaluationError {
    fn from(value: TagResolutionError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

impl From<OptionsError> for EvaluationError {
    fn from(value: OptionsError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

/// Evaluation-side resource metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceContext {
    /// Current resource locator.
    pub locator: Option<ResourceLocator>,
    /// Root resource locator.
    pub root: Option<ResourceLocator>,
    /// Human-readable source name.
    pub source_name: String,
}

impl ResourceContext {
    /// Creates a root resource context.
    #[must_use]
    pub fn new(source_name: impl Into<String>, locator: Option<ResourceLocator>) -> Self {
        Self {
            root: locator.clone(),
            locator,
            source_name: source_name.into(),
        }
    }

    /// Creates a child resource context.
    #[must_use]
    pub fn child(&self, source_name: impl Into<String>, locator: ResourceLocator) -> Self {
        Self {
            locator: Some(locator),
            root: self.root.clone().or_else(|| self.locator.clone()),
            source_name: source_name.into(),
        }
    }
}
