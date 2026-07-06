//! Host-facing resource resolution contracts and safe defaults.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use lyma_syntax::{Diagnostic, DiagnosticCode, Severity};

/// Resource category using the shared resolver safety model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolutionKind {
    /// Resolve an `import` target.
    Import,
    /// Resolve an `include` target.
    Include,
    /// Resolve a schema reference.
    Schema,
    /// Resolve a module specifier.
    Module,
}

impl ResolutionKind {
    /// Returns the stable diagnostic label for this resolution category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Include => "include",
            Self::Schema => "schema",
            Self::Module => "module",
        }
    }
}

/// Stable locator for resolved resources.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceLocator {
    /// Canonical filesystem path.
    File(PathBuf),
    /// URI-like identifier.
    Uri(String),
    /// Host-defined virtual identifier.
    Virtual(String),
}

impl ResourceLocator {
    /// Returns a stable identity string for cycle detection.
    #[must_use]
    pub fn identity(&self) -> String {
        match self {
            Self::File(path) => format!("file:{}", path.display()),
            Self::Uri(uri) => format!("uri:{uri}"),
            Self::Virtual(id) => format!("virtual:{id}"),
        }
    }
}

/// Shared resolver policy. Defaults deny all external access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverPolicy {
    /// Canonical roots that resolved filesystem paths must remain within.
    pub allowed_roots: Vec<PathBuf>,
    /// URI schemes the host permits this resolver to accept.
    pub allowed_uri_schemes: BTreeSet<String>,
    /// Whether networked schemes such as `http`/`https` are permitted.
    pub allow_network: bool,
    /// Maximum number of unique resources that may be resolved in one context.
    pub max_depth: usize,
}

impl Default for ResolverPolicy {
    fn default() -> Self {
        Self::deny_all()
    }
}

impl ResolverPolicy {
    /// Returns a deny-by-default policy with every external capability disabled.
    #[must_use]
    pub const fn deny_all() -> Self {
        Self {
            allowed_roots: Vec::new(),
            allowed_uri_schemes: BTreeSet::new(),
            allow_network: false,
            max_depth: 0,
        }
    }

    /// Returns a filesystem-only policy rooted under the supplied directories.
    #[must_use]
    pub const fn filesystem_only(allowed_roots: Vec<PathBuf>) -> Self {
        Self {
            allowed_roots,
            allowed_uri_schemes: BTreeSet::new(),
            allow_network: false,
            max_depth: 16,
        }
    }

    fn reject<T>(
        kind: ResolutionKind,
        code: DiagnosticCode,
        message: String,
    ) -> Result<T, ResolutionError> {
        Err(ResolutionError::new(kind, code, message))
    }

    fn ensure_enabled(&self, kind: ResolutionKind) -> Result<(), ResolutionError> {
        if self.max_depth == 0
            && self.allowed_roots.is_empty()
            && self.allowed_uri_schemes.is_empty()
        {
            return Self::reject(
                kind,
                DiagnosticCode::UnsafeOperation,
                format!("{} resolution is disabled by policy", kind.as_str()),
            );
        }
        Ok(())
    }

    fn canonical_allowed_roots(
        &self,
        kind: ResolutionKind,
    ) -> Result<Vec<PathBuf>, ResolutionError> {
        let mut roots = Vec::with_capacity(self.allowed_roots.len());
        for root in &self.allowed_roots {
            let canonical = fs::canonicalize(root).map_err(|source| {
                ResolutionError::new(
                    kind,
                    DiagnosticCode::UnsafeOperation,
                    format!(
                        "failed to canonicalize allowed root '{}': {source}",
                        root.display()
                    ),
                )
            })?;
            roots.push(canonical);
        }
        Ok(roots)
    }

    fn validate_scheme(&self, kind: ResolutionKind, scheme: &str) -> Result<(), ResolutionError> {
        if !self.allowed_uri_schemes.contains(scheme) {
            return Self::reject(
                kind,
                DiagnosticCode::UnsafeOperation,
                format!("{} resolver rejected URI scheme '{scheme}'", kind.as_str()),
            );
        }

        if matches!(scheme, "http" | "https") && !self.allow_network {
            return Self::reject(
                kind,
                DiagnosticCode::UnsafeOperation,
                format!(
                    "{} resolver rejected network access for scheme '{scheme}'",
                    kind.as_str()
                ),
            );
        }

        Ok(())
    }
}

/// Mutable resolution state used to enforce cycle/depth limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionContext {
    max_depth: usize,
    visited: BTreeSet<String>,
}

