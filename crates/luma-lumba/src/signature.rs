//! Signature, digest, and inert integrity metadata helpers.
//!
//! Core LUMBA does not define a trusted algorithm set and this module does not
//! perform cryptographic verification. Hosts may use these records for
//! structural coverage inspection and then apply their own trust policy.

use crate::blob::BlobId;
use crate::container::LumbaFile;
use crate::error::{ErrorContext, LumbaError, Result};
use crate::policy::Limits;
use crate::primitives::{Identifier, UVar};
use crate::section::SectionId;
use crate::string_table::StringTable;
use crate::symbol::SymbolTable;
use crate::value::Value;

/// `SIGN`
pub const SIGNATURE_SECTION_NAME: &str = "SIGN";

/// Digest record.
pub const SIGNATURE_RECORD_KIND_DIGEST: u64 = 0;
/// Signature record.
pub const SIGNATURE_RECORD_KIND_SIGNATURE: u64 = 1;
/// Certificate-chain record.
pub const SIGNATURE_RECORD_KIND_CERTIFICATE_CHAIN: u64 = 2;
/// Transparency-log record.
pub const SIGNATURE_RECORD_KIND_TRANSPARENCY_RECORD: u64 = 3;
/// Extension-defined record.
pub const SIGNATURE_RECORD_KIND_EXTENSION: u64 = 4;

/// Covered sections are listed explicitly by section-table index.
pub const SIGNATURE_COVERED_RANGE_KIND_EXPLICIT_SECTIONS: u64 = 0;

/// Common digest algorithm symbol.
pub const SIGNATURE_ALGORITHM_SHA256: &str = "sha-256";
/// Common digest algorithm symbol.
pub const SIGNATURE_ALGORITHM_SHA384: &str = "sha-384";
/// Common digest algorithm symbol.
pub const SIGNATURE_ALGORITHM_SHA512: &str = "sha-512";
/// Common digest algorithm symbol.
pub const SIGNATURE_ALGORITHM_BLAKE3_256: &str = "blake3-256";
/// Common signature algorithm symbol.
pub const SIGNATURE_ALGORITHM_ED25519: &str = "ed25519";
/// Common signature algorithm symbol.
pub const SIGNATURE_ALGORITHM_ECDSA_P256_SHA256: &str = "ecdsa-p256-sha256";
/// Common signature algorithm symbol.
pub const SIGNATURE_ALGORITHM_RSA_PSS_SHA256: &str = "rsa-pss-sha256";

/// One decoded `SIGN` record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignatureRecord {
    /// Stored record kind.
    pub kind: u64,
    /// Optional inert algorithm symbol stored through `SYMS`/`STRS`.
    pub algorithm: Option<Identifier>,
    /// Stored raw covered-range kind.
    pub covered_range_kind: u64,
    /// Covered section-table indexes.
    pub covered_section_refs: Vec<u64>,
    /// Optional inert payload blob reference into `BLOB`.
    pub payload_blob_ref: Option<BlobId>,
    /// Optional metadata value stored through `VALS`.
    pub metadata_value: Option<Value>,
}

impl SignatureRecord {
    /// Creates an empty record of the provided kind.
    #[must_use]
    pub fn new(kind: u64) -> Self {
        Self {
            kind,
            covered_range_kind: SIGNATURE_COVERED_RANGE_KIND_EXPLICIT_SECTIONS,
            ..Self::default()
        }
    }

    /// Sets the optional algorithm symbol.
    #[must_use]
    pub fn with_algorithm(mut self, algorithm: Option<Identifier>) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Sets the covered-range kind.
    #[must_use]
    pub fn with_covered_range_kind(mut self, covered_range_kind: u64) -> Self {
        self.covered_range_kind = covered_range_kind;
        self
    }

    /// Sets the covered section refs.
    #[must_use]
    pub fn with_covered_section_refs(mut self, covered_section_refs: Vec<u64>) -> Self {
        self.covered_section_refs = covered_section_refs;
        self
    }

    /// Sets the optional payload blob ref.
    #[must_use]
    pub fn with_payload_blob_ref(mut self, payload_blob_ref: Option<BlobId>) -> Self {
        self.payload_blob_ref = payload_blob_ref;
        self
    }

