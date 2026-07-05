//! Writer configuration and entry points.

use crate::blob::{BlobRecord, BlobTable, encode_blob_table};
use crate::bundle::{
    DependencyTable, EmbeddedResourceTable, encode_dependency_table, encode_embedded_resource_table,
};
use crate::capability::{CapabilityTable, encode_capability_table};
use crate::checksum::encode_section_checksum;
use crate::codec::CODEC_NONE;
use crate::container::{
    ContainerFooter, ContainerHeader, FOOTER_LEN, HEADER_SIZE, HeaderCrcMode, LumbaFile,
};
use crate::diagnostic::{DiagnosticTable, encode_diagnostic_table};
use crate::document::Document;
use crate::document::encode_document_table;
use crate::error::{LumbaError, Result};
use crate::extension::{ExtensionTable, encode_extension_table};
use crate::meta::{Metadata, encode_metadata, metadata_item_count};
pub use crate::mode::{CanonicalMode, TextReconstructionMode, WriterMode};
use crate::policy::Limits;
use crate::primitives::{UVar, pad_to_eight};
use crate::schema::{SchemaTable, encode_schema_table};
use crate::section::{
    SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
    compare_canonical_section_ids,
};
use crate::signature::{SignatureTable, encode_signature_table};
use crate::source::{SourceFileTable, encode_source_file_table, encode_source_span_table};
use crate::string_table::{StringInterner, encode_string_table};
use crate::symbol::{SymbolInterner, SymbolTable, encode_symbol_table};
use crate::syntax::{SyntaxNodeTable, encode_syntax_node_table};
use crate::tag::{TagTable, encode_tag_table};
use crate::trivia::{TriviaTable, encode_trivia_table};
use crate::value::{VALUE_SECTION_NAME, Value, encode_value_table};
use crate::verify::verify_level1_minimal_value_image_file;
use luma_syntax::{LumaKey, LumaValue};

const CONTAINER_FLAG_REQUIRES_EVAL_CAPABILITIES: u32 = 1 << 8;

/// Writer configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriteOptions {
    /// Output formatting mode.
    pub mode: WriterMode,
    /// Resource limits to enforce while writing.
    pub limits: Limits,
    /// Whether to emit the optional header CRC32C.
    pub header_crc_mode: HeaderCrcMode,
    /// Checksum algorithm to encode in section table entries.
    pub section_checksum_id: u16,
    /// Whether to emit the optional fixed footer.
    pub emit_footer: bool,
    /// Preferred text reconstruction policy for downstream tooling.
    pub text_reconstruction_mode: TextReconstructionMode,
}

impl WriteOptions {
    /// Creates default write options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the writer mode.
    #[must_use]
    pub fn with_mode(mut self, mode: WriterMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets limits.
    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Sets optional header CRC emission policy.
    #[must_use]
    pub fn with_header_crc_mode(mut self, header_crc_mode: HeaderCrcMode) -> Self {
        self.header_crc_mode = header_crc_mode;
        self
    }

    /// Sets the section checksum algorithm ID.
    #[must_use]
    pub fn with_section_checksum_id(mut self, section_checksum_id: u16) -> Self {
        self.section_checksum_id = section_checksum_id;
        self
    }

    /// Enables or disables footer emission.
    #[must_use]
    pub fn with_footer(mut self, emit_footer: bool) -> Self {
        self.emit_footer = emit_footer;
        self
    }

    /// Sets the preferred text reconstruction policy.
    #[must_use]
    pub fn with_text_reconstruction_mode(
        mut self,
        text_reconstruction_mode: TextReconstructionMode,
    ) -> Self {
        self.text_reconstruction_mode = text_reconstruction_mode;
        self
    }
}

/// Writer entry point for encoding LUMBA documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Writer {
    options: WriteOptions,
}

impl Writer {
    /// Creates a writer with explicit options.
    #[must_use]
    pub fn new(options: WriteOptions) -> Self {
        Self { options }
    }

    /// Returns the configured options.
    #[must_use]
    pub fn options(&self) -> &WriteOptions {
        &self.options
    }

