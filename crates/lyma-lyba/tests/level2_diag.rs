//! Integration tests for Level 2 stored diagnostics.

use lyma_lyba::container::{ContainerHeader, HEADER_SIZE, HeaderCrcMode, SECTION_ENTRY_SIZE};
use lyma_lyba::primitives::UVar;
use lyma_lyba::section::{
    CHECKSUM_NONE, CODEC_NONE, SECTION_FLAG_REQUIRED, SECTION_FLAG_UNIQUE, SectionEntry, SectionId,
};
use lyma_lyba::symbol::{SymbolRecord, SymbolTable, encode_symbol_table};
use lyma_lyba::{
    DiagnosticLoadPolicy, DiagnosticRecord, DiagnosticTable, LybaError, LybaFile, ReadOptions,
    Reader, RelatedDiagnosticRecord, StoredDiagnosticSeverity, WriteOptions, Writer,
};
use lyma_parser::{Diagnostic, DiagnosticCode, FileId, Severity, parse_str};
use lyma_syntax::{RelatedDiagnosticSpan, Span};

fn build_file(entries: &[SectionEntry], payloads: &[&[u8]], table_offset: u64) -> Vec<u8> {
    let table = entries
        .iter()
        .flat_map(|entry| entry.encode().expect("entry should encode"))
        .collect::<Vec<_>>();
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
    bytes
}

fn entry(
    section_id: SectionId,
    payload_offset: u64,
    payload_len: usize,
    item_count: u64,
) -> SectionEntry {
    SectionEntry {
        section_id,
        section_version: 1,
        entry_flags: SECTION_FLAG_REQUIRED | SECTION_FLAG_UNIQUE,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset,
        stored_size: payload_len as u64,
        logical_size: payload_len as u64,
        item_count,
        checksum_low: 0,
        checksum_high: 0,
    }
}

fn strs_payload(values: &[&str]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(values.len() as u64).encode_into(&mut payload);
    for value in values {
        UVar(0).encode_into(&mut payload);
        UVar(value.len() as u64).encode_into(&mut payload);
        payload.extend_from_slice(value.as_bytes());
    }
    payload
}

fn srcf_payload(record_count: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(record_count).encode_into(&mut payload);
    for _ in 0..record_count {
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
        UVar(0).encode_into(&mut payload);
    }
    payload
}

fn srcs_payload(records: &[(u64, u64, u64, u64, u64, u64, u64, u64)]) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(records.len() as u64).encode_into(&mut payload);
    for (source_file_ref, byte_offset, byte_length, sl, sc, el, ec, flags) in records {
        UVar(*source_file_ref).encode_into(&mut payload);
        UVar(*byte_offset).encode_into(&mut payload);
        UVar(*byte_length).encode_into(&mut payload);
        UVar(*sl).encode_into(&mut payload);
        UVar(*sc).encode_into(&mut payload);
        UVar(*el).encode_into(&mut payload);
        UVar(*ec).encode_into(&mut payload);
        UVar(*flags).encode_into(&mut payload);
    }
    payload
}

fn diag_payload(primary_span_ref_raw: u64, related_span_refs_raw: &[u64], flags: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    UVar(1).encode_into(&mut payload);
    UVar(StoredDiagnosticSeverity::Error.as_u64()).encode_into(&mut payload);
    UVar(0).encode_into(&mut payload);
    UVar(1).encode_into(&mut payload);
    UVar(primary_span_ref_raw).encode_into(&mut payload);
    UVar(related_span_refs_raw.len() as u64).encode_into(&mut payload);
    for span_ref in related_span_refs_raw {
        UVar(*span_ref).encode_into(&mut payload);
        UVar(2).encode_into(&mut payload);
    }
    UVar(flags).encode_into(&mut payload);
    payload
}

#[test]
fn parse_warning_fixture_round_trips() {
    let mut parsed = parse_str(FileId(7), "warning.lyma", "\tname: value\n");
    assert!(!parsed.diagnostics.is_empty());
    parsed.diagnostics[0].severity = Severity::Warning;

    let record = DiagnosticRecord::from_lyma_syntax(&parsed.diagnostics[0]);
    let file = LybaFile::new().with_diagnostic_table(DiagnosticTable::new().with_record(record));

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("writer should encode DIAG"),
        )
        .expect("reader should decode DIAG");

    let record = &decoded
        .diagnostic_table
        .expect("DIAG should decode")
        .records[0];
    assert_eq!(record.severity, StoredDiagnosticSeverity::Warning);
    assert_eq!(
        record.code_symbol.as_str(),
        DiagnosticCode::TabUsedForIndentation.code()
    );
    assert_eq!(record.message, parsed.diagnostics[0].message);
}

#[test]
fn error_bearing_load_is_accepted_or_rejected_by_policy() {
    let file = LybaFile::new().with_diagnostic_table(DiagnosticTable::new().with_record(
        DiagnosticRecord::new(
            StoredDiagnosticSeverity::Error,
            "E0012",
            "fixture parse error",
        ),
    ));
    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode error-bearing DIAG");

    assert!(Reader::new(ReadOptions::new()).read(&bytes).is_ok());

    assert!(
        Reader::new(ReadOptions::new().with_diagnostic_policy(DiagnosticLoadPolicy::Allow))
            .read(&bytes)
            .is_ok()
    );

    let error =
        Reader::new(ReadOptions::new().with_diagnostic_policy(DiagnosticLoadPolicy::RejectErrors))
            .read(&bytes)
            .expect_err("RejectErrors policy should fail error-bearing DIAG");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));

    let warning_file = LybaFile::new().with_diagnostic_table(DiagnosticTable::new().with_record(
        DiagnosticRecord::new(
            StoredDiagnosticSeverity::Warning,
            "E0003",
            "tab used for indentation",
        ),
    ));
    let warning_bytes = Writer::new(WriteOptions::new())
        .write(&warning_file)
        .expect("writer should encode warning-bearing DIAG");

    let warning_error = Reader::new(
        ReadOptions::new().with_diagnostic_policy(DiagnosticLoadPolicy::RejectWarnings),
    )
    .read(&warning_bytes)
    .expect_err("RejectWarnings policy should fail warning-bearing DIAG");

    assert!(matches!(warning_error, LybaError::TrustedOnlyRejected(_)));
}

