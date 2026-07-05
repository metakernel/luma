//! Evaluated Luma value model.

use crate::{
    ast::LumaTag,
    source::{DuplicateKey, Span},
};

/// Explicit Luma null sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LumaNull;

/// Numeric value preserving integer/float distinction.
#[derive(Debug, Clone, PartialEq)]
pub enum LumaNumber {
    /// Integer number.
    Integer(i64),
    /// Floating-point number.
    Float(f64),
}

/// Evaluated mapping key.
#[derive(Debug, Clone, PartialEq)]
pub enum LumaKey {
    /// String key.
    String(String),
    /// Numeric key.
    Number(LumaNumber),
    /// Boolean key.
    Boolean(bool),
    /// Host-approved userdata or object key.
    Host(LumaHostValue),
}

/// Opaque runtime value placeholder for syntax-level sharing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LumaHostValue {
    /// Host-defined kind name.
    pub kind: String,
    /// Optional host-supplied display label.
    pub label: Option<String>,
}

/// Ordered mapping representation.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaMapping {
    /// Entries in stable source or construction order.
    pub entries: Vec<LumaMappingEntry>,
    /// Duplicate-key tracking, when retained.
    pub duplicate_keys: Vec<DuplicateKey<LumaKey>>,
    /// Optional source span for the mapping as a whole.
    pub span: Option<Span>,
}

/// Ordered mapping entry.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaMappingEntry {
    /// Entry key.
    pub key: LumaKey,
    /// Entry value.
    pub value: LumaValue,
    /// Optional source span for the entry.
    pub span: Option<Span>,
}

/// Ordered sequence representation.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaSequence {
    /// Items in source or construction order.
    pub items: Vec<LumaValue>,
    /// Optional source span for the sequence as a whole.
    pub span: Option<Span>,
}

/// Tagged evaluated value.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaTaggedValue {
    /// Tag metadata.
    pub tag: LumaTag,
    /// Tagged payload.
    pub value: Box<LumaValue>,
    /// Optional source span for the tagged value.
    pub span: Option<Span>,
}

/// Evaluated value model from spec section 10.2.
#[derive(Debug, Clone, PartialEq)]
pub enum LumaValue {
    /// Explicit Luma null sentinel.
    Null(LumaNull),
    /// Boolean value.
    Boolean(bool),
    /// Number value.
    Number(LumaNumber),
    /// String value.
    String(String),
    /// Ordered sequence.
    Sequence(LumaSequence),
    /// Ordered mapping.
    Mapping(LumaMapping),
    /// Tagged value when tags are preserved.
    Tagged(LumaTaggedValue),
    /// Runtime function value when the active profile permits it.
    Function(LumaHostValue),
    /// Runtime userdata value when the active profile permits it.
    UserData(LumaHostValue),
    /// Host-defined object value when the active profile permits it.
    HostObject(LumaHostValue),
}