    /// Serializes a file model into bytes.
    pub fn write(&self, file: &LumbaFile) -> Result<Vec<u8>> {
        let mut sections = Vec::<(SectionEntry, Vec<u8>)>::new();
        let metadata = runtime_metadata_for_mode(file, self.options.mode);
        let extension_table = file.extension_table.clone().unwrap_or_default();
        let mut blob_table = file.blob_table.clone().unwrap_or_default();
        let mut tag_table = file.tag_table.clone().unwrap_or_default();
        let mut schema_table = file.schema_table.clone().unwrap_or_default();
        let mut diagnostic_table = file.diagnostic_table.clone().unwrap_or_default();
        let dependency_table = file.dependency_table.clone().unwrap_or_default();
        let embedded_resource_table = file.embedded_resource_table.clone().unwrap_or_default();
        let capability_table = file.capability_table.clone().unwrap_or_default();
        let signature_table = file.signature_table.clone().unwrap_or_default();
        let mut source_file_table = file.source_file_table.clone().unwrap_or_default();
        let mut source_span_table = file.source_span_table.clone().unwrap_or_default();
        let mut syntax_node_table = file.syntax_node_table.clone().unwrap_or_default();
        let mut trivia_table = file.trivia_table.clone().unwrap_or_default();
        let mut documents = file.documents.clone();
        let mut value_roots = file
            .sections
            .iter()
            .find(|section| section.name.as_str() == VALUE_SECTION_NAME)
            .map(|section| section.values.clone())
            .unwrap_or_default();

        if !self.options.mode.include_diagnostics() {
            diagnostic_table = DiagnosticTable::new();
        }
        if !self.options.mode.include_source() {
            source_file_table = SourceFileTable::new();
            source_span_table = crate::source::SourceSpanTable::new();
        }
        if !self.options.mode.include_syntax() {
            syntax_node_table = SyntaxNodeTable::new();
        }
        if !self.options.mode.include_trivia() {
            trivia_table = TriviaTable::new();
        }

        for document in &documents {
            if let Some(root_value) = &document.root_value {
                if !value_roots.iter().any(|value| value == root_value) {
                    value_roots.push(root_value.clone());
                }
            }
        }
        for declaration in &extension_table.declarations {
            if let Some(metadata_value) = &declaration.metadata_value {
                if !value_roots.iter().any(|value| value == metadata_value) {
                    value_roots.push(metadata_value.clone());
                }
            }
        }
        for declaration in &tag_table.declarations {
            if let Some(resolver_hint) = &declaration.resolver_hint {
                if !value_roots.iter().any(|value| value == resolver_hint) {
                    value_roots.push(resolver_hint.clone());
                }
            }
            if let Some(metadata_value) = &declaration.metadata_value {
                if !value_roots.iter().any(|value| value == metadata_value) {
                    value_roots.push(metadata_value.clone());
                }
            }
        }
        for record in &schema_table.records {
            if let Some(value) = &record.value {
                if !value_roots.iter().any(|candidate| candidate == value) {
                    value_roots.push(value.clone());
                }
            }
            if let Some(metadata_value) = &record.metadata_value {
                if !value_roots
                    .iter()
                    .any(|candidate| candidate == metadata_value)
                {
                    value_roots.push(metadata_value.clone());
                }
            }
        }
        for record in &dependency_table.records {
            if let Some(metadata_value) = &record.metadata_value {
                if !value_roots
                    .iter()
                    .any(|candidate| candidate == metadata_value)
                {
                    value_roots.push(metadata_value.clone());
                }
            }
        }
        for record in &capability_table.records {
            if let Some(metadata_value) = &record.metadata_value {
                if !value_roots
                    .iter()
                    .any(|candidate| candidate == metadata_value)
                {
                    value_roots.push(metadata_value.clone());
                }
            }
            for requirement in &record.requirements {
                if let Some(metadata_value) = &requirement.metadata_value {
                    if !value_roots
                        .iter()
                        .any(|candidate| candidate == metadata_value)
                    {
                        value_roots.push(metadata_value.clone());
                    }
                }
            }
        }
        for record in &signature_table.records {
            if let Some(metadata_value) = &record.metadata_value {
                if !value_roots
                    .iter()
                    .any(|candidate| candidate == metadata_value)
                {
                    value_roots.push(metadata_value.clone());
                }
            }
        }
        let original_value_roots = value_roots.clone();
        value_roots =
            normalize_value_roots_for_mode(&value_roots, &mut blob_table, self.options.mode)?;
        let dependency_count = dependency_table.len();
        for document in &mut documents {
            if let Some(root_value) = &document.root_value {
                let position = original_value_roots
                    .iter()
                    .position(|value| value == root_value)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "document root value was not present in the encoded VALS table",
                        ))
                    })?;
                document.root_value = Some(value_roots[position].clone());
            }
        }
        for declaration in &mut tag_table.declarations {
            if let Some(resolver_hint) = &declaration.resolver_hint {
                let position = original_value_roots
                    .iter()
                    .position(|value| value == resolver_hint)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "tag resolver hint was not present in the encoded VALS table",
                        ))
                    })?;
                declaration.resolver_hint = Some(value_roots[position].clone());
            }
            if let Some(metadata_value) = &declaration.metadata_value {
                let position = original_value_roots
                    .iter()
                    .position(|value| value == metadata_value)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "tag metadata value was not present in the encoded VALS table",
                        ))
                    })?;
                declaration.metadata_value = Some(value_roots[position].clone());
            }
        }
        for record in &mut schema_table.records {
            if let Some(value) = &record.value {
                let position = original_value_roots
                    .iter()
                    .position(|candidate| candidate == value)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "schema value was not present in the encoded VALS table",
                        ))
                    })?;
                record.value = Some(value_roots[position].clone());
            }
            if let Some(metadata_value) = &record.metadata_value {
                let position = original_value_roots
                    .iter()
                    .position(|candidate| candidate == metadata_value)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "schema metadata value was not present in the encoded VALS table",
                        ))
                    })?;
                record.metadata_value = Some(value_roots[position].clone());
            }
        }
        let mut capability_table = capability_table;
        for record in &mut capability_table.records {
            if let Some(metadata_value) = &record.metadata_value {
                let position = original_value_roots
                    .iter()
                    .position(|candidate| candidate == metadata_value)
                    .ok_or_else(|| {
                        LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                            "capability-set metadata value was not present in the encoded VALS table",
                        ))
                    })?;
                record.metadata_value = Some(value_roots[position].clone());
            }
            for requirement in &mut record.requirements {
                if let Some(metadata_value) = &requirement.metadata_value {
                    let position = original_value_roots
                        .iter()
                        .position(|candidate| candidate == metadata_value)
                        .ok_or_else(|| {
                            LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                                "capability metadata value was not present in the encoded VALS table",
                            ))
                        })?;
                    requirement.metadata_value = Some(value_roots[position].clone());
                }
            }
        }
        let capability_count = capability_table.len();
        for value in &value_roots {
            value.validate_capability_refs(capability_count)?;
        }
        for document in &documents {
            document.validate_capability_refs(capability_count)?;
        }

        let (string_table, symbol_table) = runtime_tables_for_mode(
            file,
            self.options.mode,
            metadata.as_ref(),
            &extension_table,
            &tag_table,
            &schema_table,
            &diagnostic_table,
            &dependency_table,
            &embedded_resource_table,
            &capability_table,
            &signature_table,
            &source_file_table,
            &syntax_node_table,
            &trivia_table,
            &value_roots,
        )?;

        if let Some(metadata) = metadata.as_ref() {
            let payload = encode_metadata(metadata, &self.options.limits)?;
            let item_count = metadata_item_count(&payload)?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::META,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                payload,
            ));
        }

        if let Some(extension_table) =
            (!extension_table.is_empty()).then_some(extension_table.clone())
        {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::MalformedExtensionPayload(crate::error::ErrorContext::new(
                    "EXTS requires STRS so extension names and versions can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::EXTS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: extension_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_extension_table(&extension_table, string_table, &value_roots)?,
            ));
        }

        if let Some(string_table) = string_table.as_ref() {
            sections.push((
                SectionEntry {
                    section_id: SectionId::STRS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: string_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_string_table(string_table)?,
            ));
        }

        if let Some(symbol_table) = symbol_table.as_ref() {
            let symbol_table = canonicalize_symbol_table_for_mode(symbol_table, self.options.mode);
            sections.push((
                SectionEntry {
                    section_id: SectionId::SYMS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: symbol_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_symbol_table(&symbol_table)?,
            ));
        }

        if !blob_table.is_empty() || self.options.mode.force_blob() {
            sections.push((
                SectionEntry {
                    section_id: SectionId::BLOB,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: blob_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_blob_table(&blob_table, &self.options.limits)?,
            ));
        }

        if !value_roots.is_empty() || self.options.mode.force_values() {
            let payload =
                encode_value_table(&value_roots, &self.options.limits, self.options.mode)?;
            let mut payload_offset = 0;
            let value_count = UVar::decode(&payload, &mut payload_offset)
                .map_err(|_| {
                    LumbaError::invalid_section_table("writer produced invalid VALS payload")
                })?
                .0;
            sections.push((
                SectionEntry {
                    section_id: SectionId::VALS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: value_count,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                payload,
            ));
        }

        if !documents.is_empty() || self.options.mode.force_documents() {
            let payload = encode_document_table(
                &documents,
                &value_roots,
                schema_table.len(),
                capability_count,
            )?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::DOCS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: documents.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                payload,
            ));
        }

        if let Some(tag_table) = (!tag_table.is_empty()).then_some(tag_table.clone()) {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TAGS requires STRS so tag URIs can be encoded",
                ))
            })?;
            let symbol_table = symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TAGS requires SYMS so tag symbols can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::TAGS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: tag_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_tag_table(
                    &tag_table,
                    string_table,
                    symbol_table,
                    &value_roots,
                    schema_table.len(),
                )?,
            ));
        }

        if let Some(schema_table) = (!schema_table.is_empty()).then_some(schema_table.clone()) {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "SCMA requires STRS so schema URIs can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::SCMA,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: schema_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_schema_table(&schema_table, string_table, &value_roots, blob_table.len())?,
            ));
        }

        if !source_file_table.is_empty() || self.options.mode.force_source_files() {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "SRCF requires STRS so source URIs and display strings can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::SRCF,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: source_file_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_source_file_table(&source_file_table, string_table, blob_table.len())?,
            ));
        }

        if !source_span_table.is_empty() || self.options.mode.force_source_spans() {
            sections.push((
                SectionEntry {
                    section_id: SectionId::SRCS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: source_span_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_source_span_table(
                    &source_span_table,
                    &self.options.limits,
                    &source_file_table,
                    file.blob_table.as_ref(),
                )?,
            ));
        }

        if !syntax_node_table.is_empty() || self.options.mode.force_syntax_nodes() {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "ASTN requires STRS so text fields can be encoded",
                ))
            })?;
            let symbol_table = symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "ASTN requires SYMS so node kinds and field names can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::ASTN,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: syntax_node_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_syntax_node_table(
                    &syntax_node_table,
                    &self.options.limits,
                    string_table,
                    symbol_table,
                    value_roots.len(),
                    source_span_table.len(),
                    blob_table.len(),
                    trivia_table.len(),
                )?,
            ));
        }

        if !trivia_table.is_empty() || self.options.mode.force_trivia() {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TRIV requires STRS so preserved trivia text can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::TRIV,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: trivia_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_trivia_table(
                    &trivia_table,
                    &self.options.limits,
                    string_table,
                    (!source_span_table.is_empty()).then_some(&source_span_table),
                )?,
            ));
        }

        if let Some(dependency_table) = (!dependency_table.is_empty()).then_some(dependency_table) {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DEPS requires STRS so dependency URIs can be encoded",
                ))
            })?;
            let symbol_table = symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DEPS requires SYMS so dependency aliases can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::DEPS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: dependency_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_dependency_table(
                    &dependency_table,
                    string_table,
                    symbol_table,
                    &value_roots,
                    source_span_table.len(),
                    blob_table.len(),
                )?,
            ));
        }

        if let Some(embedded_resource_table) =
            (!embedded_resource_table.is_empty()).then_some(embedded_resource_table)
        {
            sections.push((
                SectionEntry {
                    section_id: SectionId::EMBD,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: embedded_resource_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_embedded_resource_table(
                    &embedded_resource_table,
                    dependency_count,
                    blob_table.len(),
                    string_table.as_ref(),
                    symbol_table.as_ref(),
                )?,
            ));
        }

        if let Some(capability_table) = (!capability_table.is_empty()).then_some(capability_table) {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "CAPS requires STRS so capability names can be encoded",
                ))
            })?;
            let symbol_table = symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "CAPS requires SYMS so capability symbols can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::CAPS,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: capability_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_capability_table(
                    &capability_table,
                    string_table,
                    symbol_table,
                    &value_roots,
                )?,
            ));
        }

        if let Some(diagnostic_table) = (!diagnostic_table.is_empty()
            || self.options.mode.force_diagnostic_section())
        .then_some(diagnostic_table)
        {
            let string_table = string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DIAG requires STRS so diagnostic messages can be encoded",
                ))
            })?;
            let symbol_table = symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DIAG requires SYMS so diagnostic codes can be encoded",
                ))
            })?;
            sections.push((
                SectionEntry {
                    section_id: SectionId::DIAG,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: diagnostic_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_diagnostic_table(
                    &diagnostic_table,
                    &self.options.limits,
                    string_table,
                    symbol_table,
                    source_span_table.len(),
                )?,
            ));
        }

        if let Some(signature_table) = (!signature_table.is_empty()).then_some(signature_table) {
            let emitted_section_count = sections.len() + 1;
            sections.push((
                SectionEntry {
                    section_id: SectionId::SIGN,
                    section_version: 1,
                    entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                    payload_flags: 0,
                    codec_id: CODEC_NONE,
                    checksum_id: self.options.section_checksum_id,
                    payload_offset: 0,
                    stored_size: 0,
                    logical_size: 0,
                    item_count: signature_table.len() as u64,
                    checksum_low: 0,
                    checksum_high: 0,
                },
                encode_signature_table(
                    &signature_table,
                    string_table.as_ref(),
                    symbol_table.as_ref(),
                    &value_roots,
                    blob_table.len(),
                    emitted_section_count,
                )?,
            ));
        }

        if sections.is_empty() {
            let mut header = ContainerHeader::new();
            header.file_length = if self.options.emit_footer {
                u64::from(HEADER_SIZE) + FOOTER_LEN as u64
            } else {
                u64::from(HEADER_SIZE)
            };
            let mut footer_header = header.clone();
            let header = header.encode(self.options.header_crc_mode)?;
            let mut bytes = Vec::with_capacity(
                header.len()
                    + if self.options.emit_footer {
                        FOOTER_LEN
                    } else {
                        0
                    },
            );
            bytes.extend_from_slice(&header);
            if self.options.emit_footer {
                footer_header.header_crc32c =
                    u32::from_le_bytes(header[56..60].try_into().expect("slice length is fixed"));
                let footer = ContainerFooter::from_header(&footer_header).encode();
                bytes.extend_from_slice(&footer);
            }
            return Ok(bytes);
        }

        sections.sort_by(|(left, _), (right, _)| {
            compare_canonical_section_ids(left.section_id, right.section_id)
        });

        let entry_bytes =
            u64::try_from(encode_section_table_len(sections.len())?).map_err(|_| {
                LumbaError::invalid_section_table("section table length overflowed u64")
            })?;
        let table_offset = u64::from(HEADER_SIZE);
        let mut payload_offset = table_offset
            .checked_add(entry_bytes)
            .ok_or_else(|| LumbaError::invalid_section_table("section table end overflowed u64"))?;

        for (entry, payload) in &mut sections {
            encode_section_checksum(entry, payload)?;
            let mut padded = payload.clone();
            pad_to_eight(&mut padded);
            entry.payload_offset = payload_offset;
            entry.stored_size = payload.len() as u64;
            entry.logical_size = payload.len() as u64;
            payload_offset = payload_offset
                .checked_add(padded.len() as u64)
                .ok_or_else(|| {
                    LumbaError::invalid_section_table("section payload range overflowed u64")
                })?;
            *payload = padded;
        }

        let entries = sections.iter().map(|(entry, _)| *entry).collect::<Vec<_>>();
        let table = encode_section_table(&entries)?;
        let mut header = ContainerHeader::new();
        header.container_flags = self.options.mode.recommended_container_flags();
        header.profile_flags = self.options.mode.recommended_profile_flags();
        if !file.capability_table.clone().unwrap_or_default().is_empty() {
            header.container_flags |= CONTAINER_FLAG_REQUIRES_EVAL_CAPABILITIES;
        }
        header.section_count = entries.len() as u32;
        header.file_length = payload_offset
            + if self.options.emit_footer {
                FOOTER_LEN as u64
            } else {
                0
            };
        header.root_document_count = documents.len() as u64;
        let mut footer_header = header.clone();

        let capacity = usize::from(HEADER_SIZE)
            .checked_add(table.len())
            .and_then(|value| {
                sections
                    .iter()
                    .try_fold(value, |acc, (_, payload)| acc.checked_add(payload.len()))
            })
            .and_then(|value| {
                if self.options.emit_footer {
                    value.checked_add(FOOTER_LEN)
                } else {
                    Some(value)
                }
            })
            .ok_or_else(|| LumbaError::invalid_section_table("output length overflowed usize"))?;
        let header = header.encode(self.options.header_crc_mode)?;
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&table);
        for (_, payload) in sections {
            bytes.extend_from_slice(&payload);
        }
        if self.options.emit_footer {
            footer_header.header_crc32c =
                u32::from_le_bytes(header[56..60].try_into().expect("slice length is fixed"));
            let footer = ContainerFooter::from_header(&footer_header).encode();
            bytes.extend_from_slice(&footer);
        }
        Ok(bytes)
    }
}