#[test]
fn reader_accepts_related_spans_when_srcs_count_covers_refs() {
    let strs = strs_payload(&["E0012", "fixture parse error", "first note"]);
    let syms = encode_symbol_table(&SymbolTable::new().with_symbol(SymbolRecord::new(0)))
        .expect("symbol table should encode");
    let diag = diag_payload(1, &[2], 0);
    let srcf = srcf_payload(1);
    let srcs = srcs_payload(&[(0, 0, 1, 1, 1, 1, 2, 0), (0, 1, 1, 1, 2, 1, 3, 0)]);

    let bytes = build_file(
        &[
            entry(SectionId::STRS, 384, strs.len(), 3),
            entry(SectionId::SYMS, 448, syms.len(), 1),
            entry(SectionId::DIAG, 512, diag.len(), 1),
            entry(SectionId::SRCF, 576, srcf.len(), 1),
            entry(SectionId::SRCS, 640, srcs.len(), 2),
        ],
        &[&strs, &syms, &diag, &srcf, &srcs],
        64,
    );

    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("SRCS-backed span refs should decode");
    let record = &decoded
        .diagnostic_table
        .expect("DIAG should decode")
        .records[0];

    assert_eq!(record.primary_span_ref, Some(0));
    assert_eq!(record.related_spans.len(), 1);
    assert_eq!(record.related_spans[0].span_ref, Some(1));
    assert_eq!(record.related_spans[0].message, "first note");
}

#[test]
fn conversion_helpers_preserve_related_spans_when_mapping_is_available() {
    let primary = Span::new(FileId(3), 1, 4);
    let related = Span::new(FileId(3), 8, 12);
    let mut diagnostic = Diagnostic::new(DiagnosticCode::DuplicateKey, Severity::Error);
    diagnostic.message = "duplicate key".to_owned();
    diagnostic.primary_span = Some(primary);
    diagnostic.related_spans.push(RelatedDiagnosticSpan {
        span: related,
        message: "first key appeared here".to_owned(),
    });

    let stored = DiagnosticRecord::from_lyma_syntax_with_span_encoder(&diagnostic, |span| {
        if span == primary {
            Some(0)
        } else if span == related {
            Some(1)
        } else {
            None
        }
    })
    .with_related_span(RelatedDiagnosticRecord::new("second note"));

    assert_eq!(stored.primary_span_ref, Some(0));
    assert_eq!(stored.related_spans[0].span_ref, Some(1));

    let round_tripped = stored
        .to_lyma_syntax_with_span_resolver(|span_ref| match span_ref {
            0 => Some(primary),
            1 => Some(related),
            _ => None,
        })
        .expect("known E-code should convert back");

    assert_eq!(round_tripped.code, DiagnosticCode::DuplicateKey);
    assert_eq!(round_tripped.primary_span, Some(primary));
    assert_eq!(round_tripped.related_spans.len(), 1);
    assert_eq!(round_tripped.related_spans[0].span, related);
    assert_eq!(
        round_tripped.related_spans[0].message,
        "first key appeared here"
    );
}

#[test]
fn reader_rejects_invalid_span_refs_with_lb0022() {
    let strs = strs_payload(&["E0012", "fixture parse error", "first note"]);
    let syms = encode_symbol_table(&SymbolTable::new().with_symbol(SymbolRecord::new(0)))
        .expect("symbol table should encode");
    let diag = diag_payload(1, &[1], 0);

    let bytes = build_file(
        &[
            entry(SectionId::STRS, 256, strs.len(), 3),
            entry(SectionId::SYMS, 320, syms.len(), 1),
            entry(SectionId::DIAG, 384, diag.len(), 1),
        ],
        &[&strs, &syms, &diag],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("non-zero SRCS-less span ref should fail");

    assert!(matches!(error, LybaError::InvalidSourceSpan(_)));
    assert_eq!(error.code().as_str(), "LB0022");
}

#[test]
fn diag_reserved_flags_are_rejected_with_lb0025_on_read() {
    let strs = strs_payload(&["E0012", "fixture parse error"]);
    let syms = encode_symbol_table(&SymbolTable::new().with_symbol(SymbolRecord::new(0)))
        .expect("symbol table should encode");
    let diag = diag_payload(0, &[], 1);

    let bytes = build_file(
        &[
            entry(SectionId::STRS, 256, strs.len(), 2),
            entry(SectionId::SYMS, 320, syms.len(), 1),
            entry(SectionId::DIAG, 384, diag.len(), 1),
        ],
        &[&strs, &syms, &diag],
        64,
    );

    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("non-zero DIAG flags should fail");

    assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}

#[test]
fn diag_reserved_flags_are_rejected_with_lb0025_on_write() {
    let file = LybaFile::new().with_diagnostic_table(
        DiagnosticTable::new().with_record(
            DiagnosticRecord::new(
                StoredDiagnosticSeverity::Error,
                "E0012",
                "fixture parse error",
            )
            .with_flags(1),
        ),
    );

    let error = Writer::new(WriteOptions::new())
        .write(&file)
        .expect_err("non-zero DIAG flags should fail");

    assert!(matches!(error, LybaError::InvalidReservedFlags(_)));
    assert_eq!(error.code().as_str(), "LB0025");
}
