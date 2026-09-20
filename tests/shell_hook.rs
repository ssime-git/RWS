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
        // These tests exercise detection and suppression, not the prompt:
        // force the remote choice so they run without a TTY.
        .env("RWS_AUTO_MODE", "remote")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap()
}

/// Run zsh under a pseudo-TTY via script(1), feeding `input` to the prompt.
fn run_zsh_pty(snippet: &str, prefix: &Path, body: &str, input: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("case.zsh");
    std::fs::write(&file, format!("emulate -L zsh\n{snippet}\n{body}\n")).unwrap();
    use std::io::Write;
    let mut child = std::process::Command::new("script")
        .args(["-q", "/dev/null", "zsh", "-f"])
        .arg(&file)
        .env("RWS_AUTO_PREFIX", prefix)
        .env_remove("RWS_AUTO_MODE")
        .env_remove("RWS_NO_AUTO_SHELL")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Answer only once the prompt is visible: closing stdin earlier makes
    // script(1) deliver EOF ahead of the answer.
    use std::io::Read;
    let mut stdout = child.stdout.take().unwrap();
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let sink = collected.clone();
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut chunk) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&chunk[..n]);
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if String::from_utf8_lossy(&collected.lock().unwrap()).contains("[Y/n]") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    reader.join().unwrap();
    let seen = collected.lock().unwrap().clone();
    format!(
        "{}{}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&out.stderr)
    )
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
        .env("RWS_AUTO_MODE", "remote")
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
        .env("RWS_AUTO_MODE", "remote")
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
        .env("RWS_AUTO_MODE", "remote")
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
        .env("RWS_AUTO_MODE", "remote")
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
        .env("RWS_AUTO_MODE", "remote")
        .env("RWS_CONFIG", "/env/config.json")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("SHELL-ENTERED"));
    let log = calls(temp.path());
    assert!(log.contains("--config /env/config.json context"), "{log}");
    assert!(!log.contains("/baked/config.json"), "{log}");
}

#[test]
fn without_tty_or_forced_mode_entry_stays_local_and_silent() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(temp.path(), r#"{"mode":"remote","workspace":"demo"}"#, 0);
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let script = format!(
        "emulate -L zsh\n{snippet}\ncd '{v}'\n_rws_auto_shell\necho STILL-LOCAL\n",
        v = volume.display()
    );
    let out = std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .env("RWS_AUTO_PREFIX", temp.path())
        .env_remove("RWS_AUTO_MODE")
        .env_remove("RWS_NO_AUTO_SHELL")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("SHELL-ENTERED"), "{stdout}");
    assert!(stdout.contains("STILL-LOCAL"), "{stdout}");
    assert!(!stdout.contains("switch to"), "{stdout}");
}

#[test]
fn rws_auto_mode_local_prevents_switching_without_a_prompt() {
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
        .env("RWS_AUTO_MODE", "local")
        .env_remove("RWS_NO_AUTO_SHELL")
        .output()
        .unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all.contains("SHELL-ENTERED"), "{all}");
    assert!(!all.contains("switch to"), "{all}");
}

#[test]
fn prompt_defaults_to_remote_on_enter() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(
        temp.path(),
        r#"{"host":"dev@server","mode":"remote","workspace":"demo"}"#,
        0,
    );
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!("cd '{v}'\n_rws_auto_shell\n", v = volume.display());
    let all = run_zsh_pty(&snippet, temp.path(), &body, "\n");
    assert!(all.contains("switch to dev@server (demo)?"), "{all}");
    assert_eq!(all.matches("SHELL-ENTERED").count(), 1, "{all}");
}

#[test]
fn prompt_answer_n_stays_local_and_is_remembered() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(
        temp.path(),
        r#"{"host":"dev@server","mode":"remote","workspace":"demo"}"#,
        0,
    );
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    // Enter, leave, re-enter: one question, zero switches.
    let body = format!(
        "cd '{v}'\n_rws_auto_shell\ncd /\n_rws_auto_shell\ncd '{v}'\n_rws_auto_shell\necho DONE\n",
        v = volume.display()
    );
    let all = run_zsh_pty(&snippet, temp.path(), &body, "n");
    assert_eq!(all.matches("switch to").count(), 1, "{all}");
    assert!(!all.contains("SHELL-ENTERED"), "{all}");
    assert!(all.contains("DONE"), "{all}");
}

#[test]
fn remembered_remote_choice_reapplies_on_reentry_without_asking() {
    let temp = tempfile::tempdir().unwrap();
    let volume = temp.path().join("RWS-demo");
    std::fs::create_dir_all(&volume).unwrap();
    let bin = write_fake_rws(
        temp.path(),
        r#"{"host":"dev@server","mode":"remote","workspace":"demo"}"#,
        0,
    );
    let snippet = rws::shell_hook::zsh_snippet(&bin, None).unwrap();
    let body = format!(
        "cd '{v}'\n_rws_auto_shell\ncd /\n_rws_auto_shell\ncd '{v}'\n_rws_auto_shell\n",
        v = volume.display()
    );
    let all = run_zsh_pty(&snippet, temp.path(), &body, "\n");
    assert_eq!(all.matches("switch to").count(), 1, "{all}");
    assert_eq!(all.matches("SHELL-ENTERED").count(), 2, "{all}");
}
