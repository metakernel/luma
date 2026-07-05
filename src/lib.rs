//! Public facade for the Luma workspace.
//!
//! Default features are parser-only and do **not** pull in a Lua runtime.
//! Enable `omnilua` for the ergonomic evaluation facade plus the OmniLua backend,
//! or keep using `eval` + `engine-omnilua` for existing compatibility.
//!
//! # Parse without Lua
//!
//! ```rust
//! use luma::{LumaDocument, LumaValue, Parser};
//! use luma::parser::FileId;
//!
//! let parsed = Parser::new().parse_str(FileId(1), "example.luma", "name: Example\n");
//! let _document: &LumaDocument = &parsed.file.documents[0];
//! let _value_type: Option<LumaValue> = None;
//! assert!(parsed.diagnostics.is_empty());
//! ```
//!
//! # Serialize with serde
//!
//! ```rust
//! # #[cfg(feature = "serde")]
//! # {
//! use luma::serde::to_value;
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
//! luma = { version = "0.1", features = ["omnilua"] }
//! ```
//!
//! Existing compatibility also remains available via:
//!
//! ```toml
//! [dependencies]
//! luma = { version = "0.1", features = ["eval", "engine-omnilua"] }
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "omnilua")]
//! # {
//! use luma::{Loader, OmniLuaEngine, Parser};
//! use luma::parser::FileId;
//!
//! let parsed = Parser::new().parse_str(FileId(1), "example.luma", "answer: =40 + 2\n");
//! let engine = OmniLuaEngine::default();
//! let documents = Loader::new(&engine)
//!     .load_file(&parsed.file, "example.luma", None)
//!     .unwrap();
//!
//! assert_eq!(documents.len(), 1);
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod prelude;
pub mod tooling;

#[cfg(feature = "syntax")]
pub use luma_syntax as syntax;

#[cfg(feature = "parser")]
pub use luma_parser as parser;

#[cfg(feature = "serde")]
pub use luma_serde as serde;

#[cfg(feature = "runtime")]
pub use luma_runtime as runtime;

#[cfg(feature = "eval")]
pub use luma_eval as eval;

#[cfg(feature = "engine-omnilua")]
pub use luma_engine_omnilua as engine_omnilua;

#[cfg(feature = "syntax")]
pub use luma_syntax::{Diagnostic, Document as LumaDocument, LumaValue};

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
        file_id: luma_parser::FileId,
        name: &str,
        text: &str,
    ) -> luma_parser::Parsed {
        luma_parser::parse_str(file_id, name, text)
    }

    /// Parses already-decoded source text without executing Lua.
    #[must_use]
    pub fn parse_source(self, source: luma_parser::SourceText) -> luma_parser::Parsed {
        luma_parser::parse_source(source)
    }
}

#[cfg(feature = "eval")]
pub use luma_eval::EvaluationOptions as LoadOptions;
#[cfg(feature = "eval")]
pub use luma_eval::EvaluationProfile as Profile;
#[cfg(feature = "eval")]
pub type Resolver = dyn luma_eval::ResourceResolver;
#[cfg(feature = "eval")]
pub type ModuleRegistry<E> = dyn luma_eval::ModuleRegistry<E>;
#[cfg(feature = "eval")]
pub type TagResolver = dyn luma_eval::TagResolver;

#[cfg(feature = "eval")]
/// Ergonomic loader facade over `luma_eval::AstEvaluator`.
pub struct Loader<'a, E: luma_runtime::LuaRuntimeEngine> {
    engine: &'a E,
    options: luma_eval::EvaluationOptions<'a, E>,
}

#[cfg(feature = "eval")]
impl<'a, E: luma_runtime::LuaRuntimeEngine> Loader<'a, E> {
    fn options(&self) -> luma_eval::EvaluationOptions<'a, E> {
        luma_eval::EvaluationOptions {
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
            options: luma_eval::EvaluationOptions::default(),
        }
    }

    /// Replaces the full option bundle.
    #[must_use]
    pub fn with_options(mut self, options: luma_eval::EvaluationOptions<'a, E>) -> Self {
        self.options = options;
        self
    }

    /// Overrides the active evaluation profile policy.
    #[must_use]
    pub fn profile(mut self, profile: &'a dyn luma_eval::ProfilePolicy) -> Self {
        self.options.profile = profile;
        self
    }

    /// Installs a resource resolver for imports/includes/schema loads.
    #[must_use]
    pub fn resolver(mut self, resolver: &'a dyn luma_eval::ResourceResolver) -> Self {
        self.options.resolver = Some(resolver);
        self
    }

    /// Installs a host module registry.
    #[must_use]
    pub fn module_registry(
        mut self,
        module_registry: &'a dyn luma_eval::ModuleRegistry<E>,
    ) -> Self {
        self.options.module_registry = Some(module_registry);
        self
    }

    /// Installs a tag resolver.
    #[must_use]
    pub fn tag_resolver(mut self, tag_resolver: &'a dyn luma_eval::TagResolver) -> Self {
        self.options.tag_resolver = Some(tag_resolver);
        self
    }

    /// Installs a schema validator.
    #[must_use]
    pub fn schema_validator(
        mut self,
        schema_validator: &'a dyn luma_eval::SchemaValidator,
    ) -> Self {
        self.options.schema_validator = Some(schema_validator);
        self
    }

    /// Overrides the unknown-tag policy.
    #[must_use]
    pub fn unknown_tag_policy(mut self, policy: luma_eval::UnknownTagPolicy) -> Self {
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
        file: &luma_syntax::LumaFile,
        source_name: &str,
        locator: Option<luma_eval::ResourceLocator>,
    ) -> Result<Vec<luma_syntax::LumaValue>, luma_eval::EvaluationError> {
        luma_eval::AstEvaluator {
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
        file: &luma_syntax::LumaFile,
        source_name: &str,
        locator: Option<luma_eval::ResourceLocator>,
    ) -> Result<Vec<luma_eval::EvaluatedDocument>, luma_eval::EvaluationError> {
        luma_eval::AstEvaluator {
            engine: self.engine,
            options: self.options(),
        }
        .evaluate_file_with_metadata(file, source_name, locator)
    }
}

#[cfg(feature = "omnilua")]
pub use luma_engine_omnilua::OmniLuaEngine;

/// Returns the crate version at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
