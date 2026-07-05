#![no_main]

use libfuzzer_sys::fuzz_target;
use luma_lumba::{Limits, ReadOptions, Reader};

const MAX_INPUT_BYTES: usize = 8 * 1024;

fn fuzz_limits() -> Limits {
    let mut limits = Limits::strict();
    limits.max_document_bytes = MAX_INPUT_BYTES;
    limits.max_section_payload_bytes = 4 * 1024;
    limits.max_decoded_logical_bytes = 4 * 1024;
    limits.max_string_bytes = 512;
    limits.max_table_record_count = 128;
    limits.max_string_count = 256;
    limits.max_value_count = 256;
    limits.max_document_count = 64;
    limits.max_syntax_node_count = 256;
    limits.max_resource_count = 64;
    limits
}

fn decode_once(data: &[u8]) -> Result<Option<&'static str>, &'static str> {
    let reader = Reader::new(ReadOptions::new().with_limits(fuzz_limits()));
    match reader.read(data) {
        Ok(file) => {
            let _ = file.documents.len();
            let _ = file.sections.len();
            let _ = file.blob_table.as_ref().map(|table| table.len()).unwrap_or(0);
            Ok(None)
        }
        Err(error) => Err(error.code().as_str()),
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }

    let first = std::panic::catch_unwind(|| decode_once(data));
    assert!(first.is_ok(), "lumba_read panicked on {} bytes", data.len());

    let second = std::panic::catch_unwind(|| decode_once(data));
    assert!(second.is_ok(), "lumba_read panicked on deterministic replay");

    assert_eq!(first.unwrap(), second.unwrap(), "decoder error classification must be deterministic");
});
