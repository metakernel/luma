//! Editor-style helpers: whole-document format edits and portable values.
//!
//! Run with:
//! `cargo run --example tooling`

use luma::LumaValue;
use luma::tooling::{format_document_text_edit, serialize_portable_value};

fn main() {
    let source = "name:'api'\nenabled:true\n";
    let (formatting, edit) = format_document_text_edit("service.luma", source);

    assert!(formatting.parsed.diagnostics.is_empty());
    println!("replace bytes {}..{}", edit.range.start, edit.range.end);
    println!("replacement text:\n{}", edit.text);

    let value = LumaValue::Boolean(true);
    let serialized = serialize_portable_value(&value).expect("portable value serializes");
    println!("portable value:\n{}", serialized);
}
