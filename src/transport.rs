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

/// Capture a small diagnostic with a deadline. Descendants are stopped before
/// reading pipes; large output reaches the deadline instead of growing memory.
pub fn bounded_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::{
        os::unix::process::CommandExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let result = loop {
        match crate::mount::exited(&child) {
            Ok(true) => break Ok(()),
            Ok(false) => {}
            Err(e) => break Err(e.to_string()),
        }
        if started.elapsed() >= timeout {
            break Err("diagnostic timed out".to_string());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    // waitid(WNOWAIT) reserves the child's PID even on successful exit.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    if result.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    result.map(|_| output)
}

#[cfg(test)]
mod probe_tests {
    #[test]
    fn capture_preserves_remote_failure_and_stops_pipe_holding_descendants() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & printf proof; exit 37"]);
        let started = std::time::Instant::now();
        let output =
            super::bounded_output(&mut command, std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(output.status.code(), Some(37));
        assert_eq!(output.stdout, b"proof");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn excessive_output_times_out_instead_of_deadlocking() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "while :; do printf '0123456789'; done"]);
        assert!(
            super::bounded_output(&mut command, std::time::Duration::from_millis(100))
                .unwrap_err()
                .contains("timed out")
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
