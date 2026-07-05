//! Reader configuration and entry points.

use crate::blob::decode_blob_table;
use crate::bundle::{decode_dependency_table, decode_embedded_resource_table};
use crate::capability::decode_capability_table;
use crate::codec::{CodecReadStrategy, read_section_codec_strategy};
use crate::container::{
    ContainerHeader, DocumentImage, HeaderCrcMode, LumbaFile, discover_footer,
    validate_section_table_with_reserved_flag_policy,
};
use crate::diagnostic::{DiagnosticLoadPolicy, decode_diagnostic_table};
use crate::document::{decode_document_table, materialize_value_only_documents};
use crate::error::{LumbaError, Result};
use crate::extension::{decode_extension_table, is_supported_extension};
use crate::meta::{decode_metadata, metadata_item_count};
use crate::policy::Limits;
use crate::schema::decode_schema_table;
use crate::section::Section;
use crate::section::{
    SECTION_FLAG_REQUIRED, SECTION_FLAG_TRUSTED_ONLY, SECTION_FLAG_UNIQUE, SectionId,
    ValidatedSection,
};
use crate::signature::decode_signature_table;
use crate::source::{decode_source_file_table, decode_source_span_table};
use crate::string_table::decode_string_table;
use crate::symbol::decode_symbol_table;
use crate::syntax::decode_syntax_node_table;
use crate::tag::decode_tag_table;
use crate::trivia::decode_trivia_table;
use crate::value::{VALUE_SECTION_NAME, Value, ValueDecodeMode, decode_value_table};
use crate::verify::verify_level1_minimal_value_image_file;
use luma_syntax::LumaValue;

/// Reader configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadOptions {
    /// Resource limits to enforce while reading.
    pub limits: Limits,
    /// Whether the reader should retain source bytes.
    pub retain_document_image: bool,
    /// Whether an optional non-zero header CRC should be validated.
    pub header_crc_mode: HeaderCrcMode,
    /// Policy for stored diagnostics carried by the `DIAG` section.
    pub diagnostic_policy: DiagnosticLoadPolicy,
}

impl ReadOptions {
    /// Creates default read options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets limits.
    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Configures whether source bytes should be preserved.
    #[must_use]
    pub fn with_document_image(mut self, retain_document_image: bool) -> Self {
        self.retain_document_image = retain_document_image;
        self
    }

    /// Sets optional header CRC validation policy.
    #[must_use]
    pub fn with_header_crc_mode(mut self, header_crc_mode: HeaderCrcMode) -> Self {
        self.header_crc_mode = header_crc_mode;
        self
    }

    /// Sets stored diagnostic acceptance policy.
    #[must_use]
    pub fn with_diagnostic_policy(mut self, diagnostic_policy: DiagnosticLoadPolicy) -> Self {
        self.diagnostic_policy = diagnostic_policy;
        self
    }
}

/// Reader entry point for decoding LUMBA documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reader {
    options: ReadOptions,
}

impl Reader {
    /// Creates a reader with explicit options.
    #[must_use]
    pub fn new(options: ReadOptions) -> Self {
        Self { options }
    }

    /// Returns the configured options.
    #[must_use]
    pub fn options(&self) -> &ReadOptions {
        &self.options
    }

