//! Schema validation contracts and safe defaults.

use std::collections::BTreeMap;

use lyma_syntax::{Diagnostic, DiagnosticCode, LymaValue, Severity};

use crate::resolver::{ResolutionContext, ResolutionError, ResolutionKind, ResourceLocator};

/// Schema validation request.
#[derive(Debug)]
pub struct SchemaValidationRequest<'a> {
    /// Raw schema specifier supplied by the document.
    pub schema: &'a str,
    /// Value being validated.
    pub value: &'a LymaValue,
    /// Previously resolved resource used as a base, if any.
    pub from: Option<&'a ResourceLocator>,
    /// Mutable resolution state for cycle and depth enforcement.
    pub context: &'a mut ResolutionContext,
}

/// Stable schema validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationError {
    /// Stable diagnostic describing validation failure.
    pub diagnostic: Diagnostic,
}

impl SchemaValidationError {
    /// Creates a stable schema validation error.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

impl From<ResolutionError> for SchemaValidationError {
    fn from(value: ResolutionError) -> Self {
        Self {
            diagnostic: value.diagnostic,
        }
    }
}

/// Host-supplied schema validator.
pub trait SchemaValidator {
    /// Validates a value against a host-approved schema.
    ///
    /// # Errors
    /// Returns [`SchemaValidationError`] when validation is disabled, violates the
    /// shared safety model, exceeds limits, or the value does not satisfy the schema.
    fn validate(&self, request: SchemaValidationRequest<'_>) -> Result<(), SchemaValidationError>;
}

/// Default validator that rejects all schema validation.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllSchemaValidator;

impl SchemaValidator for DenyAllSchemaValidator {
    fn validate(&self, request: SchemaValidationRequest<'_>) -> Result<(), SchemaValidationError> {
        Err(SchemaValidationError::new(
            DiagnosticCode::UnsafeOperation,
            format!(
                "schema validation requires an explicit host validator for '{}'",
                request.schema
            ),
        ))
    }
}

/// In-memory schema validator for tests and embedding.
#[derive(Debug, Clone, Default)]
pub struct InMemorySchemaValidator {
    max_depth: usize,
    schemas: BTreeMap<String, LymaValue>,
}

impl InMemorySchemaValidator {
    /// Creates an in-memory validator governed by `max_depth`.
    #[must_use]
    pub const fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            schemas: BTreeMap::new(),
        }
    }

    /// Registers an expected value for `specifier`.
    #[must_use]
    pub fn with_schema(mut self, specifier: impl Into<String>, expected: LymaValue) -> Self {
        self.schemas.insert(specifier.into(), expected);
        self
    }
}

impl SchemaValidator for InMemorySchemaValidator {
    fn validate(&self, request: SchemaValidationRequest<'_>) -> Result<(), SchemaValidationError> {
        if self.max_depth == 0 {
            return Err(SchemaValidationError::new(
                DiagnosticCode::UnsafeOperation,
                "schema validation is disabled by policy",
            ));
        }

        if std::path::Path::new(request.schema)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(SchemaValidationError::new(
                DiagnosticCode::UnsafeOperation,
                format!(
                    "schema resolver rejected parent traversal in '{}'",
                    request.schema
                ),
            ));
        }

        let specifier = match request.from {
            Some(ResourceLocator::Virtual(base)) => match base.rsplit_once('/') {
                Some((parent, _)) if !parent.is_empty() => format!("{parent}/{}", request.schema),
                _ => request.schema.to_owned(),
            },
            _ => request.schema.to_owned(),
        };

        request
            .context
            .record(
                ResolutionKind::Schema,
                &ResourceLocator::Virtual(specifier.clone()),
            )
            .map_err(SchemaValidationError::from)?;

        let expected = self.schemas.get(&specifier).ok_or_else(|| {
            SchemaValidationError::new(
                DiagnosticCode::ImportNotFound,
                format!("schema '{specifier}' was not found"),
            )
        })?;

        if expected == request.value {
            return Ok(());
        }

        Err(SchemaValidationError::new(
            DiagnosticCode::SchemaValidationError,
            format!("value did not satisfy schema '{specifier}'"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DenyAllSchemaValidator, InMemorySchemaValidator, SchemaValidationRequest, SchemaValidator,
    };
    use crate::resolver::ResolutionContext;

    #[test]
    fn deny_all_schema_validator_rejects_by_default() {
        let mut context = ResolutionContext::new(1);
        let error = DenyAllSchemaValidator
            .validate(SchemaValidationRequest {
                schema: "schemas/app",
                value: &lyma_syntax::LymaValue::Boolean(true),
                from: None,
                context: &mut context,
            })
            .expect_err("schema validation should be denied by default");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::UnsafeOperation
        );
    }

    #[test]
    fn in_memory_schema_validator_uses_stable_resolution_rules() {
        let validator = InMemorySchemaValidator::new(4)
            .with_schema("schemas/flag", lyma_syntax::LymaValue::Boolean(true));
        let mut context = ResolutionContext::new(4);

        validator
            .validate(SchemaValidationRequest {
                schema: "schemas/flag",
                value: &lyma_syntax::LymaValue::Boolean(true),
                from: None,
                context: &mut context,
            })
            .expect("matching schema should validate");

        let mut context = ResolutionContext::new(4);
        let error = validator
            .validate(SchemaValidationRequest {
                schema: "../escape",
                value: &lyma_syntax::LymaValue::Boolean(true),
                from: None,
                context: &mut context,
            })
            .expect_err("parent traversal should be rejected");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::UnsafeOperation
        );
    }
}
