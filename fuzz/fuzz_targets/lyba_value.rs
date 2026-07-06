#![no_main]

use libfuzzer_sys::fuzz_target;
use lyma_lyba::{Limits, ReadOptions, Reader};
use lyma_lyba::container::{ContainerHeader, HeaderCrcMode, SECTION_ENTRY_SIZE};
use lyma_lyba::section::{CHECKSUM_NONE, CODEC_NONE, SectionEntry, SectionId};

const MAX_VALS_PAYLOAD_BYTES: usize = 4 * 1024;

fn fuzz_limits() -> Limits {
    let mut limits = Limits::strict();
    limits.max_document_bytes = 8 * 1024;
    limits.max_section_payload_bytes = MAX_VALS_PAYLOAD_BYTES;
    limits.max_decoded_logical_bytes = MAX_VALS_PAYLOAD_BYTES;
    limits.max_string_bytes = 512;
    limits.max_table_record_count = 128;
    limits.max_value_count = 256;
    limits.max_document_count = 8;
    limits.max_nesting_depth = 8;
    limits
}

fn wrap_vals_payload(payload: &[u8]) -> Vec<u8> {
    let header_len = 64_usize;
    let table_len = SECTION_ENTRY_SIZE as usize;
    let payload_offset = (header_len + table_len) as u64;
    let padding = (8 - (payload.len() % 8)) % 8;
    let file_len = header_len + table_len + payload.len() + padding;

    let header = ContainerHeader {
        container_flags: 0,
        profile_flags: 0,
        section_table_offset: header_len as u64,
        section_count: 1,
        section_entry_size: SECTION_ENTRY_SIZE,
        file_length: file_len as u64,
        root_document_count: 0,
        header_crc32c: 0,
    };
    let entry = SectionEntry {
        section_id: SectionId::VALS,
        section_version: 1,
        entry_flags: 0x0003,
        payload_flags: 0,
        codec_id: CODEC_NONE,
        checksum_id: CHECKSUM_NONE,
        payload_offset,
        stored_size: payload.len() as u64,
        logical_size: payload.len() as u64,
        item_count: 0,
        checksum_low: 0,
        checksum_high: 0,
    };

    let mut bytes = Vec::with_capacity(file_len);
    bytes.extend_from_slice(&header.encode(HeaderCrcMode::Disabled).expect("valid header"));
    bytes.extend_from_slice(&entry.encode().expect("valid entry"));
    bytes.extend_from_slice(payload);
    bytes.resize(file_len, 0);
    bytes
}

fn decode_once(payload: &[u8]) -> Result<Option<&'static str>, &'static str> {
    let bytes = wrap_vals_payload(payload);
    let reader = Reader::new(ReadOptions::new().with_limits(fuzz_limits()));
    match reader.read(&bytes) {
        Ok(file) => {
            let _ = file.sections.len();
            let _ = file.documents.len();
            Ok(None)
        }
        Err(error) => Err(error.code().as_str()),
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_VALS_PAYLOAD_BYTES {
        return;
    }

    let first = std::panic::catch_unwind(|| decode_once(data));
    assert!(first.is_ok(), "lyba_value panicked on {} bytes", data.len());

    let second = std::panic::catch_unwind(|| decode_once(data));
    assert!(second.is_ok(), "lyba_value panicked on deterministic replay");

    assert_eq!(first.unwrap(), second.unwrap(), "value decoder error classification must be deterministic");
});
