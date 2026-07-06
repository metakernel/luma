//! Resource-limit configuration for backend-agnostic Lua execution.

/// Resource limits enforced while compiling or executing Lua.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeLimits {
    /// Maximum Lua VM instructions that may execute.
    pub max_instructions: Option<u64>,
    /// Maximum call-stack depth.
    pub max_call_depth: Option<u32>,
    /// Maximum memory budget, in bytes.
    pub max_memory_bytes: Option<usize>,
    /// Maximum wall-clock execution time, in milliseconds.
    pub max_runtime_millis: Option<u64>,
    /// Maximum number of table entries created during a conversion.
    pub max_table_entries: Option<usize>,
    /// Whether loading arbitrary host modules is allowed.
    pub allow_host_modules: bool,
}

impl RuntimeLimits {
    /// Returns a conservative baseline suitable for untrusted input.
    #[must_use]
    pub const fn sandboxed() -> Self {
        Self {
            max_instructions: Some(100_000),
            max_call_depth: Some(256),
            max_memory_bytes: Some(16 * 1024 * 1024),
            max_runtime_millis: Some(250),
            max_table_entries: Some(10_000),
            allow_host_modules: false,
        }
    }

    /// Returns limits with all optional caps disabled.
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            max_instructions: None,
            max_call_depth: None,
            max_memory_bytes: None,
            max_runtime_millis: None,
            max_table_entries: None,
            allow_host_modules: true,
        }
    }
}

/// Named limit categories for diagnostics and backend adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeLimitKind {
    /// Instruction-count budget.
    Instructions,
    /// Call-stack budget.
    CallDepth,
    /// Memory budget.
    Memory,
    /// Wall-clock budget.
    Runtime,
    /// Table-size budget during conversion.
    TableEntries,
    /// Restricted host module loading.
    HostModules,
}
