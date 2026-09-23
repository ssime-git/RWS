use std::process::{Command, Output};
fn run(config: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}

fn run_in_home(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", home)
        .args(args)
        .output()
        .unwrap()
}
fn register(config: &std::path::Path, root: &std::path::Path) {
    let out = run(
        config,
        &[
            "workspace",
            "add",
            "demo",
            "--ssh",
            "dev@server",
            "--remote",
            "/srv/project with 'quotes'",
            "--mount",
            root.to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn status_shows_saved_auto_reconnect_intent() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    rws::config::Config::set_mount_intent(&config, "demo", rws::config::MountIntent::Paused)
        .unwrap();
    let out = run(&config, &["status", "demo", "--no-probe"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("  Auto-reconnect: paused"));
}

#[test]
#[cfg(target_os = "macos")]
fn disconnect_alias_and_dry_run_preserve_a_single_intent_registry() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let alias = temp.path().join("alias.json");
    std::os::unix::fs::symlink(&config, &alias).unwrap();
    let before = std::fs::read(&config).unwrap();
    for command in ["connect", "mount", "disconnect", "unmount"] {
        let _ = run(&alias, &[command, "demo", "--dry-run"]);
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }
    let out = run(&alias, &["unmount", "demo"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        rws::config::Config::load(&config)
            .unwrap()
            .mount_intent("demo"),
        rws::config::MountIntent::Paused
    );
    let out = run(&config, &["connect", "demo", "--if-desired"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("paused"));
}
#[test]
fn registration_is_persistent_and_rejects_duplicates_and_overlapping_roots() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    register(&config, &root);
    let out = run(&config, &["workspace", "list"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("dev@server"));
    assert!(
        !run(
            &config,
            &[
                "workspace",
                "add",
                "demo",
                "--ssh",
                "server",
                "--remote",
                "/srv",
                "--mount",
                root.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert!(
        !run(
            &config,
            &[
                "workspace",
                "add",
                "nested",
                "--ssh",
                "server",
                "--remote",
                "/srv",
                "--mount",
                root.join("sub").to_str().unwrap()
            ]
        )
        .status
        .success()
    );
}
// A stalled SSHFS volume blocks any path resolution under its mount root, so
// reading the configuration must never resolve mount roots: every command —
// including the app's startup validation via `workspace list` — would hang
// with it. A symlink loop makes resolution fail deterministically instead of
// hanging, which is enough to prove no resolution happens while loading.
#[test]
fn loading_configuration_never_resolves_mount_roots() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("stalled/mount"));
    let out = run(
        &config,
        &[
            "workspace",
            "add",
            "healthy",
            "--ssh",
            "server",
            "--remote",
            "/srv",
            "--mount",
            temp.path().join("healthy").to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::os::unix::fs::symlink("stalled", temp.path().join("stalled")).unwrap();
    let out = run(&config, &["workspace", "list"]);
    assert!(
        out.status.success(),
        "an unresolvable mount root must not invalidate the configuration: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("demo") && text.contains("healthy"), "{text}");
    // The unavailable volume surfaces as per-workspace state, never as a
    // global configuration failure.
    let out = run(&config, &["status", "--no-probe"]);
    // Per-workspace output remains available even when an unavailable volume
    // makes the overall health status fail.
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("demo:") && text.contains("healthy:"),
        "{text}"
    );
}
#[test]
fn hand_edited_duplicate_names_or_nested_roots_are_rejected_without_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("loop/mount"));
    std::os::unix::fs::symlink("loop", temp.path().join("loop")).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    let mut second = value["workspaces"][0].clone();
    second["mount_root"] = temp
        .path()
        .join("loop/mount/nested")
        .to_str()
        .unwrap()
        .into();
    for (name, expected) in [("demo", "duplicate name"), ("other", "nested root")] {
        second["name"] = name.into();
        value["workspaces"].as_array_mut().unwrap().truncate(1);
        value["workspaces"]
            .as_array_mut()
            .unwrap()
            .push(second.clone());
        std::fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let out = run(&config, &["workspace", "list"]);
        assert!(!out.status.success(), "{expected} must be rejected");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("overlap"),
            "{expected}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
#[test]
fn dry_run_preserves_argv_and_does_not_create_mount() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    register(&config, &root);
    let out = run(
        &config,
        &[
            "exec",
            "--workspace",
            "demo",
            "--dry-run",
            "--",
            "printf",
            "%s",
            "$(id)",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["program"], "ssh");
    assert!(
        value["args"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_str()
            .unwrap()
            .contains("'$(id)'")
    );
    let out = run(&config, &["mount", "demo", "--dry-run"]);
    assert_eq!(out.status.success(), cfg!(target_os = "macos"));
    assert!(!root.exists());
}
#[test]
fn invalid_config_and_missing_command_fail_without_panic() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    std::fs::write(&config, "{broken").unwrap();
    let out = run(&config, &["workspace", "list"]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
}
#[test]
fn exec_infers_nested_directory_and_propagates_exit_code() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    std::fs::create_dir_all(root.join("src")).unwrap();
    register(&config, &root);
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let fake = bin.join("ssh");
    std::fs::write(&fake, "#!/bin/sh\nexit 37\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["exec", "--dry-run", "--", "pwd"])
        .current_dir(root.join("src"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("/src"));
    let status = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["exec", "--workspace", "demo", "--", "false"])
        .env("PATH", &bin)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(37));
}
#[test]
fn config_lock_preserves_existing_content() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let before = std::fs::read(&config).unwrap();
    std::fs::write(config.with_extension("lock"), "held").unwrap();
    let output = run(
        &config,
        &[
            "workspace",
            "add",
            "other",
            "--ssh",
            "server",
            "--remote",
            "/srv",
            "--mount",
            temp.path().join("other").to_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&config).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(config.with_extension("lock")).unwrap(),
        "held"
    );
}
#[test]
fn missing_sshfs_has_actionable_error_and_creates_no_mount_directory() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    register(&config, &root);
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["mount", "demo"])
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SSHFS is missing"));
    assert!(!root.exists());
}

#[test]
fn install_copies_a_source_config_to_the_canonical_durable_layout() {
    use std::os::unix::fs::PermissionsExt;

    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let source = temp.path().join("source.json");
    let sshfs = temp.path().join("sshfs");
    std::fs::write(&sshfs, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&sshfs, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        &source,
        format!(
            r#"{{"version":1,"workspaces":[],"mount":{{"sshfs":"{}","fskit":false}}}}"#,
            sshfs.display()
        ),
    )
    .unwrap();
    let source_before = std::fs::read(&source).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", &home)
        .arg("--config")
        .arg(&source)
        .arg("install")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let support = home.join("Library/Application Support/RWS");
    assert!(support.join("bin/rws").is_file());
    let installed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(support.join("config.json")).unwrap()).unwrap();
    assert!(
        installed["mount"]["sshfs"]
            .as_str()
            .unwrap()
            .contains("/releases/")
    );
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    let active_before = std::fs::read(support.join("config.json")).unwrap();
    let again = run_in_home(&home, &["install"]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(
        std::fs::read(support.join("config.json")).unwrap(),
        active_before,
        "unchanged app relaunch must not create a release or rotate mount receipts"
    );
}

#[test]
fn autostart_uses_only_the_canonical_completed_installation() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let source = temp.path().join("source.json");
    std::fs::write(&source, r#"{"version":1,"workspaces":[]}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", &home)
        .arg("--config")
        .arg(&source)
        .args(["autostart", "run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--config"));

    let output = run_in_home(&home, &["autostart", "install"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("managed RWS binary"));
}

#[test]
fn autostart_run_attempts_every_workspace_and_aggregates_failures() {
    use std::os::unix::fs::PermissionsExt;

    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let support = home.join("Library/Application Support/RWS");
    std::fs::create_dir_all(&support).unwrap();
    let sshfs = temp.path().join("sshfs");
    std::fs::write(
        &sshfs,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nexit 9\n",
    )
    .unwrap();
    std::fs::set_permissions(&sshfs, std::fs::Permissions::from_mode(0o700)).unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::write(
        support.join("config.json"),
        format!(
            r#"{{"version":1,"workspaces":[{{"name":"first","host":"dev@host","remote_root":"/srv/first","mount_root":"{}"}},{{"name":"second","host":"dev@host","remote_root":"/srv/second","mount_root":"{}"}}],"mount":{{"sshfs":"{}","fskit":false}}}}"#,
            first.display(),
            second.display(),
            sshfs.display(),
        ),
    )
    .unwrap();

    let output = run_in_home(&home, &["autostart", "run"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("first"), "{stderr}");
    assert!(stderr.contains("second"), "{stderr}");
}

#[test]
fn canonical_installation_rejects_a_conflicting_sshfs_override_everywhere() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let support = home.join("Library/Application Support/RWS");
    std::fs::create_dir_all(&support).unwrap();
    std::fs::write(
        support.join("config.json"),
        format!(
            r#"{{"version":1,"workspaces":[{{"name":"demo","host":"dev@host","remote_root":"/srv/demo","mount_root":"{}"}}],"mount":{{"sshfs":"/managed/sshfs","fskit":false}}}}"#,
            temp.path().join("mount").display(),
        ),
    )
    .unwrap();

    for args in [
        vec!["mount", "demo", "--dry-run"],
        vec!["mount", "demo", "--repair", "--dry-run"],
        vec!["doctor"],
        vec!["autostart", "run"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rws"))
            .env("HOME", &home)
            .env("RWS_SSHFS", "/checkout/sshfs")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains("RWS_SSHFS"), "{text}");
        assert!(text.contains("canonical"), "{text}");
    }
}

#[test]
fn canonical_config_aliases_cannot_bypass_sshfs_override_rejection() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let support = home.join("Library/Application Support/RWS");
    std::fs::create_dir_all(&support).unwrap();
    let config = support.join("config.json");
    std::fs::write(
        &config,
        format!(
            r#"{{"version":1,"workspaces":[{{"name":"demo","host":"dev@host","remote_root":"/srv/demo","mount_root":"{}"}}],"mount":{{"sshfs":"/managed/sshfs","fskit":false}}}}"#,
            temp.path().join("mount").display(),
        ),
    )
    .unwrap();
    std::os::unix::fs::symlink(&config, temp.path().join("config-alias.json")).unwrap();
    std::fs::hard_link(&config, temp.path().join("config-hardlink.json")).unwrap();

    for alias in [
        "./home/Library/Application Support/RWS/config.json",
        "config-alias.json",
        "config-hardlink.json",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rws"))
            .current_dir(temp.path())
            .env("HOME", &home)
            .env("RWS_SSHFS", "/checkout/sshfs")
            .args(["--config", alias, "mount", "demo", "--dry-run"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "alias {alias} must be rejected");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("RWS_SSHFS"),
            "alias {alias}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn doctor_reports_missing_canonical_config_before_dependency_selection() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("HOME", &home)
        .env("RWS_SSHFS", "/missing/sshfs")
        .arg("doctor")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("configuration [error]") && stdout.contains("No such file"),
        "{stdout}"
    );
    assert!(!stdout.contains("conflicts with the canonical"), "{stdout}");
}
#[test]
fn rejects_mount_overlap_through_parent_symlink_before_mount_exists() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let config = temp.path().join("config.json");
    register(&config, &real.join("mount"));
    let out = run(
        &config,
        &[
            "workspace",
            "add",
            "aliased",
            "--ssh",
            "server",
            "--remote",
            "/srv",
            "--mount",
            alias.join("mount").to_str().unwrap(),
        ],
    );
    assert!(
        !out.status.success(),
        "alias must not register the same physical mount twice"
    );
}

#[test]
fn shell_launches_remote_default_shell_in_workspace() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let remote = temp.path().join("remote ' workspace");
    std::fs::create_dir(&remote).unwrap();
    let shell = temp.path().join("custom shell");
    std::fs::write(
        &shell,
        "#!/bin/sh\nprintf 'custom-shell:%s\\n' \"$1\"\npwd -P\n",
    )
    .unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = run(
        &config,
        &[
            "workspace",
            "add",
            "demo",
            "--ssh",
            "dev@server",
            "--remote",
            remote.to_str().unwrap(),
            "--mount",
            temp.path().join("mount").to_str().unwrap(),
        ],
    );
    assert!(out.status.success());
    let out = run(&config, &["shell", "--workspace", "demo", "--dry-run"]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let args = value["args"].as_array().unwrap();
    assert!(args.iter().any(|v| v == "-t"));
    let script = args.last().unwrap().as_str().unwrap();
    let out = Command::new("/bin/sh")
        .args(["-c", script])
        .env("SHELL", &shell)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!(
            "custom-shell:-l\n{}\n",
            remote.canonicalize().unwrap().display()
        )
    );
}

#[test]
fn doctor_and_mount_reject_installed_but_broken_sshfs() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    register(&config, &root);
    for name in ["ssh", "sftp", "sshfs"] {
        let file = temp.path().join(name);
        let script = if name == "sshfs" {
            "#!/bin/sh\necho 'Library not loaded: libfuse3.4.dylib' >&2\nexit 1\n"
        } else {
            "#!/bin/sh\nexit 0\n"
        };
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .arg("doctor")
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "doctor must not accept a broken SSHFS installation"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("sshfs [error]"));
    if cfg!(target_os = "macos") {
        let out = Command::new(env!("CARGO_BIN_EXE_rws"))
            .arg("--config")
            .arg(&config)
            .args(["mount", "demo"])
            .env("PATH", temp.path())
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("macFUSE"));
        assert!(
            !root.exists(),
            "failed prerequisite checks must not create a mount directory"
        );
    }
}

#[test]
fn fskit_mount_uses_native_volume_path_and_explicit_backend() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, std::path::Path::new("/Volumes/RWS-unit-test"));
    let out = run(&config, &["mount", "demo", "--fskit", "--dry-run"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "backend=fskit")
    );
    let other = temp.path().join("other.json");
    register(&other, &temp.path().join("mount"));
    assert!(
        !run(&other, &["mount", "demo", "--fskit", "--dry-run"])
            .status
            .success()
    );
}