pub(crate) fn write_level1_minimal_value_image(values: &[LumaValue]) -> Result<Vec<u8>> {
    let mut file = LumbaFile::new().with_string_table(collect_level1_minimal_strings(values));
    for value in values {
        file = file
            .with_document(Document::new().with_root_value(crate::value::Value::try_from(value)?));
    }
    verify_level1_minimal_value_image_file(&file)?;

    Writer::new(
        WriteOptions::new()
            .with_mode(WriterMode::Canonical(CanonicalMode::Strict))
            .with_header_crc_mode(HeaderCrcMode::Disabled),
    )
    .write(&file)
}

fn collect_level1_minimal_strings(values: &[LumaValue]) -> crate::string_table::StringTable {
    let mut interner = StringInterner::new();
    for value in values {
        collect_value_strings(value, &mut interner);
    }
    interner.into_table()
}

fn runtime_metadata_for_mode(file: &LumbaFile, mode: WriterMode) -> Option<Metadata> {
    file.metadata.clone().or_else(|| match mode {
        WriterMode::RuntimeData => Some(Metadata::runtime_data_value_image()),
        WriterMode::BuildBundle | WriterMode::EditorCache | WriterMode::ConformanceFixture => {
            Some(default_metadata_for_mode(mode))
        }
        _ => None,
    })
}

