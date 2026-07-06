//! Writer mode policy helpers.

/// Header container flag: some source text or spans are present.
pub(crate) const CONTAINER_FLAG_HAS_SOURCE: u32 = 1 << 2;
/// Header container flag: syntax nodes are present.
pub(crate) const CONTAINER_FLAG_HAS_SYNTAX: u32 = 1 << 3;
/// Header container flag: value sections are present.
pub(crate) const CONTAINER_FLAG_HAS_VALUES: u32 = 1 << 4;
/// Header container flag: diagnostics are present.
pub(crate) const CONTAINER_FLAG_HAS_DIAGNOSTICS: u32 = 1 << 6;

/// Header profile flag: value image intent.
pub(crate) const PROFILE_FLAG_VALUE_IMAGE: u32 = 1 << 3;
/// Header profile flag: syntax image intent.
pub(crate) const PROFILE_FLAG_SYNTAX_IMAGE: u32 = 1 << 4;

/// Canonicalization policy for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CanonicalMode {
    /// Preserve input-like ordering where possible.
    #[default]
    None,
    /// Apply a stable best-effort canonical ordering.
    Relaxed,
    /// Require strict canonical output rules.
    Strict,
}

/// Output mode used by the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WriterMode {
    /// Human-oriented formatting.
    #[default]
    Pretty,
    /// Compact formatting.
    Compact,
    /// Deterministic runtime data image with recommended section layout.
    RuntimeData,
    /// Build-bundle image with bundle metadata and inert dependency/resource manifests.
    BuildBundle,
    /// Editor-oriented cache with source, syntax, trivia, and diagnostics.
    EditorCache,
    /// Conformance fixture image with value and syntax expectations.
    ConformanceFixture,
    /// Canonical formatting with explicit policy.
    Canonical(CanonicalMode),
}

/// Source-text reconstruction preference for tooling-oriented consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextReconstructionMode {
    /// Reconstruct from canonical value data only; omits source, syntax, and trivia.
    #[default]
    CanonicalValueText,
    /// Reconstruct from structured syntax using stable pretty-print rules.
    StructuredSyntaxPretty,
    /// Reconstruct from preserved source/trivia when available, else fall back best-effort.
    PreservedSourceBestEffort,
}

impl WriterMode {
    pub(crate) const fn include_source(self) -> bool {
        !matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::Canonical(_)
        )
    }

    pub(crate) const fn include_syntax(self) -> bool {
        !matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::Canonical(_)
        )
    }

    pub(crate) const fn include_trivia(self) -> bool {
        !matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::Canonical(_)
        )
    }

    pub(crate) const fn include_diagnostics(self) -> bool {
        !matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::Canonical(_)
        )
    }

    pub(crate) const fn force_metadata(self) -> bool {
        matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::EditorCache | Self::ConformanceFixture
        )
    }

    pub(crate) const fn force_symbols(self) -> bool {
        self.force_metadata()
    }

    pub(crate) const fn force_blob(self) -> bool {
        matches!(self, Self::BuildBundle | Self::EditorCache)
    }

    pub(crate) const fn force_values(self) -> bool {
        matches!(
            self,
            Self::RuntimeData | Self::BuildBundle | Self::EditorCache | Self::ConformanceFixture
        )
    }

    pub(crate) const fn force_documents(self) -> bool {
        self.force_values()
    }

    pub(crate) const fn force_source_files(self) -> bool {
        matches!(self, Self::EditorCache | Self::ConformanceFixture)
    }

    pub(crate) const fn force_source_spans(self) -> bool {
        self.force_source_files()
    }

    pub(crate) const fn force_syntax_nodes(self) -> bool {
        self.force_source_files()
    }

    pub(crate) const fn force_trivia(self) -> bool {
        self.force_source_files()
    }

    pub(crate) const fn force_diagnostic_section(self) -> bool {
        matches!(self, Self::EditorCache | Self::ConformanceFixture)
    }

    pub(crate) const fn default_image_kind(self) -> Option<&'static str> {
        match self {
            Self::RuntimeData => Some("value"),
            Self::BuildBundle => Some("build_bundle"),
            Self::EditorCache => Some("editor_cache"),
            Self::ConformanceFixture => Some("conformance_fixture"),
            _ => None,
        }
    }

    pub(crate) const fn recommended_container_flags(self) -> u32 {
        match self {
            Self::BuildBundle => CONTAINER_FLAG_HAS_VALUES,
            Self::EditorCache | Self::ConformanceFixture => {
                CONTAINER_FLAG_HAS_SOURCE
                    | CONTAINER_FLAG_HAS_SYNTAX
                    | CONTAINER_FLAG_HAS_VALUES
                    | CONTAINER_FLAG_HAS_DIAGNOSTICS
            }
            _ => 0,
        }
    }

    pub(crate) const fn recommended_profile_flags(self) -> u32 {
        match self {
            Self::BuildBundle => PROFILE_FLAG_VALUE_IMAGE,
            Self::EditorCache | Self::ConformanceFixture => {
                PROFILE_FLAG_VALUE_IMAGE | PROFILE_FLAG_SYNTAX_IMAGE
            }
            _ => 0,
        }
    }
}