    /// Sets the optional metadata value.
    #[must_use]
    pub fn with_metadata_value(mut self, metadata_value: Option<Value>) -> Self {
        self.metadata_value = metadata_value;
        self
    }

    /// Returns whether this is a digest record.
    #[must_use]
    pub const fn is_digest(&self) -> bool {
        self.kind == SIGNATURE_RECORD_KIND_DIGEST
    }

    /// Returns whether this is a signature record.
    #[must_use]
    pub const fn is_signature(&self) -> bool {
        self.kind == SIGNATURE_RECORD_KIND_SIGNATURE
    }
}

/// In-memory `SIGN` table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignatureTable {
    /// Ordered records.
    pub records: Vec<SignatureRecord>,
}

impl SignatureTable {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record.
    #[must_use]
    pub fn with_record(mut self, record: SignatureRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Returns the record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Covered section resolved during structural verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CoveredSection {
    /// Encoded section-table index.
    pub section_index: u64,
    /// Resolved section identifier.
    pub section_id: SectionId,
}

/// Structural verification output for one `SIGN` record.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralSignatureRecord {
    /// Zero-based signature record index.
    pub record_index: usize,
    /// Stored record kind.
    pub kind: u64,
    /// Preserved inert algorithm symbol.
    pub algorithm: Option<Identifier>,
    /// Stored covered-range kind.
    pub covered_range_kind: u64,
    /// Resolved covered sections.
    pub covered_sections: Vec<CoveredSection>,
    /// Optional inert payload blob ref.
    pub payload_blob_ref: Option<BlobId>,
    /// Optional metadata value.
    pub metadata_value: Option<Value>,
}

/// Structural verification output for `SIGN`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructuralSignatureReport {
    /// Verified records.
    pub records: Vec<StructuralSignatureRecord>,
}

/// Structural verifier for inert `SIGN` coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SignatureVerifier;

impl SignatureVerifier {
    /// Creates a structural verifier.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Resolves covered section refs without establishing cryptographic trust.
    pub fn verify_structural_coverage(
        &self,
        file: &LumbaFile,
    ) -> Result<StructuralSignatureReport> {
        let Some(table) = &file.signature_table else {
            return Ok(StructuralSignatureReport::default());
        };
        let section_manifest = encoded_section_manifest(file);
        let mut records = Vec::with_capacity(table.records.len());
        for (record_index, record) in table.records.iter().enumerate() {
            validate_record_kind(record.kind, record_index)?;
            let mut covered_sections = Vec::with_capacity(record.covered_section_refs.len());
            for section_index in &record.covered_section_refs {
                let section_id =
                    *section_manifest
                        .get(*section_index as usize)
                        .ok_or_else(|| {
                            LumbaError::InvalidValueReference(
                                ErrorContext::new(
                                    "signature covered section reference was out of range",
                                )
                                .with_record_index(record_index),
                            )
                        })?;
                covered_sections.push(CoveredSection {
                    section_index: *section_index,
                    section_id,
                });
            }
            records.push(StructuralSignatureRecord {
                record_index,
                kind: record.kind,
                algorithm: record.algorithm.clone(),
                covered_range_kind: record.covered_range_kind,
                covered_sections,
                payload_blob_ref: record.payload_blob_ref,
                metadata_value: record.metadata_value.clone(),
            });
        }
        Ok(StructuralSignatureReport { records })
    }
}