fn default_metadata_for_mode(mode: WriterMode) -> Metadata {
    let mut metadata = Metadata::new()
        .with_entry("format", Value::String(String::from("lumba")))
        .with_entry("luma_version", Value::String(String::from("0.1")))
        .with_entry("lumba_version", Value::String(String::from("0.1")));
    if let Some(image_kind) = mode.default_image_kind() {
        metadata = metadata.with_entry("image_kind", Value::String(String::from(image_kind)));
    }
    metadata
}

fn runtime_tables_for_mode(
    file: &LumbaFile,
    mode: WriterMode,
    metadata: Option<&Metadata>,
    extension_table: &ExtensionTable,
    tag_table: &TagTable,
    schema_table: &SchemaTable,
    diagnostic_table: &DiagnosticTable,
    dependency_table: &DependencyTable,
    embedded_resource_table: &EmbeddedResourceTable,
    capability_table: &CapabilityTable,
    signature_table: &SignatureTable,
    source_file_table: &SourceFileTable,
    syntax_node_table: &SyntaxNodeTable,
    trivia_table: &TriviaTable,
    value_roots: &[crate::value::Value],
) -> Result<(
    Option<crate::string_table::StringTable>,
    Option<SymbolTable>,
)> {
    if let Some(symbol_table) = &file.symbol_table {
        let Some(string_table) = &file.string_table else {
            return Err(LumbaError::invalid_section_table(
                "SYMS requires STRS when an explicit symbol table is provided",
            ));
        };
        let string_table = augment_string_table(
            string_table,
            extension_table,
            tag_table,
            schema_table,
            diagnostic_table,
            dependency_table,
            embedded_resource_table,
            capability_table,
            signature_table,
            source_file_table,
            syntax_node_table,
            trivia_table,
        );
        return Ok((
            Some(string_table.clone()),
            Some(augment_symbol_table(
                &string_table,
                symbol_table,
                tag_table,
                diagnostic_table,
                dependency_table,
                embedded_resource_table,
                capability_table,
                signature_table,
                syntax_node_table,
            )?),
        ));
    }
    if let Some(string_table) = &file.string_table {
        let string_table = augment_string_table(
            string_table,
            extension_table,
            tag_table,
            schema_table,
            diagnostic_table,
            dependency_table,
            embedded_resource_table,
            capability_table,
            signature_table,
            source_file_table,
            syntax_node_table,
            trivia_table,
        );
        if tag_table.is_empty()
            && diagnostic_table.is_empty()
            && dependency_table.is_empty()
            && embedded_resource_table.is_empty()
            && capability_table.is_empty()
            && signature_table.is_empty()
        {
            if syntax_node_table.is_empty() {
                return Ok((
                    Some(string_table.clone()),
                    if mode.force_symbols() {
                        Some(SymbolTable::new())
                    } else {
                        None
                    },
                ));
            }
            return Ok((
                Some(string_table.clone()),
                Some(build_symbol_table(
                    &string_table,
                    tag_table,
                    diagnostic_table,
                    dependency_table,
                    embedded_resource_table,
                    capability_table,
                    signature_table,
                    syntax_node_table,
                )?),
            ));
        }
        return Ok((
            Some(string_table.clone()),
            Some(build_symbol_table(
                &string_table,
                tag_table,
                diagnostic_table,
                dependency_table,
                embedded_resource_table,
                capability_table,
                signature_table,
                syntax_node_table,
            )?),
        ));
    }
    if !matches!(
        mode,
        WriterMode::RuntimeData
            | WriterMode::BuildBundle
            | WriterMode::EditorCache
            | WriterMode::ConformanceFixture
    ) {
        if extension_table.is_empty() {
            if tag_table.is_empty()
                && schema_table.is_empty()
                && diagnostic_table.is_empty()
                && dependency_table.is_empty()
                && embedded_resource_table.is_empty()
                && capability_table.is_empty()
                && signature_table.is_empty()
                && source_file_table.is_empty()
                && syntax_node_table.is_empty()
                && trivia_table.is_empty()
            {
                return Ok((None, None));
            }
            let string_table = build_string_table(
                &ExtensionTable::new(),
                tag_table,
                schema_table,
                diagnostic_table,
                dependency_table,
                embedded_resource_table,
                capability_table,
                signature_table,
                source_file_table,
                syntax_node_table,
                trivia_table,
            );
            let symbol_table = build_symbol_table(
                &string_table,
                tag_table,
                diagnostic_table,
                dependency_table,
                embedded_resource_table,
                capability_table,
                signature_table,
                syntax_node_table,
            )?;
            return Ok((Some(string_table), Some(symbol_table)));
        }
        let string_table = build_string_table(
            extension_table,
            tag_table,
            schema_table,
            diagnostic_table,
            dependency_table,
            embedded_resource_table,
            capability_table,
            signature_table,
            source_file_table,
            syntax_node_table,
            trivia_table,
        );
        let symbol_table = if tag_table.is_empty()
            && diagnostic_table.is_empty()
            && dependency_table.is_empty()
            && embedded_resource_table.is_empty()
            && capability_table.is_empty()
            && signature_table.is_empty()
            && syntax_node_table.is_empty()
        {
            None
        } else {
            Some(build_symbol_table(
                &string_table,
                tag_table,
                diagnostic_table,
                dependency_table,
                embedded_resource_table,
                capability_table,
                signature_table,
                syntax_node_table,
            )?)
        };
        return Ok((Some(string_table), symbol_table));
    }

    let mut interner = SymbolInterner::new();
    if let Some(metadata) = metadata {
        collect_native_value_symbols(&metadata.as_map_value(), &mut interner)?;
    }
    for declaration in &extension_table.declarations {
        interner.intern_string(&declaration.name);
        interner.intern_string(&declaration.version);
        if let Some(metadata_value) = &declaration.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
    }
    for declaration in &tag_table.declarations {
        interner.intern_string(declaration.tag.as_str());
        interner.intern_string(&declaration.uri);
        let _ = interner.intern_tag(declaration.tag.as_str(), None)?;
        if let Some(resolver_hint) = &declaration.resolver_hint {
            collect_native_value_symbols(resolver_hint, &mut interner)?;
        }
        if let Some(metadata_value) = &declaration.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
    }
    for record in &schema_table.records {
        if let Some(uri) = &record.uri {
            interner.intern_string(uri);
        }
        if let Some(value) = &record.value {
            collect_native_value_symbols(value, &mut interner)?;
        }
        if let Some(metadata_value) = &record.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
    }
    for record in &diagnostic_table.records {
        interner.intern_string(record.code_symbol.as_str());
        interner.intern_string(&record.message);
        let _ = interner.intern_symbol(record.code_symbol.as_str(), None, 0)?;
        for related in &record.related_spans {
            interner.intern_string(&related.message);
        }
    }
    for record in &dependency_table.records {
        if let Some(uri) = &record.uri {
            interner.intern_string(uri);
        }
        if let Some(alias) = &record.alias {
            interner.intern_string(alias.as_str());
            let _ = interner.intern_symbol(alias.as_str(), None, 0)?;
        }
        if let Some(metadata_value) = &record.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
    }
    for record in &embedded_resource_table.records {
        if let Some(extension_kind) = &record.extension_kind {
            interner.intern_string(extension_kind.as_str());
            let _ = interner.intern_symbol(extension_kind.as_str(), None, 0)?;
        }
    }
    for record in &capability_table.records {
        if let Some(metadata_value) = &record.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
        for requirement in &record.requirements {
            interner.intern_string(requirement.capability.as_str());
            let _ = interner.intern_symbol(requirement.capability.as_str(), None, 0)?;
            if let Some(metadata_value) = &requirement.metadata_value {
                collect_native_value_symbols(metadata_value, &mut interner)?;
            }
        }
    }
    for record in &signature_table.records {
        if let Some(algorithm) = &record.algorithm {
            interner.intern_string(algorithm.as_str());
            let _ = interner.intern_symbol(algorithm.as_str(), None, 0)?;
        }
        if let Some(metadata_value) = &record.metadata_value {
            collect_native_value_symbols(metadata_value, &mut interner)?;
        }
    }
    for record in &source_file_table.records {
        if let Some(uri) = &record.uri {
            interner.intern_string(uri);
        }
        if let Some(display) = &record.display {
            interner.intern_string(display);
        }
    }
    collect_syntax_table_symbols(syntax_node_table, &mut interner)?;
    for record in &trivia_table.records {
        interner.intern_string(&record.text);
    }
    for value in value_roots {
        collect_native_value_symbols(value, &mut interner)?;
    }
    let (string_table, symbol_table) = interner.into_tables();
    Ok((Some(string_table), Some(symbol_table)))
}

