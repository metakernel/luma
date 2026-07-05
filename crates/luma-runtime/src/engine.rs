//! Backend-agnostic Lua engine contracts.

use luma_syntax::{Diagnostic, DiagnosticCode, Severity, source::Span};

use crate::{
    conversion::RuntimeValueCodec,
    environment::RuntimeEnvironmentFactory,
    limits::{RuntimeLimitKind, RuntimeLimits},
    module::RuntimeModuleFactory,
};

/// Stable marker describing a named runtime engine.
pub trait Engine {
    /// Stable engine identifier.
    fn engine_name(&self) -> &'static str;
}

/// Lua source category being compiled or executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LuaChunkKind {
    /// Standalone expression returning a value.
    Expression,
    /// General Lua chunk.
    Chunk,
}

/// Execution phase used for backend-neutral diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LuaRuntimePhase {
    /// Building or validating a runtime environment.
    Environment,
    /// Compiling Lua source.
    Compile(LuaChunkKind),
    /// Evaluating previously compiled Lua.
    Evaluate(LuaChunkKind),
    /// Converting values across the adapter boundary.
    Conversion,
    /// Creating or loading safe modules.
    Module,
    /// Applying or tripping resource limits.
    Limits,
}

/// Backend-neutral Lua runtime failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaRuntimeError {
    /// Engine identifier that produced the error.
    pub engine: &'static str,
    /// Phase in which the error occurred.
    pub phase: LuaRuntimePhase,
    /// Public diagnostic payload.
    pub diagnostic: Box<Diagnostic>,
}

impl LuaRuntimeError {
    /// Creates an error from an existing diagnostic.
    #[must_use]
    pub const fn new(
        engine: &'static str,
        phase: LuaRuntimePhase,
        diagnostic: Box<Diagnostic>,
    ) -> Self {
        Self {
            engine,
            phase,
            diagnostic,
        }
    }

    /// Creates a syntax error diagnostic.
    #[must_use]
    pub fn syntax_error(
        engine: &'static str,
        phase: LuaRuntimePhase,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::LuaSyntaxError, Severity::Error);
        diagnostic.message = message.into();
        diagnostic.primary_span = primary_span;
        Self::new(engine, phase, Box::new(diagnostic))
    }

    /// Creates a runtime error diagnostic.
    #[must_use]
    pub fn runtime_error(
        engine: &'static str,
        phase: LuaRuntimePhase,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::LuaRuntimeError, Severity::Error);
        diagnostic.message = message.into();
        diagnostic.primary_span = primary_span;
        Self::new(engine, phase, Box::new(diagnostic))
    }

    /// Creates a resource-limit diagnostic.
    #[must_use]
    pub fn limit_exceeded(
        engine: &'static str,
        limit: RuntimeLimitKind,
        primary_span: Option<Span>,
    ) -> Self {
        let mut diagnostic =
            Diagnostic::new(DiagnosticCode::ResourceLimitExceeded, Severity::Error);
        diagnostic.message = format!("resource limit exceeded: {limit:?}");
        diagnostic.primary_span = primary_span;
        Self::new(engine, LuaRuntimePhase::Limits, Box::new(diagnostic))
    }
}

/// Source text to compile as either an expression or a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSourceText<'a> {
    /// Human-readable source name.
    pub name: &'a str,
    /// Source text content.
    pub text: &'a str,
    /// Optional full-span identity for the source.
    pub span: Option<Span>,
}

impl<'a> LuaSourceText<'a> {
    /// Creates a new source descriptor.
    #[must_use]
    pub const fn new(name: &'a str, text: &'a str) -> Self {
        Self {
            name,
            text,
            span: None,
        }
    }

    /// Attaches a span to the source descriptor.
    #[must_use]
    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

/// Full engine boundary required by `luma-eval` and higher layers.
pub trait LuaRuntimeEngine:
    Engine + RuntimeEnvironmentFactory + RuntimeModuleFactory + RuntimeValueCodec
where
    Self: RuntimeEnvironmentFactory<
            RuntimeValue = <Self as RuntimeValueCodec>::Value,
            RuntimeModule = <Self as RuntimeModuleFactory>::Module,
        > + RuntimeModuleFactory<RuntimeValue = <Self as RuntimeValueCodec>::Value>,
{
    /// Engine-specific compiled expression handle.
    type CompiledExpression;
    /// Engine-specific compiled chunk handle.
    type CompiledChunk;

    /// Compiles a Lua expression.
    ///
    /// # Errors
    ///
    /// Returns an error when source validation, compilation, or compile-time limit checks fail.
    fn compile_expression(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<Self::CompiledExpression, LuaRuntimeError>;

    /// Compiles a general Lua chunk.
    ///
    /// # Errors
    ///
    /// Returns an error when source validation, compilation, or compile-time limit checks fail.
    fn compile_chunk(
        &self,
        source: LuaSourceText<'_>,
        limits: &RuntimeLimits,
    ) -> Result<Self::CompiledChunk, LuaRuntimeError>;

    /// Evaluates a compiled expression inside an environment.
    ///
    /// # Errors
    ///
    /// Returns an error when evaluation fails or any runtime limit is exceeded.
    fn evaluate_expression(
        &self,
        compiled: &Self::CompiledExpression,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<<Self as RuntimeValueCodec>::Value, LuaRuntimeError>;

    /// Evaluates a compiled chunk inside an environment.
    ///
    /// # Errors
    ///
    /// Returns an error when evaluation fails or any runtime limit is exceeded.
    fn evaluate_chunk(
        &self,
        compiled: &Self::CompiledChunk,
        environment: &mut Self::Environment,
        limits: &RuntimeLimits,
    ) -> Result<<Self as RuntimeValueCodec>::Value, LuaRuntimeError>;
}
