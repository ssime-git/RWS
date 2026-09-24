//! Native macOS NFS transport. Keeping this separate from SSHFS makes the
//! mount command and its identity rules independently testable.
use crate::{lifecycle::MountIdentity, workspace::Workspace};
use std::path::Path;

fn parse_ssh_hostname(output: &str) -> Result<String, String> {
    let host = output
        .lines()
        .filter_map(|line| line.split_once(' '))
        .find_map(|(key, value)| (key == "hostname").then_some(value.trim()))
        .ok_or("SSH configuration did not provide a HostName")?;
    if host.is_empty()
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("NFS requires an IPv4 address or DNS name in SSH HostName".into());
    }
    Ok(host.into())
}

/// Resolve the same SSH alias used by remote commands. The NFS client does
/// not read ~/.ssh/config, so passing an unresolved alias to mount_nfs fails.
pub fn resolve_endpoint(workspace: &Workspace) -> Result<String, String> {
    use std::{process::Command, time::Duration};
    workspace.validate()?;
    let output = crate::process::output(
        Command::new("/usr/bin/ssh").args(["-G", "--", &workspace.host]),
        Duration::from_secs(5),
    )?;
    if !output.status.success() {
        return Err(format!("resolve SSH host for NFS: {}", output.status));
    }
    let output = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    parse_ssh_hostname(&output)
}

pub fn source(endpoint: &str) -> Result<String, String> {
    if endpoint.is_empty()
        || !endpoint
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("NFS endpoint must be an IPv4 address or DNS name".into());
    }
    // The dedicated Linux export is the NFSv4 pseudoroot (fsid=0). SSH
    // attestation verifies that this root is the configured remote directory.
    Ok(format!("{endpoint}:/"))
}

pub fn mount_args(workspace: &Workspace, endpoint: &str) -> Result<Vec<String>, String> {
    validate_mountpoint_path(workspace)?;
    Ok(vec![
        "-o".into(),
        // NFSv4.0 with callbacks disabled passed the real Arch export trial.
        // intr permits the test process to be stopped if server I/O stalls.
        "vers=4.0,tcp,hard,intr,nocallback,nfc,port=2049".into(),
        source(endpoint)?,
        workspace.mount_root.to_string_lossy().into_owned(),
    ])
}

/// A newly mounted macOS NFS volume can take one failed first read to finish
/// negotiating, particularly while another export mounts concurrently. Retry
/// only that bounded read timeout. Any mismatch, identity change, or failed
/// remote challenge cleanup remains a hard failure.
pub fn attest(w: &Workspace, identity: &MountIdentity) -> Result<(), String> {
    retry_initial_read_timeout(|| crate::lifecycle::attest(w, identity))
}

fn retry_initial_read_timeout(mut check: impl FnMut() -> Result<(), String>) -> Result<(), String> {
    match check() {
        Err(first) if first.contains("/bin/cat timed out after 12s") => {
            eprintln!("NFS initial proof read timed out; retrying once");
            check().map_err(|second| {
                format!("initial NFS proof read timed out; retry failed: {second}")
            })
        }
        result => result,
    }
}

pub fn matches_source(_workspace: &Workspace, identity: &MountIdentity) -> bool {
    identity.filesystem == "nfs"
        && identity
            .source
            .rsplit_once(':')
            .is_some_and(|(host, export)| !host.is_empty() && export == "/")
}

/// Only RWS-owned volume names are eligible for privileged creation.
pub fn validate_mountpoint_path(workspace: &Workspace) -> Result<&Path, String> {
    workspace.validate()?;
    let root = workspace.mount_root.as_path();
    if root.parent() != Some(Path::new("/Volumes"))
        || root.file_name().and_then(|name| name.to_str())
            != Some(format!("RWS-{}", workspace.name).as_str())
    {
        return Err("native NFS mount point must be /Volumes/RWS-NAME".into());
    }
    Ok(root)
}

