//! Public facade for the Lyma workspace.
//!
//! Default features are parser-only and do **not** pull in a Lua runtime.
//! Enable `omnilua` for the ergonomic evaluation facade plus the OmniLua backend,
//! or keep using `eval` + `engine-omnilua` for existing compatibility.
//!
//! # Parse without Lua
//!
//! ```rust
//! use lyma::{LymaDocument, LymaValue, Parser};
//! use lyma::parser::FileId;
//! use lyma::syntax::SyntaxKind;
//!
//! let parsed = Parser::new().parse_str(FileId(1), "example.lyma", "name: Example\n");
//! let _document: &LymaDocument = &parsed.file.documents[0];
//! let _value_type: Option<LymaValue> = None;
//! assert!(parsed.diagnostics.is_empty());
//!
//! let index = parsed.syntax_index();
//! let name_offset = parsed.source.as_str().find("name").unwrap(); // source-relative byte offset
//! let key_id = index.smallest_node_at_offset(name_offset).unwrap();
//! let parent_id = index.parent(key_id).unwrap();
//! assert_eq!(index.node(key_id).unwrap().kind, SyntaxKind::PlainMappingKey);
//! assert_eq!(index.node(parent_id).unwrap().kind, SyntaxKind::MappingEntry);
//! ```
//!
//! `SyntaxNodeId` values are deterministic for that indexed parse result only.
//! They are not persistent identities across later edits or reparses.
//!
//! Parser and tooling APIs expose lexical/syntactic editor primitives in
//! source-relative byte offsets/ranges. Higher-level LSP semantics such as
//! semantic tokens, references, rename, and workspace indexing remain
//! downstream responsibilities for language servers such as `lymals`.
//!
//! # Serialize with serde
//!
//! ```rust
//! # #[cfg(feature = "serde")]
//! # {
//! use lyma::serde::to_value;
//!
//! let _ = to_value("example");
//! # }
//! ```
//!
//! # Evaluate with OmniLua
//!
//! The `Loader` facade and `OmniLuaEngine` require a Lua runtime. The shortest
//! way to enable that is the `omnilua` feature:
//!
//! ```toml
//! [dependencies]
//! lyma = { version = "0.1", features = ["omnilua"] }
//! ```
//!
//! Existing compatibility also remains available via:
//!
//! ```toml
//! [dependencies]
//! lyma = { version = "0.1", features = ["eval", "engine-omnilua"] }
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "omnilua")]
//! # {
//! use lyma::{Loader, OmniLuaEngine, Parser};
//! use lyma::parser::FileId;
//!
//! let parsed = Parser::new().parse_str(FileId(1), "example.lyma", "answer: =40 + 2\n");
//! let engine = OmniLuaEngine::default();
//! let documents = Loader::new(&engine)
//!     .load_file(&parsed.file, "example.lyma", None)
//!     .unwrap();
//!
//! assert_eq!(documents.len(), 1);
//! # }
//! ```

#![cfg_attr(not(feature = "python"), forbid(unsafe_code))]

pub mod prelude;
pub mod tooling;

#[cfg(feature = "python")]
mod python;

#[cfg(feature = "syntax")]
pub use lyma_syntax as syntax;

#[cfg(feature = "parser")]
pub use lyma_parser as parser;

#[cfg(feature = "serde")]
pub use lyma_serde as serde;

#[cfg(feature = "lyba")]
pub use lyma_lyba as lyba;

#[cfg(feature = "runtime")]
pub use lyma_runtime as runtime;

#[cfg(feature = "eval")]
pub use lyma_eval as eval;

#[cfg(feature = "engine-omnilua")]
pub use lyma_engine_omnilua as engine_omnilua;

#[cfg(feature = "syntax")]
pub use lyma_syntax::{
    Diagnostic, Document as LymaDocument, LymaNull, LymaValue, SyntaxIndex, SyntaxKind,
    SyntaxNodeId, SyntaxNodeInfo,
};

#[cfg(feature = "parser")]
/// Ergonomic parser facade over the engine-neutral parsing entry points.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Parser;

#[cfg(feature = "parser")]
impl Parser {
    /// Creates a parser facade.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Parses UTF-8 source text without executing Lua.
    #[must_use]
    pub fn parse_str(
        self,
        file_id: lyma_parser::FileId,
        name: &str,
        text: &str,
    ) -> lyma_parser::Parsed {
        lyma_parser::parse_str(file_id, name, text)
    }

    /// Parses already-decoded source text without executing Lua.
    #[must_use]
    pub fn parse_source(self, source: lyma_parser::SourceText) -> lyma_parser::Parsed {
        lyma_parser::parse_source(source)
    }

