//! CLI smoke tests.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
#[cfg(feature = "lumba")]
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn parse_json_emits_ast_without_engine_feature() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "name: Example\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "parse",
            path.to_str().unwrap(),
            "--output",
            "json",
            "--emit",
            "ast",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\":true"))
        .stdout(predicate::str::contains("\"ast\""));
}

#[test]
fn default_mode_checks_without_evaluation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "value: |expr\n  os.getenv('X')\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .arg(path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[cfg(not(feature = "engine-omnilua"))]
#[test]
fn eval_reports_missing_engine_when_backend_not_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "value: |expr\n  1 + 1\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args(["eval", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("engine-omnilua"));
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_cli_covers_all_subcommands_and_modes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("sample.luma");
    fs::write(&input, "name: Example\nitems:\n  - 1\n  - true\n").unwrap();

    for mode in ["value", "runtime-data", "editor-cache", "bundle", "fixture"] {
        let output = if mode == "value" {
            dir.path().join("value.lumba")
        } else {
            dir.path().join(format!("{mode}.lumba"))
        };
        let encode = Command::cargo_bin("luma")
            .unwrap()
            .args([
                "--output",
                "json",
                "lumba",
                "encode",
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                "--mode",
                mode,
                "--footer",
                "--checksum",
                "crc32c",
                "--include-source",
            ])
            .output()
            .unwrap();
        assert!(
            encode.status.success(),
            "encode failed for {mode}: {:?}",
            encode
        );
        let encode_json = parse_json(&encode.stdout);
        assert_eq!(encode_json["command"], "lumba");
        assert_eq!(encode_json["ok"], true);
        assert_eq!(encode_json["result"]["mode"], expected_mode_label(mode));
        for emit in ["header", "sections"] {
            let inspect = Command::cargo_bin("luma")
                .unwrap()
                .args([
                    "--output",
                    "json",
                    "lumba",
                    "inspect",
                    output.to_str().unwrap(),
                    "--emit",
                    emit,
                ])
                .output()
                .unwrap();
            let inspect_json = parse_json(&inspect.stdout);
            assert_eq!(inspect_json["command"], "lumba");
            assert!(
                inspect.status.success(),
                "inspect failed for {mode}/{emit}: {:?}",
                inspect
            );
            assert_eq!(inspect_json["ok"], true);
            assert!(
                inspect_json.get(emit).is_some(),
                "missing {emit} payload for {mode}"
            );
        }
    }

    let decode = Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "decode",
            dir.path().join("value.lumba").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(decode.status.success(), "decode failed: {:?}", decode);
    let decode_json = parse_json(&decode.stdout);
    assert_eq!(decode_json["command"], "lumba");
    assert_eq!(decode_json["ok"], true);
    assert!(decode_json.get("values").is_some());

    for emit in ["values", "resources", "capabilities"] {
        let inspect = Command::cargo_bin("luma")
            .unwrap()
            .args([
                "--output",
                "json",
                "lumba",
                "inspect",
                dir.path().join("value.lumba").to_str().unwrap(),
                "--emit",
                emit,
            ])
            .output()
            .unwrap();
        assert!(
            inspect.status.success(),
            "inspect failed for value/{emit}: {:?}",
            inspect
        );
        let inspect_json = parse_json(&inspect.stdout);
        assert_eq!(inspect_json["command"], "lumba");
        assert_eq!(inspect_json["ok"], true);
        assert!(
            inspect_json.get(emit).is_some(),
            "missing {emit} payload for value mode"
        );
    }

    let verify = Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "verify",
            dir.path().join("value.lumba").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(verify.status.success(), "verify failed: {:?}", verify);
    let verify_json = parse_json(&verify.stdout);
    assert_eq!(verify_json["command"], "lumba");
    assert_eq!(verify_json["ok"], true);
    assert!(verify_json.get("verification").is_some());
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_encode_rejects_runtime_syntax_without_evaluation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("sample.luma");
    let output = dir.path().join("sample.lumba");
    fs::write(&input, "value: =1 + 1\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "encode",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"command\":\"lumba\""))
        .stdout(predicate::str::contains("\"ok\":false"))
        .stdout(predicate::str::contains("E0019"));
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_inspect_sections_displays_diagnostic_counts() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("diag.lumba");
    let bytes = luma::lumba::Writer::new(luma::lumba::WriteOptions::new())
        .write(&luma::lumba::LumbaFile::new().with_diagnostic_table(
            luma::lumba::DiagnosticTable::new().with_record(luma::lumba::DiagnosticRecord::new(
                luma::lumba::StoredDiagnosticSeverity::Warning,
                "E0003",
                "tab used for indentation",
            )),
        ))
        .unwrap();
    fs::write(&input, bytes).unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "inspect",
            input.to_str().unwrap(),
            "--emit",
            "sections",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"id\":\"DIAG\""))
        .stdout(predicate::str::contains("\"diagnostic_count\":1"));
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_inspect_trusted_policy_accepts_trusted_only_inputs_without_execution() {
    use luma::lumba::primitives::Identifier;
    use luma::lumba::{
        CAPABILITY_FLAG_TRUSTED_ONLY, CapabilityRequirement, CapabilitySetRecord, CapabilityTable,
        Document, LumbaFile, RuntimeDescriptorValue, Value as LumbaValue, WriteOptions, Writer,
    };

    let dir = tempdir().unwrap();
    let input = dir.path().join("trusted.lumba");
    let bytes = Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_document(
                    Document::new()
                        .with_root_value(LumbaValue::RuntimeDescriptor(RuntimeDescriptorValue {
                            kind: Identifier::new("module.symbol"),
                            required: false,
                            trusted_only: true,
                            capability_set_ref: Some(0),
                            descriptor_value: Some(Box::new(LumbaValue::String(String::from(
                                "internal.mod",
                            )))),
                            fallback_value: Some(Box::new(LumbaValue::Null)),
                        }))
                        .with_capability_set_ref(Some(0)),
                )
                .with_capability_table(
                    CapabilityTable::new().with_record(
                        CapabilitySetRecord::new().with_requirement(
                            CapabilityRequirement::new(Identifier::new("module.resolve"))
                                .with_flags(CAPABILITY_FLAG_TRUSTED_ONLY),
                        ),
                    ),
                ),
        )
        .unwrap();
    fs::write(&input, bytes).unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "inspect",
            input.to_str().unwrap(),
            "--emit",
            "capabilities",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("LB0019"))
        .stdout(predicate::str::contains("\"ok\":false"));

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "inspect",
            input.to_str().unwrap(),
            "--emit",
            "capabilities",
            "--trusted",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"capabilities\""))
        .stdout(predicate::str::contains("module.resolve"))
        .stdout(predicate::str::contains("trusted_only"));
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_decode_bounds_oversized_output_deterministically() {
    use luma::lumba::{Document, LumbaFile, Value as LumbaValue, WriteOptions, Writer};

    let dir = tempdir().unwrap();
    let input = dir.path().join("large.lumba");
    let payload = (0..70_000)
        .map(|index| LumbaValue::Int(index.into()))
        .collect::<Vec<_>>();
    let bytes = Writer::new(WriteOptions::new())
        .write(
            &LumbaFile::new()
                .with_document(Document::new().with_root_value(LumbaValue::Sequence(payload))),
        )
        .unwrap();
    fs::write(&input, bytes).unwrap();

    let output = Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "decode",
            input.to_str().unwrap(),
            "--limits",
            "strict",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "decode should fail deterministically when strict limits are exceeded: {:?}",
        output
    );
    let json = parse_json(&output.stdout);
    assert_eq!(json["ok"], false);
    assert!(output.stdout.windows(6).any(|window| window == b"LB0018"));
}

#[cfg(feature = "lumba")]
#[test]
fn lumba_decode_rejects_oversized_input_before_full_read() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("too-large.lumba");
    fs::write(&input, vec![0_u8; 4 * 1024 * 1024 + 1]).unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lumba",
            "decode",
            input.to_str().unwrap(),
            "--limits",
            "strict",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("input exceeds configured maximum"))
        .stdout(predicate::str::contains("\"ok\":false"));
}

#[cfg(feature = "lumba")]
fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap()
}

#[cfg(feature = "lumba")]
fn expected_mode_label(mode: &str) -> &'static str {
    match mode {
        "value" => "value",
        "runtime-data" => "runtime-data",
        "editor-cache" => "editor-cache",
        "bundle" => "bundle",
        "fixture" => "fixture",
        _ => unreachable!(),
    }
}
