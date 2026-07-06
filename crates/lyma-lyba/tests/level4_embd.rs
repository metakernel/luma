//! Integration tests for Level 4 embedded resource support.

use lyma_lyba::container::{ContainerHeader, HeaderCrcMode};
use lyma_lyba::primitives::Identifier;
use lyma_lyba::section::SectionEntry;
use lyma_lyba::{
    BLOB_FLAG_LUA_SOURCE, BLOB_FLAG_SOURCE_TEXT, BLOB_FLAG_UTF8_TEXT, BlobId, BlobRecord,
    BlobTable, DEPENDENCY_FLAG_EMBEDDED, DEPENDENCY_KIND_EXTENSION,
    DEPENDENCY_KIND_EXTERNAL_RESOURCE, DEPENDENCY_KIND_GENERATED, DEPENDENCY_KIND_IMPORT,
    DependencyRecord, DependencyTable, EMBEDDED_RESOURCE_KIND_BYTES,
    EMBEDDED_RESOURCE_KIND_EXTENSION, EMBEDDED_RESOURCE_KIND_LUA_SOURCE,
    EMBEDDED_RESOURCE_KIND_LYMA_TEXT, EMBEDDED_RESOURCE_KIND_LYBA_CONTAINER,
    EMBEDDED_RESOURCE_KIND_SCHEMA_LYMA, EmbeddedResourceRecord, EmbeddedResourceTable, Limits,
    LybaError, LybaFile, ReadOptions, Reader, WriteOptions, Writer,
};

#[test]
fn embedded_resources_round_trip_text_binary_lazy_access_and_inert_lua() {
    let file = fixture_file();

    let decoded = Reader::new(ReadOptions::new())
        .read(
            &Writer::new(WriteOptions::new())
                .write(&file)
                .expect("write should succeed"),
        )
        .expect("read should succeed");

    let blob_table = decoded.blob_table.as_ref().expect("BLOB should decode");
    let resources = &decoded
        .embedded_resource_table
        .as_ref()
        .expect("EMBD should decode")
        .records;

    assert_eq!(resources.len(), 6);
    assert_eq!(resources[0].kind, EMBEDDED_RESOURCE_KIND_LYMA_TEXT);
    assert_eq!(resources[1].kind, EMBEDDED_RESOURCE_KIND_LYBA_CONTAINER);
    assert_eq!(resources[2].kind, EMBEDDED_RESOURCE_KIND_SCHEMA_LYMA);
    assert_eq!(resources[3].kind, EMBEDDED_RESOURCE_KIND_LUA_SOURCE);
    assert_eq!(resources[4].kind, EMBEDDED_RESOURCE_KIND_BYTES);
    assert_eq!(resources[5].kind, EMBEDDED_RESOURCE_KIND_EXTENSION);
    assert_eq!(
        resources[5].extension_kind.as_ref().map(Identifier::as_str),
        Some("com.example.asset")
    );

    assert_eq!(
        resources[0]
            .utf8_text(blob_table)
            .expect("text should decode"),
        Some("answer: 42\n")
    );
    assert_eq!(
        resources[2]
            .utf8_text(blob_table)
            .expect("schema text should decode"),
        Some("schema: service\n")
    );
    assert!(resources[3].is_lua_source());
    assert_eq!(
        resources[3]
            .utf8_text(blob_table)
            .expect("lua text should decode"),
        Some("print('never run')")
    );
    assert_eq!(
        resources[4]
            .utf8_text(blob_table)
            .expect("bytes access should succeed"),
        None
    );
    assert_eq!(
        resources[4]
            .blob(blob_table)
            .expect("blob lookup should be lazy and on-demand")
            .as_bytes(),
        &[0x00, 0xFF, 0x10, 0x80]
    );
}

#[test]
fn writer_rejects_invalid_embedded_dependency_and_blob_refs_with_lb0014() {
    let dependency_error = Writer::new(WriteOptions::new())
        .write(&fixture_file().with_embedded_resource_table(
            EmbeddedResourceTable::new().with_record(EmbeddedResourceRecord::new(
                99,
                EMBEDDED_RESOURCE_KIND_LYMA_TEXT,
                BlobId(0),
            )),
        ))
        .expect_err("invalid dependency ref should fail");
    assert!(matches!(
        dependency_error,
        LybaError::InvalidValueReference(_)
    ));
    assert_eq!(dependency_error.code().as_str(), "LB0014");

    let blob_error = Writer::new(WriteOptions::new())
        .write(&fixture_file().with_embedded_resource_table(
            EmbeddedResourceTable::new().with_record(EmbeddedResourceRecord::new(
                0,
                EMBEDDED_RESOURCE_KIND_BYTES,
                BlobId(99),
            )),
        ))
        .expect_err("invalid blob ref should fail");
    assert!(matches!(blob_error, LybaError::InvalidValueReference(_)));
    assert_eq!(blob_error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_invalid_embedded_dependency_and_blob_refs_with_lb0014() {
    let mut bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file())
        .expect("write should succeed");
    patch_embd_payload(&mut bytes, &[1, 99, 0, 0, 0, 0]);
    let dependency_error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid dependency ref should fail");
    assert!(matches!(
        dependency_error,
        LybaError::InvalidValueReference(_)
    ));
    assert_eq!(dependency_error.code().as_str(), "LB0014");

    let mut bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file())
        .expect("write should succeed");
    patch_embd_payload(&mut bytes, &[1, 0, 0, 0, 99, 0]);
    let blob_error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("invalid blob ref should fail");
    assert!(matches!(blob_error, LybaError::InvalidValueReference(_)));
    assert_eq!(blob_error.code().as_str(), "LB0014");
}

