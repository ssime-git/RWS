//! Read-only diagnostics and explicit, evidence-based maintenance.
#[path = "launchd_status.rs"]
mod launchd_status;
use super::*;
use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
struct Check {
    id: String,
    status: &'static str,
    detail: String,
}
fn check(id: impl Into<String>, result: Result<String, String>) -> Check {
    match result {
        Ok(detail) => Check {
            id: id.into(),
            status: "ok",
            detail,
        },
        Err(detail) => Check {
            id: id.into(),
            status: "error",
            detail,
        },
    }
}

#[derive(Serialize)]
struct Report {
    version: u32,
    report_path: Option<PathBuf>,
    timestamp: u64,
    configuration: PathBuf,
    before: Vec<Check>,
    actions: Vec<Check>,
    after: Option<Vec<Check>>,
}

fn diagnose(path: &Path, selected: Option<&str>) -> Vec<Check> {
    let config = match Config::load_existing(path) {
        Ok(config) => config,
        Err(error) => return vec![check("configuration", Err(error))],
    };
    let mut checks = vec![check("configuration", Ok(path.display().to_string()))];
    if let Some(name) = selected
        && let Err(error) = config.find(name)
    {
        checks.push(check("workspace", Err(error)));
        return checks;
    }
    for tool in ["ssh", "sftp"] {
        checks.push(check(
            tool,
            if available(tool) {
                Ok("available".into())
            } else {
                Err("missing".into())
            },
        ));
    }
    if config.mount.nfs {
        checks.push(check(
            "native_nfs",
            if Path::new("/sbin/mount_nfs").is_file() {
                Ok("macOS native NFS client available".into())
            } else {
                Err("macOS native NFS client is missing".into())
            },
        ));
    } else {
        checks.push(check("sshfs", check_sshfs(path, &config)));
    }
    match canonical_layout() {
        Ok(layout) => checks.push(check(
            "durable_binary",
            layout.require_binary().map(|p| p.display().to_string()),
        )),
        Err(error) => checks.push(check("durable_binary", Err(error))),
    }
    match integration_checks(path) {
        Ok(results) => checks.extend(results),
        Err(error) => checks.push(check("integrations", Err(error))),
    }
    checks.push(check(
        "launch_agent",
        (|| {
            let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
            let layout = canonical_layout()?;
            match rws::autostart::status(&home.join("Library/LaunchAgents"), &layout.binary())? {
                rws::autostart::Status::Absent => {
                    Ok("not installed; login remount is not configured".into())
                }
                rws::autostart::Status::Custom => {
                    Ok("custom or disabled plist preserved; not reconciled".into())
                }
                rws::autostart::Status::Current => launchd_status::verify(
                    &layout.binary(),
                    &home.join("Library/LaunchAgents/io.rws.mounts.plist"),
                ),
                rws::autostart::Status::Stale => {
                    Err("stale on-disk configuration; run repair (effective at next login)".into())
                }
            }
        })(),
    ));
    for workspace in config
        .workspaces
        .iter()
        .filter(|w| selected.is_none_or(|name| name == w.name))
    {
        let result = match rws::lifecycle::identity(&workspace.mount_root) {
            Ok(Some(identity)) if rws::lifecycle::verified(path, &config, workspace, &identity) => {
                rws::lifecycle::probe_health(&workspace.mount_root, Duration::from_secs(4))
                    .map(|()| "verified and responsive".into())
                    .map_err(|e| {
                        format!(
                            "verified but unresponsive: {e}; use repair --mounts --workspace {}",
                            workspace.name
                        )
                    })
            }
            Ok(Some(_)) => Err("unverified mount; not eligible for automatic repair".into()),
            Ok(None) => Err("disconnected".into()),
            Err(error) => Err(error),
        };
        checks.push(check(format!("mount:{}", workspace.name), result));
        let ssh = (|| {
            let mut command = Command::new("ssh");
            command.args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=1",
            ]);
            command.args(ssh_args(
                &workspace.host,
                remote_command(&workspace.remote_root, &["pwd".into()])?,
                false,
            ));
            let output = rws::process::output(&mut command, Duration::from_secs(12))?;
            if output.status.success() {
                Ok(format!(
                    "reachable: {}",
                    String::from_utf8_lossy(&output.stdout).trim()
                ))
            } else {
                Err(format!(
                    "SSH failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        })();
        checks.push(check(format!("ssh:{}", workspace.name), ssh));
    }
    checks
}

fn integration_checks(path: &Path) -> Result<Vec<Check>, String> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let layout = canonical_layout()?;
    let zshrc = std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .unwrap_or(home)
        .join(".zshrc");
    let text = match fs::read_to_string(&zshrc) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    let managed: Vec<_> = text
        .lines()
        .filter(|line| {
            line.contains(rws::shell_hook::MARKER) && !line.trim_start().starts_with('#')
        })
        .collect();
    let result = if managed.is_empty() {
        Ok("not enabled (left unchanged)".into())
    } else if !managed.iter().all(|line| manages_configuration(line, path)) {
        Ok("custom configuration retained; not reconciled by this setup".into())
    } else if rws::shell_hook::references_current(&zshrc, &layout.binary(), Some(path))? {
        Ok("durable references".into())
    } else {
        Err("stale or duplicate managed hook references; run repair".into())
    };
    let target = rws::agent_rules::default_delta_rules_path()?;
    let rules = if !rws::agent_rules::managed_block_present(&target)? {
        Ok("not enabled (left unchanged)".into())
    } else if !rules_for_configuration(&target, path)? {
        Ok("custom configuration retained; not reconciled by this setup".into())
    } else if rws::agent_rules::references_current(path, &layout.binary(), &target)? {
        Ok("durable references; rules do not redirect Delta internal processes".into())
    } else {
        Err("stale managed Delta references; run repair".into())
    };
    Ok(vec![
        check("shell_hook", result),
        check("delta_rules", rules),
    ])
}

// Refresh this configuration or the legacy development setup only. An explicit
// unrelated configuration is a user choice, not automatically stale state.
pub(super) fn manages_configuration(text: &str, path: &Path) -> bool {
    let mut found = false;
    for line in text.lines().map(str::trim_start) {
        let command = if let Some(command) = line.strip_prefix("eval \"$(") {
            command
        } else if line.starts_with('\'') {
            line
        } else {
            continue;
        };
        let Some((_, rest)) = shell_word(command) else {
            return false;
        };
        let Some((option, rest)) = shell_word(rest) else {
            return false;
        };
        if option == "hook" && rest.trim_start().starts_with("zsh)\"") {
            let Ok(layout) = canonical_layout() else {
                return false;
            };
            let canonical = layout.config_path();
            if path != canonical
                && !matches!((fs::canonicalize(path), fs::canonicalize(&canonical)), (Ok(a), Ok(b)) if a == b)
            {
                return false;
            }
            found = true;
            continue;
        }
        if option != "--config" {
            return false;
        }
        let Some((config, _)) = shell_word(rest) else {
            return false;
        };
        // Only the configuration argument can authorize migration. Executable
        // paths and trailing comments can mention unrelated legacy locations.
        let same_path = Path::new(&config) == path
            || matches!((fs::canonicalize(&config), fs::canonicalize(path)), (Ok(a), Ok(b)) if a == b);
        if !same_path && !config.contains("/.rws-local/") {
            return false;
        }
        found = true;
    }
    found
}

// Decode the single quotes and escaped quotes emitted by our generators.
// Reject expansions and unsupported shell syntax instead of guessing.
fn shell_word(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let mut result = String::new();
    let mut chars = input.char_indices();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\'' => {
                let mut closed = false;
                for (_, quoted) in chars.by_ref() {
                    if quoted == '\'' {
                        closed = true;
                        break;
                    }
                    result.push(quoted);
                }
                if !closed {
                    return None;
                }
            }
            '\\' => result.push(chars.next()?.1),
            c if c.is_whitespace() => return Some((result, &input[index..])),
            '"' | '$' | '`' | ';' | '|' | '&' | '(' | ')' | '<' | '>' => return None,
            c => result.push(c),
        }
    }
    Some((result, ""))
}

