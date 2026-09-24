//! Mount identity is read from the OS mount table, without touching remote files.
use crate::{config::Config, workspace::Workspace};
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
fn receipt_path(config_path: &Path, config: &Config, workspace: &Workspace) -> PathBuf {
    if let Some(directory) = generated_state_directory(config_path, config) {
        return directory.join(format!("{}.json", workspace.name));
    }
    // Keep legacy receipts separate for separate config files in the same directory.
    let mut directory = config_path.as_os_str().to_os_string();
    directory.push(".mount-state");
    PathBuf::from(directory).join(format!("{}.json", workspace.name))
}

fn generated_state_directory(config_path: &Path, config: &Config) -> Option<PathBuf> {
    let generation = config.mount_state_generation.as_ref()?;
    let parent = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Some(
        parent
            .join("mount-state")
            .join(generation)
            .join(config_namespace(config_path)),
    )
}

fn config_namespace(config_path: &Path) -> String {
    let name = config_path
        .file_name()
        .unwrap_or(config_path.as_os_str())
        .as_encoded_bytes();
    let mut namespace = String::from("config-");
    for byte in name {
        namespace.push_str(&format!("{byte:02x}"));
    }
    namespace
}

fn generated_state_directories(config_path: &Path, config: &Config) -> Option<[PathBuf; 3]> {
    let namespace = generated_state_directory(config_path, config)?;
    let generation = namespace.parent()?.to_path_buf();
    let root = generation.parent()?.to_path_buf();
    Some([root, generation, namespace])
}

fn validate_generated_state_directory(directory: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(directory).map_err(|error| {
        format!(
            "inspect generated mount state {}: {error}",
            directory.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "unsafe generated mount state directory: {}",
            directory.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        // SAFETY: geteuid has no pointer arguments or memory safety preconditions.
        let current_user = unsafe { libc::geteuid() };
        if metadata.uid() != current_user || metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "generated mount state directory must be owned by this user and mode 0700: {}",
                directory.display()
            ));
        }
    }
    Ok(())
}

fn prepare_generated_state_directory(config_path: &Path, config: &Config) -> Result<(), String> {
    let Some(directories) = generated_state_directories(config_path, config) else {
        return Ok(());
    };
    for directory in directories {
        match fs::symlink_metadata(&directory) {
            Ok(_) => validate_generated_state_directory(&directory)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(unix)]
                let created = {
                    use std::os::unix::fs::DirBuilderExt;

                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700);
                    builder.create(&directory)
                };
                #[cfg(not(unix))]
                let created = fs::create_dir(&directory);
                created.map_err(|error| {
                    format!(
                        "create generated mount state directory {}: {error}",
                        directory.display()
                    )
                })?;
                validate_generated_state_directory(&directory)?;
            }
            Err(error) => {
                return Err(format!(
                    "inspect generated mount state {}: {error}",
                    directory.display()
                ));
            }
        }
    }
    Ok(())
}

fn generated_state_is_safe(config_path: &Path, config: &Config) -> bool {
    generated_state_directories(config_path, config).is_some_and(|directories| {
        directories
            .iter()
            .all(|directory| validate_generated_state_directory(directory).is_ok())
    })
}

fn durable_receipt_is_safe(receipt: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(receipt) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        // SAFETY: geteuid has no pointer arguments or memory safety preconditions.
        let current_user = unsafe { libc::geteuid() };
        if metadata.uid() != current_user || metadata.permissions().mode() & 0o077 != 0 {
            return false;
        }
    }
    true
}

