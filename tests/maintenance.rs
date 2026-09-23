use std::process::Command;

#[test]
fn on_disk_agent_does_not_prove_this_installation_is_loaded() {
    let temp = tempfile::tempdir().unwrap();
    let config = fixture(temp.path());
    let binary = config.parent().unwrap().join("bin/rws");
    rws::autostart::install(
        &temp.path().join("Library/LaunchAgents"),
        &binary,
        &config,
        &[],
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", temp.path())
        .env_remove("RWS_SSHFS")
        .args(["--config", config.to_str().unwrap(), "doctor", "--json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let agent = value["before"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "launch_agent")
        .unwrap();
    assert_eq!(
        agent["status"], "error",
        "an unregistered fixture must not be reported as active: {agent}"
    );
}

fn fixture(temp: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let support = temp.join("Library/Application Support/RWS");
    std::fs::create_dir_all(support.join("bin")).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_rws"), support.join("bin/rws")).unwrap();
    let sshfs = temp.join("sshfs");
    std::fs::write(
        &sshfs,
        "#!/bin/sh\nprintf 'SSHFS version 3.7.5-rws-fskit3\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&sshfs, std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = support.join("config.json");
    std::fs::write(
        &config,
        serde_json::json!({"version":1,"workspaces":[],"mount":{"sshfs":sshfs,"fskit":true}})
            .to_string(),
    )
    .unwrap();
    config
}

#[test]
fn repair_records_stale_hook_before_and_current_hook_after() {
    let temp = tempfile::tempdir().unwrap();
    let config = fixture(temp.path());
    let zshrc = temp.path().join(".zshrc");
    let stale = rws::shell_hook::install_line(
        std::path::Path::new("/old/target/debug/rws"),
        Some(std::path::Path::new("/old/.rws-local/setup.json")),
    )
    .unwrap();
    std::fs::write(&zshrc, format!("# personal\n{stale}\n\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", temp.path())
        .env_remove("ZDOTDIR")
        .env_remove("DELTA_CONFIG_DIR")
        .env_remove("RWS_SSHFS")
        .args([
            "--config",
            config.to_str().unwrap(),
            "doctor",
            "--repair",
            "--json",
        ])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        value,
        String::from_utf8_lossy(&output.stderr)
    );
    let status = |phase: &str| {
        value[phase]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "shell_hook")
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status("before"), "error");
    assert_eq!(status("after"), "ok");
    let contents = std::fs::read_to_string(zshrc).unwrap();
    assert!(contents.starts_with("# personal\n"));
    assert!(!contents.contains("target/debug"));
    assert!(!temp.path().join(".config/delta/AGENT.md").exists());
    let saved = value["report_path"]
        .as_str()
        .expect("repair always saves its report");
    assert!(std::path::Path::new(saved).is_file());
    let before = std::path::Path::new(saved).with_extension("before.json");
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(before).unwrap()).unwrap();
    assert!(snapshot["after"].is_null());
    assert_eq!(snapshot["before"], value["before"]);
}

#[test]
fn repair_keeps_a_deliberately_custom_hook_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let config = fixture(temp.path());
    let zshrc = temp.path().join(".zshrc");
    let custom = rws::shell_hook::install_line(
        std::path::Path::new("/opt/rws"),
        Some(std::path::Path::new("/opt/custom/config.json")),
    )
    .unwrap();
    std::fs::write(&zshrc, &custom).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", temp.path())
        .env_remove("ZDOTDIR")
        .env_remove("DELTA_CONFIG_DIR")
        .env_remove("RWS_SSHFS")
        .args(["--config", config.to_str().unwrap(), "repair", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(std::fs::read_to_string(zshrc).unwrap(), custom);
}

#[test]
fn doctor_records_missing_configuration_without_creating_it() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("missing.json");
    let report = temp.path().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", temp.path())
        .args([
            "--config",
            config.to_str().unwrap(),
            "doctor",
            "--json",
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON doctor report");
    assert!(!output.status.success());
    assert_eq!(value["before"][0]["id"], "configuration");
    assert_eq!(value["before"][0]["status"], "error");
    assert!(!config.exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&report).unwrap()).unwrap(),
        value
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(report).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn doctor_never_overwrites_an_existing_report() {
    let temp = tempfile::tempdir().unwrap();
    let report = temp.path().join("report.json");
    std::fs::write(&report, "keep me").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", temp.path())
        .args(["doctor", "--report", report.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(report).unwrap(), "keep me");
}

#[test]
fn repair_upgrades_an_implicit_default_hook_without_enabling_a_disabled_one() {
    let temp = tempfile::tempdir().unwrap();
    let config = fixture(temp.path());
    let zshrc = temp.path().join(".zshrc");
    let hook = rws::shell_hook::install_line(std::path::Path::new("/old/rws"), None).unwrap();
    for disabled in [false, true] {
        let original = format!("{}{hook}\n", if disabled { "# " } else { "" });
        std::fs::write(&zshrc, &original).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rws"))
            .env("HOME", temp.path())
            .env_remove("ZDOTDIR")
            .env_remove("DELTA_CONFIG_DIR")
            .env_remove("RWS_SSHFS")
            .args(["--config", config.to_str().unwrap(), "repair", "--json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let updated = std::fs::read_to_string(&zshrc).unwrap();
        if disabled {
            assert_eq!(updated, original);
        } else {
            assert!(!updated.contains("/old/rws"));
        }
    }
}
