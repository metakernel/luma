use luma::parser::{FileId as ParserFileId, FormatRangeFallback, FormatRangeOptions};
use luma::tooling::{
    TextRange, apply_text_edits, format_document_range_text_edits, format_document_text_edit,
    format_document_text_edits,
};
use luma::tooling::{format_document_edit, serialize_portable_value};
use luma_parser::{FileId, parse_str};
use luma_syntax::{
    LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber, LumaSequence, LumaValue,
};

#[test]
fn formatter_is_idempotent_for_generated_portable_documents() {
    let mut generator = Generator::new(0x5eed_cafe_d00d_f00d);

    for case in 0..256 {
        let value = generator.value(0);
        let source = serialize_portable_value(&value).expect("generated value should serialize");

        let first = format_document_edit("generated.luma", &source);
        assert!(
            first.parsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            first.parsed.diagnostics
        );

        let second = format_document_edit("generated.luma", &first.formatted.text);
        assert!(
            second.parsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            second.parsed.diagnostics
        );
        assert_eq!(
            first.formatted.text, second.formatted.text,
            "formatter should be idempotent for case {case}"
        );
    }
}

#[test]
fn parse_and_serialize_stay_stable_for_generated_values() {
    let mut generator = Generator::new(0x0123_4567_89ab_cdef);

    for case in 0..256 {
        let value = generator.value(0);
        let serialized =
            serialize_portable_value(&value).expect("generated value should serialize");
        let parsed = parse_str(FileId(1), "stable.luma", &serialized);
        assert!(
            parsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            parsed.diagnostics
        );

        let formatted = format_document_edit("stable.luma", &serialized);
        assert!(
            formatted.parsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            formatted.parsed.diagnostics
        );
        assert_eq!(
            serialized, formatted.formatted.text,
            "canonical serialization should remain stable for case {case}"
        );

        let reparsed = parse_str(FileId(2), "stable.luma", &formatted.formatted.text);
        assert!(
            reparsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            reparsed.diagnostics
        );
    }
}

#[test]
fn editor_formatting_edits_match_canonical_output_for_generated_documents() {
    let mut generator = Generator::new(0xface_feed_dead_beef);

    for case in 0..256 {
        let value = generator.value(0);
        let canonical = serialize_portable_value(&value).expect("generated value should serialize");
        let source = perturb_editor_source(&canonical, case);

        let formatted = format_document_edit("editor-generated.luma", &source);
        assert!(
            formatted.parsed.diagnostics.is_empty(),
            "case {case}: {:#?}",
            formatted.parsed.diagnostics
        );

        let (_, whole_edit) = format_document_text_edit("editor-generated.luma", &source);
        assert_eq!(
            apply_text_edits(&source, &[whole_edit]),
            Some(formatted.formatted.text.clone()),
            "whole-document edit mismatch for case {case}"
        );

        let (_, minimal_edits) = format_document_text_edits("editor-generated.luma", &source);
        assert_eq!(
            apply_text_edits(&source, &minimal_edits),
            Some(formatted.formatted.text.clone()),
            "minimal edits mismatch for case {case}"
        );

        let normalized_source = formatted.parsed.source.as_str();
        let (_, range_edits) = format_document_range_text_edits(
            "editor-generated.luma",
            normalized_source,
            TextRange::new(0, normalized_source.len()),
            FormatRangeOptions {
                fallback: FormatRangeFallback::Reject,
                ..FormatRangeOptions::default()
            },
        )
        .expect("full-range formatting should succeed");
        assert_eq!(
            apply_text_edits(normalized_source, &range_edits),
            Some(formatted.formatted.text),
            "range edits mismatch for case {case}"
        );
    }
}

#[test]
fn editor_parser_range_formatting_matches_tooling_for_generated_documents() {
    let mut generator = Generator::new(0x0ddc_0ffe_e15e_babe);

    for case in 0..256 {
        let value = generator.value(0);
        let canonical = serialize_portable_value(&value).expect("generated value should serialize");
        let source = perturb_editor_source(&canonical, case + 17);
        let normalized = format_document_edit("editor-range.luma", &source)
            .parsed
            .source;
        let normalized_source = normalized.as_str();
        let range = TextRange::new(0, normalized_source.len());
        let options = FormatRangeOptions {
            fallback: FormatRangeFallback::Reject,
            ..FormatRangeOptions::default()
        };

        let (_, tooling_edits) = format_document_range_text_edits(
            "editor-range.luma",
            normalized_source,
            range,
            options,
        )
        .expect("tooling range formatting should succeed");
        let (parser_formatted, parser_edits) = luma::parser::format_range_edits(
            ParserFileId(42),
            "editor-range.luma",
            normalized_source,
            range,
            options,
        )
        .expect("parser range formatting should succeed");

        assert_eq!(
            tooling_edits, parser_edits,
            "tooling/parser edits diverged for case {case}"
        );
        assert_eq!(
            apply_text_edits(normalized_source, &parser_edits),
            Some(parser_formatted.formatted.text),
            "parser edits did not reconstruct canonical output for case {case}"
        );
    }
}

fn perturb_editor_source(canonical: &str, case: usize) -> String {
    let mut source = canonical.replace('\n', "\r\n");
    if case % 2 == 0 {
        source = source.replace(": ", ":");
    }
    if case % 3 == 0 {
        source = source.replace("- ", "-   ");
    }
    if case % 5 == 0 {
        source = source.replace("\r\n", "  \r\n");
    }
    source
}

struct Generator {
    state: u64,
}

impl Generator {
    const MAX_DEPTH: usize = 3;
    const MAX_ITEMS: usize = 4;

    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    fn string(&mut self) -> String {
        match self.next_usize(6) {
            0 => format!("plain{}", self.next_usize(10_000)),
            1 => format!("quoted value {}", self.next_usize(10_000)),
            2 => format!("dash--{}", self.next_usize(100)),
            3 => format!("trim {} ", self.next_usize(100)),
            4 => format!("@tagged-{}", self.next_usize(100)),
            _ => format!("emoji-{}-🙂", self.next_usize(100)),
        }
    }

    fn value(&mut self, depth: usize) -> LumaValue {
        if depth >= Self::MAX_DEPTH {
            return self.leaf_value();
        }

        match self.next_usize(7) {
            0..=3 => self.leaf_value(),
            4 => {
                let len = self.next_usize(Self::MAX_ITEMS).saturating_add(1);
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.value(depth + 1));
                }
                LumaValue::Sequence(LumaSequence { items, span: None })
            }
            _ => {
                let len = self.next_usize(Self::MAX_ITEMS).saturating_add(1);
                let mut entries = Vec::with_capacity(len);
                for index in 0..len {
                    entries.push(LumaMappingEntry {
                        key: LumaKey::String(format!(
                            "key-{depth}-{index}-{}",
                            self.next_usize(100)
                        )),
                        value: self.value(depth + 1),
                        span: None,
                    });
                }
                LumaValue::Mapping(LumaMapping {
                    entries,
                    duplicate_keys: Vec::new(),
                    span: None,
                })
            }
        }
    }

    fn leaf_value(&mut self) -> LumaValue {
        match self.next_usize(5) {
            0 => LumaValue::Null(LumaNull),
            1 => LumaValue::Boolean(self.next_bool()),
            2 => LumaValue::Number(LumaNumber::Integer(self.next_usize(2_048) as i64 - 1_024)),
            3 => LumaValue::Number(LumaNumber::Float((self.next_usize(10_000) as f64) / 10.0)),
            _ => LumaValue::String(self.string()),
        }
    }
}