#[test]
fn successful_sshfs_exit_without_volume_is_a_mount_failure() {
    if !cfg!(target_os = "macos") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let fake = temp.path().join("sshfs");
    std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["mount", "demo"])
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "an exit code alone cannot prove the mount exists"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no mounted filesystem"));
}

#[test]
fn fskit_uses_selected_binary_in_foreground() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, std::path::Path::new("/Volumes/RWS-unit-test"));
    let output = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["mount", "demo", "--fskit", "--dry-run"])
        .env("RWS_SSHFS", "/tmp/custom sshfs")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["program"], "/tmp/custom sshfs");
    let args = value["args"].as_array().unwrap();
    assert!(args.iter().any(|v| v == "-f"));
    assert!(
        args.iter()
            .any(|v| v.as_str().is_some_and(|s| s.contains("BatchMode=yes")))
    );
}

#[test]
fn mount_converts_unicode_names_and_labels_volume() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, std::path::Path::new("/Volumes/RWS-unit-test"));
    let output = run(&config, &["mount", "demo", "--fskit", "--dry-run"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let args = value["args"].as_array().unwrap();
    for option in ["rws_unicode", "volname=RWS-demo"] {
        assert!(args.iter().any(|v| v == option), "missing {option}");
    }
    let output = run(
        &config,
        &["mount", "demo", "--fskit", "--raw-names", "--dry-run"],
    );
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("rws_unicode"));
}