pub(super) fn rules_for_configuration(target: &Path, path: &Path) -> Result<bool, String> {
    let text = fs::read_to_string(target).map_err(|e| e.to_string())?;
    let block = text
        .split("<!-- BEGIN RWS REMOTE EXECUTION -->")
        .nth(1)
        .and_then(|s| s.split("<!-- END RWS REMOTE EXECUTION -->").next())
        .ok_or("missing managed rules block")?;
    Ok(manages_configuration(block, path))
}

pub(super) fn refresh_integrations(path: &Path) -> Result<(), String> {
    let layout = canonical_layout()?;
    let binary = layout.require_binary()?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let zshrc = std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .unwrap_or(home)
        .join(".zshrc");
    let text = match fs::read_to_string(&zshrc) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    let managed: Vec<_> = text
        .lines()
        .filter(|line| {
            line.contains(rws::shell_hook::MARKER) && !line.trim_start().starts_with('#')
        })
        .collect();
    if managed.iter().all(|line| manages_configuration(line, path)) {
        rws::shell_hook::refresh_existing(&zshrc, &binary, Some(path))?;
    }
    let target = rws::agent_rules::default_delta_rules_path()?;
    if rws::agent_rules::managed_block_present(&target)? && rules_for_configuration(&target, path)?
    {
        rws::agent_rules::install(path, &binary, &target)?;
    }
    refresh_launch_agent()?;
    Ok(())
}

