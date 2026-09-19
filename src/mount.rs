use std::{
    path::Path,
    process::{Child, Command},
    time::Duration,
};

// Observe exit without reaping: keep the group leader PID reserved until cleanup.
fn exited(child: &Child) -> std::io::Result<bool> {
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        if libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        ) == -1
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(info.si_pid() != 0)
    }
}

/// Start a foreground filesystem server. The caller owns the returned live child.
/// The readiness callback must return promptly; OS filesystem calls can still block.
pub fn start(
    command: &mut Command,
    log: &Path,
    timeout: Duration,
    mut ready: impl FnMut() -> bool,
) -> Result<Child, String> {
    use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
    use std::{fs::OpenOptions, process::Stdio, time::Instant};
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(log)
        .map_err(|e| format!("create mount log: {e}"))?;
    let stderr = file.try_clone().map_err(|e| e.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(stderr)
        .process_group(0);
    let mut child = command.spawn().map_err(|e| format!("start SSHFS: {e}"))?;
    let started = Instant::now();
    let error = loop {
        match exited(&child) {
            Ok(true) => break "SSHFS exited; no mounted filesystem is ready".into(),
            Err(e) => break format!("inspect SSHFS: {e}"),
            Ok(false) => {}
        }
        if ready() {
            match exited(&child) {
                Ok(false) => return Ok(child),
                Ok(true) => break "SSHFS exited during mount startup".into(),
                Err(e) => break format!("inspect SSHFS: {e}"),
            }
        }
        if started.elapsed() >= timeout {
            break format!(
                "SSHFS mount timed out after {} seconds",
                timeout.as_secs_f32()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // Only this child's newly created process group: also stop an SSH subprocess.
    // The unreaped child (running or zombie) reserves the group leader PID.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(format!(
        "{error}; startup process stopped. See {}",
        log.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Instant};

    #[test]
    fn readiness_returns_with_live_child_and_private_log() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("ready");
        let log = temp.path().join("mount.log");
        let mut cmd = Command::new("/bin/sh");
        cmd.args([
            "-c",
            "echo diagnostic >&2; touch \"$1\"; exec sleep 30",
            "probe",
        ])
        .arg(&marker);
        let mut child = start(&mut cmd, &log, Duration::from_secs(3), || marker.exists()).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        assert_eq!(
            fs::metadata(&log).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(fs::read_to_string(&log).unwrap().contains("diagnostic"));
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn early_exit_never_proves_readiness() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("mount.log");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "echo startup-failed >&2; exit 0"]);
        let error = start(&mut cmd, &log, Duration::from_secs(3), || false).unwrap_err();
        assert!(error.contains("no mounted filesystem"), "{error}");
        assert!(fs::read_to_string(&log).unwrap().contains("startup-failed"));
    }

    #[test]
    fn early_exit_stops_descendants() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("mount.log");
        let leaked = temp.path().join("leaked");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "(sleep 0.4; echo leaked > \"$1\") & exit 1", "probe"])
            .arg(&leaked);
        assert!(start(&mut cmd, &log, Duration::from_secs(3), || false).is_err());
        std::thread::sleep(Duration::from_millis(700));
        assert!(!leaked.exists(), "a descendant survived startup failure");
    }

    #[test]
    fn timeout_reaps_child_and_preserves_existing_log() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("mount.log");
        let pidfile = temp.path().join("pid");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "echo $$ > \"$1\"; exec sleep 30", "probe"])
            .arg(&pidfile);
        let now = Instant::now();
        let error = start(&mut cmd, &log, Duration::from_millis(250), || false).unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert!(now.elapsed() < Duration::from_secs(3));
        let pid = fs::read_to_string(pidfile).unwrap();
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .output()
                .unwrap()
                .status
                .success()
        );
        fs::write(&log, "keep me").unwrap();
        assert!(
            start(
                &mut Command::new("/bin/true"),
                &log,
                Duration::from_secs(1),
                || true
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(log).unwrap(), "keep me");
    }
}