#[test]
fn fskit_normalization_requires_compatible_sshfs_before_mounting() {
    if !cfg!(target_os = "macos") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(
        &config,
        std::path::Path::new("/Volumes/RWS-incompatible-test"),
    );
    let fake = temp.path().join("sshfs");
    std::fs::write(&fake, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'SSHFS version 3.7.5'; exit 0; fi\necho should-not-run >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(config)
        .args(["mount", "demo", "--fskit"])
        .env("RWS_SSHFS", fake)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("build-sshfs-fskit.sh"));
}

#[test]
fn saved_mount_settings_are_reused_without_environment_variables() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, std::path::Path::new("/Volumes/RWS-settings-test"));
    let out = run(
        &config,
        &[
            "settings",
            "--sshfs",
            "/tmp/custom sshfs",
            "--backend",
            "fskit",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run(&config, &["connect", "demo", "--dry-run"]);
    assert_eq!(out.status.success(), cfg!(target_os = "macos"));
    if cfg!(target_os = "macos") {
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["program"], "/tmp/custom sshfs");
        assert!(
            value["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "backend=fskit")
        );
    }
    assert!(run(&config, &["workspace", "list"]).status.success());
}

#[test]
fn status_distinguishes_mount_from_execution_and_disconnect_is_repeatable() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let out = run(&config, &["status", "demo", "--no-probe"]);
    // Unsupported mount inspection is an error, not a healthy status. macOS
    // can positively establish that this test workspace is disconnected.
    assert_eq!(
        out.status.code(),
        Some(if cfg!(target_os = "macos") { 0 } else { 1 })
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(if cfg!(target_os = "macos") {
            "disconnected"
        } else {
            "unavailable"
        }),
        "{text}"
    );
    assert!(text.contains("not redirected"), "{text}");
    if cfg!(target_os = "macos") {
        for _ in 0..2 {
            assert!(run(&config, &["disconnect", "demo"]).status.success());
        }
    }
}