#[test]
fn reader_rejects_embedded_resource_count_over_limit_with_lb0018() {
    let bytes = Writer::new(WriteOptions::new())
        .write(&fixture_file())
        .expect("write should succeed");
    let mut limits = Limits::public();
    limits.max_resource_count = 1;

    let error = Reader::new(ReadOptions::new().with_limits(limits))
        .read(&bytes)
        .expect_err("resource limit should fail");

    assert!(matches!(error, LybaError::ResourceLimitExceeded(_)));
    assert_eq!(error.code().as_str(), "LB0018");
}

fn fixture_file() -> LybaFile {
    let nested_container = Writer::new(WriteOptions::new())
        .write(&LybaFile::new())
        .expect("nested container should encode");
    let mut blob_table = BlobTable::new();
    blob_table
        .push(
            BlobRecord::new(b"answer: 42\n".to_vec())
                .with_flags(BLOB_FLAG_UTF8_TEXT | BLOB_FLAG_SOURCE_TEXT),
        )
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(nested_container))
        .expect("blob append should succeed");
    blob_table
        .push(
            BlobRecord::new(b"schema: service\n".to_vec())
                .with_flags(BLOB_FLAG_UTF8_TEXT | BLOB_FLAG_SOURCE_TEXT),
        )
        .expect("blob append should succeed");
    blob_table
        .push(
            BlobRecord::new(b"print('never run')".to_vec())
                .with_flags(BLOB_FLAG_UTF8_TEXT | BLOB_FLAG_SOURCE_TEXT | BLOB_FLAG_LUA_SOURCE),
        )
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(vec![0x00, 0xFF, 0x10, 0x80]))
        .expect("blob append should succeed");
    blob_table
        .push(BlobRecord::new(b"extension payload".to_vec()))
        .expect("blob append should succeed");

    LybaFile::new()
        .with_blob_table(blob_table)
        .with_dependency_table(
            DependencyTable::new()
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_IMPORT)
                        .with_uri(Some(String::from("mem://text.lyma")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_GENERATED)
                        .with_uri(Some(String::from("mem://nested.lyba")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_IMPORT)
                        .with_uri(Some(String::from("mem://schema.lyma")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                        .with_uri(Some(String::from("mem://script.lua")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_EXTERNAL_RESOURCE)
                        .with_uri(Some(String::from("mem://asset.bin")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                )
                .with_record(
                    DependencyRecord::new(DEPENDENCY_KIND_EXTENSION)
                        .with_uri(Some(String::from("ext://com.example/asset")))
                        .with_flags(DEPENDENCY_FLAG_EMBEDDED),
                ),
        )
        .with_embedded_resource_table(
            EmbeddedResourceTable::new()
                .with_record(EmbeddedResourceRecord::new(
                    0,
                    EMBEDDED_RESOURCE_KIND_LYMA_TEXT,
                    BlobId(0),
                ))
                .with_record(EmbeddedResourceRecord::new(
                    1,
                    EMBEDDED_RESOURCE_KIND_LYBA_CONTAINER,
                    BlobId(1),
                ))
                .with_record(EmbeddedResourceRecord::new(
                    2,
                    EMBEDDED_RESOURCE_KIND_SCHEMA_LYMA,
                    BlobId(2),
                ))
                .with_record(EmbeddedResourceRecord::new(
                    3,
                    EMBEDDED_RESOURCE_KIND_LUA_SOURCE,
                    BlobId(3),
                ))
                .with_record(EmbeddedResourceRecord::new(
                    4,
                    EMBEDDED_RESOURCE_KIND_BYTES,
                    BlobId(4),
                ))
                .with_record(
                    EmbeddedResourceRecord::new(5, EMBEDDED_RESOURCE_KIND_EXTENSION, BlobId(5))
                        .with_extension_kind(Some(Identifier::new("com.example.asset"))),
                ),
        )
}

fn patch_embd_payload(bytes: &mut [u8], payload: &[u8]) {
    let header =
        ContainerHeader::decode(bytes, HeaderCrcMode::Enabled).expect("header should decode");
    for index in 0..header.section_count as usize {
        let start = 64 + index * 64;
        let end = start + 64;
        let entry = SectionEntry::decode(&bytes[start..end]).expect("entry should decode");
        if entry.section_id == lyma_lyba::section::SectionId::EMBD {
            let payload_offset = entry.payload_offset as usize;
            let payload_end = payload_offset + payload.len();
            bytes[payload_offset..payload_end].copy_from_slice(payload);
            return;
        }
    }
    panic!("EMBD section not found");
}
