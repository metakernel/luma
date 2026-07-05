use std::{
    fs::{self, File},
    io::Read,
    path::PathBuf,
};

use clap::{Args, Subcommand, ValueEnum};
use luma::lumba::container::{ContainerHeader, HeaderCrcMode, validate_section_table};
use luma::lumba::policy::TrustPolicy;
use luma::lumba::primitives::Identifier;
use luma::lumba::verify::Verifier;
use luma::lumba::{
    BLOB_FLAG_SOURCE_TEXT, BLOB_FLAG_UTF8_TEXT, BlobRecord, BlobTable, CanonicalMode,
    CapabilityTable, DiagnosticLoadPolicy, Document, Limits, ReadOptions, Reader, SourceFileRecord,
    SourceFileTable, Value as LumbaValue, WriteOptions, Writer, WriterMode,
};
use luma_parser::{Diagnostic, DiagnosticCode, FileId, Severity, parse_str};
use luma_syntax::{
    Directive, Document as SyntaxDocument, DocumentItem, LumaKey, LumaMapping, LumaMappingEntry,
    LumaNode, LumaNull, LumaNumber, LumaSequence, LumaTag, LumaTagName, LumaTaggedValue,
    LumaValue as PortableValue, MappingBlock, MappingItem, MappingKey, SequenceBlock, SequenceItem,
    Span, serialize_value,
};
use serde_json::{Value, json};

use crate::output::{CliError, CommandReport};

#[derive(Debug, Clone, Args)]
pub struct LumbaArgs {
    #[command(subcommand)]
    pub command: LumbaCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum LumbaCommand {
    Encode(LumbaEncodeArgs),
    Decode(LumbaDecodeArgs),
    Inspect(LumbaInspectArgs),
    Verify(LumbaVerifyArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LumbaModeArg {
    Value,
    RuntimeData,
    EditorCache,
    Bundle,
    Fixture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LumbaInspectEmit {
    Header,
    Sections,
    Values,
    Resources,
    Capabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LumbaLimitsPreset {
    Public,
    Strict,
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LumbaChecksumArg {
    None,
    Crc32c,
}

#[derive(Debug, Clone, Args)]
pub struct LumbaPolicyArgs {
    #[arg(long, value_enum, default_value_t = LumbaLimitsPreset::Public)]
    pub limits: LumbaLimitsPreset,

    #[arg(long, conflicts_with = "trusted")]
    pub public: bool,

    #[arg(long, conflicts_with = "public")]
    pub trusted: bool,
}

#[derive(Debug, Clone, Args)]
pub struct LumbaEncodeArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[arg(value_name = "OUTPUT")]
    pub destination: PathBuf,

    #[arg(long, value_enum, default_value_t = LumbaModeArg::Value)]
    pub mode: LumbaModeArg,

    #[arg(long)]
    pub canonical: bool,

    #[arg(long)]
    pub strict: bool,

    #[arg(long)]
    pub include_source: bool,

    #[arg(long)]
    pub footer: bool,

    #[arg(long, value_enum, default_value_t = LumbaChecksumArg::None)]
    pub checksum: LumbaChecksumArg,

    #[command(flatten)]
    pub policy: LumbaPolicyArgs,
}

#[derive(Debug, Clone, Args)]
pub struct LumbaDecodeArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[command(flatten)]
    pub policy: LumbaPolicyArgs,
}

#[derive(Debug, Clone, Args)]
pub struct LumbaInspectArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[arg(long, value_enum, default_value_t = LumbaInspectEmit::Header)]
    pub emit: LumbaInspectEmit,

    #[command(flatten)]
    pub policy: LumbaPolicyArgs,
}

#[derive(Debug, Clone, Args)]
pub struct LumbaVerifyArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[command(flatten)]
    pub policy: LumbaPolicyArgs,
}

pub fn run(args: &LumbaArgs) -> Result<CommandReport, CliError> {
    match &args.command {
        LumbaCommand::Encode(args) => encode(args),
        LumbaCommand::Decode(args) => decode(args),
        LumbaCommand::Inspect(args) => inspect(args),
        LumbaCommand::Verify(args) => verify(args),
    }
}

fn encode(args: &LumbaEncodeArgs) -> Result<CommandReport, CliError> {
    let limits = resolve_limits(&args.policy);
    let text = read_text(&args.input, &limits)?;
    let name = args.input.display().to_string();
    let parsed = parse_str(FileId(1), &name, &text);
    let mut diagnostics = parsed.diagnostics;

    let values = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        None
    } else {
        documents_to_values(&parsed.file.documents, &mut diagnostics)
    };

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Ok(CommandReport {
            command: "lumba",
            source: Some(parsed.source.source),
            diagnostics,
            payload: None,
            human_text: None,
            success: None,
        });
    }

