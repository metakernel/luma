//! Opt-in LYBA support crate for the Lyma workspace.
//!
//! Enable it from the facade crate with:
//!
//! ```toml
//! [dependencies]
//! lyma = { version = "0.1", features = ["lyba"] }
//! ```
//!
//! `lyma-lyba` is a binary container library, not an execution engine.
//! Reading, inspecting, verifying, and writing LYBA never executes Lua,
//! compiles stored chunks, resolves imports, or activates host modules.
//!
//! Draft 0.1 implementation notes:
//!
//! - supports Level 0-5 section families as inert data/model structures
//! - default `Limits` are the public/untrusted-input preset
//! - current codec support is write/read for codec `0` (`none`) only
//! - writer modes include value/default, runtime-data, editor-cache, bundle,
//!   fixture, and relaxed/strict canonical output
//!
//! The crate intentionally keeps a builder-friendly API surface so parsing,
//! serialization, verification, and canonicalization behavior can grow without
//! breaking the public shape.
//!
//! ```
//! use lyma_lyba::{CanonicalMode, Limits, LybaFile, ReadOptions, Reader, WriteOptions, Writer, WriterMode};
//!
//! let file = LybaFile::new();
//! let limits = Limits::default();
//! let read_options = ReadOptions::new().with_limits(limits.clone());
//! let write_options = WriteOptions::new()
//!     .with_mode(WriterMode::Canonical(CanonicalMode::Strict))
//!     .with_limits(limits);
//!
//! let _reader = Reader::new(read_options);
//! let _writer = Writer::new(write_options);
//! let _ = file;
//! ```
//!
//! Round-trip a simple document image:
//!
//! ```
//! use lyma_lyba::{Document, Limits, LybaFile, ReadOptions, Reader, Value, WriteOptions, Writer};
//!
//! let file = LybaFile::new()
//!     .with_document(Document::new().with_root_value(Value::String(String::from("hello"))));
//!
//! let bytes = Writer::new(WriteOptions::new().with_limits(Limits::public())).write(&file)?;
//! let decoded = Reader::new(ReadOptions::new().with_limits(Limits::public())).read(&bytes)?;
//!
//! assert_eq!(decoded.documents.len(), 1);
//! # Ok::<(), lyma_lyba::LybaError>(())
//! ```
//!
//! Convert portable `lyma_syntax::LymaValue` values with the helper API:
//!
//! ```
//! use lyma_lyba::{try_from_lyba_value_image, try_to_lyba_value_image};
//! use lyma_syntax::{LymaNull, LymaValue};
//!
//! let bytes = try_to_lyba_value_image(&[LymaValue::Null(LymaNull)])?;
//! let values = try_from_lyba_value_image(&bytes)?;
//!
//! assert_eq!(values, vec![LymaValue::Null(LymaNull)]);
//! # Ok::<(), lyma_lyba::LybaError>(())
//! ```

#![forbid(unsafe_code)]
#![allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::identity_op,
    clippy::ignored_unit_patterns,
    clippy::items_after_test_module,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_lifetimes,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unused_enumerate_index,
    clippy::unused_self,
    clippy::use_self
)]

pub mod blob;
pub mod bundle;
pub mod capability;
pub(crate) mod checksum;
pub mod codec;
pub mod container;
pub mod diagnostic;
pub mod document;
pub mod error;
pub mod extension;
pub mod meta;
pub mod mode;
pub mod policy;
pub mod primitives;
pub mod read;
pub mod schema;
pub mod section;
pub mod signature;
pub mod source;
pub mod string_table;
pub mod symbol;
pub mod syntax;
pub mod tag;
pub mod trivia;
pub mod value;
pub mod verify;
pub mod write;

/// Backward-compatible bundle module path.
pub mod bundles {
    pub use crate::bundle::BundleDescriptor;
}

/// Backward-compatible diagnostics module path for structured LYBA spec codes.
pub mod diagnostics {
    pub use crate::diagnostic::{
        Diagnostic, DiagnosticClass, DiagnosticCode, DiagnosticLoadPolicy, DiagnosticRecord,
        DiagnosticTable, RelatedDiagnosticRecord, Severity, StoredDiagnosticSeverity,
    };
}

/// Fixture helpers used by tests and examples.
pub mod fixtures {
    use crate::container::LybaFile;