#[test]
fn shortcuts_preserve_quoted_paths_and_do_not_overwrite() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config ' $(touch BAD).json");
    register(&config, &temp.path().join("mount"));
    let directory = temp.path().join("shortcuts");
    let args = [
        "shortcuts",
        "demo",
        "--directory",
        directory.to_str().unwrap(),
    ];
    let out = run(&config, &args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let script = directory.join("Status-demo.command");
    assert_eq!(
        std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let out = Command::new("/bin/bash")
        .arg(&script)
        .env("PATH", "/usr/bin:/bin")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    // SSH can fail; the script must still resolve the correct config, not default HOME.
    assert!(String::from_utf8_lossy(&out.stdout).contains("dev@server"));
    assert!(!temp.path().join("BAD").exists());
    assert!(!run(&config, &args).status.success());
}

#[test]
fn invalid_settings_and_locked_settings_preserve_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let before = std::fs::read(&config).unwrap();
    assert!(
        !run(&config, &["settings", "--sshfs", "relative/path"])
            .status
            .success()
    );
    assert_eq!(std::fs::read(&config).unwrap(), before);
    std::fs::write(config.with_extension("lock"), "held").unwrap();
    assert!(
        !run(&config, &["settings", "--backend", "fskit"])
            .status
            .success()
    );
    assert_eq!(std::fs::read(&config).unwrap(), before);
}