/// Create exactly one absent volume directory through a directory file
/// descriptor. A symlink or renamed path cannot redirect the ownership change.
#[cfg(target_os = "macos")]
pub fn prepare_mountpoint_as_root(
    workspace: &Workspace,
    owner_uid: libc::uid_t,
    owner_gid: libc::gid_t,
) -> Result<(), String> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
    };
    if unsafe { libc::geteuid() } != 0 {
        return Err("NFS mount-point helper requires administrator authorization".into());
    }
    validate_mountpoint_path(workspace)?;
    let parent = CString::new("/Volumes").unwrap();
    let directory = unsafe {
        libc::open(
            parent.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if directory < 0 {
        return Err(format!(
            "open /Volumes: {}",
            std::io::Error::last_os_error()
        ));
    }
    let directory = unsafe { std::fs::File::from_raw_fd(directory) };
    let name = CString::new(format!("RWS-{}", workspace.name)).map_err(|e| e.to_string())?;
    let created = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) } == 0;
    if !created && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
        return Err(format!(
            "create NFS mount point: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mounted = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if mounted < 0 {
        return Err(format!(
            "open new NFS mount point: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mounted = unsafe { std::fs::File::from_raw_fd(mounted) };
    if !created {
        if crate::lifecycle::identity(&workspace.mount_root)?.is_some() {
            return Err("NFS mount point became mounted; leaving it untouched".into());
        }
        // Inspect through the opened descriptor: a path swap cannot make us
        // change ownership of a different directory.
        let duplicate = unsafe { libc::dup(mounted.as_raw_fd()) };
        if duplicate < 0 {
            return Err(format!(
                "inspect NFS mount point: {}",
                std::io::Error::last_os_error()
            ));
        }
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            unsafe { libc::close(duplicate) };
            return Err(format!(
                "inspect NFS mount point: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut nonempty = false;
        loop {
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                break;
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() != b"." && name.to_bytes() != b".." {
                nonempty = true;
                break;
            }
        }
        unsafe { libc::closedir(stream) };
        if nonempty {
            return Err("existing NFS mount directory is not empty; leaving it untouched".into());
        }
    }
    if unsafe { libc::fchown(mounted.as_raw_fd(), owner_uid, owner_gid) } != 0 {
        return Err(format!(
            "own NFS mount point: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { libc::fchmod(mounted.as_raw_fd(), 0o700) } != 0 {
        return Err(format!(
            "secure NFS mount point: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace() -> Workspace {
        Workspace {
            name: "demo".into(),
            host: "user@devbox".into(),
            remote_root: "/home/user/Projects".into(),
            mount_root: PathBuf::from("/Volumes/RWS-demo"),
        }
    }

    #[test]
    fn command_uses_native_nfs_and_preserves_exact_paths() {
        assert_eq!(
            mount_args(&workspace(), "100.106.23.6").unwrap(),
            [
                "-o",
                "vers=4.0,tcp,hard,intr,nocallback,nfc,port=2049",
                "100.106.23.6:/",
                "/Volumes/RWS-demo"
            ]
        );
    }

    #[test]
    fn privileged_path_is_limited_to_exact_rws_volume() {
        let mut w = workspace();
        assert!(validate_mountpoint_path(&w).is_ok());
        w.mount_root = PathBuf::from("/Volumes/Other");
        assert!(validate_mountpoint_path(&w).is_err());
        assert!(mount_args(&w, "100.106.23.6").is_err());
        w.mount_root = PathBuf::from("/private/tmp/RWS-demo");
        assert!(validate_mountpoint_path(&w).is_err());
    }

    #[test]
    fn ssh_hostname_resolution_rejects_invalid_endpoints() {
        assert_eq!(
            parse_ssh_hostname("user razer\nhostname 100.106.23.6\n").unwrap(),
            "100.106.23.6"
        );
        assert!(parse_ssh_hostname("hostname host:/other\n").is_err());
        assert!(source("host:/other").is_err());
    }

    #[test]
    fn retries_only_a_transient_initial_nfs_read_timeout() {
        let mut attempts = 0;
        assert!(
            retry_initial_read_timeout(|| {
                attempts += 1;
                if attempts == 1 {
                    Err("/bin/cat timed out after 12s".into())
                } else {
                    Ok(())
                }
            })
            .is_ok()
        );
        assert_eq!(attempts, 2);
        let mut attempts = 0;
        let result = retry_initial_read_timeout(|| {
            attempts += 1;
            Err("mounted files do not match the configured SSH destination".into())
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn nfs_identity_requires_matching_export_and_filesystem() {
        let mut identity = MountIdentity {
            source: "devbox:/".into(),
            filesystem: "nfs".into(),
            id: vec![1],
        };
        assert!(matches_source(&workspace(), &identity));
        identity.source = "devbox:/other".into();
        assert!(!matches_source(&workspace(), &identity));
        identity.source = "devbox:/".into();
        identity.filesystem = "smbfs".into();
        assert!(!matches_source(&workspace(), &identity));
    }
}
