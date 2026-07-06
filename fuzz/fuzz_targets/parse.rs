#![no_main]

use libfuzzer_sys::fuzz_target;
use lyma_parser::{FileId, decode_bytes, lex_source, parse_source};

fuzz_target!(|data: &[u8]| {
    let Ok(source) = decode_bytes(FileId(1), "fuzz.lyma", data) else {
        return;
    };

    let lexed = lex_source(source.clone());
    let parsed = parse_source(source);

    let _ = lexed.tokens.len();
    let _ = lexed.diagnostics.len();
    let _ = lexed.indents.len();
    let _ = parsed.file.documents.len();
    let _ = parsed.diagnostics.len();
});
