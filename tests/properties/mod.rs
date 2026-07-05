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
