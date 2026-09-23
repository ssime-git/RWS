/// OpenSSH sends a command string to the remote login shell. Quote each literal
/// argument for a POSIX shell; local argument arrays alone do not protect it.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Expand SHELL on the remote host, keeping its executable path a single word.
pub fn remote_shell(directory: &str) -> Result<String, String> {
    if !directory.starts_with('/') || directory.contains('\0') {
        return Err("remote directory must be absolute and contain no NUL".into());
    }
    Ok(format!(
        "cd {} && exec \"${{SHELL:-/bin/sh}}\" -l",
        quote(directory)
    ))
}

pub fn remote_command(directory: &str, argv: &[String]) -> Result<String, String> {
    if !directory.starts_with('/') || directory.contains('\0') {
        return Err("remote directory must be absolute and contain no NUL".into());
    }
    if argv.is_empty()
        || argv[0].is_empty()
        || argv[0].starts_with('-')
        || argv.iter().any(|v| v.contains('\0'))
    {
        return Err("provide a command after --; arguments must contain no NUL".into());
    }
    Ok(format!(
        "cd {} && exec {}",
        quote(directory),
        argv.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" ")
    ))
}

/// A status probe must not wait forever for an authenticated but stalled server.
pub fn bounded_status(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<bool, String> {
    bounded_output(command, timeout).map(|output| output.status.success())
}

/// Capture a diagnostic with bounded time and memory, including pipe draining.
pub fn bounded_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    crate::process::output(command.stdin(std::process::Stdio::null()), timeout)
}

/// Start the executable only inside the remote login environment. Change directory
/// after profiles run, since they may change cwd. Diagnostics go to stderr.
pub fn remote_agent(directory: &str, argv: &[String]) -> Result<String, String> {
    remote_command(directory, argv)?;
    let cd = format!("cd {}", quote(directory));
    let exec = argv.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" ");
    let inner = format!(
        "{cd} && {{ printf 'RWS remote identity: ' >&2; hostname >&2; uname -s >&2; pwd >&2; }} && exec {exec}"
    );
    Ok(format!(
        "exec \"${{SHELL:-/bin/sh}}\" -lc {}",
        quote(&inner)
    ))
}

#[cfg(test)]
mod probe_tests {
    #[test]
    fn capture_preserves_remote_failure() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "printf proof; exit 37"]);
        let started = std::time::Instant::now();
        let output =
            super::bounded_output(&mut command, std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(output.status.code(), Some(37));
        assert_eq!(output.stdout, b"proof");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn excessive_output_is_bounded_instead_of_deadlocking() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "while :; do printf '0123456789'; done"]);
        let error =
            super::bounded_output(&mut command, std::time::Duration::from_millis(100)).unwrap_err();
        assert!(
            error.contains("timed out") || error.contains("limit"),
            "{error}"
        );
    }

    #[test]
    fn stalled_probe_has_a_hard_deadline() {
        let started = std::time::Instant::now();
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exec sleep 30"]);
        assert!(
            super::bounded_status(&mut command, std::time::Duration::from_millis(100))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }
}