    /// Reads a file model from bytes.
    pub fn read(&self, bytes: &[u8]) -> Result<LumbaFile> {
        self.enforce_input_limit(bytes.len())?;

        let header = ContainerHeader::decode_with_reserved_flag_policy(
            bytes,
            self.options.header_crc_mode,
            self.options.limits.reserved_flag_policy,
        )?;
        if let Some(footer) = discover_footer(bytes)? {
            footer.validate_against_header(&header)?;
        }
        self.enforce_header_limits(&header)?;
        let sections = validate_section_table_with_reserved_flag_policy(
            &header,
            bytes,
            self.options.limits.reserved_flag_policy,
        )?;
        self.enforce_section_limits(&header, &sections)?;

        let mut file = LumbaFile::new();
        let mut exts_section = None;
        let mut docs_section = None;
        let mut tags_section = None;
        let mut scma_section = None;
        let mut diag_section = None;
        let mut deps_section = None;
        let mut embd_section = None;
        let mut srcf_section = None;
        let mut srcs_section = None;
        let mut astn_section = None;
        let mut triv_section = None;
        let mut caps_section = None;
        let mut sign_section = None;
        for (section_index, section) in sections.iter().enumerate() {
            match read_section_codec_strategy(section.entry)? {
                CodecReadStrategy::ReadStoredPayload => {}
                CodecReadStrategy::SkipOptionalSection => continue,
            }

            match section.entry.section_id {
                SectionId::META => {
                    file.metadata = Some(self.decode_meta(section, section_index)?);
                }
                SectionId::EXTS => exts_section = Some((section_index, section)),
                SectionId::STRS => {
                    file.string_table = Some(self.decode_strs(section, section_index)?);
                }
                SectionId::SYMS => {
                    let string_count = file
                        .string_table
                        .as_ref()
                        .map(|table| table.strings.len())
                        .unwrap_or(0);
                    file.symbol_table =
                        Some(self.decode_syms(section, section_index, string_count)?);
                }
                SectionId::BLOB => {
                    file.blob_table = Some(self.decode_blob(section, section_index)?);
                }
                SectionId::VALS => {
                    file.sections.push(
                        self.decode_vals(
                            section,
                            section_index,
                            file.blob_table
                                .as_ref()
                                .map_or(0, |table| table.records.len()),
                        )?,
                    );
                }
                SectionId::DOCS => docs_section = Some((section_index, section)),
                SectionId::TAGS => tags_section = Some((section_index, section)),
                SectionId::SCMA => scma_section = Some((section_index, section)),
                SectionId::DIAG => diag_section = Some((section_index, section)),
                SectionId::DEPS => deps_section = Some((section_index, section)),
                SectionId::EMBD => embd_section = Some((section_index, section)),
                SectionId::CAPS => caps_section = Some((section_index, section)),
                SectionId::SRCF => srcf_section = Some((section_index, section)),
                SectionId::SRCS => srcs_section = Some((section_index, section)),
                SectionId::ASTN => astn_section = Some((section_index, section)),
                SectionId::TRIV => triv_section = Some((section_index, section)),
                SectionId::SIGN => sign_section = Some((section_index, section)),
                _ => {}
            }
        }

        if let Some((section_index, section)) = exts_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::MalformedExtensionPayload(crate::error::ErrorContext::new(
                    "EXTS requires STRS so extension names and versions can be resolved",
                ))
            })?;
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|vals| vals.values.as_slice());
            file.extension_table =
                Some(self.decode_exts(section, section_index, string_table, value_roots)?);
            self.enforce_extension_policy(file.extension_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = scma_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "SCMA requires STRS so schema URIs can be resolved",
                ))
            })?;
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|vals| vals.values.as_slice());
            file.schema_table = Some(
                self.decode_scma(
                    section,
                    section_index,
                    string_table,
                    value_roots,
                    file.blob_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                )?,
            );
            self.enforce_schema_policy(file.schema_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = tags_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TAGS requires STRS so tag URIs can be resolved",
                ))
            })?;
            let symbol_table = file.symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TAGS requires SYMS so tag symbols can be resolved",
                ))
            })?;
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|vals| vals.values.as_slice());
            let schema_count = file
                .schema_table
                .as_ref()
                .map_or(0, |table| table.records.len());
            file.tag_table = Some(self.decode_tags(
                section,
                section_index,
                string_table,
                symbol_table,
                value_roots,
                schema_count,
            )?);
            self.enforce_tag_policy(file.tag_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = srcf_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "SRCF requires STRS so source URIs and display strings can be resolved",
                ))
            })?;
            file.source_file_table = Some(
                self.decode_srcf(
                    section,
                    section_index,
                    string_table,
                    file.blob_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                )?,
            );
            self.enforce_source_policy(file.source_file_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = srcs_section {
            let source_file_table = file.source_file_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidValueReference(crate::error::ErrorContext::new(
                    "SRCS requires SRCF so source file references can be resolved",
                ))
            })?;
            file.source_span_table = Some(self.decode_srcs(
                section,
                section_index,
                source_file_table,
                file.blob_table.as_ref(),
            )?);
        }

        if let Some((section_index, section)) = deps_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DEPS requires STRS so dependency URIs can be resolved",
                ))
            })?;
            let symbol_table = file.symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DEPS requires SYMS so dependency aliases can be resolved",
                ))
            })?;
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|vals| vals.values.as_slice());
            file.dependency_table = Some(
                self.decode_deps(
                    section,
                    section_index,
                    string_table,
                    symbol_table,
                    value_roots,
                    file.source_span_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                    file.blob_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                )?,
            );
            self.enforce_dependency_policy(file.dependency_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = embd_section {
            let dependency_count = file.dependency_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "EMBD requires DEPS so dependency references can be resolved",
                ))
            })?;
            let blob_count = file.blob_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "EMBD requires BLOB so resource payloads can be resolved",
                ))
            })?;
            file.embedded_resource_table = Some(self.decode_embd(
                section,
                section_index,
                dependency_count.records.len(),
                blob_count.records.len(),
                file.string_table.as_ref(),
                file.symbol_table.as_ref(),
            )?);
        }

        if let Some((section_index, section)) = triv_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "TRIV requires STRS so preserved trivia text can be resolved",
                ))
            })?;
            file.trivia_table = Some(self.decode_triv(
                section,
                section_index,
                string_table,
                file.source_span_table.as_ref(),
            )?);
        }

        if let Some((section_index, section)) = astn_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "ASTN requires STRS so string and token-text fields can be resolved",
                ))
            })?;
            let symbol_table = file.symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "ASTN requires SYMS so node kinds and field names can be resolved",
                ))
            })?;
            let value_count = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map_or(0, |section| section.values.len());
            file.syntax_node_table = Some(
                self.decode_astn(
                    section,
                    section_index,
                    string_table,
                    symbol_table,
                    value_count,
                    file.source_span_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                    file.blob_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                    file.trivia_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                )?,
            );
        }

        if let Some((section_index, section)) = diag_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DIAG requires STRS so diagnostic messages can be resolved",
                ))
            })?;
            let symbol_table = file.symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "DIAG requires SYMS so diagnostic codes can be resolved",
                ))
            })?;
            file.diagnostic_table = Some(
                self.decode_diag(
                    section,
                    section_index,
                    string_table,
                    symbol_table,
                    file.source_span_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                )?,
            );
            self.enforce_diagnostic_policy(file.diagnostic_table.as_ref().expect("set above"))?;
        }

        if let Some((section_index, section)) = sign_section {
            file.signature_table = Some(
                self.decode_sign(
                    section,
                    section_index,
                    file.string_table.as_ref(),
                    file.symbol_table.as_ref(),
                    file.sections
                        .iter()
                        .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                        .map(|vals| vals.values.as_slice()),
                    file.blob_table
                        .as_ref()
                        .map_or(0, |table| table.records.len()),
                    sections.len(),
                )?,
            );
        }

        if let Some((section_index, section)) = caps_section {
            let string_table = file.string_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "CAPS requires STRS so capability names can be resolved",
                ))
            })?;
            let symbol_table = file.symbol_table.as_ref().ok_or_else(|| {
                LumbaError::InvalidSectionTable(crate::error::ErrorContext::new(
                    "CAPS requires SYMS so capability symbols can be resolved",
                ))
            })?;
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|vals| vals.values.as_slice());
            file.capability_table = Some(self.decode_caps(
                section,
                section_index,
                string_table,
                symbol_table,
                value_roots,
            )?);
            self.enforce_capability_policy(file.capability_table.as_ref().expect("set above"))?;
        }

        file.documents = if let Some((section_index, section)) = docs_section {
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|section| section.values.as_slice());
            let schema_count = file
                .schema_table
                .as_ref()
                .map_or(0, |table| table.records.len());
            let capability_count = file
                .capability_table
                .as_ref()
                .map_or(0, |table| table.records.len());
            let documents = self.decode_docs(
                section,
                section_index,
                value_roots,
                schema_count,
                capability_count,
            )?;
            if section.entry.item_count != documents.len() as u64 {
                return Err(LumbaError::InvalidDocumentTable(
                    crate::error::ErrorContext::new(format!(
                        "DOCS item_count {} did not match decoded document count {}",
                        section.entry.item_count,
                        documents.len()
                    )),
                ));
            }
            if header.root_document_count != documents.len() as u64 {
                return Err(LumbaError::InvalidDocumentTable(
                    crate::error::ErrorContext::new(format!(
                        "header root_document_count {} did not match DOCS document count {}",
                        header.root_document_count,
                        documents.len()
                    )),
                ));
            }
            documents
        } else if header.root_document_count > 0 {
            let value_roots = file
                .sections
                .iter()
                .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
                .map(|section| section.values.as_slice())
                .ok_or_else(|| {
                    LumbaError::InvalidDocumentTable(crate::error::ErrorContext::new(
                        "header declared root documents but no DOCS or VALS roots were available",
                    ))
                })?;
            materialize_value_only_documents(value_roots, header.root_document_count)?
        } else {
            Vec::new()
        };

        if let Some(value_section) = file
            .sections
            .iter()
            .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
        {
            let capability_count = file
                .capability_table
                .as_ref()
                .map_or(0, |table| table.records.len());
            for value in &value_section.values {
                value.validate_capability_refs(capability_count)?;
            }
        }
        for document in &file.documents {
            document.validate_capability_refs(
                file.capability_table
                    .as_ref()
                    .map_or(0, |table| table.records.len()),
            )?;
            if let Some(root_value) = &document.root_value {
                self.enforce_value_policy(
                    root_value,
                    file.capability_table.as_ref(),
                    file.extension_table.as_ref(),
                )?;
            }
        }
        if let Some(value_section) = file
            .sections
            .iter()
            .find(|candidate| candidate.name.as_str() == VALUE_SECTION_NAME)
        {
            for value in &value_section.values {
                self.enforce_value_policy(
                    value,
                    file.capability_table.as_ref(),
                    file.extension_table.as_ref(),
                )?;
            }
        }

        Ok(file)
    }

    fn enforce_value_policy(
        &self,
        value: &Value,
        capability_table: Option<&crate::capability::CapabilityTable>,
        extension_table: Option<&crate::extension::ExtensionTable>,
    ) -> Result<()> {
        match value {
            Value::Sequence(items) => {
                for item in items {
                    self.enforce_value_policy(item, capability_table, extension_table)?;
                }
            }
            Value::Map(entries) => {
                for entry in entries {
                    self.enforce_value_policy(&entry.key, capability_table, extension_table)?;
                    self.enforce_value_policy(&entry.value, capability_table, extension_table)?;
                }
            }
            Value::Tagged(tagged) => {
                self.enforce_value_policy(
                    tagged.value.as_ref(),
                    capability_table,
                    extension_table,
                )?;
            }
            Value::ExpressionSource(expression) => {
                if let Some(value) = &expression.result_value {
                    self.enforce_value_policy(value, capability_table, extension_table)?;
                }
            }
            Value::LuaChunkSource(chunk) => {
                if let Some(value) = &chunk.result_value {
                    self.enforce_value_policy(value, capability_table, extension_table)?;
                }
            }
            Value::RuntimeDescriptor(descriptor) => {
                if descriptor.trusted_only && !self.options.limits.allows_trusted_only() {
                    return Err(LumbaError::trusted_only_rejected(format!(
                        "trusted-only runtime descriptor {} is not allowed by the active reader policy",
                        descriptor.kind.as_str()
                    )));
                }
                if descriptor.required && !self.options.limits.allows_trusted_only() {
                    return Err(LumbaError::UnsafeEvaluationRequest(
                        crate::error::ErrorContext::new(format!(
                            "runtime descriptor {} requires host resolution",
                            descriptor.kind.as_str()
                        )),
                    ));
                }
                if let Some(capability_set_ref) = descriptor.capability_set_ref {
                    if let Some(record) =
                        capability_table.and_then(|table| table.get(capability_set_ref))
                    {
                        for requirement in &record.requirements {
                            if requirement.is_trusted_only()
                                && !self.options.limits.allows_trusted_only()
                            {
                                return Err(LumbaError::trusted_only_rejected(format!(
                                    "trusted-only capability {} is not allowed by the active reader policy",
                                    requirement.capability.as_str()
                                )));
                            }
                            if !self.options.limits.allows_trusted_only()
                                && (requirement.is_required_for_evaluation()
                                    || requirement.may_read_external()
                                    || requirement.may_write_external())
                            {
                                return Err(LumbaError::UnsafeEvaluationRequest(
                                    crate::error::ErrorContext::new(format!(
                                        "capability {} requested evaluation or external effects",
                                        requirement.capability.as_str()
                                    )),
                                ));
                            }
                        }
                    }
                }
                if let Some(value) = &descriptor.descriptor_value {
                    self.enforce_value_policy(value, capability_table, extension_table)?;
                }
                if let Some(value) = &descriptor.fallback_value {
                    self.enforce_value_policy(value, capability_table, extension_table)?;
                }
            }
            Value::ExtensionValue(extension) => {
                if let Some(declaration) =
                    extension_table.and_then(|table| table.declaration(&extension.extension_name))
                {
                    if declaration.is_trusted_only() && !self.options.limits.allows_trusted_only() {
                        return Err(LumbaError::trusted_only_rejected(format!(
                            "trusted-only extension value {} is not allowed by the active reader policy",
                            extension.extension_name
                        )));
                    }
                }
                if let Some(value) = &extension.fallback_value {
                    self.enforce_value_policy(value, capability_table, extension_table)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn enforce_extension_policy(
        &self,
        extension_table: &crate::extension::ExtensionTable,
    ) -> Result<()> {
        for declaration in &extension_table.declarations {
            if declaration.is_trusted_only() && !self.options.limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "trusted-only extension {} is not allowed by the active reader policy",
                    declaration.name
                )));
            }
            if declaration.is_required()
                && !is_supported_extension(&declaration.name, &declaration.version)
            {
                return Err(LumbaError::UnsupportedRequiredExtension(
                    crate::error::ErrorContext::new(format!(
                        "required extension {}@{} is not supported",
                        declaration.name, declaration.version
                    )),
                ));
            }
        }

        Ok(())
    }

    fn enforce_tag_policy(&self, tag_table: &crate::tag::TagTable) -> Result<()> {
        for declaration in &tag_table.declarations {
            if declaration.is_trusted_only() && !self.options.limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "trusted-only tag {} is not allowed by the active reader policy",
                    declaration.tag.as_str()
                )));
            }
        }

        Ok(())
    }

    fn enforce_schema_policy(&self, schema_table: &crate::schema::SchemaTable) -> Result<()> {
        for record in &schema_table.records {
            if record.requires_trusted_validator() && !self.options.limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(
                    "schema requiring trusted validator is not allowed by the active reader policy",
                ));
            }
        }

        Ok(())
    }

    fn enforce_diagnostic_policy(
        &self,
        diagnostic_table: &crate::diagnostic::DiagnosticTable,
    ) -> Result<()> {
        for record in &diagnostic_table.records {
            if !self.options.diagnostic_policy.allows(record.severity) {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "stored diagnostic {} ({}) is not allowed by the active reader diagnostic policy",
                    record.code_symbol.as_str(),
                    record.message
                )));
            }
        }

        Ok(())
    }

    fn enforce_dependency_policy(
        &self,
        dependency_table: &crate::bundle::DependencyTable,
    ) -> Result<()> {
        for record in &dependency_table.records {
            if record.is_trusted_only() && !self.options.limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "trusted-only dependency {} is not allowed by the active reader policy",
                    record.uri.as_deref().unwrap_or("<unnamed dependency>")
                )));
            }
        }

        Ok(())
    }

    fn enforce_capability_policy(
        &self,
        capability_table: &crate::capability::CapabilityTable,
    ) -> Result<()> {
        for record in &capability_table.records {
            for requirement in &record.requirements {
                if requirement.is_trusted_only() && !self.options.limits.allows_trusted_only() {
                    return Err(LumbaError::trusted_only_rejected(format!(
                        "trusted-only capability {} is not allowed by the active reader policy",
                        requirement.capability.as_str()
                    )));
                }
                if !self.options.limits.allows_trusted_only()
                    && (requirement.is_required_for_evaluation()
                        || requirement.may_read_external()
                        || requirement.may_write_external())
                {
                    return Err(LumbaError::UnsafeEvaluationRequest(
                        crate::error::ErrorContext::new(format!(
                            "capability {} requested evaluation or external effects",
                            requirement.capability.as_str()
                        )),
                    ));
                }
            }
        }

        Ok(())
    }

    fn enforce_source_policy(
        &self,
        source_file_table: &crate::source::SourceFileTable,
    ) -> Result<()> {
        for record in &source_file_table.records {
            if record.is_private() && !self.options.limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "private source {} is not allowed by the active reader policy",
                    record
                        .display
                        .as_deref()
                        .or(record.uri.as_deref())
                        .unwrap_or("<unnamed source>")
                )));
            }
        }

        Ok(())
    }

    /// Captures a document image from bytes.
    pub fn read_image(&self, bytes: &[u8]) -> Result<DocumentImage> {
        self.enforce_input_limit(bytes.len())?;

        let header = ContainerHeader::decode_with_reserved_flag_policy(
            bytes,
            self.options.header_crc_mode,
            self.options.limits.reserved_flag_policy,
        )?;
        if let Some(footer) = discover_footer(bytes)? {
            footer.validate_against_header(&header)?;
        }
        self.enforce_header_limits(&header)?;
        let sections = validate_section_table_with_reserved_flag_policy(
            &header,
            bytes,
            self.options.limits.reserved_flag_policy,
        )?;
        self.enforce_section_limits(&header, &sections)?;

        Ok(DocumentImage::new(bytes))
    }

    fn enforce_input_limit(&self, byte_len: usize) -> Result<()> {
        if byte_len > self.options.limits.max_document_bytes {
            return Err(LumbaError::limit_exceeded(
                "document size exceeds configured maximum input bytes",
            ));
        }

        Ok(())
    }

    fn enforce_header_limits(&self, header: &ContainerHeader) -> Result<()> {
        let limits = &self.options.limits;
        enforce_limit_u64(
            u64::from(header.section_count),
            limits.max_sections,
            "section count exceeds configured maximum",
        )?;
        enforce_limit_u64(
            header.root_document_count,
            limits.max_document_count,
            "document count exceeds configured maximum",
        )?;
        if limits.max_nesting_depth == 0 && header.root_document_count > 0 {
            return Err(LumbaError::limit_exceeded(
                "nesting depth exceeds configured maximum",
            ));
        }

        Ok(())
    }

    fn enforce_section_limits(
        &self,
        header: &ContainerHeader,
        sections: &[ValidatedSection<'_>],
    ) -> Result<()> {
        let limits = &self.options.limits;
        let mut total_strings = 0_u64;
        let mut total_values = 0_u64;
        let mut total_documents = header.root_document_count;
        let mut total_syntax_nodes = 0_u64;
        let mut total_resources = 0_u64;

        for section in sections {
            let entry = section.entry;

            if entry.entry_flags & SECTION_FLAG_TRUSTED_ONLY != 0 && !limits.allows_trusted_only() {
                return Err(LumbaError::trusted_only_rejected(format!(
                    "trusted-only section {} is not allowed by the active reader policy",
                    entry.section_id.as_str()
                )));
            }

            enforce_limit_u64(
                entry.stored_size,
                limits.max_section_payload_bytes,
                "section payload size exceeds configured maximum",
            )?;
            enforce_limit_u64(
                entry.logical_size,
                limits.max_decoded_logical_bytes,
                "decoded logical payload size exceeds configured maximum",
            )?;

            if is_table_like_section(entry.section_id) {
                enforce_limit_u64(
                    entry.item_count,
                    limits.max_table_record_count,
                    "table record count exceeds configured maximum",
                )?;
            }

            match entry.section_id {
                SectionId::STRS => {
                    total_strings =
                        total_strings.checked_add(entry.item_count).ok_or_else(|| {
                            LumbaError::limit_exceeded("string count exceeds configured maximum")
                        })?;
                    enforce_limit_u64(
                        total_strings,
                        limits.max_string_count,
                        "string count exceeds configured maximum",
                    )?;
                    let per_string_limit = entry.item_count.max(1);
                    if entry.logical_size / per_string_limit > limit_to_u64(limits.max_string_bytes)
                    {
                        return Err(LumbaError::limit_exceeded(
                            "string length exceeds configured maximum",
                        ));
                    }
                }
                SectionId::EXTS => {
                    enforce_limit_u64(
                        entry.item_count,
                        limits.max_table_record_count,
                        "table record count exceeds configured maximum",
                    )?;
                }
                SectionId::VALS => {
                    total_values = total_values.checked_add(entry.item_count).ok_or_else(|| {
                        LumbaError::limit_exceeded("value count exceeds configured maximum")
                    })?;
                    enforce_limit_u64(
                        total_values,
                        limits.max_value_count,
                        "value count exceeds configured maximum",
                    )?;
                    if limits.max_nesting_depth == 0 && entry.item_count > 0 {
                        return Err(LumbaError::limit_exceeded(
                            "nesting depth exceeds configured maximum",
                        ));
                    }
                }
                SectionId::DOCS => {
                    total_documents =
                        total_documents
                            .checked_add(entry.item_count)
                            .ok_or_else(|| {
                                LumbaError::limit_exceeded(
                                    "document count exceeds configured maximum",
                                )
                            })?;
                    enforce_limit_u64(
                        total_documents,
                        limits.max_document_count,
                        "document count exceeds configured maximum",
                    )?;
                }
                SectionId::ASTN => {
                    total_syntax_nodes = total_syntax_nodes
                        .checked_add(entry.item_count)
                        .ok_or_else(|| {
                            LumbaError::limit_exceeded(
                                "syntax node count exceeds configured maximum",
                            )
                        })?;
                    enforce_limit_u64(
                        total_syntax_nodes,
                        limits.max_syntax_node_count,
                        "syntax node count exceeds configured maximum",
                    )?;
                }
                SectionId::EMBD => {
                    total_resources =
                        total_resources
                            .checked_add(entry.item_count)
                            .ok_or_else(|| {
                                LumbaError::limit_exceeded(
                                    "embedded resource count exceeds configured maximum",
                                )
                            })?;
                    enforce_limit_u64(
                        total_resources,
                        limits.max_resource_count,
                        "embedded resource count exceeds configured maximum",
                    )?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn decode_strs(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
    ) -> Result<crate::string_table::StringTable> {
        decode_string_table(section.payload, &self.options.limits).map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })
    }

    fn decode_meta(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
    ) -> Result<crate::meta::Metadata> {
        let item_count = metadata_item_count(section.payload)?;
        if section.entry.item_count != item_count {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "META item_count {} did not match decoded value record count {}",
                    section.entry.item_count, item_count
                )),
            ));
        }
        decode_metadata(section.payload, &self.options.limits).map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })
    }

    fn decode_syms(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        string_count: usize,
    ) -> Result<crate::symbol::SymbolTable> {
        let table = decode_symbol_table(section.payload, &self.options.limits, string_count)
            .map_err(|error| {
                let mut context = error.context().clone();
                context.byte_offset = context
                    .byte_offset
                    .and_then(|offset| {
                        usize::try_from(section.entry.payload_offset)
                            .ok()?
                            .checked_add(offset)
                    })
                    .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                context.section_index = Some(section_index);
                error.with_context(context)
            })?;
        if section.entry.item_count != table.symbols.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "SYMS item_count {} did not match decoded symbol record count {}",
                    section.entry.item_count,
                    table.symbols.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_exts(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        string_table: &crate::string_table::StringTable,
        values: Option<&[crate::value::Value]>,
    ) -> Result<crate::extension::ExtensionTable> {
        let table =
            decode_extension_table(section.payload, &self.options.limits, string_table, values)
                .map_err(|error| {
                    let mut context = error.context().clone();
                    context.byte_offset = context
                        .byte_offset
                        .and_then(|offset| {
                            usize::try_from(section.entry.payload_offset)
                                .ok()?
                                .checked_add(offset)
                        })
                        .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                    context.section_index = Some(section_index);
                    error.with_context(context)
                })?;
        if section.entry.item_count != table.declarations.len() as u64 {
            return Err(LumbaError::MalformedExtensionPayload(
                crate::error::ErrorContext::new(format!(
                    "EXTS item_count {} did not match decoded extension count {}",
                    section.entry.item_count,
                    table.declarations.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_vals(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        blob_count: usize,
    ) -> Result<Section> {
        let values = decode_value_table(
            section.payload,
            &self.options.limits,
            ValueDecodeMode::Portable,
            blob_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;

        Ok(Section {
            name: VALUE_SECTION_NAME.into(),
            values,
        })
    }

    fn decode_blob(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
    ) -> Result<crate::blob::BlobTable> {
        let table = decode_blob_table(section.payload, &self.options.limits).map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;

        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "BLOB item_count {} did not match decoded blob record count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }

        Ok(table)
    }

    fn decode_docs(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        values: Option<&[crate::value::Value]>,
        schema_count: usize,
        capability_count: usize,
    ) -> Result<Vec<crate::document::Document>> {
        decode_document_table(section.payload, values, schema_count, capability_count).map_err(
            |error| {
                let mut context = error.context().clone();
                context.byte_offset = context
                    .byte_offset
                    .and_then(|offset| {
                        usize::try_from(section.entry.payload_offset)
                            .ok()?
                            .checked_add(offset)
                    })
                    .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                context.section_index = Some(section_index);
                error.with_context(context)
            },
        )
    }

    fn decode_tags(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        symbols: &crate::symbol::SymbolTable,
        values: Option<&[crate::value::Value]>,
        schema_count: usize,
    ) -> Result<crate::tag::TagTable> {
        let table = decode_tag_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            values,
            schema_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.declarations.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "TAGS item_count {} did not match decoded tag count {}",
                    section.entry.item_count,
                    table.declarations.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_scma(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        values: Option<&[crate::value::Value]>,
        blob_count: usize,
    ) -> Result<crate::schema::SchemaTable> {
        let table = decode_schema_table(
            section.payload,
            &self.options.limits,
            strings,
            values,
            blob_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "SCMA item_count {} did not match decoded schema count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_diag(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        symbols: &crate::symbol::SymbolTable,
        span_count: usize,
    ) -> Result<crate::diagnostic::DiagnosticTable> {
        let table = decode_diagnostic_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            span_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "DIAG item_count {} did not match decoded diagnostic count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_deps(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        symbols: &crate::symbol::SymbolTable,
        values: Option<&[crate::value::Value]>,
        span_count: usize,
        blob_count: usize,
    ) -> Result<crate::bundle::DependencyTable> {
        let table = decode_dependency_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            values,
            span_count,
            blob_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "DEPS item_count {} did not match decoded dependency count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_srcf(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        blob_count: usize,
    ) -> Result<crate::source::SourceFileTable> {
        let table =
            decode_source_file_table(section.payload, &self.options.limits, strings, blob_count)
                .map_err(|error| {
                    let mut context = error.context().clone();
                    context.byte_offset = context
                        .byte_offset
                        .and_then(|offset| {
                            usize::try_from(section.entry.payload_offset)
                                .ok()?
                                .checked_add(offset)
                        })
                        .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                    context.section_index = Some(section_index);
                    error.with_context(context)
                })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "SRCF item_count {} did not match decoded source file count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_embd(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        dependency_count: usize,
        blob_count: usize,
        strings: Option<&crate::string_table::StringTable>,
        symbols: Option<&crate::symbol::SymbolTable>,
    ) -> Result<crate::bundle::EmbeddedResourceTable> {
        let table = decode_embedded_resource_table(
            section.payload,
            &self.options.limits,
            dependency_count,
            blob_count,
            strings,
            symbols,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "EMBD item_count {} did not match decoded embedded resource count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_srcs(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        source_files: &crate::source::SourceFileTable,
        blobs: Option<&crate::blob::BlobTable>,
    ) -> Result<crate::source::SourceSpanTable> {
        let table =
            decode_source_span_table(section.payload, &self.options.limits, source_files, blobs)
                .map_err(|error| {
                    let mut context = error.context().clone();
                    context.byte_offset = context
                        .byte_offset
                        .and_then(|offset| {
                            usize::try_from(section.entry.payload_offset)
                                .ok()?
                                .checked_add(offset)
                        })
                        .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                    context.section_index = Some(section_index);
                    error.with_context(context)
                })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "SRCS item_count {} did not match decoded source span count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_astn(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        symbols: &crate::symbol::SymbolTable,
        value_count: usize,
        span_count: usize,
        blob_count: usize,
        trivia_count: usize,
    ) -> Result<crate::syntax::SyntaxNodeTable> {
        let table = decode_syntax_node_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            value_count,
            span_count,
            blob_count,
            trivia_count,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "ASTN item_count {} did not match decoded syntax node count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_triv(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        spans: Option<&crate::source::SourceSpanTable>,
    ) -> Result<crate::trivia::TriviaTable> {
        let table = decode_trivia_table(section.payload, &self.options.limits, strings, spans)
            .map_err(|error| {
                let mut context = error.context().clone();
                context.byte_offset = context
                    .byte_offset
                    .and_then(|offset| {
                        usize::try_from(section.entry.payload_offset)
                            .ok()?
                            .checked_add(offset)
                    })
                    .or_else(|| usize::try_from(section.entry.payload_offset).ok());
                context.section_index = Some(section_index);
                error.with_context(context)
            })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "TRIV item_count {} did not match decoded trivia count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }
}

pub(crate) fn read_level1_minimal_value_image(bytes: &[u8]) -> Result<Vec<LumaValue>> {
    verify_level1_minimal_value_image_bytes(bytes)?;

    let file = Reader::new(ReadOptions::new()).read(bytes)?;
    verify_level1_minimal_value_image_file(&file)?;

    file.documents
        .into_iter()
        .enumerate()
        .map(|(record_index, document)| {
            let root_value = document.root_value.ok_or_else(|| {
                LumbaError::InvalidDocumentTable(crate::error::ErrorContext::new(
                    "level1 minimal value images require every DOCS record to reference a root value",
                )
                .with_record_index(record_index))
            })?;
            LumaValue::try_from(root_value)
        })
        .collect()
}

fn verify_level1_minimal_value_image_bytes(bytes: &[u8]) -> Result<()> {
    let header = ContainerHeader::decode(bytes, HeaderCrcMode::Enabled)?;
    if header.header_crc32c != 0 {
        return Err(LumbaError::checksum_mismatch(
            "level1 minimal value images require a zero header CRC",
        ));
    }

    let sections = validate_section_table_with_reserved_flag_policy(
        &header,
        bytes,
        crate::policy::ReservedFlagPolicy::Reject,
    )?;
    for section in sections {
        if !matches!(
            section.entry.section_id,
            SectionId::STRS | SectionId::VALS | SectionId::DOCS
        ) {
            return Err(LumbaError::invalid_section_table(
                "level1 minimal value images may only contain STRS, VALS, and DOCS sections",
            ));
        }
        if section.entry.section_version != 1 {
            return Err(LumbaError::invalid_section_table(
                "level1 minimal value images require section version 1",
            ));
        }
        if section.entry.entry_flags != (SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE) {
            return Err(LumbaError::non_canonical_encoding(
                "level1 minimal value images require canonical REQUIRED|UNIQUE section flags",
            ));
        }
        if section.entry.payload_flags != 0 {
            return Err(LumbaError::invalid_reserved_flags(
                "level1 minimal value images require zero payload flags",
            ));
        }
        if section.entry.codec_id != 0 {
            return Err(LumbaError::unsupported_codec(
                "level1 minimal value images require uncompressed payloads",
            ));
        }
        if section.entry.checksum_id != 0
            || section.entry.checksum_low != 0
            || section.entry.checksum_high != 0
        {
            return Err(LumbaError::checksum_mismatch(
                "level1 minimal value images require zero section checksums",
            ));
        }
    }

    Ok(())
}

fn limit_to_u64(limit: usize) -> u64 {
    u64::try_from(limit).unwrap_or(u64::MAX)
}

fn enforce_limit_u64(value: u64, limit: usize, message: &'static str) -> Result<()> {
    if value > limit_to_u64(limit) {
        Err(LumbaError::limit_exceeded(message))
    } else {
        Ok(())
    }
}

const fn is_table_like_section(section_id: SectionId) -> bool {
    matches!(
        section_id,
        SectionId::META
            | SectionId::STRS
            | SectionId::EXTS
            | SectionId::SYMS
            | SectionId::BLOB
            | SectionId::VALS
            | SectionId::DOCS
            | SectionId::TAGS
            | SectionId::SCMA
            | SectionId::DIAG
            | SectionId::SRCF
            | SectionId::SRCS
            | SectionId::ASTN
            | SectionId::TRIV
            | SectionId::DEPS
            | SectionId::EMBD
            | SectionId::CAPS
            | SectionId::SIGN
            | SectionId::FOOT
    )
}

impl Reader {
    fn decode_caps(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: &crate::string_table::StringTable,
        symbols: &crate::symbol::SymbolTable,
        values: Option<&[crate::value::Value]>,
    ) -> Result<crate::capability::CapabilityTable> {
        let table = decode_capability_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            values,
        )
        .map_err(|error| {
            let mut context = error.context().clone();
            context.byte_offset = context
                .byte_offset
                .and_then(|offset| {
                    usize::try_from(section.entry.payload_offset)
                        .ok()?
                        .checked_add(offset)
                })
                .or_else(|| usize::try_from(section.entry.payload_offset).ok());
            context.section_index = Some(section_index);
            error.with_context(context)
        })?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "CAPS item_count {} did not match decoded capability-set count {}",
                    section.entry.item_count,
                    table.records.len()
                )),
            ));
        }
        Ok(table)
    }

    fn decode_sign(
        &self,
        section: &ValidatedSection<'_>,
        section_index: usize,
        strings: Option<&crate::string_table::StringTable>,
        symbols: Option<&crate::symbol::SymbolTable>,
        values: Option<&[crate::value::Value]>,
        blob_count: usize,
        section_count: usize,
    ) -> Result<crate::signature::SignatureTable> {
        let table = decode_signature_table(
            section.payload,
            &self.options.limits,
            strings,
            symbols,
            values,
            blob_count,
            section_count,
        )?;
        if section.entry.item_count != table.records.len() as u64 {
            return Err(LumbaError::InvalidSectionTable(
                crate::error::ErrorContext::new(format!(
                    "SIGN item_count {} did not match decoded signature count {}",
                    section.entry.item_count,
                    table.records.len()
                ))
                .with_section(b'S', section_index),
            ));
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadOptions, Reader};
    use crate::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE};
    use crate::document::{DOCUMENT_FLAG_HAS_VALUE_ROOT, Document};
    use crate::error::LumbaError;
    use crate::policy::{Limits, ReservedFlagPolicy};
    use crate::primitives::UVar;
    use crate::section::{
        CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_TRUSTED_ONLY,
        SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
    };
    use crate::string_table::STRING_FLAG_RESERVED_MASK;
    use crate::value::{Value, encode_value_table};
    use crate::write::encode_section_table;
    use luma_syntax::{LumaNull, LumaValue};

    fn build_file(entries: &[SectionEntry], payloads: &[&[u8]], table_offset: u64) -> Vec<u8> {
        let table = encode_section_table(entries).expect("table should encode");
        let mut file_len = usize::from(HEADER_SIZE).max((table_offset as usize) + table.len());
        for (entry, payload) in entries.iter().zip(payloads) {
            file_len = file_len.max((entry.payload_offset as usize) + payload.len());
        }

        let mut header = ContainerHeader::new();
        header.section_table_offset = table_offset;
        header.section_count = entries.len() as u32;
        header.section_entry_size = SECTION_ENTRY_SIZE;
        header.file_length = file_len as u64;

        let mut bytes = vec![0_u8; file_len];
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should encode"),
        );
        bytes[table_offset as usize..table_offset as usize + table.len()].copy_from_slice(&table);
        for (entry, payload) in entries.iter().zip(payloads) {
            bytes[entry.payload_offset as usize..entry.payload_offset as usize + payload.len()]
                .copy_from_slice(payload);
        }

        let mut header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
        header.header_crc32c = 0;
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should reencode"),
        );
        bytes
    }

    fn limit_error(bytes: &[u8], limits: Limits) -> LumbaError {
        Reader::new(ReadOptions::new().with_limits(limits))
            .read(bytes)
            .expect_err("limits should fail")
    }

    fn canonical_test_entry(section_id: SectionId) -> SectionEntry {
        SectionEntry {
            section_id,
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
        }
    }

    fn strs_payload(records: &[(u64, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        UVar(records.len() as u64).encode_into(&mut bytes);
        for (flags, value) in records {
            UVar(*flags).encode_into(&mut bytes);
            UVar(value.len() as u64).encode_into(&mut bytes);
            bytes.extend_from_slice(value);
        }
        bytes
    }

    fn docs_payload(records: &[(u64, Option<u64>)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        UVar(records.len() as u64).encode_into(&mut bytes);
        for (flags, root_ref) in records {
            UVar(*flags).encode_into(&mut bytes);
            if let Some(root_ref) = root_ref {
                UVar(*root_ref).encode_into(&mut bytes);
            }
        }
        bytes
    }

    #[test]
    fn reader_accepts_minimal_valid_header_only_file() {
        let bytes = ContainerHeader::new()
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode");

        let result = Reader::new(ReadOptions::new()).read(&bytes);

        assert!(result.is_ok());
    }

    #[test]
    fn reader_rejects_section_table_overlap_with_lb0006() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 64,
            stored_size: 8,
            logical_size: 8,
            item_count: 1,
            checksum_low: 0,
            checksum_high: 0,
        };
        let entry_bytes = entry.encode().expect("entry should encode");
        let bytes = build_file(&[entry], &[&entry_bytes[..8]], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("overlap should fail");

        assert!(matches!(error, LumbaError::OverlappingSections(_)));
        assert_eq!(error.code().as_str(), "LB0006");
    }

    #[test]
    fn reader_rejects_payload_overlap_with_lb0006() {
        let entries = [
            SectionEntry {
                section_id: SectionId::STRS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 192,
                stored_size: 8,
                logical_size: 8,
                item_count: 1,
                checksum_low: 0,
                checksum_high: 0,
            },
            SectionEntry {
                section_id: SectionId::DOCS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 196,
                stored_size: 8,
                logical_size: 8,
                item_count: 1,
                checksum_low: 0,
                checksum_high: 0,
            },
        ];
        let bytes = build_file(&entries, &[b"12345678", b"abcdefgh"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("overlap should fail");

        assert!(matches!(error, LumbaError::OverlappingSections(_)));
        assert_eq!(error.code().as_str(), "LB0006");
    }

    #[test]
    fn reader_rejects_payload_outside_file_with_lb0007() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 16,
            logical_size: 16,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let mut bytes = build_file(&[entry], &[b"12345678"], 72);
        bytes.truncate(136);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("offset should fail");

        assert!(matches!(error, LumbaError::OffsetOutsideFile(_)));
        assert_eq!(error.code().as_str(), "LB0007");
    }

    #[test]
    fn reader_rejects_required_unknown_section_with_lb0008() {
        let entry = SectionEntry {
            section_id: SectionId::new(*b"UNKN"),
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 8,
            logical_size: 8,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("required section should fail");

        assert!(matches!(error, LumbaError::UnsupportedRequiredSection(_)));
        assert_eq!(error.code().as_str(), "LB0008");
    }

    #[test]
    fn reader_rejects_non_ascii_required_section_id_without_panicking() {
        let entry = SectionEntry {
            section_id: SectionId::new([0xFF, b'B', b'A', b'D']),
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 8,
            logical_size: 8,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let result = std::panic::catch_unwind(|| Reader::new(ReadOptions::new()).read(&bytes));

        let error = result
            .expect("reader should return an error instead of panicking")
            .expect_err("non-ascii required section should fail");
        assert!(matches!(error, LumbaError::InvalidSectionTable(_)));
    }

    #[test]
    fn reader_rejects_required_unsupported_codec_with_lb0010() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: 1,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 8,
            logical_size: 8,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("codec should fail");

        assert!(matches!(error, LumbaError::UnsupportedCodec(_)));
        assert_eq!(error.code().as_str(), "LB0010");
    }

    #[test]
    fn reader_rejects_duplicate_unique_section() {
        let entries = [
            SectionEntry {
                section_id: SectionId::STRS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 192,
                stored_size: 8,
                logical_size: 8,
                item_count: 0,
                checksum_low: 0,
                checksum_high: 0,
            },
            SectionEntry {
                section_id: SectionId::STRS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 200,
                stored_size: 8,
                logical_size: 8,
                item_count: 0,
                checksum_low: 0,
                checksum_high: 0,
            },
        ];
        let bytes = build_file(&entries, &[b"12345678", b"abcdefgh"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("duplicate unique should fail");

        assert!(matches!(error, LumbaError::InvalidSectionTable(_)));
    }

    #[test]
    fn reader_rejects_unsorted_section_table_with_lb0017() {
        let entries = [
            SectionEntry {
                section_id: SectionId::DOCS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 192,
                stored_size: 8,
                logical_size: 8,
                item_count: 0,
                checksum_low: 0,
                checksum_high: 0,
            },
            SectionEntry {
                section_id: SectionId::STRS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 200,
                stored_size: 8,
                logical_size: 8,
                item_count: 0,
                checksum_low: 0,
                checksum_high: 0,
            },
        ];
        let bytes = build_file(&entries, &[b"12345678", b"abcdefgh"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("non-canonical order should fail");

        assert!(matches!(error, LumbaError::NonCanonicalEncoding(_)));
        assert_eq!(error.code().as_str(), "LB0017");
    }

    #[test]
    fn reader_rejects_non_zero_padding_with_lb0025() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: 3,
            logical_size: 3,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let mut bytes = build_file(&[entry], &[b"abc"], 64);
        let mut header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
        bytes.resize(136, 0);
        header.file_length = bytes.len() as u64;
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should reencode"),
        );
        bytes[131] = 9;

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("padding should fail");

        assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }

    #[test]
    fn reader_rejects_offset_plus_size_overflow_before_slicing() {
        let entry = SectionEntry {
            section_id: SectionId::STRS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: u64::MAX - 4,
            stored_size: 8,
            logical_size: 8,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        };
        let mut header = ContainerHeader::new();
        header.section_count = 1;
        header.file_length = 128;
        let table = encode_section_table(&[entry]).expect("table should encode");
        let mut bytes = vec![0_u8; 128];
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should encode"),
        );
        bytes[64..128].copy_from_slice(&table);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("overflow should fail");

        assert!(matches!(error, LumbaError::OffsetOutsideFile(_)));
    }

    #[test]
    fn reader_rejects_table_length_multiplication_overflow_before_slicing() {
        let error = crate::section::checked_table_allocation_len(usize::MAX, 64)
            .expect_err("table length overflow should fail");

        assert!(matches!(error, LumbaError::InvalidSectionTable(_)));
    }

    #[test]
    fn reader_rejects_section_count_with_lb0018_before_table_allocation() {
        let mut header = ContainerHeader::new();
        header.section_count = 2;
        let bytes = header
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode");

        let mut limits = Limits::public();
        limits.max_sections = 1;
        let error = limit_error(&bytes, limits);

        assert!(matches!(error, LumbaError::ResourceLimitExceeded(_)));
        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_string_count_with_lb0018() {
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.item_count = 3;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_string_count = 2;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_section_payload_size_with_lb0018_before_copying_image_bytes() {
        let mut entry = canonical_test_entry(SectionId::BLOB);
        entry.stored_size = 9;
        let bytes = build_file(&[entry], &[b"123456789"], 64);

        let mut limits = Limits::public();
        limits.max_section_payload_bytes = 8;
        let error = Reader::new(ReadOptions::new().with_limits(limits))
            .read_image(&bytes)
            .expect_err("oversized payload should fail before image copy");

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_decoded_logical_size_with_lb0018_before_materialization() {
        let mut entry = canonical_test_entry(SectionId::BLOB);
        entry.logical_size = 9;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_decoded_logical_bytes = 8;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_string_length_with_lb0018() {
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.logical_size = 9;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_string_bytes = 8;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_value_count_with_lb0018() {
        let mut entry = canonical_test_entry(SectionId::VALS);
        entry.item_count = 4;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_value_count = 3;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_nesting_depth_with_lb0018() {
        let mut header = ContainerHeader::new();
        header.root_document_count = 1;
        let bytes = header
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode");

        let mut limits = Limits::public();
        limits.max_nesting_depth = 0;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_document_count_with_lb0018() {
        let mut header = ContainerHeader::new();
        header.root_document_count = 2;
        let bytes = header
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode");

        let mut limits = Limits::public();
        limits.max_document_count = 1;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_syntax_node_count_with_lb0018() {
        let mut entry = canonical_test_entry(SectionId::ASTN);
        entry.item_count = 5;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_syntax_node_count = 4;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_resource_count_with_lb0018() {
        let mut entry = canonical_test_entry(SectionId::EMBD);
        entry.item_count = 2;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let mut limits = Limits::public();
        limits.max_resource_count = 1;
        let error = limit_error(&bytes, limits);

        assert_eq!(error.code().as_str(), "LB0018");
    }

    #[test]
    fn reader_rejects_trusted_only_section_with_lb0019_for_public_policy() {
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.entry_flags |= SECTION_FLAG_TRUSTED_ONLY;
        let bytes = build_file(&[entry], &[b"12345678"], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("public policy should reject trusted-only section");

        assert_eq!(error.code().as_str(), "LB0019");
    }

    #[test]
    fn reader_rejects_invalid_utf8_strings_with_lb0013() {
        let payload = strs_payload(&[(0, &[0xFF])]);
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.stored_size = payload.len() as u64;
        entry.logical_size = payload.len() as u64;
        let bytes = build_file(&[entry], &[&payload], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("invalid UTF-8 should fail");

        assert!(matches!(error, LumbaError::InvalidUtf8(_)));
        assert_eq!(error.code().as_str(), "LB0013");
    }

    #[test]
    fn reader_rejects_reserved_string_flags_with_lb0025() {
        let payload = strs_payload(&[(STRING_FLAG_RESERVED_MASK, b"abc")]);
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.stored_size = payload.len() as u64;
        entry.logical_size = payload.len() as u64;
        let bytes = build_file(&[entry], &[&payload], 64);

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("reserved string flags should fail");

        assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }

    #[test]
    fn reader_accepts_empty_string_table() {
        let payload = strs_payload(&[]);
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.stored_size = payload.len() as u64;
        entry.logical_size = payload.len() as u64;
        entry.item_count = 0;
        let bytes = build_file(&[entry], &[&payload], 64);

        let file = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect("empty string table should decode");

        assert_eq!(
            file.string_table
                .expect("STRS should be present")
                .strings
                .len(),
            0
        );
    }

    #[test]
    fn reader_accepts_trusted_only_section_for_trusted_policy() {
        let mut entry = canonical_test_entry(SectionId::STRS);
        entry.entry_flags |= SECTION_FLAG_TRUSTED_ONLY;
        let payload = strs_payload(&[]);
        entry.stored_size = payload.len() as u64;
        entry.logical_size = payload.len() as u64;
        entry.item_count = 0;
        let bytes = build_file(&[entry], &[&payload], 64);

        let result = Reader::new(ReadOptions::new().with_limits(Limits::trusted())).read(&bytes);

        assert!(result.is_ok());
    }

    #[test]
    fn reader_materializes_value_only_documents_when_header_count_matches_vals() {
        let file = crate::container::LumbaFile::new()
            .with_document(Document::new().with_root_value(Value::Int(7)));

        let bytes = crate::write::Writer::new(crate::write::WriteOptions::new())
            .write(&file)
            .expect("writer should encode docs");

        let decoded = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect("reader should decode docs");

        assert_eq!(
            decoded.documents,
            vec![Document::new().with_root_value(Value::Int(7))]
        );
    }

    #[test]
    fn reader_rejects_docs_missing_root_value_when_flagged_with_lb0023() {
        let payload = docs_payload(&[(DOCUMENT_FLAG_HAS_VALUE_ROOT, None)]);
        let entry = SectionEntry {
            section_id: SectionId::DOCS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: payload.len() as u64,
            logical_size: payload.len() as u64,
            item_count: 1,
            checksum_low: 0,
            checksum_high: 0,
        };
        let mut bytes = build_file(&[entry], &[&payload], 64);
        let mut header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
        header.root_document_count = 1;
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should encode"),
        );

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("missing root value ref should fail");

        assert!(matches!(error, LumbaError::InvalidDocumentTable(_)));
        assert_eq!(error.code().as_str(), "LB0023");
    }

    #[test]
    fn reader_rejects_invalid_docs_root_value_ref_with_lb0014() {
        let vals_payload = encode_value_table(
            &[Value::Null],
            &Limits::public(),
            crate::write::WriterMode::Pretty,
        )
        .expect("VALS payload should encode");
        let docs_payload = docs_payload(&[(DOCUMENT_FLAG_HAS_VALUE_ROOT, Some(1))]);
        let entries = [
            SectionEntry {
                section_id: SectionId::VALS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 256,
                stored_size: vals_payload.len() as u64,
                logical_size: vals_payload.len() as u64,
                item_count: 1,
                checksum_low: 0,
                checksum_high: 0,
            },
            SectionEntry {
                section_id: SectionId::DOCS,
                section_version: 1,
                entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
                payload_flags: 0,
                codec_id: CODEC_NONE,
                checksum_id: CHECKSUM_NONE,
                payload_offset: 320,
                stored_size: docs_payload.len() as u64,
                logical_size: docs_payload.len() as u64,
                item_count: 1,
                checksum_low: 0,
                checksum_high: 0,
            },
        ];
        let mut bytes = build_file(&entries, &[&vals_payload, &docs_payload], 64);
        let mut header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
        header.root_document_count = 1;
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should encode"),
        );

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("invalid root ref should fail");

        assert!(matches!(error, LumbaError::InvalidValueReference(_)));
        assert_eq!(error.code().as_str(), "LB0014");
    }

    #[test]
    fn reader_rejects_root_document_count_mismatch_with_lb0023() {
        let payload = docs_payload(&[(0, None)]);
        let entry = SectionEntry {
            section_id: SectionId::DOCS,
            section_version: 1,
            entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
            payload_flags: 0,
            codec_id: CODEC_NONE,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 128,
            stored_size: payload.len() as u64,
            logical_size: payload.len() as u64,
            item_count: 1,
            checksum_low: 0,
            checksum_high: 0,
        };
        let mut bytes = build_file(&[entry], &[&payload], 64);
        let mut header =
            ContainerHeader::decode(&bytes, HeaderCrcMode::Disabled).expect("header should decode");
        header.root_document_count = 2;
        bytes[..usize::from(HEADER_SIZE)].copy_from_slice(
            &header
                .encode(HeaderCrcMode::Enabled)
                .expect("header should encode"),
        );

        let error = Reader::new(ReadOptions::new())
            .read(&bytes)
            .expect_err("count mismatch should fail");

        assert!(matches!(error, LumbaError::InvalidDocumentTable(_)));
        assert_eq!(error.code().as_str(), "LB0023");
    }

    #[test]
    fn reader_can_tolerate_future_reserved_header_bits_when_enabled() {
        let mut bytes = ContainerHeader::new()
            .encode(HeaderCrcMode::Enabled)
            .expect("header should encode")
            .to_vec();
        let reserved_container_flags = (1_u32 << 12).to_le_bytes();
        bytes[16..20].copy_from_slice(&reserved_container_flags);

        let mut limits = Limits::public();
        limits.reserved_flag_policy = ReservedFlagPolicy::AllowFuture;
        let result = Reader::new(
            ReadOptions::new()
                .with_limits(limits)
                .with_header_crc_mode(HeaderCrcMode::Disabled),
        )
        .read(&bytes);

        assert!(result.is_ok());
    }

    #[test]
    fn level1_minimal_reader_round_trips_portable_documents() {
        let values = vec![
            LumaValue::Null(LumaNull),
            LumaValue::String(String::from("hello")),
        ];

        let bytes = crate::write::write_level1_minimal_value_image(&values)
            .expect("level1 minimal bytes should encode");
        let decoded = super::read_level1_minimal_value_image(&bytes)
            .expect("level1 minimal bytes should decode");

        assert_eq!(decoded, values);
    }
}
