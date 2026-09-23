use crate::workspace::Workspace;
use std::{
    fs,
    path::{Path, PathBuf},
};

const LABEL: &str = "io.rws.mounts";
const PLIST_FILENAME: &str = "io.rws.mounts.plist";
const LEGACY_LABEL_PREFIX: &str = "io.rws.mount.";

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Absent,
    Custom,
    Current,
    Stale,
}

pub fn status(directory: &Path, binary: &Path) -> Result<Status, String> {
    let Some(existing) = existing_managed(directory)? else {
        return Ok(
            if fs::symlink_metadata(directory.join(PLIST_FILENAME)).is_ok() {
                Status::Custom
            } else {
                Status::Absent
            },
        );
    };
    Ok(if existing == plist(binary)? {
        Status::Current
    } else {
        Status::Stale
    })
}

/// Refresh the on-disk configuration only; never bootstrap or enable an agent.
/// Absent, customized, disabled, and symlinked plists are left untouched.
pub fn refresh_existing(directory: &Path, binary: &Path) -> Result<bool, String> {
    use std::{
        io::Write,
        os::unix::fs::OpenOptionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };
    let Some(existing) = existing_managed(directory)? else {
        return Ok(false);
    };
    let updated = plist(binary)?;
    prepare_logs(binary)?;
    if updated == existing {
        return Ok(false);
    }
    let target = directory.join(PLIST_FILENAME);
    let permissions = fs::metadata(&target)
        .map_err(|e| e.to_string())?
        .permissions();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = directory.join(format!(".rws-autostart-{stamp}-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(updated.as_bytes())
            .map_err(|e| e.to_string())?;
        file.set_permissions(permissions)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if existing_managed(directory)?.as_deref() != Some(&existing) {
            return Err("LaunchAgent changed during refresh; left unchanged".into());
        }
        fs::rename(&temporary, &target).map_err(|e| format!("replace LaunchAgent: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map(|()| true)
}

fn existing_managed(directory: &Path) -> Result<Option<String>, String> {
    let target = directory.join(PLIST_FILENAME);
    match fs::symlink_metadata(&target) {
        Ok(m) if !m.is_file() => return Ok(None),
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("inspect LaunchAgent: {e}")),
    }
    let contents = fs::read_to_string(&target).map_err(|e| format!("read LaunchAgent: {e}"))?;
    let Some(encoded_binary) = contents
        .split("<key>ProgramArguments</key><array><string>")
        .nth(1)
        .and_then(|s| s.split("</string>").next())
    else {
        return Ok(None);
    };
    let binary = encoded_binary
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let Ok(current) = plist(Path::new(&binary)) else {
        return Ok(None);
    };
    let legacy: String = current
        .lines()
        .filter(|line| {
            !line.starts_with("<key>StandardOutPath") && !line.starts_with("<key>StandardErrorPath")
        })
        .map(|line| format!("{line}\n"))
        .collect();
    let old = current.replace("<key>StartInterval</key><integer>30</integer>\n", "");
    let old_without_logs = legacy.replace("<key>StartInterval</key><integer>30</integer>\n", "");
    Ok((contents == current
        || contents == legacy
        || contents == old
        || contents == old_without_logs)
        .then_some(contents))
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn plist(binary: &Path) -> Result<String, String> {
    if !binary.is_absolute() {
        return Err("autostart requires an absolute binary path".into());
    }
    let logs = log_directory(binary)?;
    let binary = binary.to_str().ok_or("RWS binary path must be UTF-8")?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>{}</string><string>autostart</string><string>run</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>30</integer>
<key>StartInterval</key><integer>30</integer>
<key>StandardOutPath</key><string>{}</string>
<key>StandardErrorPath</key><string>{}</string>
</dict></plist>
"#,
        xml(binary),
        xml(&logs.join("autostart.log").to_string_lossy()),
        xml(&logs.join("autostart.err.log").to_string_lossy())
    ))
}

fn log_directory(binary: &Path) -> Result<PathBuf, String> {
    let root = binary
        .parent()
        .and_then(Path::parent)
        .ok_or("binary has no installation root")?;
    Ok(root.join("logs"))
}

pub fn is_legacy_plist_filename(path: &Path) -> bool {
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(name) = filename
        .strip_prefix(LEGACY_LABEL_PREFIX)
        .and_then(|name| name.strip_suffix(".plist"))
    else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn cleanup_legacy_plists(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|e| format!("read LaunchAgents directory {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| format!("read LaunchAgents entry: {e}"))?;
        if entry
            .file_type()
            .map_err(|e| format!("read {}: {e}", entry.path().display()))?
            .is_file()
            && is_legacy_plist_filename(&entry.path())
        {
            fs::remove_file(entry.path())
                .map_err(|e| format!("remove {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

pub fn install(
    directory: &Path,
    binary: &Path,
    _config: &Path,
    _workspaces: &[Workspace],
) -> Result<Vec<PathBuf>, String> {
    let contents = plist(binary)?;
    prepare_logs(binary)?;
    fs::create_dir_all(directory).map_err(|e| format!("create LaunchAgents directory: {e}"))?;
    let path = directory.join(PLIST_FILENAME);
    fs::write(&path, contents).map_err(|e| format!("write {}: {e}", path.display()))?;
    cleanup_legacy_plists(directory)?;
    Ok(vec![path])
}

fn prepare_logs(binary: &Path) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let logs = log_directory(binary)?;
    match fs::symlink_metadata(&logs) {
        Ok(m) if !m.is_dir() => {
            return Err("diagnostic log directory must be a real directory, not a symlink".into());
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("inspect diagnostic logs: {e}")),
    }
    fs::create_dir_all(&logs).map_err(|e| format!("create diagnostic logs: {e}"))?;
    let directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(&logs)
        .map_err(|e| e.to_string())?;
    directory
        .set_permissions(fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for name in ["autostart.log", "autostart.err.log"] {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(logs.join(name))
            .map_err(|e| e.to_string())?;
        if !log.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("diagnostic log must be a regular file".into());
        }
        log.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_exact_pre_interval_agents_with_or_without_logs() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("RWS/bin/rws");
        let current = plist(&binary).unwrap();
        for with_logs in [true, false] {
            let old = current
                .lines()
                .filter(|line| {
                    !line.contains("StartInterval")
                        && (with_logs
                            || (!line.contains("StandardOutPath")
                                && !line.contains("StandardErrorPath")))
                })
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            let target = temp.path().join(PLIST_FILENAME);
            fs::write(&target, old).unwrap();
            assert_eq!(status(temp.path(), &binary).unwrap(), Status::Stale);
            assert!(refresh_existing(temp.path(), &binary).unwrap());
            assert_eq!(fs::read_to_string(target).unwrap(), current);
        }
    }

    #[test]
    fn refresh_only_updates_recognized_existing_generic_agent() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("LaunchAgents");
        let binary = temp.path().join("RWS/bin/rws");
        assert!(!refresh_existing(&directory, &binary).unwrap());
        assert!(!directory.exists());
        fs::create_dir(&directory).unwrap();
        let target = directory.join(PLIST_FILENAME);
        fs::write(&target, "custom").unwrap();
        assert!(!refresh_existing(&directory, &binary).unwrap());
        assert_eq!(fs::read_to_string(&target).unwrap(), "custom");
        let legacy = plist(Path::new("/old & setup/bin/rws"))
            .unwrap()
            .lines()
            .filter(|l| !l.contains("StandardOutPath") && !l.contains("StandardErrorPath"))
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        fs::write(&target, legacy).unwrap();
        assert!(refresh_existing(&directory, &binary).unwrap());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            plist(&binary).unwrap()
        );
        let metadata = fs::metadata(&target).unwrap();
        assert!(!refresh_existing(&directory, &binary).unwrap());
        assert_eq!(
            fs::metadata(&target).unwrap().modified().unwrap(),
            metadata.modified().unwrap()
        );
        let disabled = plist(&binary).unwrap().replace(
            "<key>RunAtLoad</key><true/>",
            "<key>RunAtLoad</key><false/>",
        );
        fs::write(&target, &disabled).unwrap();
        assert!(!refresh_existing(&directory, &binary).unwrap());
        assert_eq!(fs::read_to_string(target).unwrap(), disabled);
    }

    #[test]
    fn log_directory_symlink_is_rejected_without_chmod() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("RWS");
        let victim = temp.path().join("other");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&victim).unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&victim, root.join("logs")).unwrap();
        assert!(
            install(
                &temp.path().join("LaunchAgents"),
                &root.join("bin/rws"),
                Path::new("/config"),
                &[]
            )
            .is_err()
        );
        assert_eq!(
            fs::metadata(victim).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn plist_starts_the_generic_autostart_runner() {
        let text = plist(Path::new("/opt/rws")).unwrap();
        assert!(text.contains("<key>StartInterval</key><integer>30</integer>"));
        assert!(text.contains("<key>Label</key><string>io.rws.mounts</string>"));
        assert!(text.contains(
            "<key>ProgramArguments</key><array><string>/opt/rws</string><string>autostart</string><string>run</string></array>"
        ));
        assert!(text.contains("<key>RunAtLoad</key><true/>"));
        assert!(text.contains("<key>SuccessfulExit</key><false/>"));
        assert!(!text.contains("--config"));
        assert!(!text.contains("<string>mount</string>"));
    }

    #[test]
    fn autostart_logs_are_private_and_stable() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("RWS/bin/rws");
        install(
            &temp.path().join("LaunchAgents"),
            &binary,
            &temp.path().join("RWS/config.json"),
            &[],
        )
        .unwrap();
        let text =
            fs::read_to_string(temp.path().join("LaunchAgents/io.rws.mounts.plist")).unwrap();
        assert!(text.contains("StandardOutPath") && text.contains("StandardErrorPath"));
        for name in ["autostart.log", "autostart.err.log"] {
            let log = temp.path().join("RWS/logs").join(name);
            assert_eq!(
                fs::metadata(log).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn plist_requires_an_absolute_binary_path() {
        assert_eq!(
            plist(Path::new("rws")).unwrap_err(),
            "autostart requires an absolute binary path"
        );
    }

    #[test]
    fn recognizes_only_exact_validated_legacy_agent_filenames() {
        for name in ["io.rws.mount.demo.plist", "io.rws.mount.-demo_42.plist"] {
            assert!(is_legacy_plist_filename(Path::new(name)), "{name}");
        }
        for name in [
            "io.rws.mounts.plist",
            "io.rws.mount..plist",
            "io.rws.mount.demo.extra.plist",
            "io.rws.mount.demo",
            "com.example.plist",
            "io.rws.mount.démo.plist",
        ] {
            assert!(!is_legacy_plist_filename(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn install_replaces_legacy_rws_agents_without_touching_other_agents() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("io.rws.mount.demo.plist");
        let unrelated = temp.path().join("com.example.agent.plist");
        fs::write(&legacy, "legacy").unwrap();
        fs::write(&unrelated, "unrelated").unwrap();

        let binary = temp.path().join("installation/bin/rws");
        let paths = install(temp.path(), &binary, Path::new("/opt/config.json"), &[]).unwrap();

        let generic = temp.path().join("io.rws.mounts.plist");
        assert_eq!(paths, vec![generic.clone()]);
        assert!(!legacy.exists());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "unrelated");
        assert_eq!(
            fs::read_to_string(generic).unwrap(),
            plist(&binary).unwrap()
        );
    }
}