    /// Creates a stateful incremental parse session for one source buffer.
    ///
    /// Today this is a validated full-reparse shell that reports incremental
    /// metadata without promising subtree reuse yet.
    #[must_use]
    pub fn session(self, file_id: lyma_parser::FileId, name: &str) -> lyma_parser::ParseSession {
        lyma_parser::ParseSession::new(file_id, name)
    }
}

#[cfg(feature = "eval")]
pub use lyma_eval::EvaluationOptions as LoadOptions;
#[cfg(feature = "eval")]
pub use lyma_eval::EvaluationProfile as Profile;
#[cfg(feature = "eval")]
pub type Resolver = dyn lyma_eval::ResourceResolver;
#[cfg(feature = "eval")]
pub type ModuleRegistry<E> = dyn lyma_eval::ModuleRegistry<E>;
#[cfg(feature = "eval")]
pub type TagResolver = dyn lyma_eval::TagResolver;

#[cfg(feature = "eval")]
/// Ergonomic loader facade over `lyma_eval::AstEvaluator`.
pub struct Loader<'a, E: lyma_runtime::LuaRuntimeEngine> {
    engine: &'a E,
    options: lyma_eval::EvaluationOptions<'a, E>,
}

#[cfg(feature = "eval")]
impl<'a, E: lyma_runtime::LuaRuntimeEngine> Loader<'a, E> {
    fn options(&self) -> lyma_eval::EvaluationOptions<'a, E> {
        lyma_eval::EvaluationOptions {
            profile: self.options.profile,
            resolver: self.options.resolver,
            module_registry: self.options.module_registry,
            tag_resolver: self.options.tag_resolver,
            schema_validator: self.options.schema_validator,
            unknown_tag_policy: self.options.unknown_tag_policy,
        }
    }

    /// Creates a loader with default restricted evaluation options.
    #[must_use]
    pub fn new(engine: &'a E) -> Self {
        Self {
            engine,
            options: lyma_eval::EvaluationOptions::default(),
        }
    }

    /// Replaces the full option bundle.
    #[must_use]
    pub fn with_options(mut self, options: lyma_eval::EvaluationOptions<'a, E>) -> Self {
        self.options = options;
        self
    }

    /// Overrides the active evaluation profile policy.
    #[must_use]
    pub fn profile(mut self, profile: &'a dyn lyma_eval::ProfilePolicy) -> Self {
        self.options.profile = profile;
        self
    }

    /// Installs a resource resolver for imports/includes/schema loads.
    #[must_use]
    pub fn resolver(mut self, resolver: &'a dyn lyma_eval::ResourceResolver) -> Self {
        self.options.resolver = Some(resolver);
        self
    }

    /// Installs a host module registry.
    #[must_use]
    pub fn module_registry(
        mut self,
        module_registry: &'a dyn lyma_eval::ModuleRegistry<E>,
    ) -> Self {
        self.options.module_registry = Some(module_registry);
        self
    }

    /// Installs a tag resolver.
    #[must_use]
    pub fn tag_resolver(mut self, tag_resolver: &'a dyn lyma_eval::TagResolver) -> Self {
        self.options.tag_resolver = Some(tag_resolver);
        self
    }

    /// Installs a schema validator.
    #[must_use]
    pub fn schema_validator(
        mut self,
        schema_validator: &'a dyn lyma_eval::SchemaValidator,
    ) -> Self {
        self.options.schema_validator = Some(schema_validator);
        self
    }

    /// Overrides the unknown-tag policy.
    #[must_use]
    pub fn unknown_tag_policy(mut self, policy: lyma_eval::UnknownTagPolicy) -> Self {
        self.options.unknown_tag_policy = policy;
        self
    }

    /// Evaluates all documents in a parsed file.
    ///
    /// # Errors
    ///
    /// Returns an error when evaluation or any enabled host capability fails.
    pub fn load_file(
        &self,
        file: &lyma_syntax::LymaFile,
        source_name: &str,
        locator: Option<lyma_eval::ResourceLocator>,
    ) -> Result<Vec<lyma_syntax::LymaValue>, lyma_eval::EvaluationError> {
        lyma_eval::AstEvaluator {
            engine: self.engine,
            options: self.options(),
        }
        .evaluate_file(file, source_name, locator)
    }

    /// Evaluates all documents in a parsed file while preserving extracted metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when evaluation or any enabled host capability fails.
    pub fn load_file_with_metadata(
        &self,
        file: &lyma_syntax::LymaFile,
        source_name: &str,
        locator: Option<lyma_eval::ResourceLocator>,
    ) -> Result<Vec<lyma_eval::EvaluatedDocument>, lyma_eval::EvaluationError> {
        lyma_eval::AstEvaluator {
            engine: self.engine,
            options: self.options(),
        }
        .evaluate_file_with_metadata(file, source_name, locator)
    }
}

#[cfg(feature = "omnilua")]
pub use lyma_engine_omnilua::OmniLuaEngine;

/// Returns the crate version at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
