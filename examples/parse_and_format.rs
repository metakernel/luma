//! Parse and format Luma source without evaluating Lua.
//!
//! Run with:
//! `cargo run --example parse_and_format`

use luma::parser::{FileId, format_str, parse_str};

fn main() {
    let source = "name:'api'\nenabled:true\nports:\n  - 8080\n  - 9090\n";

    let parsed = parse_str(FileId(1), "service.luma", source);
    if !parsed.diagnostics.is_empty() {
        eprintln!("parse diagnostics: {:#?}", parsed.diagnostics);
        std::process::exit(1);
    }

    println!("documents parsed: {}", parsed.file.documents.len());

    let formatted = format_str(FileId(1), "service.luma", source);
    println!("changed by formatter: {}", formatted.formatted.changed);
    println!("\n{}", formatted.formatted.text);
}
