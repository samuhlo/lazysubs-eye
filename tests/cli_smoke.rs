use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lazysubs-eye")
}

#[test]
fn version_help_and_doctor_json_are_valid() {
    let version = Command::new(binary()).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("lazysubs-eye "));

    let help = Command::new(binary()).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("EXIT CODES"));

    let doctor = Command::new(binary())
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(doctor.status.success() || doctor.status.code() == Some(1));
    let value: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert!(value["checks"].is_array());
}

#[test]
fn verbose_is_stderr_only_and_missing_notify_is_a_warning_not_exit_four() {
    let root = std::env::temp_dir().join(format!("lazysubs-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("cache/lazysubs-eye")).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("cache/lazysubs-eye/status.json"),
        format!(
            "{{\"fetched_at\":{},\"providers\":[]}}",
            chrono::Utc::now().timestamp()
        ),
    )
    .unwrap();
    // [FLOW] JSON remains machine-readable on stdout while verbose refresh diagnostics stay on stderr.
    let verbose = Command::new(binary())
        .args(["--verbose", "--json"])
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("HOME", root.join("home"))
        .output()
        .unwrap();
    assert!(verbose.status.success());
    assert!(serde_json::from_slice::<serde_json::Value>(&verbose.stdout).is_ok());
    let stderr = String::from_utf8_lossy(&verbose.stderr);
    assert!(stderr.contains("decisión de refresh"));
    assert!(stderr.contains("checkpoint de caché"));

    // [FLOW] Missing optional notification tooling degrades doctor to a warning, never an internal-error exit.
    let doctor = Command::new(binary())
        .args(["doctor", "--json"])
        .env("PATH", "")
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("HOME", root.join("home"))
        .output()
        .unwrap();
    assert_ne!(doctor.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let notify = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "notify-send")
        .unwrap();
    assert_eq!(notify["state"], "warn");
    let _ = std::fs::remove_dir_all(root);
}
