//! CLI smoke tests.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
#[cfg(feature = "lyba")]
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn parse_json_emits_ast_without_engine_feature() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.lyma");
    fs::write(&path, "name: Example\n").unwrap();

    Command::cargo_bin("lyma")
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
    let path = dir.path().join("sample.lyma");
    fs::write(&path, "value: |expr\n  os.getenv('X')\n").unwrap();

    Command::cargo_bin("lyma")
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
    let path = dir.path().join("sample.lyma");
    fs::write(&path, "value: |expr\n  1 + 1\n").unwrap();

    Command::cargo_bin("lyma")
        .unwrap()
        .args(["eval", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("engine-omnilua"));
}

#[cfg(feature = "lyba")]
#[test]
fn lyba_cli_covers_all_subcommands_and_modes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("sample.lyma");
    fs::write(&input, "name: Example\nitems:\n  - 1\n  - true\n").unwrap();

    for mode in ["value", "runtime-data", "editor-cache", "bundle", "fixture"] {
        let output = if mode == "value" {
            dir.path().join("value.lyba")
        } else {
            dir.path().join(format!("{mode}.lyba"))
        };
        let encode = Command::cargo_bin("lyma")
            .unwrap()
            .args([
                "--output",
                "json",
                "lyba",
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
        assert_eq!(encode_json["command"], "lyba");
        assert_eq!(encode_json["ok"], true);
        assert_eq!(encode_json["result"]["mode"], expected_mode_label(mode));
        for emit in ["header", "sections"] {
            let inspect = Command::cargo_bin("lyma")
                .unwrap()
                .args([
                    "--output",
                    "json",
                    "lyba",
                    "inspect",
                    output.to_str().unwrap(),
                    "--emit",
                    emit,
                ])
                .output()
                .unwrap();
            let inspect_json = parse_json(&inspect.stdout);
            assert_eq!(inspect_json["command"], "lyba");
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

    let decode = Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
            "decode",
            dir.path().join("value.lyba").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(decode.status.success(), "decode failed: {:?}", decode);
    let decode_json = parse_json(&decode.stdout);
    assert_eq!(decode_json["command"], "lyba");
    assert_eq!(decode_json["ok"], true);
    assert!(decode_json.get("values").is_some());

    for emit in ["values", "resources", "capabilities"] {
        let inspect = Command::cargo_bin("lyma")
            .unwrap()
            .args([
                "--output",
                "json",
                "lyba",
                "inspect",
                dir.path().join("value.lyba").to_str().unwrap(),
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
        assert_eq!(inspect_json["command"], "lyba");
        assert_eq!(inspect_json["ok"], true);
        assert!(
            inspect_json.get(emit).is_some(),
            "missing {emit} payload for value mode"
        );
    }

    let verify = Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
            "verify",
            dir.path().join("value.lyba").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(verify.status.success(), "verify failed: {:?}", verify);
    let verify_json = parse_json(&verify.stdout);
    assert_eq!(verify_json["command"], "lyba");
    assert_eq!(verify_json["ok"], true);
    assert!(verify_json.get("verification").is_some());
}

#[cfg(feature = "lyba")]
#[test]
fn lyba_encode_rejects_runtime_syntax_without_evaluation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("sample.lyma");
    let output = dir.path().join("sample.lyba");
    fs::write(&input, "value: =1 + 1\n").unwrap();

    Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
            "encode",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"command\":\"lyba\""))
        .stdout(predicate::str::contains("\"ok\":false"))
        .stdout(predicate::str::contains("E0019"));
}

#[cfg(feature = "lyba")]
#[test]
fn lyba_inspect_sections_displays_diagnostic_counts() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("diag.lyba");
    let bytes = lyma::lyba::Writer::new(lyma::lyba::WriteOptions::new())
        .write(&lyma::lyba::LybaFile::new().with_diagnostic_table(
            lyma::lyba::DiagnosticTable::new().with_record(lyma::lyba::DiagnosticRecord::new(
                lyma::lyba::StoredDiagnosticSeverity::Warning,
                "E0003",
                "tab used for indentation",
            )),
        ))
        .unwrap();
    fs::write(&input, bytes).unwrap();

    Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
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

#[cfg(feature = "lyba")]
#[test]
fn lyba_inspect_trusted_policy_accepts_trusted_only_inputs_without_execution() {
    use lyma::lyba::primitives::Identifier;
    use lyma::lyba::{
        CAPABILITY_FLAG_TRUSTED_ONLY, CapabilityRequirement, CapabilitySetRecord, CapabilityTable,
        Document, LybaFile, RuntimeDescriptorValue, Value as LybaValue, WriteOptions, Writer,
    };

    let dir = tempdir().unwrap();
    let input = dir.path().join("trusted.lyba");
    let bytes = Writer::new(WriteOptions::new())
        .write(
            &LybaFile::new()
                .with_document(
                    Document::new()
                        .with_root_value(LybaValue::RuntimeDescriptor(RuntimeDescriptorValue {
                            kind: Identifier::new("module.symbol"),
                            required: false,
                            trusted_only: true,
                            capability_set_ref: Some(0),
                            descriptor_value: Some(Box::new(LybaValue::String(String::from(
                                "internal.mod",
                            )))),
                            fallback_value: Some(Box::new(LybaValue::Null)),
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

    Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
            "inspect",
            input.to_str().unwrap(),
            "--emit",
            "capabilities",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("LB0019"))
        .stdout(predicate::str::contains("\"ok\":false"));

    Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
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

#[cfg(feature = "lyba")]
#[test]
fn lyba_decode_bounds_oversized_output_deterministically() {
    use lyma::lyba::{Document, LybaFile, Value as LybaValue, WriteOptions, Writer};

    let dir = tempdir().unwrap();
    let input = dir.path().join("large.lyba");
    let payload = (0..70_000)
        .map(|index| LybaValue::Int(index.into()))
        .collect::<Vec<_>>();
    let bytes = Writer::new(WriteOptions::new())
        .write(
            &LybaFile::new()
                .with_document(Document::new().with_root_value(LybaValue::Sequence(payload))),
        )
        .unwrap();
    fs::write(&input, bytes).unwrap();

    let output = Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
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

#[cfg(feature = "lyba")]
#[test]
fn lyba_decode_rejects_oversized_input_before_full_read() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("too-large.lyba");
    fs::write(&input, vec![0_u8; 4 * 1024 * 1024 + 1]).unwrap();

    Command::cargo_bin("lyma")
        .unwrap()
        .args([
            "--output",
            "json",
            "lyba",
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

#[cfg(feature = "lyba")]
fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap()
}

#[cfg(feature = "lyba")]
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