    let values = values.expect("values should be present when diagnostics are clean");
    let mode = resolve_writer_mode(args.mode, args.canonical, args.strict);
    let write_options = WriteOptions::new()
        .with_mode(mode)
        .with_limits(limits.clone())
        .with_section_checksum_id(resolve_checksum_id(args.checksum))
        .with_footer(args.footer);
    let file = lumba_file_from_values(&values, &args.input, &text, args.include_source)?;
    let bytes = match Writer::new(write_options).write(&file) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(lumba_error_to_diagnostic(&error));
            return Ok(CommandReport {
                command: "lumba",
                source: Some(parsed.source.source),
                diagnostics,
                payload: None,
                human_text: None,
                success: None,
            });
        }
    };

    fs::write(&args.destination, &bytes).map_err(|error| CliError {
        message: format!("failed to write '{}': {error}", args.destination.display()),
    })?;

    Ok(CommandReport {
        command: "lumba",
        source: None,
        diagnostics: Vec::new(),
        payload: Some((
            "result",
            json!({
                "action": "encode",
                "input": args.input.display().to_string(),
                "output": args.destination.display().to_string(),
                "mode": mode_label(mode),
                "document_count": values.len(),
                "byte_length": bytes.len(),
                "limits": limits_to_json(&limits),
                "include_source": args.include_source,
                "footer": args.footer,
                "checksum": checksum_label(args.checksum),
            }),
        )),
        human_text: Some(format!(
            "encoded {} document(s) to '{}'\n",
            values.len(),
            args.destination.display()
        )),
        success: Some(true),
    })
}

fn decode(args: &LumbaDecodeArgs) -> Result<CommandReport, CliError> {
    let limits = resolve_limits(&args.policy);
    let bytes = read_bytes(&args.input, &limits)?;
    let file = match Reader::new(read_options(&limits)).read(&bytes) {
        Ok(file) => file,
        Err(error) => return Ok(lumba_error_report("lumba", error)),
    };

    let values = collect_root_values(&file);
    let payload = bounded_payload(
        "values",
        Value::Array(
            values
                .iter()
                .map(|value| lumba_value_to_json(value, &limits))
                .collect(),
        ),
        &limits,
    );

    Ok(CommandReport {
        command: "lumba",
        source: None,
        diagnostics: Vec::new(),
        payload: Some(payload),
        human_text: Some(render_decoded_values(&values, &limits)?),
        success: Some(true),
    })
}

fn inspect(args: &LumbaInspectArgs) -> Result<CommandReport, CliError> {
    let limits = resolve_limits(&args.policy);
    let bytes = read_bytes(&args.input, &limits)?;
    let payload = match args.emit {
        LumbaInspectEmit::Header => inspect_header_payload(&bytes),
        LumbaInspectEmit::Sections => inspect_sections_payload(&bytes),
        LumbaInspectEmit::Values => inspect_values_payload(&bytes, &limits),
        LumbaInspectEmit::Resources => inspect_resources_payload(&bytes, &limits),
        LumbaInspectEmit::Capabilities => inspect_capabilities_payload(&bytes, &limits),
    };

    match payload {
        Ok((label, payload)) => Ok(CommandReport {
            command: "lumba",
            source: None,
            diagnostics: Vec::new(),
            payload: Some(bounded_payload(label, payload, &limits)),
            human_text: None,
            success: Some(true),
        }),
        Err(error) => Ok(lumba_error_report("lumba", error)),
    }
}

fn verify(args: &LumbaVerifyArgs) -> Result<CommandReport, CliError> {
    let limits = resolve_limits(&args.policy);
    let bytes = read_bytes(&args.input, &limits)?;
    let file = match Reader::new(read_options(&limits)).read(&bytes) {
        Ok(file) => file,
        Err(error) => return Ok(lumba_error_report("lumba", error)),
    };
    let verification = match Verifier::new().verify(&file) {
        Ok(report) => report,
        Err(error) => return Ok(lumba_error_report("lumba", error)),
    };
    let diagnostics = verification
        .diagnostics
        .iter()
        .map(lumba_verifier_diagnostic_to_diagnostic)
        .collect::<Vec<_>>();

    Ok(CommandReport {
        command: "lumba",
        source: None,
        diagnostics,
        payload: Some((
            "verification",
            json!({
                "action": "verify",
                "document_count": file.documents.len(),
                "diagnostic_count": verification.diagnostics.len(),
                "limits": limits_to_json(&limits),
            }),
        )),
        human_text: Some(format!(
            "verified '{}' ({}) document(s), {} diagnostic(s)\n",
            args.input.display(),
            file.documents.len(),
            verification.diagnostics.len()
        )),
        success: Some(verification.is_clean()),
    })
}

