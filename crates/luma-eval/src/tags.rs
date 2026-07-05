//! Tag resolver contracts and deny-by-default implementations.

use std::collections::BTreeMap;
use std::sync::Arc;

use luma_syntax::{Diagnostic, DiagnosticCode, LumaTag, LumaValue, Severity};

/// Policy for unknown tags during evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownTagPolicy {
    /// Preserve unknown tags as stable tagged values.
    Preserve,
    /// Reject unknown tags for every document.
    Reject,
    /// Preserve unknown tags unless the document is schema-validated.
    #[default]
    RejectForSchemaValidatedDocuments,
}

/// Request to construct a tagged value.
#[derive(Debug)]
pub struct TagResolutionRequest<'a> {
    /// Parsed tag to resolve.
    pub tag: &'a LumaTag,
    /// Untagged payload value.
    pub value: &'a LumaValue,
}

/// Stable tag resolution error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagResolutionError {
    /// Stable diagnostic describing tag resolution failure.
    pub diagnostic: Diagnostic,
}

impl TagResolutionError {
    /// Creates a stable tag resolution error.
    #[must_use]
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message.into();
        Self { diagnostic }
    }
}

/// Host-supplied tag resolver.
pub trait TagResolver {
    /// Constructs a host-defined tagged value.
    ///
    /// # Errors
    /// Returns [`TagResolutionError`] when the tag is unsupported or the host
    /// resolver rejects the payload.
    fn resolve_tag(
        &self,
        request: TagResolutionRequest<'_>,
    ) -> Result<LumaValue, TagResolutionError>;
}

/// Default resolver that rejects all tags.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllTagResolver;

impl TagResolver for DenyAllTagResolver {
    fn resolve_tag(
        &self,
        request: TagResolutionRequest<'_>,
    ) -> Result<LumaValue, TagResolutionError> {
        Err(TagResolutionError::new(
            DiagnosticCode::UnknownTag,
            format!(
                "tag '!{}' requires an explicit host tag resolver",
                request.tag.name.value
            ),
        ))
    }
}

type TagHandler = Arc<dyn Fn(&LumaValue) -> Result<LumaValue, TagResolutionError> + Send + Sync>;

/// In-memory resolver for tests and embedding.
#[derive(Clone, Default)]
pub struct InMemoryTagResolver {
    handlers: BTreeMap<String, TagHandler>,
}

impl std::fmt::Debug for InMemoryTagResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryTagResolver")
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl InMemoryTagResolver {
    /// Creates an empty in-memory tag resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tag handler for `tag_name`.
    #[must_use]
    pub fn with_handler<F>(mut self, tag_name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(&LumaValue) -> Result<LumaValue, TagResolutionError> + Send + Sync + 'static,
    {
        self.handlers.insert(tag_name.into(), Arc::new(handler));
        self
    }
}

impl TagResolver for InMemoryTagResolver {
    fn resolve_tag(
        &self,
        request: TagResolutionRequest<'_>,
    ) -> Result<LumaValue, TagResolutionError> {
        let handler = self.handlers.get(&request.tag.name.value).ok_or_else(|| {
            TagResolutionError::new(
                DiagnosticCode::UnknownTag,
                format!("unknown tag '!{}'", request.tag.name.value),
            )
        })?;
        handler(request.value)
    }
}

#[cfg(test)]
mod tests {
    use luma_syntax::{FileId, LumaTagName, Span};

    use super::{DenyAllTagResolver, InMemoryTagResolver, TagResolutionRequest, TagResolver};

    #[test]
    fn deny_all_tag_resolver_rejects_tag_construction() {
        let tag = luma_syntax::LumaTag {
            name: LumaTagName {
                value: String::from("env"),
                span: Span::new(FileId(1), 1, 4),
            },
            span: Span::new(FileId(1), 0, 4),
        };
        let error = DenyAllTagResolver
            .resolve_tag(TagResolutionRequest {
                tag: &tag,
                value: &luma_syntax::LumaValue::String(String::from("x")),
            })
            .expect_err("tag resolution should be rejected by default");
        assert_eq!(
            error.diagnostic.code,
            luma_syntax::DiagnosticCode::UnknownTag
        );
    }

    #[test]
    fn in_memory_tag_resolver_runs_registered_handlers() {
        let tag = luma_syntax::LumaTag {
            name: LumaTagName {
                value: String::from("upper"),
                span: Span::new(FileId(1), 1, 6),
            },
            span: Span::new(FileId(1), 0, 6),
        };
        let resolver = InMemoryTagResolver::new().with_handler("upper", |value| match value {
            luma_syntax::LumaValue::String(text) => {
                Ok(luma_syntax::LumaValue::String(text.to_uppercase()))
            }
            _ => Err(super::TagResolutionError::new(
                luma_syntax::DiagnosticCode::InvalidTagResolverResult,
                "expected a string payload",
            )),
        });

        let result = resolver
            .resolve_tag(TagResolutionRequest {
                tag: &tag,
                value: &luma_syntax::LumaValue::String(String::from("hello")),
            })
            .expect("registered tag handler should run");

        assert_eq!(
            result,
            luma_syntax::LumaValue::String(String::from("HELLO"))
        );
    }
}