impl ResolutionContext {
    /// Creates a new context that enforces cycle detection and `max_depth`.
    #[must_use]
    pub const fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            visited: BTreeSet::new(),
        }
    }

    /// Records a newly resolved resource.
    ///
    /// # Errors
    /// Returns [`ResolutionError`] when the context exceeds `max_depth` or revisits
    /// the same locator identity.
    pub fn record(
        &mut self,
        kind: ResolutionKind,
        locator: &ResourceLocator,
    ) -> Result<(), ResolutionError> {
        if self.visited.len() >= self.max_depth {
            return Err(ResolutionError::new(
                kind,
                DiagnosticCode::ResourceLimitExceeded,
                format!(
                    "{} resolution depth exceeded maximum of {}",
                    kind.as_str(),
                    self.max_depth
                ),
            ));
        }

        let id = locator.identity();
        if !self.visited.insert(id.clone()) {
            return Err(ResolutionError::new(
                kind,
                DiagnosticCode::ImportCycle,
                format!("{} resolution cycle detected at '{id}'", kind.as_str()),
            ));
        }

        Ok(())
    }
}

/// Request to resolve a resource.
#[derive(Debug)]
pub struct ResolutionRequest<'a> {
    /// Resolution category being performed.
    pub kind: ResolutionKind,
    /// Raw host-facing specifier to resolve.
    pub specifier: &'a str,
    /// Previously resolved resource providing the base location, if any.
    pub from: Option<&'a ResourceLocator>,
    /// Mutable resolution state for cycle and depth enforcement.
    pub context: &'a mut ResolutionContext,
}

/// Resolved resource content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResource {
    /// Stable locator for the resolved resource.
    pub locator: ResourceLocator,
    /// UTF-8 resource contents.
    pub content: String,
}

/// Stable resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionError {
    /// Resolution category that failed.
    pub kind: ResolutionKind,
    /// Stable diagnostic emitted for the failure.
    pub diagnostic: Diagnostic,
}

impl ResolutionError {
    /// Creates a stable resolution error diagnostic.
    #[must_use]
    pub fn new(kind: ResolutionKind, code: DiagnosticCode, message: String) -> Self {
        let mut diagnostic = Diagnostic::new(code, Severity::Error);
        diagnostic.message = message;
        Self { kind, diagnostic }
    }
}

/// Host-supplied resource resolver.
pub trait ResourceResolver {
    /// Resolves a host-approved resource.
    ///
    /// # Errors
    /// Returns [`ResolutionError`] when resolution is disabled, violates policy,
    /// exceeds limits, forms a cycle, or the resource cannot be loaded.
    fn resolve(&self, request: ResolutionRequest<'_>) -> Result<ResolvedResource, ResolutionError>;
}

/// Default resolver that rejects every access.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllResolver;

impl ResourceResolver for DenyAllResolver {
    fn resolve(&self, request: ResolutionRequest<'_>) -> Result<ResolvedResource, ResolutionError> {
        Err(ResolutionError::new(
            request.kind,
            DiagnosticCode::UnsafeOperation,
            format!(
                "{} resolution requires an explicit host resolver",
                request.kind.as_str()
            ),
        ))
    }
}

/// Filesystem-backed resolver using the shared policy model.
#[derive(Debug, Clone)]
pub struct FilesystemResolver {
    policy: ResolverPolicy,
}

impl FilesystemResolver {
    /// Creates a filesystem resolver governed by `policy`.
    #[must_use]
    pub const fn new(policy: ResolverPolicy) -> Self {
        Self { policy }
    }

    /// Returns the active resolver policy.
    #[must_use]
    pub const fn policy(&self) -> &ResolverPolicy {
        &self.policy
    }

    fn resolve_file(
        &self,
        kind: ResolutionKind,
        specifier: &str,
        from: Option<&ResourceLocator>,
        context: &mut ResolutionContext,
    ) -> Result<ResolvedResource, ResolutionError> {
        self.policy.ensure_enabled(kind)?;
        reject_parent_traversal(kind, specifier)?;
        let roots = self.policy.canonical_allowed_roots(kind)?;
        let candidate = select_candidate_path(kind, specifier, from, &roots)?;
        let canonical = fs::canonicalize(&candidate).map_err(|_| {
            ResolutionError::new(
                kind,
                DiagnosticCode::ImportNotFound,
                format!("{} target '{specifier}' was not found", kind.as_str()),
            )
        })?;

        ensure_contained(kind, &canonical, &roots)?;

        let locator = ResourceLocator::File(canonical.clone());
        context.record(kind, &locator)?;

        let text = fs::read_to_string(&canonical).map_err(|source| {
            ResolutionError::new(
                kind,
                DiagnosticCode::ImportNotFound,
                format!(
                    "failed to read {} target '{}': {source}",
                    kind.as_str(),
                    canonical.display()
                ),
            )
        })?;

        Ok(ResolvedResource {
            locator,
            content: text,
        })
    }
}

impl ResourceResolver for FilesystemResolver {
    fn resolve(&self, request: ResolutionRequest<'_>) -> Result<ResolvedResource, ResolutionError> {
        if let Some(scheme) = parse_scheme(request.specifier) {
            self.policy.validate_scheme(request.kind, scheme)?;
            return Err(ResolutionError::new(
                request.kind,
                DiagnosticCode::ImportNotFound,
                format!(
                    "{} resolver has no URI loader for '{}'",
                    request.kind.as_str(),
                    request.specifier
                ),
            ));
        }

        self.resolve_file(
            request.kind,
            request.specifier,
            request.from,
            request.context,
        )
    }
}

/// In-memory resolver for tests and embedding scenarios.
#[derive(Debug, Clone, Default)]
pub struct InMemoryResolver {
    policy: ResolverPolicy,
    files: BTreeMap<String, String>,
}

impl InMemoryResolver {
    /// Creates an in-memory resolver governed by `policy`.
    #[must_use]
    pub const fn new(policy: ResolverPolicy) -> Self {
        Self {
            policy,
            files: BTreeMap::new(),
        }
    }

