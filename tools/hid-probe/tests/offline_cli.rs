use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

const FIXTURE: &str = "tests/fixtures/sample-v1.jsonl";

#[test]
fn decode_fixture_produces_expected_json_summary() {
    let output = Command::cargo_bin("hid-probe")
        .unwrap()
        .args(["decode", FIXTURE, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["summary"]["report_count"], 3);
    assert_eq!(value["summary"]["capture_complete"], true);
    assert_eq!(value["summary"]["duration_us"], 5000);
    assert_eq!(value["summary"]["report_id_counts"]["0x01"], 3);
    assert_eq!(value["summary"]["changed_byte_positions"]["3"], 2);
    assert!(value.get("reports").is_none());
}

#[test]
fn decode_reports_is_hardware_independent() {
    Command::cargo_bin("hid-probe")
        .unwrap()
        .args(["decode", FIXTURE, "--reports"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "seq=0 elapsed_us=1000 len=9 report_id=0x01 payload=0000040000000000",
        ));
}

#[test]
fn summarize_alias_works() {
    Command::cargo_bin("hid-probe")
        .unwrap()
        .args(["summarize", FIXTURE])
        .assert()
        .success()
        .stdout(predicate::str::contains("reports: 3 complete=true"));
}

#[test]
fn unsupported_schema_version_fails_with_context() {
    let fixture = fs::read_to_string(FIXTURE).unwrap();
    let invalid = fixture.replacen("\"schema_version\":1", "\"schema_version\":99", 1);
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(invalid.as_bytes()).unwrap();
    file.flush().unwrap();

    Command::cargo_bin("hid-probe")
        .unwrap()
        .arg("decode")
        .arg(file.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported trace schema version 99",
        ));
}

#[test]
fn interrupted_trace_without_footer_is_summarized() {
    let fixture = fs::read_to_string(FIXTURE).unwrap();
    let incomplete = fixture.lines().take(4).collect::<Vec<_>>().join("\n");
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(incomplete.as_bytes()).unwrap();
    file.flush().unwrap();

    let output = Command::cargo_bin("hid-probe")
        .unwrap()
        .arg("decode")
        .arg(file.path())
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["summary"]["capture_complete"], false);
    assert_eq!(value["summary"]["duration_us"], 4000);
}

#[test]
fn shared_fixture_contains_no_stable_identifiers() {
    let fixture = fs::read_to_string(FIXTURE).unwrap();
    assert!(!fixture.contains("serial_number"));
    assert!(!fixture.contains("\"path\""));
}

#[test]
fn hardware_module_contains_no_hid_write_calls() {
    let source = include_str!("../src/hardware.rs");
    for forbidden in [
        "device.write(",
        "send_feature_report(",
        "send_output_report(",
    ] {
        assert!(
            !source.contains(forbidden),
            "read-only safety invariant violated by {forbidden}"
        );
    }
}