fn create_receipt_directory(
    config_path: &Path,
    config: &Config,
    receipt: &Path,
) -> Result<(), String> {
    if config.mount_state_generation.is_some() {
        prepare_generated_state_directory(config_path, config)?;
    } else {
        fs::create_dir_all(receipt.parent().unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub struct OperationLock {
    // Never unlink this file: other callers may already have its inode open.
    _advisory: AdvisoryLock,
    legacy: PathBuf,
}
struct AdvisoryLock(fs::File);
impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        // A forked child may hold the same open-file description until exec.
        // Closing our fd alone would leave its flock active in that child.
        // SAFETY: this is a live descriptor for the file locked below.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.legacy);
    }
}
pub fn lock(config_path: &Path, config: &Config, w: &Workspace) -> Result<OperationLock, String> {
    use std::{
        io::Write,
        os::unix::{
            fs::{DirBuilderExt, OpenOptionsExt},
            io::AsRawFd,
        },
    };
    let resolved = match config_path.canonicalize() {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => config_path.to_path_buf(),
        Err(error) => return Err(format!("resolve configuration: {error}")),
    };
    let config_path = resolved.as_path();
    let parent = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = parent.join(format!(
        "{}.mount-operations",
        config_namespace(config_path)
    ));
    match fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(format!("operation lock directory: {e}")),
    }
    validate_generated_state_directory(&directory)?;
    let advisory_path = directory.join(format!("{}.lock", w.name));
    let advisory = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&advisory_path)
        .map_err(|e| format!("operation lock {}: {e}", advisory_path.display()))?;
    if !advisory.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("operation lock must be a regular file".into());
    }
    // SAFETY: flock operates on a live file descriptor and has no pointer arguments.
    if unsafe { libc::flock(advisory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!(
            "operation lock {}: another operation is active; retry after it finishes",
            advisory_path.display()
        ));
    }
    // Own the unlock immediately, including errors before the legacy sentinel
    // is created. OperationLock removes that sentinel before this guard drops.
    let advisory = AdvisoryLock(advisory);
    // Keep the old generation-specific sentinel while operating, so an older
    // RWS binary cannot overlap this operation during an upgrade.
    let fresh = if config_path.exists() {
        Some(Config::load_existing(config_path)?)
    } else {
        None
    };
    let config = fresh.as_ref().unwrap_or(config);
    let path = receipt_path(config_path, config, w).with_extension("lock");
    create_receipt_directory(config_path, config, &path)?;
    recover_dead_legacy_lock(&path)?;
    let mut file = fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path)
        .map_err(|e| format!("operation lock {}: {e}; another operation may be active; inspect stale locks after a crash", path.display()))?;
    let guard = OperationLock {
        _advisory: advisory,
        legacy: path,
    };
    writeln!(file, "{}", std::process::id()).map_err(|e| e.to_string())?;
    Ok(guard)
}