fn resolve_writer_mode(mode: LumbaModeArg, canonical: bool, strict: bool) -> WriterMode {
    if strict {
        return WriterMode::Canonical(CanonicalMode::Strict);
    }
    if canonical {
        return WriterMode::Canonical(CanonicalMode::Relaxed);
    }
    match mode {
        LumbaModeArg::Value => WriterMode::Pretty,
        LumbaModeArg::RuntimeData => WriterMode::RuntimeData,
        LumbaModeArg::EditorCache => WriterMode::EditorCache,
        LumbaModeArg::Bundle => WriterMode::BuildBundle,
        LumbaModeArg::Fixture => WriterMode::ConformanceFixture,
    }
}

fn resolve_limits(args: &LumbaPolicyArgs) -> Limits {
    let mut limits = match args.limits {
        LumbaLimitsPreset::Public => Limits::public(),
        LumbaLimitsPreset::Strict => Limits::strict(),
        LumbaLimitsPreset::Trusted => Limits::trusted(),
    };
    if args.public {
        limits.trust_policy = TrustPolicy::Public;
    }
    if args.trusted {
        limits.trust_policy = TrustPolicy::Trusted;
    }
    limits
}

fn read_options(limits: &Limits) -> ReadOptions {
    ReadOptions::new()
        .with_limits(limits.clone())
        .with_header_crc_mode(HeaderCrcMode::Enabled)
        .with_diagnostic_policy(DiagnosticLoadPolicy::Allow)
}

fn resolve_checksum_id(checksum: LumbaChecksumArg) -> u16 {
    match checksum {
        LumbaChecksumArg::None => 0,
        LumbaChecksumArg::Crc32c => 1,
    }
}

fn checksum_label(checksum: LumbaChecksumArg) -> &'static str {
    match checksum {
        LumbaChecksumArg::None => "none",
        LumbaChecksumArg::Crc32c => "crc32c",
    }
}

fn mode_label(mode: WriterMode) -> &'static str {
    match mode {
        WriterMode::Pretty => "value",
        WriterMode::Compact => "compact",
        WriterMode::RuntimeData => "runtime-data",
        WriterMode::BuildBundle => "bundle",
        WriterMode::EditorCache => "editor-cache",
        WriterMode::ConformanceFixture => "fixture",
        WriterMode::Canonical(CanonicalMode::None) => "canonical",
        WriterMode::Canonical(CanonicalMode::Relaxed) => "canonical-relaxed",
        WriterMode::Canonical(CanonicalMode::Strict) => "canonical-strict",
    }
}

fn limits_to_json(limits: &Limits) -> Value {
    json!({
        "max_input_bytes": limits.max_document_bytes,
        "max_sections": limits.max_sections,
        "max_section_payload_bytes": limits.max_section_payload_bytes,
        "max_decoded_logical_bytes": limits.max_decoded_logical_bytes,
        "max_blob_display_bytes": limits.max_blob_display_bytes,
        "max_json_output_bytes": limits.max_json_output_bytes,
        "trust_policy": match limits.trust_policy {
            TrustPolicy::Public => "public",
            TrustPolicy::Trusted => "trusted",
        },
    })
}

fn lumba_file_from_values(
    values: &[PortableValue],
    input: &PathBuf,
    text: &str,
    include_source: bool,
) -> Result<luma::lumba::LumbaFile, CliError> {
    let mut file = luma::lumba::LumbaFile::new();
    for value in values {
        file = file.with_document(Document::new().with_root_value(
            LumbaValue::try_from(value).map_err(|error| CliError {
                message: format!(
                    "failed to encode portable value: {}",
                    error.to_diagnostic().message
                ),
            })?,
        ));
    }

    if include_source {
        let mut blobs = BlobTable::new();
        let blob_id = blobs
            .push(
                BlobRecord::new(text.as_bytes().to_vec())
                    .with_flags(BLOB_FLAG_UTF8_TEXT | BLOB_FLAG_SOURCE_TEXT),
            )
            .map_err(|error| CliError {
                message: format!(
                    "failed to add source blob: {}",
                    error.to_diagnostic().message
                ),
            })?;
        let source_files = SourceFileTable::new().with_record(
            SourceFileRecord::new()
                .with_display(Some(input.display().to_string()))
                .with_uri(Some(format!(
                    "file://{}",
                    input.display().to_string().replace('\\', "/")
                )))
                .with_source_blob_ref(Some(blob_id)),
        );
        file = file
            .with_blob_table(blobs)
            .with_source_file_table(source_files);
    }

    Ok(file)
}

