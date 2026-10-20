use std::process::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hostwatch"))
}

#[test]
fn invalid_options_fail_before_sampling() {
    let output = command()
        .args(["watch", "--interval", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("5–3600"));
}

#[cfg(target_os = "macos")]
#[test]
fn single_sample_watch_emits_one_versioned_json_report() {
    let output = command()
        .args(["watch", "--count", "1", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["snapshot"]["activity"]["status"], "warmup");
    assert_eq!(report["events"].as_array().unwrap().len(), 0);
    assert_eq!(report["snapshot"]["platform"], "macos");
    assert_eq!(report["snapshot"]["processes_requested"], false);
    assert_eq!(
        report["snapshot"]["top_processes"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(
        report["snapshot"]["memory"]["total_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[cfg(target_os = "macos")]
#[test]
fn snapshot_reports_rates_and_per_source_status() {
    let output = command().args(["snapshot", "--json"]).output().unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let snapshot = &report["snapshot"];
    assert_eq!(snapshot["activity"]["status"], "ready");
    assert!(snapshot["activity"]["cpu_busy_percent"].as_f64().is_some());
    assert!(snapshot["memory"]["breakdown"]["used_bytes"]
        .as_u64()
        .is_some());
    assert!(report.get("events").is_none());
    let sources = snapshot["sources"].as_array().unwrap();
    for name in ["hw.memsize", "host_statistics64", "vm.swapusage", "statfs"] {
        let source = sources.iter().find(|s| s["source"] == name).unwrap();
        assert_eq!(source["ok"], true, "{name}: {source}");
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_returns_an_explicit_unsupported_error() {
    let output = command().args(["snapshot", "--json"]).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Windows 适配尚未实现"));
}

#[test]
fn menubar_rejects_cli_only_options_before_starting_ui() {
    let output = command().args(["menubar", "--json"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--json"));
}