#[test]
#[cfg(target_os = "macos")]
fn connect_and_disconnect_refuse_an_unrelated_os_mount() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, std::path::Path::new("/System/Volumes/Data"));
    for action in ["connect", "disconnect"] {
        let out = run(&config, &[action, "demo"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unverified"));
    }
}

#[test]
fn simultaneous_lifecycle_operation_is_rejected() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let state = config.with_file_name("config.json.mount-state");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("demo.lock"), "active").unwrap();
    let out = run(&config, &["disconnect", "demo"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("operation lock"));
}

#[test]
#[cfg(target_os = "macos")]
fn mount_identity_resolves_parent_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let alias = temp.path().join("volumes");
    std::os::unix::fs::symlink("/System/Volumes", &alias).unwrap();
    let direct = rws::lifecycle::identity(std::path::Path::new("/System/Volumes/Data")).unwrap();
    assert!(direct.is_some());
    assert_eq!(
        rws::lifecycle::identity(&alias.join("Data")).unwrap(),
        direct
    );
}

#[test]
fn shortcuts_support_workspace_names_starting_with_dash() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    value["workspaces"][0]["name"] = "-demo".into();
    std::fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
    let directory = temp.path().join("shortcuts");
    let out = run(
        &config,
        &[
            "shortcuts",
            "--directory",
            directory.to_str().unwrap(),
            "--",
            "-demo",
        ],
    );
    assert!(out.status.success());
    let out = Command::new("/bin/bash")
        .arg(directory.join("Status--demo.command"))
        .env("PATH", temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("dev@server"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("unexpected argument"));
    let out = Command::new("/bin/bash")
        .arg(directory.join("Shell-VM--demo.command"))
        .env("PATH", temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("RWS remote shell"));
}