fn collect_root_values(file: &luma::lumba::LumbaFile) -> Vec<LumbaValue> {
    file.documents
        .iter()
        .filter_map(|document| document.root_value.clone())
        .collect()
}

fn inspect_header_payload(bytes: &[u8]) -> luma::lumba::Result<(&'static str, Value)> {
    let header = ContainerHeader::decode(bytes, HeaderCrcMode::Enabled)?;
    Ok((
        "header",
        json!({
            "major_version": 0,
            "minor_version": 1,
            "container_flags": header.container_flags,
            "profile_flags": header.profile_flags,
            "section_table_offset": header.section_table_offset,
            "section_count": header.section_count,
            "section_entry_size": header.section_entry_size,
            "file_length": header.file_length,
            "root_document_count": header.root_document_count,
            "header_crc32c": header.header_crc32c,
        }),
    ))
}

fn inspect_sections_payload(bytes: &[u8]) -> luma::lumba::Result<(&'static str, Value)> {
    let header = ContainerHeader::decode(bytes, HeaderCrcMode::Enabled)?;
    let sections = validate_section_table(&header, bytes)?;
    Ok((
        "sections",
        Value::Array(
            sections
                .iter()
                .map(|section| {
                    let mut value = json!({
                        "id": section.entry.section_id.as_str(),
                        "version": section.entry.section_version,
                        "entry_flags": section.entry.entry_flags,
                        "payload_flags": section.entry.payload_flags,
                        "codec_id": section.entry.codec_id,
                        "checksum_id": section.entry.checksum_id,
                        "payload_offset": section.entry.payload_offset,
                        "stored_size": section.entry.stored_size,
                        "logical_size": section.entry.logical_size,
                        "item_count": section.entry.item_count,
                        "checksum_low": section.entry.checksum_low,
                        "checksum_high": section.entry.checksum_high,
                    });
                    if section.entry.section_id == luma::lumba::section::SectionId::DIAG {
                        value["diagnostic_count"] = json!(section.entry.item_count);
                    }
                    value
                })
                .collect(),
        ),
    ))
}

fn inspect_values_payload(
    bytes: &[u8],
    limits: &Limits,
) -> luma::lumba::Result<(&'static str, Value)> {
    let file = Reader::new(read_options(limits)).read(bytes)?;
    Ok((
        "values",
        Value::Array(
            collect_root_values(&file)
                .iter()
                .map(|value| lumba_value_to_json(value, limits))
                .collect(),
        ),
    ))
}

fn inspect_resources_payload(
    bytes: &[u8],
    limits: &Limits,
) -> luma::lumba::Result<(&'static str, Value)> {
    let file = Reader::new(read_options(limits)).read(bytes)?;
    let blob_table = file.blob_table.clone().unwrap_or_default();
    let source_file_table = file.source_file_table.clone().unwrap_or_default();
    let dependency_table = file.dependency_table.clone().unwrap_or_default();
    let embedded_resource_table = file.embedded_resource_table.clone().unwrap_or_default();
    Ok((
        "resources",
        json!({
            "blob_count": blob_table.records.len(),
            "blobs": blob_table.records.iter().enumerate().map(|(index, blob)| json!({
                "id": index,
                "flags": blob.flags(),
                "byte_length": blob.len(),
                "preview": blob_preview(blob.as_bytes(), limits),
            })).collect::<Vec<_>>(),
            "source_files": source_file_table.records.iter().enumerate().map(|(index, record)| json!({
                "id": index,
                "flags": record.flags,
                "uri": truncate_text(record.uri.as_deref().unwrap_or(""), limits.max_blob_display_bytes),
                "display": truncate_text(record.display.as_deref().unwrap_or(""), limits.max_blob_display_bytes),
                "source_blob_ref": record.source_blob_ref.map(|id| id.0),
                "private": record.is_private(),
            })).collect::<Vec<_>>(),
            "dependencies": dependency_table.records.iter().enumerate().map(|(index, record)| json!({
                "id": index,
                "kind": record.kind,
                "flags": record.flags,
                "uri": record.uri,
                "trusted_only": record.is_trusted_only(),
                "required": record.is_required(),
                "resolved": record.is_resolved(),
            })).collect::<Vec<_>>(),
            "embedded_resources": embedded_resource_table.records.iter().enumerate().map(|(index, record)| json!({
                "id": index,
                "dependency_ref": record.dependency_ref,
                "kind": record.kind,
                "flags": record.flags,
                "blob_ref": record.blob_ref.0,
                "extension_kind": record.extension_kind.as_ref().map(Identifier::as_str),
            })).collect::<Vec<_>>(),
        }),
    ))
}