    /// Creates a minimal empty file fixture.
    #[must_use]
    pub fn empty_file() -> LybaFile {
        LybaFile::new()
    }
}

use lyma_syntax::LymaValue;

/// Encodes portable values into a canonical Level 1 minimal value image.
///
/// Panics when a value is not portable or the image cannot be written.
#[must_use]
pub fn to_lyba_value_image(values: &[LymaValue]) -> Vec<u8> {
    try_to_lyba_value_image(values).expect("to_lyba_value_image requires portable Level 1 values")
}

/// Decodes a canonical Level 1 minimal value image into portable values.
pub fn from_lyba_value_image(bytes: &[u8]) -> Result<Vec<LymaValue>> {
    try_from_lyba_value_image(bytes)
}

/// Fallible form of [`to_lyba_value_image`].
pub fn try_to_lyba_value_image(values: &[LymaValue]) -> Result<Vec<u8>> {
    write::write_level1_minimal_value_image(values)
}

/// Fallible form of [`from_lyba_value_image`].
pub fn try_from_lyba_value_image(bytes: &[u8]) -> Result<Vec<LymaValue>> {
    read::read_level1_minimal_value_image(bytes)
}

pub use blob::{
    BLOB_FLAG_EXTERNAL_DIGEST_TARGET, BLOB_FLAG_GENERATED, BLOB_FLAG_LUA_SOURCE, BLOB_FLAG_PRIVATE,
    BLOB_FLAG_RESERVED_MASK, BLOB_FLAG_SOURCE_TEXT, BLOB_FLAG_UTF8_TEXT, BlobId, BlobRecord,
    BlobTable,
};
pub use capability::{
    CAPABILITY_FLAG_DETERMINISTIC_EXPECTED, CAPABILITY_FLAG_MAY_READ_EXTERNAL,
    CAPABILITY_FLAG_MAY_WRITE_EXTERNAL, CAPABILITY_FLAG_PURE_EXPECTED,
    CAPABILITY_FLAG_REQUIRED_FOR_EVALUATION, CAPABILITY_FLAG_REQUIRED_FOR_REPRODUCTION,
    CAPABILITY_FLAG_RESERVED_MASK, CAPABILITY_FLAG_TRUSTED_ONLY, CAPABILITY_SECTION_NAME,
    CapabilityRequirement, CapabilitySetRecord, CapabilityTable,
};
pub use codec::{CODEC_DEFLATE, CODEC_LZ4, CODEC_NONE, CODEC_ZSTD};
pub use container::{DocumentImage, LybaFile};
pub use diagnostic::{
    Diagnostic, DiagnosticClass, DiagnosticCode, DiagnosticLoadPolicy, DiagnosticRecord,
    DiagnosticTable, RelatedDiagnosticRecord, Severity, StoredDiagnosticSeverity,
};
pub use document::{
    DOCUMENT_FLAG_HAS_CAPABILITY_SET, DOCUMENT_FLAG_HAS_SCHEMA, DOCUMENT_FLAG_HAS_VALUE_ROOT,
    Document,
};
pub use error::{ErrorContext, LybaError, Result};
pub use extension::{
    EXTENSION_FLAG_AFFECTS_CANONICAL, EXTENSION_FLAG_MAY_CONTAIN_CODE,
    EXTENSION_FLAG_MAY_RESOLVE_EXTERNAL, EXTENSION_FLAG_REQUIRED, EXTENSION_FLAG_RESERVED_MASK,
    EXTENSION_FLAG_TRUSTED_ONLY, ExtensionDeclaration, ExtensionTable,
};
pub use meta::Metadata;
pub use mode::{CanonicalMode, TextReconstructionMode, WriterMode};
pub use policy::{ExtensionNamePolicy, Limits};
pub use read::{ReadOptions, Reader};
pub use schema::{
    SCHEMA_FLAG_DIGEST_PRESENT, SCHEMA_FLAG_REQUIRED_BY_DOCUMENT, SCHEMA_FLAG_RESERVED_MASK,
    SCHEMA_FLAG_TRUSTED_VALIDATOR_REQUIRED, SCHEMA_FLAG_URI_PRESENT,
    SCHEMA_FLAG_VALIDATED_BY_PRODUCER, SCHEMA_FLAG_VALUE_PRESENT, SchemaRecord, SchemaTable,
};
pub use signature::{
    CoveredSection, SIGNATURE_ALGORITHM_BLAKE3_256, SIGNATURE_ALGORITHM_ECDSA_P256_SHA256,
    SIGNATURE_ALGORITHM_ED25519, SIGNATURE_ALGORITHM_RSA_PSS_SHA256, SIGNATURE_ALGORITHM_SHA256,
    SIGNATURE_ALGORITHM_SHA384, SIGNATURE_ALGORITHM_SHA512,
    SIGNATURE_COVERED_RANGE_KIND_EXPLICIT_SECTIONS, SIGNATURE_RECORD_KIND_CERTIFICATE_CHAIN,
    SIGNATURE_RECORD_KIND_DIGEST, SIGNATURE_RECORD_KIND_EXTENSION, SIGNATURE_RECORD_KIND_SIGNATURE,
    SIGNATURE_RECORD_KIND_TRANSPARENCY_RECORD, SIGNATURE_SECTION_NAME, SignatureRecord,
    SignatureTable, SignatureVerifier, StructuralSignatureRecord, StructuralSignatureReport,
};
pub use source::{
    SOURCE_FILE_FLAG_DIGEST_PRESENT, SOURCE_FILE_FLAG_DISPLAY_PRESENT, SOURCE_FILE_FLAG_GENERATED,
    SOURCE_FILE_FLAG_PRIVATE, SOURCE_FILE_FLAG_RESERVED_MASK, SOURCE_FILE_FLAG_SOURCE_BLOB_PRESENT,
    SOURCE_FILE_FLAG_URI_PRESENT, SOURCE_FILE_FLAG_VIRTUAL, SOURCE_SPAN_FLAG_EXPRESSION_RESULT,
    SOURCE_SPAN_FLAG_GENERATED, SOURCE_SPAN_FLAG_MACRO_OR_INCLUDE_EXPANSION,
    SOURCE_SPAN_FLAG_RESERVED_MASK, SOURCE_SPAN_FLAG_SYNTHETIC, SourceFileRecord, SourceFileTable,
    SourceSpanRecord, SourceSpanTable,
};
pub use string_table::{
    STRING_FLAG_ASCII_ONLY, STRING_FLAG_NORMALIZED_NFC, STRING_FLAG_PRIVATE,
    STRING_FLAG_RESERVED_MASK, StringInterner, StringRecord, StringTable,
};
pub use symbol::{
    SYMBOL_FLAG_DIRECTIVE, SYMBOL_FLAG_EXTENSION, SYMBOL_FLAG_KEY, SYMBOL_FLAG_NODE_KIND,
    SYMBOL_FLAG_PROFILE, SYMBOL_FLAG_RESERVED_MASK, SYMBOL_FLAG_TAG, SymbolInterner, SymbolRecord,
    SymbolTable,
};
pub use syntax::{
    SYNTAX_FIELD_KIND_ABSENT, SYNTAX_FIELD_KIND_BLOB_REF, SYNTAX_FIELD_KIND_BOOL,
    SYNTAX_FIELD_KIND_EXTENSION, SYNTAX_FIELD_KIND_NODE_LIST, SYNTAX_FIELD_KIND_NODE_REF,
    SYNTAX_FIELD_KIND_RESERVED_MASK, SYNTAX_FIELD_KIND_SPAN_REF, SYNTAX_FIELD_KIND_STRING,
    SYNTAX_FIELD_KIND_SVAR, SYNTAX_FIELD_KIND_SYMBOL, SYNTAX_FIELD_KIND_TOKEN_TEXT,
    SYNTAX_FIELD_KIND_UVAR, SYNTAX_FIELD_KIND_VALUE_REF, SYNTAX_NODE_FLAG_RESERVED_MASK, Span,
    SyntaxField, SyntaxFieldValue, SyntaxNodeRecord, SyntaxNodeTable,
};
pub use tag::{
    TAG_FLAG_HAS_SCHEMA, TAG_FLAG_KNOWN_TO_PRODUCER, TAG_FLAG_PORTABLE, TAG_FLAG_REQUIRES_RESOLVER,
    TAG_FLAG_RESERVED_MASK, TAG_FLAG_TRUSTED_ONLY, TagDeclaration, TagTable,
};
pub use trivia::{
    TRIVIA_FLAG_RESERVED_MASK, TRIVIA_KIND_BLANK_LINE, TRIVIA_KIND_COMMENT, TRIVIA_KIND_EXTENSION,
    TRIVIA_KIND_INDENTATION, TRIVIA_KIND_MALFORMED, TRIVIA_KIND_NEWLINE, TRIVIA_KIND_PUNCTUATION,
    TRIVIA_KIND_RESERVED_MASK, TRIVIA_KIND_WHITESPACE, TriviaRecord, TriviaTable,
};
pub use value::{
    DecimalString, ExpressionSource, ExpressionValue, ExtensionValue, FiniteFloat, LuaChunkValue,
    MapEntry, RuntimeDescriptorValue, TaggedValue, Value,
};
pub use write::{WriteOptions, Writer};