#[test]
fn explicit_cwd_routes_commands_and_never_falls_back_locally() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    std::fs::create_dir_all(root.join("other-project/src")).unwrap();
    register(&config, &root);
    let out = run(
        &config,
        &[
            "exec",
            "--cwd",
            root.join("other-project/src").to_str().unwrap(),
            "--git-context",
            "--dry-run",
            "--",
            "printf",
            "$(touch LOCAL)",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["program"], "ssh");
    let script = value["args"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap();
    assert!(script.contains("/other-project/src"));
    assert!(script.contains("'$(touch LOCAL)'"));
    let marker = temp.path().join("must-not-exist");
    let out = run(
        &config,
        &[
            "exec",
            "--cwd",
            temp.path().to_str().unwrap(),
            "--git-context",
            "--",
            "touch",
            marker.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!marker.exists());
    let out = run(
        &config,
        &["context", "--cwd", temp.path().to_str().unwrap()],
    );
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["mode"], "local");
    // A registered directory alone does not prove a real mounted workspace.
    assert!(
        !run(&config, &["context", "--cwd", root.to_str().unwrap()])
            .status
            .success()
    );
}

#[test]
fn missing_registry_cannot_authorize_local_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.json");
    let out = run(
        &missing,
        &["context", "--cwd", temp.path().to_str().unwrap()],
    );
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("local"));
}

