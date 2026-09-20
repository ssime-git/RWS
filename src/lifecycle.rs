//! Mount identity is read from the OS mount table, without touching remote files.
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountIdentity {
    pub source: String,
    pub filesystem: String,
    pub id: Vec<u8>,
}

#[cfg(target_os = "macos")]
pub fn identity(root: &Path) -> Result<Option<MountIdentity>, String> {
    use std::ffi::CStr;
    // Normalize the local parent only; resolving the mounted root can perform
    // remote I/O and stall when disconnected. This handles /tmp -> /private/tmp.
    let root = match (root.parent(), root.file_name()) {
        (Some(parent), Some(name)) => crate::config::resolve_existing_ancestor(parent)?.join(name),
        _ => root.to_path_buf(),
    };
    // MNT_NOWAIT reads the kernel snapshot even when a remote mount is stalled.
    unsafe {
        let count = libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT);
        if count < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let capacity = count as usize + 32;
        let mut entries: Vec<libc::statfs> = (0..capacity).map(|_| std::mem::zeroed()).collect();
        let count = libc::getfsstat(
            entries.as_mut_ptr(),
            (entries.len() * std::mem::size_of::<libc::statfs>()) as i32,
            libc::MNT_NOWAIT,
        );
        if count < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if count as usize >= capacity {
            return Err("mount table changed; retry".into());
        }
        for entry in entries.iter().take(count as usize) {
            let path = CStr::from_ptr(entry.f_mntonname.as_ptr()).to_string_lossy();
            if Path::new(path.as_ref()) == root {
                let id = std::slice::from_raw_parts(
                    std::ptr::addr_of!(entry.f_fsid).cast::<u8>(),
                    std::mem::size_of_val(&entry.f_fsid),
                )
                .to_vec();
                return Ok(Some(MountIdentity {
                    source: CStr::from_ptr(entry.f_mntfromname.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                    filesystem: CStr::from_ptr(entry.f_fstypename.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                    id,
                }));
            }
        }
    }
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub fn identity(_root: &Path) -> Result<Option<MountIdentity>, String> {
    Err("mount inspection is currently supported on macOS only".into())
}

/// Bounded I/O probe of a mounted root. A network mount can stay in the mount
/// table while operations fail or hang (a "zombie" after link loss) — and a
/// wedge can be partial: a cached root listing still answers while deeper
/// operations hang forever. The probe therefore looks up a random,
/// nonexistent name under the root: a negative lookup for a fresh name
/// cannot be served from any cache and must round-trip to the filesystem.
/// NotFound is the healthy answer. The probe runs in a helper thread so a
/// hang becomes a timeout, never a stalled caller; nothing is ever created.
pub fn probe_health(root: &Path, timeout: std::time::Duration) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let marker = root.join(format!(".rws-health-{}-{stamp}", std::process::id()));
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = match fs::symlink_metadata(&marker) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            // A name collision still proves the lookup round-tripped.
            Ok(_) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(_) => Err(format!(
            "no response within {} seconds",
            timeout.as_secs().max(1)
        )),
    }
}

/// Whether a bare path lookup on the mount point answers at all within the
/// timeout. After an ejection, a healthy system answers instantly (directory
/// present, or a clean NotFound); no answer means the FSKit service itself is
/// wedged and starting a new mount would hang — the caller must stop with a
/// remediation message instead.
pub fn mountpoint_answers(root: &Path, timeout: std::time::Duration) -> Result<(), String> {
    let root = root.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let answer = match fs::symlink_metadata(&root) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        let _ = sender.send(answer);
    });
    receiver
        .recv_timeout(timeout)
        .map_err(|_| {
            format!(
                "mount point did not answer within {} seconds",
                timeout.as_secs().max(1)
            )
        })
        .and_then(std::convert::identity)
}

/// Find SSHFS server processes started for exactly this workspace: the
/// command line must begin with the configured executable followed by this
/// source and mount point, as `connect` spawns them. Nothing looser matches.
fn stale_server_pids(program: &Path, source: &str, mount_root: &Path) -> Result<Vec<i32>, String> {
    let program = program.to_string_lossy();
    let mount_root = mount_root.to_string_lossy();
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,command="])
        .output()
        .map_err(|e| format!("list processes: {e}"))?;
    if !output.status.success() {
        return Err("list processes: ps failed".into());
    }
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((pid, command)) = line.trim_start().split_once(' ') else {
            continue;
        };
        // ps cannot show embedded spaces unambiguously; this whitespace
        // tokenization intentionally matches only the executable, source
        // and mount point as adjacent tokens (an interpreter such as
        // /bin/sh may precede a script in tests). Paths with spaces simply
        // never match — the repair then reports the survivor instead of
        // killing a wrong process.
        let words: Vec<&str> = command.split_whitespace().collect();
        let matches = |offset: usize| {
            words.get(offset) == Some(&program.as_ref())
                && words.get(offset + 1) == Some(&source)
                && words.get(offset + 2) == Some(&mount_root.as_ref())
        };
        if (matches(0) || matches(1))
            && let Ok(pid) = pid.parse::<i32>()
        {
            pids.push(pid);
        }
    }
    Ok(pids)
}

