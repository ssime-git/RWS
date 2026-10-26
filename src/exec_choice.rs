//! Remember, per workspace and for a limited time, whether the user wants agent
//! commands in a mounted directory to run on the Mac or on the owning VM.
//! The store lives next to the configuration, never inside a mount.
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_TTL_SECS: u64 = 86_400;
pub const TTL_ENV: &str = "RWS_EXEC_CHOICE_TTL_SECS";
const STORE_NAME: &str = "exec-choices.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Local,
    Remote,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Local => "local",
            Mode::Remote => "remote",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    mode: Mode,
    chosen_at: u64,
}

/// A choice that has not expired yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub mode: Mode,
    pub chosen_at: u64,
    pub expires_at: u64,
}

pub fn store_path(config: &Path) -> Result<PathBuf, String> {
    let parent = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent.join(STORE_NAME))
}

/// Lifetime of a choice; overridable for tests through `RWS_EXEC_CHOICE_TTL_SECS`.
pub fn ttl() -> Result<u64, String> {
    match std::env::var(TTL_ENV) {
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => Ok(secs),
            _ => Err(format!(
                "{TTL_ENV} must be a positive number of seconds, got {value:?}"
            )),
        },
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_TTL_SECS),
        Err(error) => Err(format!("{TTL_ENV}: {error}")),
    }
}

fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| e.to_string())
}

fn load(path: &Path) -> Result<BTreeMap<String, Entry>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "invalid execution-choice store {}: {e}; delete it to reset all choices",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

fn live(entry: Entry, now: u64, ttl: u64) -> Option<Choice> {
    let expires_at = entry.chosen_at.saturating_add(ttl);
    (now < expires_at && entry.chosen_at <= now).then_some(Choice {
        mode: entry.mode,
        chosen_at: entry.chosen_at,
        expires_at,
    })
}

fn save(path: &Path, entries: &BTreeMap<String, Entry>) -> Result<(), String> {
    let parent = path.parent().ok_or("choice store must have a parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} must not be a symlink", path.display()));
    }
    let bytes = serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".{STORE_NAME}.{}.{nanos}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| format!("create {}: {e}", temporary.display()))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("write {}: {error}", temporary.display()));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("replace {}: {error}", path.display()));
    }
    Ok(())
}

/// The workspace's current choice; expired entries read as absent.
pub fn get(config: &Path, workspace: &str) -> Result<Option<Choice>, String> {
    let (now, ttl) = (now()?, ttl()?);
    Ok(load(&store_path(config)?)?
        .get(workspace)
        .and_then(|entry| live(*entry, now, ttl)))
}

fn rewrite(
    config: &Path,
    change: impl FnOnce(&mut BTreeMap<String, Entry>, u64),
) -> Result<(), String> {
    let path = store_path(config)?;
    let (now, ttl) = (now()?, ttl()?);
    let mut entries = load(&path)?;
    entries.retain(|_, entry| live(*entry, now, ttl).is_some());
    change(&mut entries, now);
    save(&path, &entries)
}

pub fn set(config: &Path, workspace: &str, mode: Mode) -> Result<Choice, String> {
    rewrite(config, |entries, now| {
        entries.insert(
            workspace.into(),
            Entry {
                mode,
                chosen_at: now,
            },
        );
    })?;
    get(config, workspace)?.ok_or_else(|| "choice was not recorded".into())
}

/// Forget the workspace's choice. Returns whether a live choice existed.
pub fn clear(config: &Path, workspace: &str) -> Result<bool, String> {
    let existed = get(config, workspace)?.is_some();
    rewrite(config, |entries, _| {
        entries.remove(workspace);
    })?;
    Ok(existed)
}

/// `rws context` JSON for a directory inside a verified mount.
pub fn remote_context(
    w: &Workspace,
    cwd: &Path,
    remote_cwd: &str,
    choice: Option<Choice>,
) -> serde_json::Value {
    serde_json::json!({
        "mode": "remote",
        "workspace": w.name,
        "host": w.host,
        "cwd": cwd,
        "remote_cwd": remote_cwd,
        "mount_verified": true,
        "choice": choice.map(|c| c.mode.as_str()),
        "choice_expires_at": choice.map(|c| c.expires_at),
    })
}

/// `rws choice get` JSON.
pub fn describe(w: &Workspace, choice: Option<Choice>) -> serde_json::Value {
    serde_json::json!({
        "workspace": w.name,
        "host": w.host,
        "choice": choice.map(|c| c.mode.as_str()),
        "chosen_at": choice.map(|c| c.chosen_at),
        "choice_expires_at": choice.map(|c| c.expires_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn expiry_is_exclusive_and_ignores_future_entries() {
        let entry = Entry {
            mode: Mode::Remote,
            chosen_at: 100,
        };
        assert_eq!(live(entry, 100, 10).unwrap().expires_at, 110);
        assert!(live(entry, 109, 10).is_some());
        assert!(live(entry, 110, 10).is_none());
        assert!(live(entry, 99, 10).is_none());
    }

    #[test]
    fn set_get_clear_roundtrip_privately_and_prune_expired_entries() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config.json");
        assert_eq!(get(&config, "demo").unwrap(), None);
        let choice = set(&config, "demo", Mode::Local).unwrap();
        assert_eq!(choice.mode, Mode::Local);
        assert_eq!(choice.expires_at, choice.chosen_at + ttl().unwrap());
        assert_eq!(get(&config, "demo").unwrap(), Some(choice));
        let store = temp.path().join(STORE_NAME);
        assert_eq!(
            fs::metadata(&store).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::write(
            &store,
            r#"{"old":{"mode":"remote","chosen_at":1},"demo":{"mode":"local","chosen_at":1}}"#,
        )
        .unwrap();
        assert_eq!(get(&config, "demo").unwrap(), None);
        set(&config, "other", Mode::Remote).unwrap();
        let text = fs::read_to_string(&store).unwrap();
        assert!(
            !text.contains("\"old\"") && !text.contains("\"demo\""),
            "{text}"
        );
        assert!(clear(&config, "other").unwrap());
        assert!(!clear(&config, "other").unwrap());
        assert_eq!(get(&config, "other").unwrap(), None);
    }

    #[test]
    fn context_carries_choice_and_expiry_without_changing_existing_fields() {
        let w = Workspace {
            name: "demo".into(),
            host: "dev@vm".into(),
            remote_root: "/srv".into(),
            mount_root: "/Volumes/demo".into(),
        };
        let none = remote_context(&w, Path::new("/Volumes/demo/a"), "/srv/a", None);
        assert_eq!(none["mode"], "remote");
        assert_eq!(none["host"], "dev@vm");
        assert_eq!(none["remote_cwd"], "/srv/a");
        assert_eq!(none["mount_verified"], true);
        assert!(none["choice"].is_null() && none["choice_expires_at"].is_null());
        let some = remote_context(
            &w,
            Path::new("/Volumes/demo/a"),
            "/srv/a",
            Some(Choice {
                mode: Mode::Remote,
                chosen_at: 5,
                expires_at: 15,
            }),
        );
        assert_eq!(some["choice"], "remote");
        assert_eq!(some["choice_expires_at"], 15);
    }
}
