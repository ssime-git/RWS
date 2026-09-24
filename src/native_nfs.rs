//! Native macOS NFS transport. Keeping this separate from SSHFS makes the
//! mount command and its identity rules independently testable.
use crate::{lifecycle::MountIdentity, workspace::Workspace};
use std::path::Path;

pub fn source(workspace: &Workspace) -> Result<String, String> {
    workspace.validate()?;
    let host = workspace
        .host
        .rsplit_once('@')
        .map_or(workspace.host.as_str(), |(_, host)| host);
    if host.is_empty() || host.contains(':') {
        return Err("NFS requires a DNS or Tailscale host name without a port".into());
    }
    // The dedicated Linux export is the NFSv4 pseudoroot (fsid=0). SSH
    // attestation verifies that this root is the configured remote directory.
    Ok(format!("{host}:/"))
}

pub fn mount_args(workspace: &Workspace) -> Result<Vec<String>, String> {
    validate_mountpoint_path(workspace)?;
    Ok(vec![
        "-o".into(),
        // Hard mounts preserve write errors rather than returning success after
        // a network timeout. Recovery must be observed via bounded health probes.
        "vers=4.1,tcp,hard,nfc,port=2049".into(),
        source(workspace)?,
        workspace.mount_root.to_string_lossy().into_owned(),
    ])
}

pub fn matches_source(workspace: &Workspace, identity: &MountIdentity) -> bool {
    if identity.filesystem != "nfs" {
        return false;
    }
    let Ok(source) = source(workspace) else {
        return false;
    };
    // The mount table may replace a DNS name with an IP address. An SSH
    // challenge attests the first mount; receipts pin the full identity later.
    identity.source == source
        || identity
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
            mount_args(&workspace()).unwrap(),
            [
                "-o",
                "vers=4.1,tcp,hard,nfc,port=2049",
                "devbox:/",
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
        assert!(mount_args(&w).is_err());
        w.mount_root = PathBuf::from("/private/tmp/RWS-demo");
        assert!(validate_mountpoint_path(&w).is_err());
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