fn collect_native_value_symbols(
    value: &crate::value::Value,
    interner: &mut SymbolInterner,
) -> Result<()> {
    use crate::value::Value;

    match value {
        Value::String(value) => {
            interner.intern_string(value);
        }
        Value::Sequence(items) => {
            for item in items {
                collect_native_value_symbols(item, interner)?;
            }
        }
        Value::Map(entries) => {
            for entry in entries {
                if let Value::String(key) = &entry.key {
                    let _ = interner.intern_key(key)?;
                }
                collect_native_value_symbols(&entry.key, interner)?;
                collect_native_value_symbols(&entry.value, interner)?;
            }
        }
        Value::Tagged(tagged) => {
            let _ = interner.intern_tag(tagged.tag.as_str(), None)?;
            collect_native_value_symbols(tagged.value.as_ref(), interner)?;
        }
        Value::ExpressionSource(expression) => {
            if let crate::value::ExpressionSource::Text(source) = &expression.source {
                interner.intern_string(source);
            }
            if let Some(value) = &expression.result_value {
                collect_native_value_symbols(value, interner)?;
            }
        }
        Value::LuaChunkSource(chunk) => {
            if let Some(value) = &chunk.result_value {
                collect_native_value_symbols(value, interner)?;
            }
        }
        Value::RuntimeDescriptor(descriptor) => {
            if let Some(value) = &descriptor.descriptor_value {
                collect_native_value_symbols(value, interner)?;
            }
            if let Some(value) = &descriptor.fallback_value {
                collect_native_value_symbols(value, interner)?;
            }
        }
        Value::ExtensionValue(extension) => {
            if let Some(value) = &extension.fallback_value {
                collect_native_value_symbols(value, interner)?;
            }
        }
        Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::UInt(_)
        | Value::Float(_)
        | Value::Decimal(_)
        | Value::BytesInline(_)
        | Value::BytesBlob(_) => {}
    }

    Ok(())
}

fn normalize_value_roots_for_mode(
    values: &[crate::value::Value],
    blob_table: &mut BlobTable,
    mode: WriterMode,
) -> Result<Vec<crate::value::Value>> {
    values
        .iter()
        .map(|value| normalize_value_for_mode(value, blob_table, mode))
        .collect()
}

