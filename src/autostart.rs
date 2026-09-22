use crate::workspace::Workspace;
use std::{
    fs,
    path::{Path, PathBuf},
};

const LABEL: &str = "io.rws.mounts";
const PLIST_FILENAME: &str = "io.rws.mounts.plist";
const LEGACY_LABEL_PREFIX: &str = "io.rws.mount.";

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
    let binary = binary.to_str().ok_or("RWS binary path must be UTF-8")?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>{}</string><string>autostart</string><string>run</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>30</integer>
</dict></plist>
"#,
        xml(binary)
    ))
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
    fs::create_dir_all(directory).map_err(|e| format!("create LaunchAgents directory: {e}"))?;
    let path = directory.join(PLIST_FILENAME);
    fs::write(&path, plist(binary)?).map_err(|e| format!("write {}: {e}", path.display()))?;
    cleanup_legacy_plists(directory)?;
    Ok(vec![path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_starts_the_generic_autostart_runner() {
        let text = plist(Path::new("/opt/rws")).unwrap();
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

        let paths = install(
            temp.path(),
            Path::new("/opt/rws"),
            Path::new("/opt/config.json"),
            &[],
        )
        .unwrap();

        let generic = temp.path().join("io.rws.mounts.plist");
        assert_eq!(paths, vec![generic.clone()]);
        assert!(!legacy.exists());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "unrelated");
        assert_eq!(
            fs::read_to_string(generic).unwrap(),
            plist(Path::new("/opt/rws")).unwrap()
        );
    }
}