pub(crate) fn decode_signature_table(
    payload: &[u8],
    limits: &Limits,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    values: Option<&[Value]>,
    blob_count: usize,
    section_count: usize,
) -> Result<SignatureTable> {
    let mut offset = 0_usize;
    let record_count = usize::try_from(UVar::decode(payload, &mut offset)?.0).map_err(|_| {
        LumbaError::limit_exceeded("signature record count exceeds configured maximum")
    })?;
    if record_count > limits.max_table_record_count {
        return Err(LumbaError::limit_exceeded(
            "signature record count exceeds configured maximum",
        ));
    }

    let mut records = Vec::with_capacity(record_count);
    for record_index in 0..record_count {
        let kind = UVar::decode(payload, &mut offset)?.0;
        validate_record_kind(kind, record_index)?;
        let algorithm = decode_optional_symbol_string_ref(
            payload,
            &mut offset,
            strings,
            symbols,
            record_index,
            "signature algorithm",
        )?;
        let covered_range_kind = UVar::decode(payload, &mut offset)?.0;
        let covered_section_count = usize::try_from(UVar::decode(payload, &mut offset)?.0)
            .map_err(|_| {
                LumbaError::limit_exceeded(
                    "signature covered section count exceeds configured maximum",
                )
            })?;
        if covered_section_count > limits.max_table_record_count {
            return Err(LumbaError::limit_exceeded(
                "signature covered section count exceeds configured maximum",
            ));
        }
        let mut covered_section_refs = Vec::with_capacity(covered_section_count);
        for _ in 0..covered_section_count {
            covered_section_refs.push(decode_required_ref(
                payload,
                &mut offset,
                section_count,
                record_index,
                "signature covered section",
            )?);
        }
        let payload_blob_ref = decode_blob_ref(
            payload,
            &mut offset,
            blob_count,
            record_index,
            "signature payload",
        )?
        .map(BlobId);
        let metadata_value = decode_value_ref(
            payload,
            &mut offset,
            values,
            record_index,
            "signature metadata",
        )?;
        records.push(SignatureRecord {
            kind,
            algorithm,
            covered_range_kind,
            covered_section_refs,
            payload_blob_ref,
            metadata_value,
        });
    }

    if offset != payload.len() {
        return Err(LumbaError::InvalidSectionTable(
            ErrorContext::new("signature table payload had trailing bytes")
                .with_byte_offset(offset),
        ));
    }

    Ok(SignatureTable { records })
}

pub(crate) fn encode_signature_table(
    table: &SignatureTable,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    values: &[Value],
    blob_count: usize,
    section_count: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    UVar(table.records.len() as u64).encode_into(&mut bytes);
    for (record_index, record) in table.records.iter().enumerate() {
        validate_record_kind(record.kind, record_index)?;
        UVar(record.kind).encode_into(&mut bytes);
        UVar(encode_optional_symbol_string_ref(
            record.algorithm.as_ref(),
            strings,
            symbols,
            record_index,
            "signature algorithm",
        )?)
        .encode_into(&mut bytes);
        UVar(record.covered_range_kind).encode_into(&mut bytes);
        UVar(record.covered_section_refs.len() as u64).encode_into(&mut bytes);
        for section_ref in &record.covered_section_refs {
            UVar(encode_required_ref(
                *section_ref,
                section_count,
                record_index,
                "signature covered section",
            )?)
            .encode_into(&mut bytes);
        }
        UVar(encode_blob_ref(
            record.payload_blob_ref,
            blob_count,
            record_index,
            "signature payload",
        )?)
        .encode_into(&mut bytes);
        UVar(encode_value_ref(
            record.metadata_value.as_ref(),
            values,
            record_index,
            "signature metadata",
        )?)
        .encode_into(&mut bytes);
    }
    Ok(bytes)
}

pub(crate) fn encoded_section_manifest(file: &LumbaFile) -> Vec<SectionId> {
    let mut manifest = Vec::new();
    if file.metadata.is_some() {
        manifest.push(SectionId::META);
    }
    if file
        .extension_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::EXTS);
    }
    if file
        .string_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::STRS);
    }
    if file
        .symbol_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::SYMS);
    }
    if file
        .blob_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::BLOB);
    }
    if file
        .sections
        .iter()
        .any(|section| !section.values.is_empty())
    {
        manifest.push(SectionId::VALS);
    }
    if !file.documents.is_empty() {
        manifest.push(SectionId::DOCS);
    }
    if file
        .tag_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::TAGS);
    }
    if file
        .schema_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::SCMA);
    }
    if file
        .source_file_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::SRCF);
    }
    if file
        .source_span_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::SRCS);
    }
    if file
        .syntax_node_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::ASTN);
    }
    if file
        .trivia_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::TRIV);
    }
    if file
        .dependency_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::DEPS);
    }
    if file
        .embedded_resource_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::EMBD);
    }
    if file
        .diagnostic_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::DIAG);
    }
    if file
        .signature_table
        .as_ref()
        .is_some_and(|table| !table.is_empty())
    {
        manifest.push(SectionId::SIGN);
    }
    manifest
}