fn normalize_value_for_mode(
    value: &crate::value::Value,
    blob_table: &mut BlobTable,
    mode: WriterMode,
) -> Result<crate::value::Value> {
    use crate::value::{
        ExpressionValue, ExtensionValue as NativeExtensionValue, LuaChunkValue, MapEntry,
        RuntimeDescriptorValue, TaggedValue, Value,
    };

    match value {
        Value::BytesInline(bytes)
            if matches!(mode, WriterMode::Canonical(_) | WriterMode::RuntimeData)
                && bytes.len() > 64 =>
        {
            Ok(Value::BytesBlob(
                blob_table.push(BlobRecord::new(bytes.clone()))?,
            ))
        }
        Value::BytesBlob(blob_id) => {
            if blob_table.get(*blob_id).is_none() {
                return Err(LumbaError::InvalidValueReference(
                    crate::error::ErrorContext::new(
                        "blob reference pointed outside the blob table during encoding",
                    ),
                ));
            }
            Ok(Value::BytesBlob(*blob_id))
        }
        Value::Sequence(items) => Ok(Value::Sequence(
            items
                .iter()
                .map(|item| normalize_value_for_mode(item, blob_table, mode))
                .collect::<Result<Vec<_>>>()?,
        )),
        Value::Map(entries) => Ok(Value::Map(
            entries
                .iter()
                .map(|entry| {
                    Ok(MapEntry {
                        key: normalize_value_for_mode(&entry.key, blob_table, mode)?,
                        value: normalize_value_for_mode(&entry.value, blob_table, mode)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )),
        Value::Tagged(tagged) => Ok(Value::Tagged(TaggedValue {
            tag: tagged.tag.clone(),
            value: Box::new(normalize_value_for_mode(
                tagged.value.as_ref(),
                blob_table,
                mode,
            )?),
        })),
        Value::ExpressionSource(expression) => {
            if let crate::value::ExpressionSource::Blob(blob_id) = &expression.source {
                if blob_table.get(*blob_id).is_none() {
                    return Err(LumbaError::InvalidValueReference(
                        crate::error::ErrorContext::new(
                            "blob reference pointed outside the blob table during encoding",
                        ),
                    ));
                }
            }
            Ok(Value::ExpressionSource(ExpressionValue {
                language: expression.language.clone(),
                source: expression.source.clone(),
                capability_set_ref: expression.capability_set_ref,
                result_value: expression
                    .result_value
                    .as_ref()
                    .map(|value| {
                        normalize_value_for_mode(value.as_ref(), blob_table, mode).map(Box::new)
                    })
                    .transpose()?,
            }))
        }
        Value::LuaChunkSource(chunk) => {
            if blob_table.get(chunk.source_blob_ref).is_none() {
                return Err(LumbaError::InvalidValueReference(
                    crate::error::ErrorContext::new(
                        "blob reference pointed outside the blob table during encoding",
                    ),
                ));
            }
            Ok(Value::LuaChunkSource(LuaChunkValue {
                language: chunk.language.clone(),
                source_blob_ref: chunk.source_blob_ref,
                capability_set_ref: chunk.capability_set_ref,
                result_value: chunk
                    .result_value
                    .as_ref()
                    .map(|value| {
                        normalize_value_for_mode(value.as_ref(), blob_table, mode).map(Box::new)
                    })
                    .transpose()?,
            }))
        }
        Value::RuntimeDescriptor(descriptor) => {
            Ok(Value::RuntimeDescriptor(RuntimeDescriptorValue {
                kind: descriptor.kind.clone(),
                required: descriptor.required,
                trusted_only: descriptor.trusted_only,
                capability_set_ref: descriptor.capability_set_ref,
                descriptor_value: descriptor
                    .descriptor_value
                    .as_ref()
                    .map(|value| {
                        normalize_value_for_mode(value.as_ref(), blob_table, mode).map(Box::new)
                    })
                    .transpose()?,
                fallback_value: descriptor
                    .fallback_value
                    .as_ref()
                    .map(|value| {
                        normalize_value_for_mode(value.as_ref(), blob_table, mode).map(Box::new)
                    })
                    .transpose()?,
            }))
        }
        Value::ExtensionValue(extension) => {
            if blob_table.get(extension.payload_blob_ref).is_none() {
                return Err(LumbaError::InvalidValueReference(
                    crate::error::ErrorContext::new(
                        "blob reference pointed outside the blob table during encoding",
                    ),
                ));
            }
            Ok(Value::ExtensionValue(NativeExtensionValue {
                extension_name: extension.extension_name.clone(),
                type_name: extension.type_name.clone(),
                payload_blob_ref: extension.payload_blob_ref,
                fallback_value: extension
                    .fallback_value
                    .as_ref()
                    .map(|value| {
                        normalize_value_for_mode(value.as_ref(), blob_table, mode).map(Box::new)
                    })
                    .transpose()?,
            }))
        }
        _ => Ok(value.clone()),
    }
}

fn collect_value_strings(value: &LumaValue, interner: &mut StringInterner) {
    match value {
        LumaValue::String(value) => {
            interner.intern(value);
        }
        LumaValue::Sequence(sequence) => {
            for item in &sequence.items {
                collect_value_strings(item, interner);
            }
        }
        LumaValue::Mapping(mapping) => {
            for entry in &mapping.entries {
                if let LumaKey::String(key) = &entry.key {
                    interner.intern(key);
                }
                collect_value_strings(&entry.value, interner);
            }
        }
        LumaValue::Tagged(tagged) => {
            interner.intern(&tagged.tag.name.value);
            collect_value_strings(tagged.value.as_ref(), interner);
        }
        LumaValue::Null(_)
        | LumaValue::Boolean(_)
        | LumaValue::Number(_)
        | LumaValue::Function(_)
        | LumaValue::UserData(_)
        | LumaValue::HostObject(_) => {}
    }
}

fn canonicalize_symbol_table_for_mode(symbol_table: &SymbolTable, mode: WriterMode) -> SymbolTable {
    if !matches!(mode, WriterMode::Canonical(_)) {
        return symbol_table.clone();
    }

    let mut symbols = symbol_table.symbols.clone();
    symbols.sort_by_key(|record| {
        (
            record.string_id,
            record
                .namespace_string_id
                .map(|value| value + 1)
                .unwrap_or(0),
            record.flags,
        )
    });
    SymbolTable { symbols }
}

fn augment_string_table(
    string_table: &crate::string_table::StringTable,
    extension_table: &ExtensionTable,
    tag_table: &TagTable,
    schema_table: &SchemaTable,
    diagnostic_table: &DiagnosticTable,
    dependency_table: &DependencyTable,
    embedded_resource_table: &EmbeddedResourceTable,
    capability_table: &CapabilityTable,
    signature_table: &SignatureTable,
    source_file_table: &SourceFileTable,
    syntax_node_table: &SyntaxNodeTable,
    trivia_table: &TriviaTable,
) -> crate::string_table::StringTable {
    let mut strings = string_table.strings.clone();
    for declaration in &extension_table.declarations {
        append_string_if_missing(&mut strings, &declaration.name);
        append_string_if_missing(&mut strings, &declaration.version);
    }
    for declaration in &tag_table.declarations {
        append_string_if_missing(&mut strings, declaration.tag.as_str());
        append_string_if_missing(&mut strings, &declaration.uri);
    }
    for record in &schema_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut strings, uri);
        }
    }
    for record in &diagnostic_table.records {
        append_string_if_missing(&mut strings, record.code_symbol.as_str());
        append_string_if_missing(&mut strings, &record.message);
        for related in &record.related_spans {
            append_string_if_missing(&mut strings, &related.message);
        }
    }
    for record in &dependency_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut strings, uri);
        }
        if let Some(alias) = &record.alias {
            append_string_if_missing(&mut strings, alias.as_str());
        }
    }
    for record in &embedded_resource_table.records {
        if let Some(extension_kind) = &record.extension_kind {
            append_string_if_missing(&mut strings, extension_kind.as_str());
        }
    }
    for record in &capability_table.records {
        for requirement in &record.requirements {
            append_string_if_missing(&mut strings, requirement.capability.as_str());
        }
    }
    for record in &signature_table.records {
        if let Some(algorithm) = &record.algorithm {
            append_string_if_missing(&mut strings, algorithm.as_str());
        }
    }
    for record in &source_file_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut strings, uri);
        }
        if let Some(display) = &record.display {
            append_string_if_missing(&mut strings, display);
        }
    }
    append_syntax_table_strings(&mut strings, syntax_node_table);
    append_trivia_table_strings(&mut strings, trivia_table);
    crate::string_table::StringTable { strings }
}

