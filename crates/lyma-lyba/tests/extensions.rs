//! Integration tests for extension declarations.

use lyma_lyba::{
    EXTENSION_FLAG_REQUIRED, ExtensionDeclaration, ExtensionNamePolicy, ExtensionTable, Limits,
    LybaError, LybaFile, ReadOptions, Reader, Value, WriteOptions, Writer,
};

#[test]
fn unsupported_required_extension_fails_with_lb0009() {
    let file = LybaFile::new().with_extension_table(
        ExtensionTable::new().with_declaration(
            ExtensionDeclaration::new("org.lyma.lua.bytecode.lua54", "1.0")
                .with_flags(EXTENSION_FLAG_REQUIRED),
        ),
    );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode EXTS");
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("unsupported required extension should fail");

    assert!(matches!(error, LybaError::UnsupportedRequiredExtension(_)));
    assert_eq!(error.code().as_str(), "LB0009");
}

#[test]
fn optional_extension_is_preserved_for_inspection() {
    let file = LybaFile::new().with_extension_table(
        ExtensionTable::new().with_declaration(
            ExtensionDeclaration::new("bad_name", "0.1")
                .with_may_contain_code(true)
                .with_may_resolve_external(true)
                .with_affects_canonical(true)
                .with_metadata_value(Some(Value::String(String::from("meta")))),
        ),
    );

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode optional extension");
    let decoded = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect("optional extension should be preserved");

    let extension = &decoded
        .extension_table
        .expect("EXTS should decode")
        .declarations[0];
    assert_eq!(extension.name, "bad_name");
    assert_eq!(extension.version, "0.1");
    assert!(extension.affects_canonical());
    assert!(extension.may_contain_code());
    assert!(extension.may_resolve_external());
    assert_eq!(
        extension.metadata_value,
        Some(Value::String(String::from("meta")))
    );
    assert!(extension.reverse_dns_warning);
}

#[test]
fn trusted_only_extension_fails_under_public_policy() {
    let file = LybaFile::new().with_extension_table(ExtensionTable::new().with_declaration(
        ExtensionDeclaration::new("com.example.trusted", "1").with_trusted_only(true),
    ));

    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode trusted-only extension");
    let error = Reader::new(ReadOptions::new())
        .read(&bytes)
        .expect_err("public policy should reject trusted-only extensions");

    assert!(matches!(error, LybaError::TrustedOnlyRejected(_)));
    assert_eq!(error.code().as_str(), "LB0019");
}

#[test]
fn reverse_dns_policy_can_reject_invalid_extension_names() {
    let file = LybaFile::new().with_extension_table(
        ExtensionTable::new().with_declaration(ExtensionDeclaration::new("bad_name", "1")),
    );
    let bytes = Writer::new(WriteOptions::new())
        .write(&file)
        .expect("writer should encode invalid name for policy test");
    let mut limits = Limits::public();
    limits.extension_name_policy = ExtensionNamePolicy::Reject;
    let error = Reader::new(ReadOptions::new().with_limits(limits))
        .read(&bytes)
        .expect_err("reject policy should fail invalid names");

    assert_eq!(error.code().as_str(), "LB0021");
}