fn recover_dead_legacy_lock(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !metadata.is_file() {
        return Ok(());
    }
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(());
    };
    let Ok(pid) = contents.trim().parse::<libc::pid_t>() else {
        return Ok(());
    };
    if pid <= 0 {
        return Ok(());
    }
    // SAFETY: signal zero only checks existence; no signal is delivered.
    let absent = unsafe { libc::kill(pid, 0) } == -1
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
    if absent
        && fs::symlink_metadata(path)
            .is_ok_and(|current| current.dev() == metadata.dev() && current.ino() == metadata.ino())
        && fs::read_to_string(path).is_ok_and(|current| current == contents)
    {
        fs::remove_file(path).map_err(|e| format!("recover stale operation lock: {e}"))?;
    }
    Ok(())
}
pub fn record(
    config_path: &Path,
    config: &Config,
    w: &Workspace,
    identity: MountIdentity,
) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let path = receipt_path(config_path, config, w);
    create_receipt_directory(config_path, config, &path)?;
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
pub fn verified(
    config_path: &Path,
    config: &Config,
    w: &Workspace,
    actual: &MountIdentity,
) -> bool {
    if config.mount_state_generation.is_some() && !generated_state_is_safe(config_path, config) {
        return false;
    }
    let receipt = receipt_path(config_path, config, w);
    if config.mount_state_generation.is_some() && !durable_receipt_is_safe(&receipt) {
        return false;
    }
    fs::read(receipt)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .is_some_and(|r| {
            r.host == w.host
                && r.remote == w.remote_root
                && r.root == w.mount_root
                && r.identity == *actual
        })
}
pub fn forget(config_path: &Path, config: &Config, w: &Workspace) -> Result<(), String> {
    match fs::remove_file(receipt_path(config_path, config, w)) {
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
    use crate::config::{Config, MountOptions};
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
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: None,
        };
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
        assert!(!verified(&config_path, &config, &w, &mount));
        record(&config_path, &config, &w, mount.clone()).unwrap();
        assert!(verified(&config_path, &config, &w, &mount));
        assert!(!verified(
            &temp.path().join("other.json"),
            &config,
            &w,
            &mount
        ));
        mount.id = vec![2];
        assert!(!verified(&config_path, &config, &w, &mount));
        mount.id = vec![1];
        w.remote_root = "/other".into();
        assert!(!verified(&config_path, &config, &w, &mount));
        forget(&config_path, &config, &w).unwrap();
        forget(&config_path, &config, &w).unwrap();
        assert!(!verified(&config_path, &config, &w, &mount));
    }

    #[test]
    fn receipt_uses_the_configured_durable_generation() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };

        let receipt = temp
            .path()
            .join("mount-state")
            .join("release-20260922")
            .join("config-636f6e6669672e6a736f6e")
            .join("demo.json");
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };

        assert_eq!(receipt_path(&config_path, &config, &workspace), receipt);
        let operation = lock(&config_path, &config, &workspace).unwrap();
        assert!(
            temp.path()
                .join("mount-state")
                .join("release-20260922")
                .join("config-636f6e6669672e6a736f6e")
                .join("demo.lock")
                .exists()
        );
        drop(operation);
        record(&config_path, &config, &workspace, identity.clone()).unwrap();
        assert!(receipt.exists());
        assert!(verified(&config_path, &config, &workspace, &identity));
        forget(&config_path, &config, &workspace).unwrap();
        assert!(!receipt.exists());
        assert!(!verified(&config_path, &config, &workspace, &identity));
    }

    #[test]
    fn operation_lock_survives_generation_changes_and_releases_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let mut config = Config::load(&path).unwrap();
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/demo".into(),
            mount_root: "/Volumes/demo".into(),
        };
        let first = lock(&path, &config, &workspace).unwrap();
        config.mount_state_generation = Some("new".into());
        assert!(lock(&path, &config, &workspace).is_err());
        drop(first);
        lock(&path, &config, &workspace)
            .unwrap_or_else(|error| panic!("reacquire after drop: {error}"));
    }

    #[test]
    fn operation_lock_releases_even_when_a_child_inherits_the_descriptor() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let config = Config::load(&path).unwrap();
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/demo".into(),
            mount_root: "/Volumes/demo".into(),
        };
        let first = lock(&path, &config, &workspace).unwrap();
        let mut pipe = [-1; 2];
        // SAFETY: pipe writes two descriptors to the supplied two-element array.
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        // SAFETY: the child calls only async-signal-safe libc functions then
        // _exit, without touching Rust allocation, unwinding, or other threads.
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            unsafe {
                libc::close(pipe[1]);
                let mut byte = 0_u8;
                libc::read(pipe[0], (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        // The child cannot exit until the parent sends a byte. It therefore
        // keeps the inherited advisory fd open throughout the reacquire below.
        unsafe {
            libc::close(pipe[0]);
        }
        drop(first);
        let reacquired = lock(&path, &config, &workspace);
        // Always release and reap the child before asserting the result.
        unsafe {
            libc::write(pipe[1], b"x".as_ptr().cast(), 1);
            libc::close(pipe[1]);
            libc::waitpid(child, std::ptr::null_mut(), 0);
        }
        reacquired
            .unwrap_or_else(|error| panic!("child retained an inherited operation lock: {error}"));
    }

    #[test]
    fn operation_lock_recovers_dead_legacy_owner_but_refuses_unknown_or_live_owner() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let config = Config::load(&path).unwrap();
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/demo".into(),
            mount_root: "/Volumes/demo".into(),
        };
        let legacy = receipt_path(&path, &config, &workspace).with_extension("lock");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        for owner in ["unknown".to_string(), std::process::id().to_string()] {
            fs::write(&legacy, &owner).unwrap();
            assert!(lock(&path, &config, &workspace).is_err());
            assert_eq!(fs::read_to_string(&legacy).unwrap(), owner);
        }
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        fs::write(&legacy, dead.to_string()).unwrap();
        assert!(lock(&path, &config, &workspace).is_ok());
    }

    #[test]
    fn receipt_without_a_generation_uses_the_legacy_adjacent_directory() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: None,
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };

        assert_eq!(
            receipt_path(&config_path, &config, &workspace),
            temp.path()
                .join("config.json.mount-state")
                .join("demo.json")
        );
    }

    #[test]
    fn durable_configs_in_the_same_directory_do_not_share_receipts_or_locks() {
        let temp = tempfile::tempdir().unwrap();
        let first_path = temp.path().join("first.json");
        let second_path = temp.path().join("second.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };

        let first_receipt = receipt_path(&first_path, &config, &workspace);
        let second_receipt = receipt_path(&second_path, &config, &workspace);
        assert_ne!(first_receipt, second_receipt);
        record(&first_path, &config, &workspace, identity.clone()).unwrap();
        assert!(verified(&first_path, &config, &workspace, &identity));
        assert!(!verified(&second_path, &config, &workspace, &identity));

        let _first_lock = lock(&first_path, &config, &workspace).unwrap();
        let _second_lock = lock(&second_path, &config, &workspace).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn generated_mount_state_directory_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };

        record(&config_path, &config, &workspace, identity).unwrap();
        let receipt = receipt_path(&config_path, &config, &workspace);
        let namespace = receipt.parent().unwrap();
        for directory in [
            namespace,
            namespace.parent().unwrap(),
            namespace.parent().unwrap().parent().unwrap(),
        ] {
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_generated_state_is_rejected_for_writes_and_verification() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let attacker = temp.path().join("attacker");
        fs::create_dir(&attacker).unwrap();
        std::os::unix::fs::symlink(&attacker, temp.path().join("mount-state")).unwrap();
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };
        let receipt = Receipt {
            host: workspace.host.clone(),
            remote: workspace.remote_root.clone(),
            root: workspace.mount_root.clone(),
            identity: identity.clone(),
        };
        let attacker_receipt = attacker
            .join("release-20260922")
            .join(config_namespace(&config_path))
            .join("demo.json");
        fs::create_dir_all(attacker_receipt.parent().unwrap()).unwrap();
        fs::write(&attacker_receipt, serde_json::to_vec(&receipt).unwrap()).unwrap();

        assert!(record(&config_path, &config, &workspace, identity.clone()).is_err());
        assert!(!verified(&config_path, &config, &workspace, &identity));
        assert!(attacker_receipt.exists());
    }

    #[cfg(unix)]
    #[test]
    fn non_private_generated_state_does_not_verify_a_receipt() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };

        record(&config_path, &config, &workspace, identity.clone()).unwrap();
        fs::set_permissions(
            temp.path().join("mount-state"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(!verified(&config_path, &config, &workspace, &identity));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_receipt_does_not_verify() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json");
        let config = Config {
            version: 1,
            workspaces: vec![],
            mount: MountOptions::default(),
            mount_intent: Default::default(),
            mount_state_generation: Some("release-20260922".into()),
        };
        let workspace = Workspace {
            name: "demo".into(),
            host: "host".into(),
            remote_root: "/srv/project".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let identity = MountIdentity {
            source: "macfuse://unique".into(),
            filesystem: "macfuse".into(),
            id: vec![1],
        };

        record(&config_path, &config, &workspace, identity.clone()).unwrap();
        let receipt = receipt_path(&config_path, &config, &workspace);
        let attacker_receipt = temp.path().join("attacker-receipt.json");
        fs::rename(&receipt, &attacker_receipt).unwrap();
        std::os::unix::fs::symlink(&attacker_receipt, &receipt).unwrap();

        assert!(!verified(&config_path, &config, &workspace, &identity));
    }
}