fn inspect_capabilities_payload(
    bytes: &[u8],
    limits: &Limits,
) -> luma::lumba::Result<(&'static str, Value)> {
    let file = Reader::new(read_options(limits)).read(bytes)?;
    let capability_table = file.capability_table.unwrap_or_else(CapabilityTable::new);
    Ok((
        "capabilities",
        json!({
            "count": capability_table.records.len(),
            "rows": capability_table.inspection_rows(),
            "sets": capability_table.records.iter().enumerate().map(|(index, record)| json!({
                "id": index,
                "flags": record.flags,
                "summary": record.inspection_summary(),
                "requirements": record.requirements.iter().map(|requirement| json!({
                    "capability": requirement.capability.as_str(),
                    "flags": requirement.flags,
                    "trusted_only": requirement.is_trusted_only(),
                    "required_for_evaluation": requirement.is_required_for_evaluation(),
                    "required_for_reproduction": requirement.is_required_for_reproduction(),
                    "may_read_external": requirement.may_read_external(),
                    "may_write_external": requirement.may_write_external(),
                    "label": requirement.inspection_label(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    ))
}

fn bounded_payload(label: &'static str, payload: Value, limits: &Limits) -> (&'static str, Value) {
    let rendered = serde_json::to_vec(&payload).unwrap_or_default();
    if rendered.len() <= limits.max_json_output_bytes {
        return (label, payload);
    }

    (
        label,
        json!({
            "summary": "output exceeded configured JSON output limit",
            "rendered_bytes": rendered.len(),
            "max_json_output_bytes": limits.max_json_output_bytes,
            "truncated": true,
        }),
    )
}

fn render_decoded_values(values: &[LumbaValue], limits: &Limits) -> Result<String, CliError> {
    let mut out = String::new();
    for (index, value) in values.iter().enumerate() {
        let rendered = match PortableValue::try_from(value) {
            Ok(portable) => serialize_value(&portable).map_err(|diagnostic| CliError {
                message: format!("failed to serialize decoded value: {}", diagnostic.message),
            })?,
            Err(_) => serde_json::to_string_pretty(&lumba_value_to_json(value, limits)).map_err(
                |error| CliError {
                    message: format!("failed to render decoded value as json: {error}"),
                },
            )?,
        };
        if index > 0 {
            out.push_str("\n---\n");
        }
        if out.len() + rendered.len() + 1 > limits.max_json_output_bytes {
            out.push_str("[output truncated by configured limit]\n");
            return Ok(out);
        }
        out.push_str(&rendered);
        out.push('\n');
    }
    Ok(out)
}

fn read_text(path: &PathBuf, limits: &Limits) -> Result<String, CliError> {
    let bytes = read_bytes(path, limits)?;
    String::from_utf8(bytes).map_err(|error| CliError {
        message: format!("failed to decode '{}' as UTF-8: {error}", path.display()),
    })
}

fn read_bytes(path: &PathBuf, limits: &Limits) -> Result<Vec<u8>, CliError> {
    let metadata = fs::metadata(path).map_err(|error| CliError {
        message: format!("failed to read '{}': {error}", path.display()),
    })?;
    if let Ok(len) = usize::try_from(metadata.len()) {
        if len > limits.max_document_bytes {
            return Err(CliError {
                message: format!(
                    "failed to read '{}': input exceeds configured maximum of {} bytes",
                    path.display(),
                    limits.max_document_bytes
                ),
            });
        }
    }

    let mut file = File::open(path).map_err(|error| CliError {
        message: format!("failed to read '{}': {error}", path.display()),
    })?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(limits.max_document_bytes)
            .min(limits.max_document_bytes),
    );
    file.take((limits.max_document_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| CliError {
            message: format!("failed to read '{}': {error}", path.display()),
        })?;
    if bytes.len() > limits.max_document_bytes {
        return Err(CliError {
            message: format!(
                "failed to read '{}': input exceeds configured maximum of {} bytes",
                path.display(),
                limits.max_document_bytes
            ),
        });
    }
    Ok(bytes)
}

fn blob_preview(bytes: &[u8], limits: &Limits) -> Value {
    let preview_len = bytes.len().min(limits.max_blob_display_bytes);
    if let Ok(text) = std::str::from_utf8(&bytes[..preview_len]) {
        json!({
            "kind": "utf8",
            "text": truncate_text(text, limits.max_blob_display_bytes),
            "truncated": bytes.len() > preview_len,
        })
    } else {
        let hex = bytes[..preview_len]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        json!({
            "kind": "hex",
            "text": hex,
            "truncated": bytes.len() > preview_len,
        })
    }
}

fn truncate_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn lumba_error_report(command: &'static str, error: luma::lumba::LumbaError) -> CommandReport {
    CommandReport {
        command,
        source: None,
        diagnostics: vec![lumba_error_to_diagnostic(&error)],
        payload: None,
        human_text: None,
        success: None,
    }
}

fn lumba_error_to_diagnostic(error: &luma::lumba::LumbaError) -> Diagnostic {
    let lumba = error.to_diagnostic();
    let mut diagnostic = Diagnostic::new(DiagnosticCode::SerializationError, Severity::Error);
    diagnostic.message = format!("{}: {}", lumba.code.as_str(), lumba.message);
    diagnostic.notes = vec![format!("class: {:?}", lumba.class)];
    diagnostic
}

fn lumba_verifier_diagnostic_to_diagnostic(diagnostic: &luma::lumba::Diagnostic) -> Diagnostic {
    let mut converted = Diagnostic::new(DiagnosticCode::SerializationError, Severity::Error);
    converted.message = format!("{}: {}", diagnostic.code.as_str(), diagnostic.message);
    converted.notes = vec![format!("class: {:?}", diagnostic.class)];
    converted
}

fn lumba_value_to_json(value: &LumbaValue, limits: &Limits) -> Value {
    match value {
        LumbaValue::Null => Value::Null,
        LumbaValue::Bool(value) => Value::Bool(*value),
        LumbaValue::Int(value) => json!({"type":"integer","value":value}),
        LumbaValue::UInt(value) => json!({"type":"unsigned","value":value}),
        LumbaValue::Float(value) => json!({"type":"float","value":value.get()}),
        LumbaValue::Decimal(value) => json!({"type":"decimal","value":value.as_str()}),
        LumbaValue::String(value) => {
            Value::String(truncate_text(value, limits.max_blob_display_bytes))
        }
        LumbaValue::BytesInline(bytes) => json!({
            "type":"bytes_inline",
            "byte_length": bytes.len(),
            "preview": blob_preview(bytes, limits),
        }),
        LumbaValue::BytesBlob(blob_id) => json!({"type":"bytes_blob","blob_id":blob_id.0}),
        LumbaValue::Sequence(items) => json!({
            "type":"sequence",
            "items": items.iter().map(|value| lumba_value_to_json(value, limits)).collect::<Vec<_>>()
        }),
        LumbaValue::Map(entries) => json!({
            "type":"mapping",
            "entries": entries.iter().map(|entry| json!({
                "key": lumba_value_to_json(&entry.key, limits),
                "value": lumba_value_to_json(&entry.value, limits),
            })).collect::<Vec<_>>()
        }),
        LumbaValue::Tagged(tagged) => json!({
            "type":"tagged",
            "tag": tagged.tag.as_str(),
            "value": lumba_value_to_json(tagged.value.as_ref(), limits),
        }),
        LumbaValue::ExpressionSource(value) => json!({
            "type":"expression_source",
            "language": value.language.as_str(),
            "source": match &value.source {
                luma::lumba::ExpressionSource::Text(text) => json!({"kind":"text","text": truncate_text(text, limits.max_blob_display_bytes)}),
                luma::lumba::ExpressionSource::Blob(blob_id) => json!({"kind":"blob","blob_id": blob_id.0}),
            },
            "capability_set_ref": value.capability_set_ref,
            "result_value": value.result_value.as_deref().map(|value| lumba_value_to_json(value, limits)),
        }),
        LumbaValue::LuaChunkSource(value) => json!({
            "type":"lua_chunk_source",
            "language": value.language.as_str(),
            "source_blob_ref": value.source_blob_ref.0,
            "capability_set_ref": value.capability_set_ref,
            "result_value": value.result_value.as_deref().map(|value| lumba_value_to_json(value, limits)),
        }),
        LumbaValue::RuntimeDescriptor(value) => json!({
            "type":"runtime_descriptor",
            "kind": value.kind.as_str(),
            "required": value.required,
            "trusted_only": value.trusted_only,
            "capability_set_ref": value.capability_set_ref,
            "descriptor_value": value.descriptor_value.as_deref().map(|value| lumba_value_to_json(value, limits)),
            "fallback_value": value.fallback_value.as_deref().map(|value| lumba_value_to_json(value, limits)),
        }),
        LumbaValue::ExtensionValue(value) => json!({
            "type":"extension_value",
            "extension_name": value.extension_name,
            "type_name": value.type_name.as_str(),
            "payload_blob_ref": value.payload_blob_ref.0,
            "fallback_value": value.fallback_value.as_deref().map(|value| lumba_value_to_json(value, limits)),
        }),
    }
}

fn documents_to_values(
    documents: &[SyntaxDocument],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<PortableValue>> {
    let mut values = Vec::with_capacity(documents.len());
    for document in documents {
        match document_to_value(document) {
            Ok(value) => values.push(value),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity != Severity::Error)
        .then_some(values)
}

fn document_to_value(document: &SyntaxDocument) -> Result<PortableValue, Diagnostic> {
    let mut root = None;
    for item in &document.items {
        match item {
            DocumentItem::Comment(_) => {}
            DocumentItem::Root(node) => {
                if root.is_some() {
                    return Err(unsupported_syntax(
                        node_span(node),
                        "lumba encode only supports one static root value per document",
                    ));
                }
                root = Some(node_to_portable_value(node)?);
            }
            DocumentItem::Directive(directive) => {
                return Err(unsupported_syntax(
                    directive_span(directive),
                    "lumba encode rejects directives; only static value documents are supported",
                ));
            }
            DocumentItem::Let(binding) => {
                return Err(unsupported_syntax(
                    binding.span,
                    "lumba encode rejects let bindings; only static value documents are supported",
                ));
            }
        }
    }

    root.ok_or_else(|| {
        unsupported_syntax(
            document.span,
            "lumba encode requires each document to contain an explicit static root value",
        )
    })
}

fn node_to_portable_value(node: &LumaNode) -> Result<PortableValue, Diagnostic> {
    match node {
        LumaNode::Null { .. } => Ok(PortableValue::Null(LumaNull)),
        LumaNode::Boolean { value, .. } => Ok(PortableValue::Boolean(*value)),
        LumaNode::Number(number) => parse_portable_number(&number.lexeme, number.span),
        LumaNode::String(string) => Ok(PortableValue::String(string.value.clone())),
        LumaNode::Sequence(sequence) => sequence_to_portable_value(sequence),
        LumaNode::Mapping(mapping) => mapping_to_portable_value(mapping),
        LumaNode::Tagged(tagged) => Ok(PortableValue::Tagged(LumaTaggedValue {
            tag: LumaTag {
                name: LumaTagName {
                    value: tagged.tag.name.value.clone(),
                    span: tagged.tag.name.span,
                },
                span: tagged.tag.span,
            },
            value: Box::new(match tagged.value.as_deref() {
                Some(value) => node_to_portable_value(value)?,
                None => PortableValue::Null(LumaNull),
            }),
            span: Some(tagged.span),
        })),
        LumaNode::LuaExpression(expr)
        | LumaNode::LuaExpressionBlock(expr)
        | LumaNode::LuaChunk(expr)
        | LumaNode::LuaTableConstructor(expr) => Err(unsupported_syntax(
            expr.span,
            "lumba encode rejects expressions and runtime constructs; evaluate first or use static values only",
        )),
    }
}

fn sequence_to_portable_value(sequence: &SequenceBlock) -> Result<PortableValue, Diagnostic> {
    let mut items = Vec::with_capacity(sequence.items.len());
    for item in &sequence.items {
        match item {
            SequenceItem::Value(value) => items.push(node_to_portable_value(value)?),
            SequenceItem::Comment(_) => {}
            SequenceItem::Spread(spread) => {
                return Err(unsupported_syntax(
                    spread.span,
                    "lumba encode rejects sequence spreads; only static values are supported",
                ));
            }
            SequenceItem::Directive(directive) => {
                return Err(unsupported_syntax(
                    directive_span(directive),
                    "lumba encode rejects directives inside sequences",
                ));
            }
            SequenceItem::Conditional(block) => {
                return Err(unsupported_syntax(
                    block.span,
                    "lumba encode rejects conditionals; only static values are supported",
                ));
            }
            SequenceItem::Loop(block) => {
                return Err(unsupported_syntax(
                    block.span,
                    "lumba encode rejects loops; only static values are supported",
                ));
            }
        }
    }

    Ok(PortableValue::Sequence(LumaSequence {
        items,
        span: Some(sequence.span),
    }))
}

fn mapping_to_portable_value(mapping: &MappingBlock) -> Result<PortableValue, Diagnostic> {
    if let Some(duplicate) = mapping.duplicate_keys.first() {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::DuplicateKey, Severity::Error);
        diagnostic.message = format!(
            "duplicate mapping key '{}' is not portable for lumba encode",
            duplicate.key
        );
        diagnostic.primary_span = Some(duplicate.duplicate_span);
        diagnostic
            .related_spans
            .push(luma_syntax::RelatedDiagnosticSpan {
                span: duplicate.first_span,
                message: "first key appeared here".to_owned(),
            });
        return Err(diagnostic);
    }

    let mut entries = Vec::new();
    for item in &mapping.items {
        match item {
            MappingItem::Entry(entry) => entries.push(LumaMappingEntry {
                key: mapping_key_to_portable_key(&entry.key)?,
                value: node_to_portable_value(&entry.value)?,
                span: Some(entry.span),
            }),
            MappingItem::Comment(_) => {}
            MappingItem::Spread(spread) => {
                return Err(unsupported_syntax(
                    spread.span,
                    "lumba encode rejects mapping spreads; only static values are supported",
                ));
            }
            MappingItem::Directive(directive) => {
                return Err(unsupported_syntax(
                    directive_span(directive),
                    "lumba encode rejects directives inside mappings",
                ));
            }
            MappingItem::Conditional(block) => {
                return Err(unsupported_syntax(
                    block.span,
                    "lumba encode rejects conditionals; only static values are supported",
                ));
            }
            MappingItem::Loop(block) => {
                return Err(unsupported_syntax(
                    block.span,
                    "lumba encode rejects loops; only static values are supported",
                ));
            }
            MappingItem::Let(binding) => {
                return Err(unsupported_syntax(
                    binding.span,
                    "lumba encode rejects let bindings inside mappings",
                ));
            }
        }
    }

    Ok(PortableValue::Mapping(LumaMapping {
        entries,
        duplicate_keys: Vec::new(),
        span: Some(mapping.span),
    }))
}

fn mapping_key_to_portable_key(key: &MappingKey) -> Result<LumaKey, Diagnostic> {
    match key {
        MappingKey::Plain { value, .. } => Ok(LumaKey::String(value.clone())),
        MappingKey::Quoted(value) => Ok(LumaKey::String(value.value.clone())),
        MappingKey::Expression { span, .. } => Err(unsupported_syntax(
            *span,
            "lumba encode rejects expression keys; mapping keys must be static strings",
        )),
    }
}

fn parse_portable_number(lexeme: &str, span: Span) -> Result<PortableValue, Diagnostic> {
    if let Ok(value) = lexeme.parse::<i64>() {
        return Ok(PortableValue::Number(LumaNumber::Integer(value)));
    }

    let value = lexeme.parse::<f64>().map_err(|_| {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::SerializationError, Severity::Error);
        diagnostic.message = format!("failed to parse numeric literal '{lexeme}' for lumba encode");
        diagnostic.primary_span = Some(span);
        diagnostic
    })?;
    if !value.is_finite() {
        let mut diagnostic = Diagnostic::new(DiagnosticCode::SerializationError, Severity::Error);
        diagnostic.message =
            format!("non-finite numeric literal '{lexeme}' is not portable for lumba encode");
        diagnostic.primary_span = Some(span);
        return Err(diagnostic);
    }

    Ok(PortableValue::Number(LumaNumber::Float(value)))
}

fn unsupported_syntax(span: Span, message: impl Into<String>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(DiagnosticCode::UnsafeOperation, Severity::Error);
    diagnostic.message = message.into();
    diagnostic.primary_span = Some(span);
    diagnostic
}

fn node_span(node: &LumaNode) -> Span {
    match node {
        LumaNode::Null { span } | LumaNode::Boolean { span, .. } => *span,
        LumaNode::Number(number) => number.span,
        LumaNode::String(string) => string.span,
        LumaNode::Sequence(sequence) => sequence.span,
        LumaNode::Mapping(mapping) => mapping.span,
        LumaNode::Tagged(tagged) => tagged.span,
        LumaNode::LuaExpression(expr)
        | LumaNode::LuaExpressionBlock(expr)
        | LumaNode::LuaChunk(expr)
        | LumaNode::LuaTableConstructor(expr) => expr.span,
    }
}

fn directive_span(directive: &Directive) -> Span {
    match directive {
        Directive::Version(value) => value.span,
        Directive::Profile(value) => value.span,
        Directive::Schema(value) => value.span,
        Directive::Import(value) => value.span,
        Directive::Include(value) => value.span,
        Directive::Use(value) => value.span,
        Directive::LuaPrelude(value) => value.span,
        Directive::Meta(value) => value.span,
    }
}
