//! Evaluate Luma source with the optional OmniLua backend.
//!
//! Run with:
//! `cargo run --example loader_omnilua --features omnilua`

#[cfg(feature = "omnilua")]
fn main() {
    use luma::parser::FileId;
    use luma::runtime::RuntimeLimits;
    use luma::{Loader, OmniLuaEngine, Parser, Profile};

    let source = "answer: =40 + 2\nmessage: ='hello from lua'\n";
    let parsed = Parser::new().parse_str(FileId(1), "computed.luma", source);
    if !parsed.diagnostics.is_empty() {
        eprintln!("parse diagnostics: {:#?}", parsed.diagnostics);
        std::process::exit(1);
    }

    let engine = OmniLuaEngine::default();
    let profile = Profile::permissive(RuntimeLimits::unbounded());
    let documents = Loader::new(&engine)
        .profile(&profile)
        .load_file(&parsed.file, "computed.luma", None)
        .expect("evaluation succeeds");

    println!("evaluated documents: {:#?}", documents);
}

#[cfg(not(feature = "omnilua"))]
fn main() {
    println!("enable evaluation with: cargo run --example loader_omnilua --features omnilua");
}