fn build_string_table(
    extension_table: &ExtensionTable,
    tag_table: &TagTable,
    schema_table: &SchemaTable,
    diagnostic_table: &DiagnosticTable,
    dependency_table: &DependencyTable,
    embedded_resource_table: &EmbeddedResourceTable,
    capability_table: &CapabilityTable,
    signature_table: &SignatureTable,
    source_file_table: &SourceFileTable,
    syntax_node_table: &SyntaxNodeTable,
    trivia_table: &TriviaTable,
) -> crate::string_table::StringTable {
    let mut table = crate::string_table::StringTable::new();
    for declaration in &extension_table.declarations {
        append_string_if_missing(&mut table.strings, &declaration.name);
        append_string_if_missing(&mut table.strings, &declaration.version);
    }
    for declaration in &tag_table.declarations {
        append_string_if_missing(&mut table.strings, declaration.tag.as_str());
        append_string_if_missing(&mut table.strings, &declaration.uri);
    }
    for record in &schema_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut table.strings, uri);
        }
    }
    for record in &diagnostic_table.records {
        append_string_if_missing(&mut table.strings, record.code_symbol.as_str());
        append_string_if_missing(&mut table.strings, &record.message);
        for related in &record.related_spans {
            append_string_if_missing(&mut table.strings, &related.message);
        }
    }
    for record in &dependency_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut table.strings, uri);
        }
        if let Some(alias) = &record.alias {
            append_string_if_missing(&mut table.strings, alias.as_str());
        }
    }
    for record in &embedded_resource_table.records {
        if let Some(extension_kind) = &record.extension_kind {
            append_string_if_missing(&mut table.strings, extension_kind.as_str());
        }
    }
    for record in &capability_table.records {
        for requirement in &record.requirements {
            append_string_if_missing(&mut table.strings, requirement.capability.as_str());
        }
    }
    for record in &signature_table.records {
        if let Some(algorithm) = &record.algorithm {
            append_string_if_missing(&mut table.strings, algorithm.as_str());
        }
    }
    for record in &source_file_table.records {
        if let Some(uri) = &record.uri {
            append_string_if_missing(&mut table.strings, uri);
        }
        if let Some(display) = &record.display {
            append_string_if_missing(&mut table.strings, display);
        }
    }
    append_syntax_table_strings(&mut table.strings, syntax_node_table);
    append_trivia_table_strings(&mut table.strings, trivia_table);
    table
}

fn append_trivia_table_strings(
    strings: &mut Vec<crate::string_table::StringRecord>,
    trivia_table: &TriviaTable,
) {
    for record in &trivia_table.records {
        append_string_if_missing(strings, &record.text);
    }
}

fn build_symbol_table(
    strings: &crate::string_table::StringTable,
    tag_table: &TagTable,
    diagnostic_table: &DiagnosticTable,
    dependency_table: &DependencyTable,
    embedded_resource_table: &EmbeddedResourceTable,
    capability_table: &CapabilityTable,
    signature_table: &SignatureTable,
    syntax_node_table: &SyntaxNodeTable,
) -> Result<SymbolTable> {
    augment_symbol_table(
        strings,
        &SymbolTable::new(),
        tag_table,
        diagnostic_table,
        dependency_table,
        embedded_resource_table,
        capability_table,
        signature_table,
        syntax_node_table,
    )
}

fn augment_symbol_table(
    strings: &crate::string_table::StringTable,
    symbol_table: &SymbolTable,
    tag_table: &TagTable,
    diagnostic_table: &DiagnosticTable,
    dependency_table: &DependencyTable,
    embedded_resource_table: &EmbeddedResourceTable,
    capability_table: &CapabilityTable,
    signature_table: &SignatureTable,
    syntax_node_table: &SyntaxNodeTable,
) -> Result<SymbolTable> {
    let mut table = symbol_table.clone();
    for declaration in &tag_table.declarations {
        let string_id = strings
            .strings
            .iter()
            .position(|record| record.value == declaration.tag.as_str())
            .ok_or_else(|| {
                LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                    "tag symbol string was not present in STRS",
                ))
            })? as u64;
        if !table.symbols.iter().any(|record| {
            record.string_id == string_id && record.flags & crate::symbol::SYMBOL_FLAG_TAG != 0
        }) {
            table.symbols.push(
                crate::symbol::SymbolRecord::new(string_id)
                    .with_flags(crate::symbol::SYMBOL_FLAG_TAG),
            );
        }
    }
    for record in &diagnostic_table.records {
        let string_id = strings
            .strings
            .iter()
            .position(|candidate| candidate.value == record.code_symbol.as_str())
            .ok_or_else(|| {
                LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                    "diagnostic code symbol string was not present in STRS",
                ))
            })? as u64;
        if !table.symbols.iter().any(|candidate| {
            candidate.string_id == string_id
                && candidate.namespace_string_id.is_none()
                && candidate.flags == 0
        }) {
            table
                .symbols
                .push(crate::symbol::SymbolRecord::new(string_id));
        }
    }
    for record in &dependency_table.records {
        if let Some(alias) = &record.alias {
            ensure_symbol(strings, &mut table, alias.as_str(), None)?;
        }
    }
    for record in &embedded_resource_table.records {
        if let Some(extension_kind) = &record.extension_kind {
            ensure_symbol(strings, &mut table, extension_kind.as_str(), None)?;
        }
    }
    for record in &capability_table.records {
        for requirement in &record.requirements {
            ensure_symbol(strings, &mut table, requirement.capability.as_str(), None)?;
        }
    }
    for record in &signature_table.records {
        if let Some(algorithm) = &record.algorithm {
            ensure_symbol(strings, &mut table, algorithm.as_str(), None)?;
        }
    }
    for record in &syntax_node_table.records {
        ensure_symbol(
            strings,
            &mut table,
            &record.kind,
            Some(crate::symbol::SYMBOL_FLAG_NODE_KIND),
        )?;
        for field in &record.fields {
            ensure_symbol(strings, &mut table, &field.name, None)?;
            if let crate::syntax::SyntaxFieldValue::Symbol(value) = &field.value {
                ensure_symbol(strings, &mut table, value, None)?;
            }
        }
    }
    Ok(table)
}

fn append_syntax_table_strings(
    strings: &mut Vec<crate::string_table::StringRecord>,
    syntax_node_table: &SyntaxNodeTable,
) {
    for record in &syntax_node_table.records {
        append_string_if_missing(strings, &record.kind);
        for field in &record.fields {
            append_string_if_missing(strings, &field.name);
            match &field.value {
                crate::syntax::SyntaxFieldValue::String(value)
                | crate::syntax::SyntaxFieldValue::TokenText(value)
                | crate::syntax::SyntaxFieldValue::Symbol(value) => {
                    append_string_if_missing(strings, value);
                }
                _ => {}
            }
        }
    }
}

fn collect_syntax_table_symbols(
    syntax_node_table: &SyntaxNodeTable,
    interner: &mut SymbolInterner,
) -> Result<()> {
    for record in &syntax_node_table.records {
        let _ = interner.intern_node_kind(&record.kind, None)?;
        for field in &record.fields {
            let _ = interner.intern_symbol(&field.name, None, 0)?;
            if let crate::syntax::SyntaxFieldValue::Symbol(value) = &field.value {
                let _ = interner.intern_symbol(value, None, 0)?;
            }
            if let crate::syntax::SyntaxFieldValue::String(value)
            | crate::syntax::SyntaxFieldValue::TokenText(value) = &field.value
            {
                interner.intern_string(value);
            }
        }
    }
    Ok(())
}

fn ensure_symbol(
    strings: &crate::string_table::StringTable,
    table: &mut SymbolTable,
    value: &str,
    flags: Option<u64>,
) -> Result<()> {
    let string_id = strings
        .strings
        .iter()
        .position(|record| record.value == value)
        .ok_or_else(|| {
            LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                "ASTN symbol string was not present in STRS",
            ))
        })? as u64;
    let flags = flags.unwrap_or(0);
    if !table.symbols.iter().any(|record| {
        record.string_id == string_id
            && record.namespace_string_id.is_none()
            && record.flags & flags == flags
    }) {
        table
            .symbols
            .push(crate::symbol::SymbolRecord::new(string_id).with_flags(flags));
    }
    Ok(())
}