fn validate_record_kind(kind: u64, record_index: usize) -> Result<()> {
    if kind <= SIGNATURE_RECORD_KIND_EXTENSION {
        Ok(())
    } else {
        Err(LumbaError::InvalidSectionTable(
            ErrorContext::new(format!("signature record kind {kind} was not recognized"))
                .with_record_index(record_index),
        ))
    }
}

fn decode_required_ref(
    payload: &[u8],
    offset: &mut usize,
    count: usize,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let value = UVar::decode(payload, offset)?.0;
    if value >= count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn encode_required_ref(value: u64, count: usize, record_index: usize, kind: &str) -> Result<u64> {
    if value >= count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(value)
}

fn decode_optional_symbol_string_ref(
    payload: &[u8],
    offset: &mut usize,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    record_index: usize,
    kind: &str,
) -> Result<Option<Identifier>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let symbol_ref = encoded - 1;
    let strings = strings.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires STRS so symbol text can be resolved"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbols = symbols.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires SYMS so symbol references can be resolved"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbol = symbols.symbols.get(symbol_ref as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        )
    })?;
    let string = strings
        .strings
        .get(symbol.string_id as usize)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string reference was out of range"))
                    .with_record_index(record_index),
            )
        })?;
    Ok(Some(Identifier::new(string.value.clone())))
}

fn encode_optional_symbol_string_ref(
    value: Option<&Identifier>,
    strings: Option<&StringTable>,
    symbols: Option<&SymbolTable>,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    let strings = strings.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires STRS so symbol text can be encoded"
            ))
            .with_record_index(record_index),
        )
    })?;
    let symbols = symbols.ok_or_else(|| {
        LumbaError::InvalidSectionTable(
            ErrorContext::new(format!(
                "{kind} requires SYMS so symbol references can be encoded"
            ))
            .with_record_index(record_index),
        )
    })?;
    let string_id = strings
        .strings
        .iter()
        .position(|record| record.value == value.as_str())
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} string was not present in STRS"))
                    .with_record_index(record_index),
            )
        })? as u64;
    let symbol_id = symbols
        .symbols
        .iter()
        .position(|record| record.string_id == string_id)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} symbol was not present in SYMS"))
                    .with_record_index(record_index),
            )
        })? as u64;
    symbol_id.checked_add(1).ok_or_else(|| {
        LumbaError::invalid_section_table("signature algorithm symbol reference overflowed u64")
    })
}

fn decode_blob_ref(
    payload: &[u8],
    offset: &mut usize,
    blob_count: usize,
    record_index: usize,
    kind: &str,
) -> Result<Option<u64>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let blob_ref = encoded - 1;
    if blob_ref >= blob_count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    Ok(Some(blob_ref))
}

fn encode_blob_ref(
    blob_ref: Option<BlobId>,
    blob_count: usize,
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(blob_ref) = blob_ref else {
        return Ok(0);
    };
    if blob_ref.0 >= blob_count as u64 {
        return Err(LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        ));
    }
    blob_ref
        .0
        .checked_add(1)
        .ok_or_else(|| LumbaError::invalid_section_table("signature blob reference overflowed u64"))
}

fn decode_value_ref(
    payload: &[u8],
    offset: &mut usize,
    values: Option<&[Value]>,
    record_index: usize,
    kind: &str,
) -> Result<Option<Value>> {
    let encoded = UVar::decode(payload, offset)?.0;
    if encoded == 0 {
        return Ok(None);
    }
    let value_ref = encoded - 1;
    let values = values.ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference required VALS"))
                .with_record_index(record_index),
        )
    })?;
    let value = values.get(value_ref as usize).ok_or_else(|| {
        LumbaError::InvalidValueReference(
            ErrorContext::new(format!("{kind} reference was out of range"))
                .with_record_index(record_index),
        )
    })?;
    Ok(Some(value.clone()))
}

fn encode_value_ref(
    value: Option<&Value>,
    values: &[Value],
    record_index: usize,
    kind: &str,
) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    let index = values
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(
                ErrorContext::new(format!("{kind} was not present in the encoded VALS table"))
                    .with_record_index(record_index),
            )
        })? as u64;
    index.checked_add(1).ok_or_else(|| {
        LumbaError::invalid_section_table("signature value reference overflowed u64")
    })
}
