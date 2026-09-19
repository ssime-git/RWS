use std::process::{Command, Output};
fn run(config: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rws"))
        .arg("--config")
        .arg(config)
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("sshfs: unusable"));
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