pub(super) fn refresh_launch_agent() -> Result<(), String> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let layout = canonical_layout()?;
    rws::autostart::refresh_existing(&home.join("Library/LaunchAgents"), &layout.binary())?;
    Ok(())
}

pub(super) fn execute(
    path: &Path,
    selected: Option<&str>,
    json: bool,
    report_path: Option<&Path>,
    repair: bool,
    mounts: bool,
) -> Result<i32, String> {
    let automatic_report = if repair && report_path.is_none() {
        let directory = canonical_layout()?
            .config_path()
            .parent()
            .unwrap()
            .join("diagnostics");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(&directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("diagnostics must be a real directory, not a symlink".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        Some(directory.join(format!("repair-{}-{stamp}.json", std::process::id())))
    } else {
        None
    };
    let report_path = report_path.or(automatic_report.as_deref());
    // Reserve the report before any repair; failure to record must not cause mutation.
    let mut file = if let Some(target) = report_path {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Some(
            options
                .open(target)
                .map_err(|e| format!("create report {}: {e}", target.display()))?,
        )
    } else {
        None
    };
    let mut report = Report {
        version: 1,
        report_path: report_path.map(Path::to_path_buf),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
        configuration: path.to_path_buf(),
        before: diagnose(path, selected),
        actions: Vec::new(),
        after: None,
    };
    if let Some(file) = file.as_mut() {
        serde_json::to_writer_pretty(&mut *file, &report).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    // Keep the original evidence even if the process crashes while saving its
    // final report. Never truncate or replace this before-repair snapshot.
    if repair && let Some(target) = report_path {
        let snapshot = target.with_extension("before.json");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&snapshot)
            .map_err(|e| format!("create snapshot {}: {e}", snapshot.display()))?;
        serde_json::to_writer_pretty(&mut file, &report).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    if repair
        && report
            .before
            .iter()
            .all(|c| !matches!(c.id.as_str(), "configuration" | "workspace") || c.status == "ok")
    {
        report.actions.push(check(
            "refresh_integrations",
            refresh_integrations(path).map(|()| "existing managed integrations refreshed".into()),
        ));
        if mounts {
            let config = Config::load_existing(path)?;
            for w in config
                .workspaces
                .iter()
                .filter(|w| selected.is_none_or(|name| name == w.name))
            {
                let health = report
                    .before
                    .iter()
                    .find(|c| c.id == format!("mount:{}", w.name));
                if health.is_some_and(|c| c.status == "ok") {
                    continue;
                }
                let ssh_ok = report
                    .before
                    .iter()
                    .any(|c| c.id == format!("ssh:{}", w.name) && c.status == "ok");
                let eligible = health.is_some_and(|c| {
                    c.detail == "disconnected" || c.detail.starts_with("verified but unresponsive:")
                });
                if !eligible || !ssh_ok {
                    report.actions.push(check(format!("repair:{}", w.name), Err("not repaired: requires reachable SSH and a disconnected or verified dead mount".into())));
                    continue;
                }
                let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
                command
                    .arg("--config")
                    .arg(path)
                    .args(["connect", &w.name, "--if-desired"]);
                if health.is_some_and(|c| c.detail.starts_with("verified but unresponsive:")) {
                    command.arg("--repair");
                }
                let result =
                    rws::process::output(&mut command, Duration::from_secs(120)).and_then(|o| {
                        if o.status.success() {
                            Ok(String::from_utf8_lossy(&o.stdout).into_owned())
                        } else {
                            Err(String::from_utf8_lossy(&o.stderr).into_owned())
                        }
                    });
                report
                    .actions
                    .push(check(format!("repair:{}", w.name), result));
            }
        }
        report.after = Some(diagnose(path, selected));
    }
    let failed = report
        .after
        .as_ref()
        .unwrap_or(&report.before)
        .iter()
        .chain(&report.actions)
        .any(|c| c.status == "error");
    let serialized = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(file) = file.as_mut() {
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        file.set_len(0).map_err(|e| e.to_string())?;
        file.write_all(serialized.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    if json {
        println!("{serialized}");
    } else {
        for (phase, checks) in [
            ("before", Some(&report.before)),
            ("actions", Some(&report.actions)),
            ("after", report.after.as_ref()),
        ] {
            if let Some(checks) = checks {
                for c in checks {
                    println!("{phase} {} [{}]: {}", c.id, c.status, c.detail);
                }
            }
        }
        if let Some(target) = report_path {
            println!("Report: {}", target.display());
        }
        println!(
            "FSKit service restarts are never automatic. Mount identity alone does not prove working I/O."
        );
    }
    Ok(i32::from(failed))
}

#[cfg(test)]
mod migration_authority_tests {
    use super::*;

    #[test]
    fn configuration_authority_comes_from_the_argument_only() {
        let current = Path::new("/durable/config's.json");
        let custom = Path::new("/opt/custom.json");
        let hook = |binary: &str, config: &Path| {
            rws::shell_hook::install_line(Path::new(binary), Some(config)).unwrap()
        };
        assert!(manages_configuration(&hook("/opt/rws", current), current));
        assert!(manages_configuration(
            &hook("/opt/rws", Path::new("/old/.rws-local/setup.json")),
            current
        ));
        assert!(!manages_configuration(
            &hook("/old/.rws-local/rws", custom),
            current
        ));
        assert!(!manages_configuration(
            &format!(
                "{} # migrated from /old/.rws-local/setup.json",
                hook("/opt/rws", custom)
            ),
            current
        ));
        assert!(!manages_configuration(
            &format!(
                "{} # --config '/durable/config'\\''s.json'",
                hook("/opt/rws", custom)
            ),
            current
        ));
    }

    #[test]
    fn rules_migration_ignores_unrelated_paths_and_rejects_mixed_configurations() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("AGENT.md");
        let config = temp.path().join("config's.json");
        rws::agent_rules::install(&config, Path::new("/old/.rws-local/rws"), &target).unwrap();
        assert!(rules_for_configuration(&target, &config).unwrap());
        assert!(!rules_for_configuration(&target, Path::new("/other/config.json")).unwrap());
        let text = fs::read_to_string(&target).unwrap();
        fs::write(
            &target,
            text.replacen("--config", "--config '/custom/config.json' ignored", 1),
        )
        .unwrap();
        assert!(!rules_for_configuration(&target, &config).unwrap());
    }
}