#[cfg(test)]
mod tests {
    use super::{from_lyba_value_image, to_lyba_value_image};
    use lyma_syntax::{
        LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaSequence, LymaTag, LymaTagName,
        LymaTaggedValue, LymaValue, source::Span,
    };

    fn span() -> Span {
        Span::new(lyma_syntax::FileId(0), 0, 0)
    }

    #[test]
    fn level1_minimal_encode_is_byte_identical_for_same_values() {
        let values = vec![
            LymaValue::Null(LymaNull),
            LymaValue::Tagged(LymaTaggedValue {
                tag: LymaTag {
                    name: LymaTagName {
                        value: String::from("tag"),
                        span: span(),
                    },
                    span: span(),
                },
                value: Box::new(LymaValue::Mapping(LymaMapping {
                    entries: vec![LymaMappingEntry {
                        key: LymaKey::String(String::from("items")),
                        value: LymaValue::Sequence(LymaSequence {
                            items: vec![LymaValue::String(String::from("x"))],
                            span: None,
                        }),
                        span: None,
                    }],
                    duplicate_keys: Vec::new(),
                    span: None,
                })),
                span: None,
            }),
        ];

        let left = to_lyba_value_image(&values);
        let right = to_lyba_value_image(&values);

        assert_eq!(left, right);
    }

