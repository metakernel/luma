//! Evaluated Lyma value model.

use crate::{
    ast::LymaTag,
    source::{DuplicateKey, Span},
};

/// Explicit Lyma null sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LymaNull;

/// Numeric value preserving integer/float distinction.
#[derive(Debug, Clone, PartialEq)]
pub enum LymaNumber {
    /// Integer number.
    Integer(i64),
    /// Floating-point number.
    Float(f64),
}

/// Evaluated mapping key.
#[derive(Debug, Clone, PartialEq)]
pub enum LymaKey {
    /// String key.
    String(String),
    /// Numeric key.
    Number(LymaNumber),
    /// Boolean key.
    Boolean(bool),
    /// Host-approved userdata or object key.
    Host(LymaHostValue),
}

/// Opaque runtime value placeholder for syntax-level sharing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LymaHostValue {
    /// Host-defined kind name.
    pub kind: String,
    /// Optional host-supplied display label.
    pub label: Option<String>,
}

/// Ordered mapping representation.
#[derive(Debug, Clone, PartialEq)]
pub struct LymaMapping {
    /// Entries in stable source or construction order.
    pub entries: Vec<LymaMappingEntry>,
    /// Duplicate-key tracking, when retained.
    pub duplicate_keys: Vec<DuplicateKey<LymaKey>>,
    /// Optional source span for the mapping as a whole.
    pub span: Option<Span>,
}

/// Ordered mapping entry.
#[derive(Debug, Clone, PartialEq)]
pub struct LymaMappingEntry {
    /// Entry key.
    pub key: LymaKey,
    /// Entry value.
    pub value: LymaValue,
    /// Optional source span for the entry.
    pub span: Option<Span>,
}

/// Ordered sequence representation.
#[derive(Debug, Clone, PartialEq)]
pub struct LymaSequence {
    /// Items in source or construction order.
    pub items: Vec<LymaValue>,
    /// Optional source span for the sequence as a whole.
    pub span: Option<Span>,
}

/// Tagged evaluated value.
#[derive(Debug, Clone, PartialEq)]
pub struct LymaTaggedValue {
    /// Tag metadata.
    pub tag: LymaTag,
    /// Tagged payload.
    pub value: Box<LymaValue>,
    /// Optional source span for the tagged value.
    pub span: Option<Span>,
}

/// Evaluated value model from spec section 10.2.
#[derive(Debug, Clone, PartialEq)]
pub enum LymaValue {
    /// Explicit Lyma null sentinel.
    Null(LymaNull),
    /// Boolean value.
    Boolean(bool),
    /// Number value.
    Number(LymaNumber),
    /// String value.
    String(String),
    /// Ordered sequence.
    Sequence(LymaSequence),
    /// Ordered mapping.
    Mapping(LymaMapping),
    /// Tagged value when tags are preserved.
    Tagged(LymaTaggedValue),
    /// Runtime function value when the active profile permits it.
    Function(LymaHostValue),
    /// Runtime userdata value when the active profile permits it.
    UserData(LymaHostValue),
    /// Host-defined object value when the active profile permits it.
    HostObject(LymaHostValue),
}