fn append_string_if_missing(strings: &mut Vec<crate::string_table::StringRecord>, value: &str) {
    if strings.iter().any(|record| record.value == value) {
        return;
    }
    strings.push(crate::string_table::StringRecord::new(value));
}

fn encode_section_table_len(section_count: usize) -> Result<usize> {
    crate::section::checked_table_allocation_len(
        section_count,
        crate::container::SECTION_ENTRY_SIZE as usize,
    )
}

/// Encodes section table entries into their contiguous 64-byte representation.
pub(crate) fn encode_section_table(entries: &[SectionEntry]) -> Result<Vec<u8>> {
    let capacity = crate::section::checked_table_allocation_len(
        entries.len(),
        crate::container::SECTION_ENTRY_SIZE as usize,
    )?;
    let mut bytes = Vec::with_capacity(capacity);
    for entry in entries {
        bytes.extend_from_slice(&entry.encode()?);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{WriteOptions, Writer, WriterMode};
    use crate::container::{ContainerHeader, HeaderCrcMode};
    use crate::document::Document;
    use crate::meta::Metadata;
    use crate::section::{
        CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry,
        SectionId,
    };
    use crate::string_table::StringTable;
    use crate::value::Value;
    use luma_syntax::{LumaKey, LumaMapping, LumaMappingEntry, LumaSequence, LumaValue};

    #[test]
    fn writer_emits_valid_minimal_header_only_file() {
        let bytes = Writer::new(WriteOptions::new())
            .write(&crate::container::LumbaFile::new())
            .expect("writer should encode header");

        let header = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect("writer output should decode");

        assert_eq!(header.file_length, 64);
        assert_eq!(bytes.len(), 64);
    }

    #[test]
    fn section_table_encoder_emits_concatenated_entries() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 8,
            logical_size: 8,
            item_count: 1,
            checksum_low: 0,
            checksum_high: 0,
        };

        let bytes = super::encode_section_table(&[entry]).expect("section table should encode");

        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..4], b"STRS");
    }

    #[test]
    fn writer_emits_empty_strs_section_when_requested() {
        let file = crate::container::LumbaFile::new().with_string_table(StringTable::new());

        let bytes = Writer::new(WriteOptions::new())
            .write(&file)
            .expect("writer should encode empty STRS section");

        let header = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect("writer output should decode");

        assert_eq!(header.section_count, 1);
        assert_eq!(bytes.len(), 136);
        assert_eq!(&bytes[64..68], b"STRS");
    }

    #[test]
    fn writer_emits_docs_and_root_document_count_for_single_document() {
        let file = crate::container::LumbaFile::new()
            .with_document(Document::new().with_root_value(Value::Int(7)));

        let bytes = Writer::new(WriteOptions::new())
            .write(&file)
            .expect("writer should encode DOCS");

        let header = ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled)
            .expect("writer output should decode");

        assert_eq!(header.root_document_count, 1);
        assert_eq!(header.section_count, 2);
        assert_eq!(&bytes[64..68], b"VALS");
        assert_eq!(&bytes[128..132], b"DOCS");
    }

    #[test]
    fn writer_round_trips_multiple_documents_with_absent_optional_root() {
        let file = crate::container::LumbaFile::new()
            .with_document(Document::new().with_root_value(Value::Int(1)))
            .with_document(Document::new())
            .with_document(Document::new().with_root_value(Value::Bool(true)));

        let bytes = Writer::new(WriteOptions::new())
            .write(&file)
            .expect("writer should encode DOCS");
        let decoded = crate::read::Reader::new(crate::read::ReadOptions::new())
            .read(&bytes)
            .expect("reader should decode DOCS");

        assert_eq!(decoded.documents, file.documents);
        let value_section = decoded
            .sections
            .iter()
            .find(|section| section.name.as_str() == "VALS")
            .expect("VALS should be present");
        assert_eq!(value_section.values, vec![Value::Int(1), Value::Bool(true)]);
    }

    #[test]
    fn level1_minimal_writer_emits_zero_header_crc_and_only_strs_vals_docs() {
        let bytes = super::write_level1_minimal_value_image(&[LumaValue::Mapping(LumaMapping {
            entries: vec![LumaMappingEntry {
                key: LumaKey::String(String::from("name")),
                value: LumaValue::Sequence(LumaSequence {
                    items: vec![LumaValue::String(String::from("Ada"))],
                    span: None,
                }),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        })])
        .expect("level1 minimal bytes should encode");

        let header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");

        assert_eq!(header.header_crc32c, 0);
        assert_eq!(header.section_count, 3);
        assert_eq!(&bytes[64..68], b"STRS");
        assert_eq!(&bytes[128..132], b"VALS");
        assert_eq!(&bytes[192..196], b"DOCS");
    }

    #[test]
    fn runtime_data_writer_emits_meta_and_empty_runtime_sections() {
        let bytes = Writer::new(
            WriteOptions::new()
                .with_mode(WriterMode::RuntimeData)
                .with_header_crc_mode(HeaderCrcMode::Disabled),
        )
        .write(&crate::container::LumbaFile::new())
        .expect("runtime data should encode");

        let header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");

        assert_eq!(header.header_crc32c, 0);
        assert_eq!(header.section_count, 5);
        assert_eq!(&bytes[64..68], b"META");
        assert_eq!(&bytes[128..132], b"STRS");
        assert_eq!(&bytes[192..196], b"SYMS");
        assert_eq!(&bytes[256..260], b"VALS");
        assert_eq!(&bytes[320..324], b"DOCS");
    }

    #[test]
    fn runtime_data_writer_uses_existing_metadata_when_present() {
        let file = crate::container::LumbaFile::new().with_metadata(
            Metadata::new().with_entry("format", Value::String(String::from("custom"))),
        );

        let decoded = crate::read::Reader::new(crate::read::ReadOptions::new())
            .read(
                &Writer::new(WriteOptions::new().with_mode(WriterMode::RuntimeData))
                    .write(&file)
                    .expect("runtime data should encode"),
            )
            .expect("runtime data should decode");

        assert_eq!(
            decoded.metadata.expect("META should decode").get("format"),
            Some(&Value::String(String::from("custom")))
        );
    }

    #[test]
    fn build_bundle_writer_emits_meta_and_empty_bundle_sections() {
        let bytes = Writer::new(
            WriteOptions::new()
                .with_mode(WriterMode::BuildBundle)
                .with_header_crc_mode(HeaderCrcMode::Disabled),
        )
        .write(&crate::container::LumbaFile::new())
        .expect("build bundle should encode");

        let header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Enabled).expect("header should decode");

        assert_eq!(header.header_crc32c, 0);
        assert_eq!(
            header.container_flags,
            crate::mode::CONTAINER_FLAG_HAS_VALUES
        );
        assert_eq!(header.profile_flags, crate::mode::PROFILE_FLAG_VALUE_IMAGE);
        assert_eq!(header.section_count, 6);
        assert_eq!(&bytes[64..68], b"META");
        assert_eq!(&bytes[128..132], b"STRS");
        assert_eq!(&bytes[192..196], b"SYMS");
        assert_eq!(&bytes[256..260], b"BLOB");
        assert_eq!(&bytes[320..324], b"VALS");
        assert_eq!(&bytes[384..388], b"DOCS");
    }
}
