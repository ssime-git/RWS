use crate::workspace::Workspace;
use std::{
    fs,
    path::{Path, PathBuf},
};

const LABEL_PREFIX: &str = "io.rws.mount.";

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn plist(binary: &Path, config: &Path, workspace: &Workspace) -> Result<String, String> {
    if !binary.is_absolute() || !config.is_absolute() {
        return Err("autostart requires absolute binary and config paths".into());
    }
    let binary = binary.to_str().ok_or("RWS binary path must be UTF-8")?;
    let config = config.to_str().ok_or("RWS config path must be UTF-8")?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL_PREFIX}{}</string>
<key>ProgramArguments</key><array><string>{}</string><string>--config</string><string>{}</string><string>mount</string><string>{}</string><string>--fskit</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>30</integer>
</dict></plist>
"#,
        xml(&workspace.name),
        xml(binary),
        xml(config),
        xml(&workspace.name)
    ))
}

pub fn install(
    directory: &Path,
    binary: &Path,
    config: &Path,
    workspaces: &[Workspace],
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(directory).map_err(|e| format!("create LaunchAgents directory: {e}"))?;
    let mut paths = Vec::new();
    for workspace in workspaces {
        let path = directory.join(format!("{LABEL_PREFIX}{}.plist", workspace.name));
        fs::write(&path, plist(binary, config, workspace)?)
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plist_retries_failed_mounts_only() {
        let w = Workspace {
            name: "demo".into(),
            host: "u@h".into(),
            remote_root: "/x".into(),
            mount_root: "/Volumes/RWS-demo".into(),
        };
        let text = plist(Path::new("/opt/rws"), Path::new("/opt/config.json"), &w).unwrap();
        assert!(text.contains("<key>RunAtLoad</key><true/>"));
        assert!(text.contains("<key>SuccessfulExit</key><false/>"));
        assert!(text.contains("<string>--fskit</string>"));
    }
}