#[test]
fn agent_always_uses_ssh_and_never_falls_back_locally() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.json");
    register(&config, &tmp.path().join("mount"));
    let dry = run(
        &config,
        &["agent", "--workspace", "demo", "--dry-run", "--", "claude"],
    );
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(value["program"], "ssh");
    assert_eq!(value["args"][0], "-t");
    let ssh = tmp.path().join("ssh");
    std::fs::write(&ssh, "#!/bin/sh\nexit 255\n").unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).unwrap();
    let local = tmp.path().join("claude");
    std::fs::write(&local, "#!/bin/sh\necho LOCAL_AGENT_MUST_NOT_RUN\n").unwrap();
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rws"))
        .env("PATH", tmp.path())
        .arg("--config")
        .arg(&config)
        .args(["agent", "--workspace", "demo", "--", "claude"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(255));
    assert!(result.stdout.is_empty());
}
#[test]
fn hook_zsh_prints_snippet_embedding_this_executable() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let out = run(&config, &["hook", "zsh"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let snippet = String::from_utf8_lossy(&out.stdout);
    assert!(snippet.contains("_rws_auto_shell"));
    assert!(snippet.contains(env!("CARGO_BIN_EXE_rws")));
    // With stdout captured — as under eval "$(rws hook zsh)" at every shell
    // start — no guidance may be printed at all.
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
// The installed line is evaluated with zsh, absent from Linux CI runners.
#[cfg(target_os = "macos")]
#[test]
fn hook_install_writes_zshrc_line_with_config_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let zshrc = temp.path().join("zshrc");
    for _ in 0..2 {
        let out = run(
            &config,
            &["hook", "install", "--zshrc", zshrc.to_str().unwrap()],
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let content = std::fs::read_to_string(&zshrc).unwrap();
    let lines: Vec<_> = content.lines().filter(|l| l.contains("hook zsh")).collect();
    assert_eq!(lines.len(), 1, "{content}");
    assert!(lines[0].contains(env!("CARGO_BIN_EXE_rws")));
    assert!(lines[0].contains(config.to_str().unwrap()));
    // The installed line must evaluate: zsh runs it and defines the hook.
    let check = Command::new("zsh")
        .args(["-f", "-c"])
        .arg(format!(
            "{}\ntypeset -f _rws_auto_shell > /dev/null && echo OK",
            lines[0]
        ))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&check.stdout).contains("OK"),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}
#[test]
fn hook_bakes_an_absolute_config_path_from_a_relative_argument() {
    let temp = tempfile::tempdir().unwrap();
    let zshrc = temp.path().join("zshrc");
    let out = Command::new(env!("CARGO_BIN_EXE_rws"))
        .current_dir(temp.path())
        .args(["--config", "rel-config.json", "hook", "zsh"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let snippet = String::from_utf8_lossy(&out.stdout);
    assert!(!snippet.contains("'rel-config.json'"), "{snippet}");
    assert!(snippet.contains("rel-config.json"), "{snippet}");
    let install = Command::new(env!("CARGO_BIN_EXE_rws"))
        .current_dir(temp.path())
        .args(["--config", "rel-config.json", "hook", "install", "--zshrc"])
        .arg(&zshrc)
        .output()
        .unwrap();
    assert!(install.status.success());
    let line = std::fs::read_to_string(&zshrc).unwrap();
    assert!(!line.contains("'rel-config.json'"), "{line}");
}
#[test]
fn delta_rules_if_installed_skips_absent_rules_and_refreshes_existing_ones() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    register(&config, &temp.path().join("mount"));
    let rules = temp.path().join("AGENT.md");
    // No rules file: refresh must succeed without creating anything.
    let out = run(
        &config,
        &[
            "delta-rules",
            "--output",
            rules.to_str().unwrap(),
            "--if-installed",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!rules.exists());
    // Existing managed block: refresh rewrites it with the current binary.
    std::fs::write(&rules, "mine\n").unwrap();
    rws::agent_rules::install(&config, std::path::Path::new("/stale/rws"), &rules).unwrap();
    let out = run(
        &config,
        &[
            "delta-rules",
            "--output",
            rules.to_str().unwrap(),
            "--if-installed",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = std::fs::read_to_string(&rules).unwrap();
    assert!(content.starts_with("mine\n"), "{content}");
    assert!(content.contains(env!("CARGO_BIN_EXE_rws")), "{content}");
    assert!(!content.contains("/stale/rws"), "{content}");
    rws::agent_rules::install(
        &temp.path().join("custom/config.json"),
        std::path::Path::new("/custom/rws"),
        &rules,
    )
    .unwrap();
    let custom = std::fs::read(&rules).unwrap();
    let out = run(
        &config,
        &[
            "delta-rules",
            "--output",
            rules.to_str().unwrap(),
            "--if-installed",
        ],
    );
    assert!(out.status.success());
    assert_eq!(std::fs::read(rules).unwrap(), custom);
}
#[test]
fn agent_cwd_maps_the_subdirectory_like_exec() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let root = temp.path().join("mount");
    std::fs::create_dir_all(root.join("appli sub")).unwrap();
    register(&config, &root);
    let out = Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(&config)
        .args(["agent", "--dry-run", "--cwd"])
        .arg(root.join("appli sub"))
        .args(["--", "claude"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The remote script must target the mapped subdirectory under the
    // remote root (its embedded quote arrives shell-escaped).
    assert!(stdout.contains("appli sub"), "{stdout}");
    assert!(stdout.contains("/srv/project with "), "{stdout}");
    // --cwd and --workspace stay mutually exclusive, like exec.
    let conflict = run(
        &config,
        &[
            "agent",
            "--workspace",
            "demo",
            "--cwd",
            "/tmp",
            "--",
            "claude",
        ],
    );
    assert!(!conflict.status.success());
}