/// Kill this workspace's stale SSHFS servers and wait, within the timeout,
/// until they are gone. Returns how many were terminated. A server that
/// survives the timeout (uninterruptible kernel wait) is an error: the
/// caller must not start a second server on the same mount point.
pub fn terminate_stale_servers(
    program: &Path,
    source: &str,
    mount_root: &Path,
    timeout: std::time::Duration,
) -> Result<usize, String> {
    let pids = stale_server_pids(program, source, mount_root)?;
    for &pid in &pids {
        // Not our child: kill directly; permission errors surface below as
        // the process surviving the deadline.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
    let deadline = std::time::Instant::now() + timeout;
    while !stale_server_pids(program, source, mount_root)?.is_empty() {
        if std::time::Instant::now() >= deadline {
            return Err(
                "a stale SSHFS server refuses to die (likely stuck in the kernel); run: sudo pkill -9 fskitd, or reboot"
                    .into(),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(pids.len())
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    host: String,
    remote: String,
    root: PathBuf,
    identity: MountIdentity,
}
fn receipt_path(config: &Path, workspace: &Workspace) -> PathBuf {
    // Keep receipts separate for separate config files in the same directory.
    let mut directory = config.as_os_str().to_os_string();
    directory.push(".mount-state");
    PathBuf::from(directory).join(format!("{}.json", workspace.name))
}
pub struct OperationLock(PathBuf);
impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn lock(config: &Path, w: &Workspace) -> Result<OperationLock, String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let path = receipt_path(config, w).with_extension("lock");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path)
        .map_err(|e| format!("operation lock {}: {e}; another operation may be active; inspect stale locks after a crash", path.display()))?;
    let guard = OperationLock(path);
    writeln!(file, "{}", std::process::id()).map_err(|e| e.to_string())?;
    Ok(guard)
}
pub fn record(config: &Path, w: &Workspace, identity: MountIdentity) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let path = receipt_path(config, w);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let receipt = Receipt {
        host: w.host.clone(),
        remote: w.remote_root.clone(),
        root: w.mount_root.clone(),
        identity,
    };
    let result = (|| {
        file.write_all(&serde_json::to_vec(&receipt).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn verified(config: &Path, w: &Workspace, actual: &MountIdentity) -> bool {
    fs::read(receipt_path(config, w))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .is_some_and(|r| {
            r.host == w.host
                && r.remote == w.remote_root
                && r.root == w.mount_root
                && r.identity == *actual
        })
}
pub fn forget(config: &Path, w: &Workspace) -> Result<(), String> {
    match fs::remove_file(receipt_path(config, w)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove mount receipt: {e}")),
    }
}

const CREATE_CHALLENGE: &str = "umask 077; mkdir -- \"$1\" || exit 1; if ! printf '%s' \"$2\" > \"$1/proof\"; then rm -f -- \"$1/proof\"; rmdir -- \"$1\"; exit 1; fi";

/// Associate an opaque FSKit volume with its SSH destination using a disposable
/// random challenge. Also supports adopting a mount created by older RWS builds.
pub fn attest(w: &Workspace, expected: &MountIdentity) -> Result<(), String> {
    use std::{io::Read, process::Command, time::Duration};
    let mut random = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|e| e.to_string())?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let directory = format!(".rws-verify-{token}");
    let ssh = |script: &str, args: &[&str]| -> Result<std::process::Output, String> {
        let mut argv = vec![
            "sh".to_string(),
            "-c".into(),
            script.into(),
            "rws-verify".into(),
        ];
        argv.extend(args.iter().map(|arg| arg.to_string()));
        let mut command = Command::new("ssh");
        command.args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "--",
            &w.host,
        ]);
        command.arg(crate::transport::remote_command(&w.remote_root, &argv)?);
        crate::transport::bounded_output(&mut command, Duration::from_secs(12))
    };
    // After an ambiguous disconnect, remove only a challenge with our token.
    // Never delete an existing directory when exclusive mkdir failed.
    let cleanup = || {
        ssh(
            "[ -f \"$1/proof\" ] && [ \"$(cat -- \"$1/proof\")\" = \"$2\" ] && rm -- \"$1/proof\" && rmdir -- \"$1\"",
            &[&directory, &token],
        )
    };
    let created = ssh(CREATE_CHALLENGE, &[&directory, &token]);
    match created {
        Ok(ref output) if output.status.success() => {}
        other => {
            let detail = match other {
                Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
                Err(error) => error,
            };
            let removed = cleanup().is_ok_and(|output| output.status.success());
            return Err(format!(
                "cannot create remote verification challenge (workspace must be writable): {detail}; {} {}/{directory}",
                if removed {
                    "removed"
                } else {
                    "inspect for any remaining challenge at"
                },
                w.remote_root,
            ));
        }
    }
    let result = (|| {
        if identity(&w.mount_root)?.as_ref() != Some(expected) {
            return Err("mount identity changed during verification".into());
        }
        let mut cat = Command::new("/bin/cat");
        cat.arg(w.mount_root.join(&directory).join("proof"));
        let read = crate::transport::bounded_output(&mut cat, Duration::from_secs(12))?;
        if !read.status.success() || read.stdout != token.as_bytes() {
            return Err("mounted files do not match the configured SSH destination".into());
        }
        if identity(&w.mount_root)?.as_ref() != Some(expected) {
            return Err("mount identity changed during verification".into());
        }
        Ok(())
    })();
    if !cleanup().is_ok_and(|output| output.status.success()) {
        return Err(format!(
            "remote verification cleanup failed; inspect {}/{directory}; mount not adopted",
            w.remote_root
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_health_accepts_a_readable_directory_and_reports_failures() {
        let temp = tempfile::tempdir().unwrap();
        assert!(probe_health(temp.path(), std::time::Duration::from_secs(2)).is_ok());
        // An I/O failure (here: permissions) must be reported, not hidden.
        let sealed = temp.path().join("sealed");
        fs::create_dir(&sealed).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o000)).unwrap();
        let result = probe_health(&sealed, std::time::Duration::from_secs(2));
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o700)).unwrap();
        if nix_is_root() {
            return; // root ignores permission bits; the error leg needs a user
        }
        assert!(result.is_err(), "{result:?}");
    }
    fn nix_is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }
    #[test]
    fn stale_servers_are_matched_exactly_and_terminated() {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("sshfs");
        // No exec: the shell must keep the script's argv visible in ps.
        fs::write(&program, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let mount = temp.path().join("mnt");
        let spawn = |source: &str, root: &Path| {
            std::process::Command::new(&program)
                .arg(source)
                .arg(root)
                .args(["-o", "options"])
                .spawn()
                .unwrap()
        };
        let mut target = spawn("dev@host:/srv/data", &mount);
        let mut other_mount = spawn("dev@host:/srv/data", &temp.path().join("other"));
        let mut other_source = spawn("dev@host:/srv/other", &mount);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let ended = terminate_stale_servers(
            &program,
            "dev@host:/srv/data",
            &mount,
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(ended, 1);
        // The exact match is gone; near misses keep running.
        assert!(target.try_wait().unwrap().is_some());
        assert!(other_mount.try_wait().unwrap().is_none());
        assert!(other_source.try_wait().unwrap().is_none());
        let _ = other_mount.kill();
        let _ = other_source.kill();
        let _ = other_mount.wait();
        let _ = other_source.wait();
        let _ = target.wait();
        // Nothing left to end on a second pass.
        assert_eq!(
            terminate_stale_servers(
                &program,
                "dev@host:/srv/data",
                &mount,
                std::time::Duration::from_secs(5),
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn mountpoint_answers_for_present_and_absent_paths() {
        let temp = tempfile::tempdir().unwrap();
        let timeout = std::time::Duration::from_secs(2);
        assert!(mountpoint_answers(temp.path(), timeout).is_ok());
        // A clean NotFound is an answer: the path can be mounted over.
        assert!(mountpoint_answers(&temp.path().join("absent"), timeout).is_ok());
    }

    #[test]
    fn failed_challenge_write_removes_only_its_exclusive_directory() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("challenge");
        let script = format!("printf() {{ return 1; }}; {CREATE_CHALLENGE}");
        let result = std::process::Command::new("/bin/sh")
            .args(["-c", &script, "probe"])
            .arg(&directory)
            .arg("token")
            .status()
            .unwrap();
        assert!(!result.success());
        assert!(!directory.exists());
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("proof"), "existing data").unwrap();
        let result = std::process::Command::new("/bin/sh")
            .args(["-c", CREATE_CHALLENGE, "probe"])
            .arg(&directory)
            .arg("token")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(
            fs::read_to_string(directory.join("proof")).unwrap(),
            "existing data"
        );
    }
    #[test]
    fn receipt_does_not_authorize_a_different_mount_or_destination() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config.json");
        let mut w = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-test".into(),
        };
        let mut mount = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };
        assert!(!verified(&config, &w, &mount));
        record(&config, &w, mount.clone()).unwrap();
        assert!(verified(&config, &w, &mount));
        assert!(!verified(&temp.path().join("other.json"), &w, &mount));
        mount.id = vec![2];
        assert!(!verified(&config, &w, &mount));
        mount.id = vec![1];
        w.remote_root = "/other".into();
        assert!(!verified(&config, &w, &mount));
        forget(&config, &w).unwrap();
        forget(&config, &w).unwrap();
        assert!(!verified(&config, &w, &mount));
    }
}
