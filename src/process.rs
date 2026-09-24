//! Deadline-bounded maintenance commands. Unix children get a process group so
//! timeout cleanup includes descendants that retain inherited pipe handles.
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

/// Run with the caller's configured standard streams and return any exit status.
pub fn run(command: &mut Command, timeout: Duration) -> Result<ExitStatus, String> {
    let start = Instant::now();
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = spawn(command, &program)?;
    let result = poll(&mut child, start, timeout, &program, || Ok(true));
    if result.is_err() {
        terminate(child);
    }
    result
}

/// Run a program that must read from the foreground terminal, such as SSH
/// prompting for remote sudo. It must inherit the caller's process group:
/// placing it in a new group makes terminal input deliver SIGTTIN instead.
/// SSH has no local descendants to clean up, so timeout kills only its PID.
pub fn run_interactive(command: &mut Command, timeout: Duration) -> Result<ExitStatus, String> {
    let start = Instant::now();
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {program}: {error}"))?;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not poll {program}: {error}"))?
        {
            return Ok(status);
        }
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = thread::Builder::new()
                .name("rws-interactive-reaper".into())
                .spawn(move || {
                    let _ = child.wait();
                });
            return Err(format!(
                "{program} timed out after {timeout:?}; remote completion is unknown"
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(25)));
    }
}

/// Capture both output streams. Pipe draining shares the command's deadline;
/// a descendant keeping a pipe open cannot cause an unbounded reader join.
pub fn output(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    let start = Instant::now();
    let program = command.get_program().to_string_lossy().into_owned();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command, &program)?;
    let captured = Arc::new(AtomicUsize::new(0));
    let stdout = reader(child.stdout.take().expect("piped stdout"), captured.clone());
    let stderr = reader(child.stderr.take().expect("piped stderr"), captured);
    let mut out = None;
    let mut err = None;
    let result = poll(&mut child, start, timeout, &program, || {
        collect(&stdout, &mut out)?;
        collect(&stderr, &mut err)?;
        Ok(out.is_some() && err.is_some())
    });
    match result {
        Ok(status) => Ok(Output {
            status,
            stdout: out.unwrap(),
            stderr: err.unwrap(),
        }),
        Err(error) => {
            terminate(child);
            Err(error)
        }
    }
}

fn spawn(command: &mut Command, program: &str) -> Result<Child, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|error| format!("could not start {program}: {error}"))
}

