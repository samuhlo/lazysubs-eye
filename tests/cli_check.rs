use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str, providers: serde_json::Value, config: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lazysubs-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("cache/lazysubs-eye")).unwrap();
    std::fs::create_dir_all(root.join("config/lazysubs-eye")).unwrap();
    let status = serde_json::json!({
        "fetched_at": chrono::Utc::now().timestamp(),
        "providers": providers,
    });
    std::fs::write(
        root.join("cache/lazysubs-eye/status.json"),
        serde_json::to_vec(&status).unwrap(),
    )
    .unwrap();
    if let Some(config) = config {
        std::fs::write(root.join("config/lazysubs-eye/config.toml"), config).unwrap();
    }
    root
}

fn provider(percent: f64, stale: bool, error: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": "test",
        "name": "Test",
        "icon": "*",
        "plan": null,
        "windows": [{
            "label": "session",
            "used_percent": percent,
            "resets_at": null,
            "active": true
        }],
        "stale_since": stale.then(|| chrono::Utc::now().timestamp() - 60),
        "error": error
    })
}

fn run(root: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lazysubs-eye"))
        .arg("--check")
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("HOME", root.join("home"))
        .output()
        .unwrap()
}

#[test]
fn check_contract_covers_only_zero_to_three() {
    // [FLOW] Exercise the public --check exit-code contract across every supported health state.
    // INVARIANT: diagnostics must not leak configuration secrets into either output stream.
    let cases = [
        (
            "ready",
            serde_json::json!([provider(20.0, false, None)]),
            None,
            0,
        ),
        (
            "warning",
            serde_json::json!([provider(85.0, false, None)]),
            None,
            1,
        ),
        (
            "stale",
            serde_json::json!([provider(20.0, true, None)]),
            None,
            1,
        ),
        (
            "critical",
            serde_json::json!([provider(99.0, false, None)]),
            None,
            2,
        ),
        (
            "error",
            serde_json::json!([provider(20.0, false, Some("unavailable"))]),
            None,
            3,
        ),
        ("empty", serde_json::json!([]), None, 3),
        (
            "bad-config",
            serde_json::json!([provider(20.0, false, None)]),
            Some("ttl = 0"),
            3,
        ),
    ];
    for (name, providers, config, expected) in cases {
        let root = fixture(name, providers, config);
        let output = run(&root);
        assert_eq!(output.status.code(), Some(expected), "case {name}");
        assert_ne!(output.status.code(), Some(4));
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!combined.contains("api_key="));
        let _ = std::fs::remove_dir_all(root);
    }
}
