//! Import/include helpers.

use luma_parser::parse_str;
use luma_syntax::{DiagnosticCode, FileId, LumaFile};

use crate::{
    ResolutionContext, ResolutionKind, ResolutionRequest, ResourceLocator, ResourceResolver,
    context::EvaluationError,
};

/// Resolves and parses a Luma resource.
///
/// # Errors
///
/// Returns an error when resource resolution fails, parsing produces an error
/// diagnostic, or the resolved file has no documents.
pub fn load_luma_resource(
    resolver: &dyn ResourceResolver,
    kind: ResolutionKind,
    specifier: &str,
    from: Option<&ResourceLocator>,
    context: &mut ResolutionContext,
) -> Result<(ResourceLocator, LumaFile, String), EvaluationError> {
    let resolved_resource = resolver.resolve(ResolutionRequest {
        kind,
        specifier,
        from,
        context,
    })?;
    let parsed = parse_str(FileId(0), specifier, &resolved_resource.content);
    if let Some(diagnostic) = parsed
        .diagnostics
        .into_iter()
        .find(|d| d.severity == luma_syntax::Severity::Error)
    {
        return Err(EvaluationError { diagnostic });
    }
    if parsed.file.documents.is_empty() {
        return Err(EvaluationError::new(
            DiagnosticCode::ImportNotFound,
            "resolved Luma resource did not contain a document",
        ));
    }
    Ok((
        resolved_resource.locator,
        parsed.file,
        resolved_resource.content,
    ))
}
