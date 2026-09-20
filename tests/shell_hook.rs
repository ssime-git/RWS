//! Behavior of the generated zsh auto-shell hook, driven by a fake rws binary.
use std::path::Path;

fn write_fake_rws(dir: &Path, mode_json: &str, exit_code: i32) -> std::path::PathBuf {
    let log = dir.join("calls.log");
    let bin = dir.join("fake-rws");
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1\" = --config ]; then shift 2; fi\nif [ \"$1\" = context ]; then\n  if [ {exit_code} -ne 0 ]; then echo 'rws: boom' >&2; exit {exit_code}; fi\n  echo '{mode_json}'\n  exit 0\nfi\nif [ \"$1\" = shell ]; then\n  echo \"SHELL-ENTERED $PWD\"\n  exit 0\nfi\nexit 2\n",
        log.display()
    );
    std::fs::write(&bin, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn run_zsh(snippet: &str, prefix: &Path, body: &str) -> std::process::Output {
    let script = format!("emulate -L zsh\nsetopt interactive\n{snippet}\n{body}\n");
    std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", prefix)
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap()
}

fn calls(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("calls.log")).unwrap_or_default()
}

#[test]
fn entering_a_remote_directory_switches_into_rws_shell_once() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    let deeper = volume.join("sub");
    std::fs::create_dir_all(&deeper).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!(
        "cd '{v}'\n_rws_auto_shell\ncd '{d}'\n_rws_auto_shell\n",
        v = volume.display(),
        d = deeper.display()
    );
    let out = run_zsh(&snippet, temp.path(), &body);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // One switch on entry; suppressed while staying inside the same volume.
    assert_eq!(stdout.matches("SHELL-ENTERED").count(), 1, "{stdout}");
    let log = calls(temp.path());
    assert!(log.contains("context --cwd"), "{log}");
}

#[test]
fn leaving_and_reentering_the_volume_switches_again() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let outside = temp.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!(
        "cd '{v}'\n_rws_auto_shell\ncd /\n_rws_auto_shell\ncd '{v}'\n_rws_auto_shell\n",
        v = volume.display()
    );
    let out = run_zsh(&snippet, temp.path(), &body);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("SHELL-ENTERED").count(), 2, "{stdout}");
    let _ = outside;
}

#[test]
fn local_mode_and_opt_out_do_not_switch() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("plain");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"local"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!("cd '{v}'\n_rws_auto_shell\n", v = volume.display());
    let out = run_zsh(&snippet, temp.path(), &body);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"));

    // Opt-out skips even the context query.
    let temp2 = tempfile::tempdir().unwrap();
    let volume2 = temp2.path().join("RWS-demo");
    std::fs::create_dir_all(&volume2).unwrap();
    let bin2 = write_fake_rws(temp2.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet2 = rws::shell_hook::zsh_snippet(&bin2, None).unwrap();
    let script = format!(
        "emulate -L zsh\n{snippet2}\ncd '{v}'\n_rws_auto_shell\n",
        v = volume2.display()
    );
    let out2 = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp2.path())
        .env("RWS_NO_AUTO_SHELL", "1")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&out2.stdout).contains("SHELL-ENTERED"));
    assert_eq!(calls(temp2.path()), "");
}

#[test]
fn context_failure_reports_once_and_suppresses_retries() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    let deeper = volume.join("sub");
    std::fs::create_dir_all(&deeper).unwrap();
    let bin = write_fake_rws(temp.path(), "", 1);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!(
        "cd '{v}'\n_rws_auto_shell\ncd '{d}'\n_rws_auto_shell\n",
        v = volume.display(),
        d = deeper.display()
    );
    let out = run_zsh(&snippet, temp.path(), &body);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.matches("boom").count(), 1, "{stderr}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"));
}

#[test]
fn sourcing_the_snippet_inside_a_volume_switches_immediately() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    // No manual _rws_auto_shell call: sourcing alone must detect the volume.
    let script = format!("emulate -L zsh\ncd '{v}'\n{snippet}\n", v = volume.display());
    let out = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp.path())
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("SHELL-ENTERED").count(), 1, "{stdout}");
}

#[test]
fn rws_config_environment_variable_is_forwarded_to_context_and_shell() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let script = format!(
        "emulate -L zsh\n{snippet}\ncd '{v}'\n_rws_auto_shell\n",
        v = volume.display()
    );
    let out = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp.path())
        .env("RWS_CONFIG", "/tmp/custom.json")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let log = calls(temp.path());
    assert!(log.contains("--config /tmp/custom.json context --cwd"), "{log}");
    assert!(log.contains("--config /tmp/custom.json shell"), "{log}");
}

#[test]
fn baked_config_is_used_and_rws_config_env_overrides_it() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet =
        rws::shell_hook::zsh_snippet(&bin, Some(std::path::Path::new("/baked/config.json")))
            .unwrap();
    let script = format!(
        "emulate -L zsh\n{snippet}\ncd '{v}'\n_rws_auto_shell\n",
        v = volume.display()
    );
    let out = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp.path())
        .env_remove("RWS_CONFIG")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let log = calls(temp.path());
    assert!(log.contains("--config /baked/config.json context"), "{log}");

    // Same snippet, but the environment variable wins.
    std::fs::remove_file(temp.path().join("calls.log")).unwrap();
    let out = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp.path())
        .env("RWS_CONFIG", "/env/config.json")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"));
    let log = calls(temp.path());
    assert!(log.contains("--config /env/config.json context"), "{log}");
    assert!(!log.contains("/baked/config.json"), "{log}");
}
