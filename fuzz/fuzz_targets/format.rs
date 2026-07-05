#![no_main]

use libfuzzer_sys::fuzz_target;
use luma::tooling::{format_document_edit, serialize_portable_value};
use luma_syntax::{
    LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber, LumaSequence, LumaValue,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let formatted = format_document_edit("fuzz.luma", source);
        let _ = formatted.parsed.diagnostics.len();
        let _ = formatted.formatted.text.len();
    }

    let value = value_from_bytes(data, 0, 0).0;
    if let Ok(serialized) = serialize_portable_value(&value) {
        let formatted = format_document_edit("value.luma", &serialized);
        let _ = formatted.parsed.diagnostics.len();
        let _ = formatted.formatted.text.len();
    }
});

fn value_from_bytes(data: &[u8], mut index: usize, depth: usize) -> (LumaValue, usize) {
    if depth >= 3 || index >= data.len() {
        return (LumaValue::Null(LumaNull), index);
    }

    let tag = data[index] % 6;
    index += 1;
    match tag {
        0 => (LumaValue::Null(LumaNull), index),
        1 => (LumaValue::Boolean(data.get(index).is_some_and(|byte| byte % 2 == 1)), index + 1),
        2 => {
            let number = i64::from(*data.get(index).unwrap_or(&0)) - 64;
            (LumaValue::Number(LumaNumber::Integer(number)), index + 1)
        }
        3 => {
            let len = usize::from(*data.get(index).unwrap_or(&0) % 8);
            index += 1;
            let end = index.saturating_add(len).min(data.len());
            let text = String::from_utf8_lossy(&data[index..end]).into_owned();
            (LumaValue::String(text), end)
        }
        4 => {
            let len = usize::from(*data.get(index).unwrap_or(&0) % 4);
            index += 1;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                let (item, next) = value_from_bytes(data, index, depth + 1);
                index = next;
                items.push(item);
            }
            (
                LumaValue::Sequence(LumaSequence { items, span: None }),
                index,
            )
        }
        _ => {
            let len = usize::from(*data.get(index).unwrap_or(&0) % 4);
            index += 1;
            let mut entries = Vec::with_capacity(len);
            for entry_index in 0..len {
                let key = format!("k{entry_index}");
                let (value, next) = value_from_bytes(data, index, depth + 1);
                index = next;
                entries.push(LumaMappingEntry {
                    key: LumaKey::String(key),
                    value,
                    span: None,
                });
            }
            (
                LumaValue::Mapping(LumaMapping {
                    entries,
                    duplicate_keys: Vec::new(),
                    span: None,
                }),
                index,
            )
        }
    }
}