    /// Adds a virtual or URI resource available to subsequent resolutions.
    #[must_use]
    pub fn with_resource(mut self, locator: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.insert(locator.into(), content.into());
        self
    }
}

impl ResourceResolver for InMemoryResolver {
    fn resolve(&self, request: ResolutionRequest<'_>) -> Result<ResolvedResource, ResolutionError> {
        self.policy.ensure_enabled(request.kind)?;
        reject_parent_traversal(request.kind, request.specifier)?;

        if let Some(scheme) = parse_scheme(request.specifier) {
            self.policy.validate_scheme(request.kind, scheme)?;
            let locator = ResourceLocator::Uri(request.specifier.to_owned());
            request.context.record(request.kind, &locator)?;
            let content = self.files.get(request.specifier).ok_or_else(|| {
                ResolutionError::new(
                    request.kind,
                    DiagnosticCode::ImportNotFound,
                    format!(
                        "{} target '{}' was not found",
                        request.kind.as_str(),
                        request.specifier
                    ),
                )
            })?;
            return Ok(ResolvedResource {
                locator,
                content: content.clone(),
            });
        }

        let virtual_id = match request.from {
            Some(ResourceLocator::Virtual(base)) => join_virtual(base, request.specifier),
            _ => request.specifier.to_owned(),
        };
        let locator = ResourceLocator::Virtual(virtual_id.clone());
        request.context.record(request.kind, &locator)?;
        let content = self.files.get(&virtual_id).ok_or_else(|| {
            ResolutionError::new(
                request.kind,
                DiagnosticCode::ImportNotFound,
                format!(
                    "{} target '{}' was not found",
                    request.kind.as_str(),
                    request.specifier
                ),
            )
        })?;
        Ok(ResolvedResource {
            locator,
            content: content.clone(),
        })
    }
}

fn reject_parent_traversal(kind: ResolutionKind, specifier: &str) -> Result<(), ResolutionError> {
    let path = Path::new(specifier);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ResolutionError::new(
            kind,
            DiagnosticCode::UnsafeOperation,
            format!(
                "{} resolver rejected parent traversal in '{}'",
                kind.as_str(),
                specifier
            ),
        ));
    }
    Ok(())
}