fn poll(
    child: &mut Child,
    start: Instant,
    timeout: Duration,
    program: &str,
    mut streams_ready: impl FnMut() -> Result<bool, String>,
) -> Result<ExitStatus, String> {
    loop {
        // Keep the group leader's PID reserved while descendants hold pipes.
        // Reaping it early would permit PID reuse before timeout group cleanup.
        let exited = crate::mount::exited(child)
            .map_err(|error| format!("could not poll {program}: {error}"))?;
        let ready =
            streams_ready().map_err(|error| format!("could not read {program} output: {error}"))?;
        if exited && ready {
            // On macOS waitid(WNOWAIT) can report an exited child one poll
            // before std::process::Child exposes its status. Retry within the
            // same deadline rather than converting that transient into a
            // failed command.
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("could not reap {program}: {error}"))?
            {
                return Ok(status);
            }
        }
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(format!("{program} timed out after {timeout:?}"));
        }
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn terminate(mut child: Child) {
    #[cfg(unix)]
    {
        // SAFETY: the child was spawned in its own group whose ID is its PID.
        // A negative PID targets that group, including pipe-holding descendants.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    // Never wait on the caller's thread: a process stuck in kernel I/O may not
    // respond even to SIGKILL. Reap asynchronously without extending the deadline.
    let _ = thread::Builder::new()
        .name("rws-process-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

const OUTPUT_LIMIT: usize = 256 * 1024;

fn reader(
    mut pipe: impl Read + Send + 'static,
    captured: Arc<AtomicUsize>,
) -> Receiver<std::io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = (|| {
            let mut buffer = [0u8; 8192];
            loop {
                let count = match pipe.read(&mut buffer) {
                    Ok(0) => return Ok(bytes),
                    Ok(count) => count,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                };
                if captured.fetch_add(count, Ordering::Relaxed) + count > OUTPUT_LIMIT {
                    return Err(std::io::Error::other(
                        "captured output limit exceeded (256 KiB)",
                    ));
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
        })();
        let _ = sender.send(result);
    });
    receiver
}

fn collect(
    receiver: &Receiver<std::io::Result<Vec<u8>>>,
    slot: &mut Option<Vec<u8>>,
) -> Result<(), String> {
    if slot.is_none() {
        match receiver.try_recv() {
            Ok(result) => *slot = Some(result.map_err(|error| error.to_string())?),
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => return Err("output reader disconnected".into()),
        }
    }
    Ok(())
}

// Test list: normal/nonzero exit; spawn failure; configured stdio; bounded
// timeout and descendant termination; captured output; descendant-held pipes.

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Stdio;
    use std::time::Instant;

    #[test]
    fn interactive_runner_reports_status_and_bounds_time() {
        let status = run_interactive(
            Command::new("/bin/sh").args(["-c", "exit 7"]),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(status.code(), Some(7));
        let started = Instant::now();
        let error = run_interactive(
            Command::new("/bin/sh").args(["-c", "exec sleep 30"]),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn normal_and_nonzero_exit_preserve_stdio() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut command = Command::new("sh");
        command.args(["-c", "printf hello; exit 7"]);
        command.stdout(Stdio::from(file.reopen().unwrap()));
        assert_eq!(
            run(&mut command, Duration::from_secs(2)).unwrap().code(),
            Some(7)
        );
        assert_eq!(fs::read_to_string(file.path()).unwrap(), "hello");
        assert!(
            run(
                Command::new("sh").args(["-c", "exit 0"]),
                Duration::from_secs(2)
            )
            .unwrap()
            .success()
        );
    }

    #[test]
    fn reports_spawn_error_with_program() {
        let error = run(
            &mut Command::new("/nonexistent/rws-test-command"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("/nonexistent/rws-test-command"));
    }

    #[test]
    fn timeout_is_bounded_and_stops_descendants() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("descendant-survived");
        let mut command = Command::new("sh");
        command
            .args(["-c", "(sleep 0.5; printf survived > \"$1\") & wait", "sh"])
            .arg(&marker);
        let start = Instant::now();
        let error = run(&mut command, Duration::from_millis(100)).unwrap_err();
        assert!(error.contains("sh") && error.contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(650));
        assert!(!marker.exists(), "descendant escaped process-group cleanup");
    }

    #[test]
    fn rapid_exits_always_report_their_status() {
        for _ in 0..100 {
            let output = output(
                Command::new("/bin/sh").args(["-c", "printf ready"]),
                Duration::from_secs(2),
            )
            .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"ready");
        }
    }

    #[test]
    fn captures_both_streams_and_nonzero_status() {
        let result = output(
            Command::new("sh").args(["-c", "printf out; printf err >&2; exit 4"]),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(4));
        assert_eq!(result.stdout, b"out");
        assert_eq!(result.stderr, b"err");
    }

    #[test]
    fn capture_rejects_output_over_limit() {
        for script in [
            "head -c 262145 /dev/zero",
            "head -c 131073 /dev/zero; head -c 131073 /dev/zero >&2",
        ] {
            let start = Instant::now();
            let error = output(
                Command::new("sh").args(["-c", script]),
                Duration::from_secs(2),
            )
            .expect_err("excessive captured output should fail");
            assert!(error.contains("output limit"), "{error}");
            assert!(start.elapsed() < Duration::from_secs(1));
        }
    }

    #[test]
    fn descendant_held_pipes_obey_deadline() {
        let start = Instant::now();
        let error = output(
            Command::new("sh").args(["-c", "sleep 5 & exit 0"]),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