    #[test]
    fn level1_minimal_decode_returns_original_portable_documents() {
        let values = vec![
            LymaValue::Null(LymaNull),
            LymaValue::Boolean(true),
            LymaValue::String(String::from("portable")),
        ];

        let decoded = from_lyba_value_image(&to_lyba_value_image(&values))
            .expect("encoded values should decode");

        assert_eq!(decoded, values);
    }

    #[test]
    fn level1_minimal_decode_is_fallible_for_malformed_input() {
        let error = from_lyba_value_image(&[0xFF]).expect_err("malformed bytes should fail");

        assert!(error.code().as_str().starts_with("LB"));
    }
}
pub use bundle::{
    BundleDescriptor, DEPENDENCY_FLAG_DIGEST_PRESENT, DEPENDENCY_FLAG_EMBEDDED,
    DEPENDENCY_FLAG_FILE_URI, DEPENDENCY_FLAG_HOST_MODULE, DEPENDENCY_FLAG_NETWORK_URI,
    DEPENDENCY_FLAG_REQUIRED, DEPENDENCY_FLAG_RESERVED_MASK, DEPENDENCY_FLAG_RESOLVED,
    DEPENDENCY_FLAG_TRUSTED_ONLY, DEPENDENCY_KIND_EXTENSION, DEPENDENCY_KIND_EXTERNAL_RESOURCE,
    DEPENDENCY_KIND_GENERATED, DEPENDENCY_KIND_IMPORT, DEPENDENCY_KIND_INCLUDE,
    DEPENDENCY_KIND_MODULE, DEPENDENCY_KIND_SCHEMA, DEPENDENCY_KIND_SOURCE,
    DEPENDENCY_SECTION_NAME, DependencyRecord, DependencyTable,
    EMBEDDED_RESOURCE_FLAG_RESERVED_MASK, EMBEDDED_RESOURCE_KIND_BYTES,
    EMBEDDED_RESOURCE_KIND_EXTENSION, EMBEDDED_RESOURCE_KIND_LUA_SOURCE,
    EMBEDDED_RESOURCE_KIND_LYMA_TEXT, EMBEDDED_RESOURCE_KIND_LYBA_CONTAINER,
    EMBEDDED_RESOURCE_KIND_SCHEMA_LYMA, EMBEDDED_RESOURCE_SECTION_NAME, EmbeddedResourceRecord,
    EmbeddedResourceTable,
};