fn parse_scheme(specifier: &str) -> Option<&str> {
    specifier.find(':').and_then(|index| {
        let scheme = &specifier[..index];
        (!scheme.is_empty()
            && scheme
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
        .then_some(scheme)
    })
}

fn select_candidate_path(
    kind: ResolutionKind,
    specifier: &str,
    from: Option<&ResourceLocator>,
    roots: &[PathBuf],
) -> Result<PathBuf, ResolutionError> {
    let requested = PathBuf::from(specifier);
    if requested.is_absolute() {
        return Ok(requested);
    }

    if let Some(ResourceLocator::File(base)) = from {
        let parent = base.parent().ok_or_else(|| {
            ResolutionError::new(
                kind,
                DiagnosticCode::ImportNotFound,
                format!(
                    "{} resolver could not derive a base directory",
                    kind.as_str()
                ),
            )
        })?;
        return Ok(parent.join(requested));
    }

    for root in roots {
        let candidate = root.join(specifier);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    roots
        .first()
        .map(|root| root.join(specifier))
        .ok_or_else(|| {
            ResolutionError::new(
                kind,
                DiagnosticCode::UnsafeOperation,
                format!("{} resolution has no allowed roots", kind.as_str()),
            )
        })
}

fn ensure_contained(
    kind: ResolutionKind,
    candidate: &Path,
    roots: &[PathBuf],
) -> Result<(), ResolutionError> {
    if roots.iter().any(|root| candidate.starts_with(root)) {
        return Ok(());
    }

    Err(ResolutionError::new(
        kind,
        DiagnosticCode::UnsafeOperation,
        format!(
            "{} resolver rejected path '{}' outside allowed roots",
            kind.as_str(),
            candidate.display()
        ),
    ))
}

fn join_virtual(base: &str, specifier: &str) -> String {
    match base.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => format!("{parent}/{specifier}"),
        _ => specifier.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        DenyAllResolver, FilesystemResolver, InMemoryResolver, ResolutionContext, ResolutionKind,
        ResolutionRequest, ResolverPolicy, ResourceLocator, ResourceResolver,
    };

    #[test]
    fn deny_all_resolver_rejects_everything() {
        let mut context = ResolutionContext::new(1);
        let error = DenyAllResolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Import,
                specifier: "foo.lyma",
                from: None,
                context: &mut context,
            })
            .expect_err("deny-all resolver should reject resolution");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::UnsafeOperation
        );
    }

    #[test]
    fn in_memory_resolver_rejects_parent_traversal_for_all_resolution_kinds() {
        let resolver = InMemoryResolver::new(ResolverPolicy {
            max_depth: 4,
            ..ResolverPolicy::deny_all()
        });

        for kind in [
            ResolutionKind::Import,
            ResolutionKind::Include,
            ResolutionKind::Schema,
            ResolutionKind::Module,
        ] {
            let mut context = ResolutionContext::new(4);
            let error = resolver
                .resolve(ResolutionRequest {
                    kind,
                    specifier: "../escape.lyma",
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

    #[test]
    fn in_memory_resolver_rejects_cycles_and_depth() {
        let resolver = InMemoryResolver::new(ResolverPolicy {
            max_depth: 1,
            ..ResolverPolicy::deny_all()
        })
        .with_resource("a", "alpha");

        let mut context = ResolutionContext::new(1);
        let _first = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Import,
                specifier: "a",
                from: None,
                context: &mut context,
            })
            .expect("first resolution should succeed");

        let depth_error = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Import,
                specifier: "a",
                from: None,
                context: &mut context,
            })
            .expect_err("second resolution should exceed depth");
        assert_eq!(
            depth_error.diagnostic.code,
            lyma_syntax::DiagnosticCode::ResourceLimitExceeded
        );

        let resolver = InMemoryResolver::new(ResolverPolicy {
            max_depth: 2,
            ..ResolverPolicy::deny_all()
        })
        .with_resource("a", "alpha");
        let mut context = ResolutionContext::new(2);
        let _first = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Import,
                specifier: "a",
                from: None,
                context: &mut context,
            })
            .expect("first resolution should succeed");
        let cycle_error = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Import,
                specifier: "a",
                from: None,
                context: &mut context,
            })
            .expect_err("revisiting the same resource should trip cycle detection");
        assert_eq!(
            cycle_error.diagnostic.code,
            lyma_syntax::DiagnosticCode::ImportCycle
        );
    }

    #[test]
    fn filesystem_resolver_rejects_symlink_escapes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lyma-eval-{unique}"));
        let outside = std::env::temp_dir().join(format!("lyma-eval-outside-{unique}"));
        fs::create_dir_all(&root).expect("temp root should be created");
        fs::create_dir_all(&outside).expect("outside dir should be created");
        fs::write(outside.join("secret.txt"), "nope").expect("outside file should be written");

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("link.txt"))
            .expect("symlink should be created");
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(outside.join("secret.txt"), root.join("link.txt"))
            .is_err()
        {
            let _ = fs::remove_dir_all(root);
            let _ = fs::remove_dir_all(outside);
            return;
        }

        let resolver = FilesystemResolver::new(ResolverPolicy::filesystem_only(vec![root.clone()]));
        let mut context = ResolutionContext::new(4);
        let error = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Include,
                specifier: "link.txt",
                from: None,
                context: &mut context,
            })
            .expect_err("symlink escape should be rejected");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::UnsafeOperation
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn in_memory_resolver_rejects_network_by_default() {
        let mut schemes = std::collections::BTreeSet::new();
        schemes.insert(String::from("https"));
        let resolver = InMemoryResolver::new(ResolverPolicy {
            allowed_uri_schemes: schemes,
            max_depth: 4,
            ..ResolverPolicy::deny_all()
        });
        let mut context = ResolutionContext::new(4);

        let error = resolver
            .resolve(ResolutionRequest {
                kind: ResolutionKind::Schema,
                specifier: "https://example.test/schema.json",
                from: Some(&ResourceLocator::Virtual(String::from("root"))),
                context: &mut context,
            })
            .expect_err("network should be disabled by default");

        assert_eq!(
            error.diagnostic.code,
            lyma_syntax::DiagnosticCode::UnsafeOperation
        );
    }
}
