use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub mount: MountOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_state_generation: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mount_intent: BTreeMap<String, MountIntent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountIntent {
    #[default]
    Connected,
    Paused,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountOptions {
    pub sshfs: Option<String>,
    #[serde(default)]
    pub fskit: bool,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    workspaces: vec![],
                    mount: MountOptions::default(),
                    mount_state_generation: None,
                    mount_intent: BTreeMap::new(),
                });
            }
            Err(e) => return Err(format!("read config: {e}")),
        };
        Self::parse(&bytes)
    }
    /// Routing must never interpret a missing registry as permission to run locally.
    pub fn load_existing(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| {
            format!(
                "read required RWS configuration {}: {e}; refusing local fallback",
                path.display()
            )
        })?;
        Self::parse(&bytes)
    }
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let config: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid config: {e}"))?;
        if config.version != 1 {
            return Err("unsupported config version".into());
        }
        config.mount.validate()?;
        if let Some(generation) = &config.mount_state_generation {
            validate_mount_state_generation(generation)?;
        }
        // Loading must never resolve mount roots: canonicalizing a path under
        // a stalled SSHFS volume blocks in the kernel, which would hang every
        // command reading the configuration. Registration performs the
        // canonical alias check; loading only re-checks what is stored.
        for (i, w) in config.workspaces.iter().enumerate() {
            w.validate()?;
            for other in &config.workspaces[..i] {
                if w.name == other.name
                    || w.mount_root.starts_with(&other.mount_root)
                    || other.mount_root.starts_with(&w.mount_root)
                {
                    return Err("workspace name already exists or mount roots overlap".into());
                }
            }
        }
        Ok(config)
    }
    pub fn find(&self, name: &str) -> Result<&Workspace, String> {
        self.workspaces
            .iter()
            .find(|w| w.name == name)
            .ok_or_else(|| format!("unknown workspace: {name}"))
    }
    pub fn add(path: &Path, workspace: Workspace) -> Result<(), String> {
        workspace.validate()?;
        Self::update(path, |config| {
            for other in &config.workspaces {
                check_pair(&workspace, other)?;
            }
            config.workspaces.push(workspace);
            Ok(())
        })
    }
    pub fn set_mount_options(path: &Path, options: MountOptions) -> Result<(), String> {
        options.validate()?;
        Self::update(path, |config| {
            config.mount = options;
            Ok(())
        })
    }
    pub fn mount_intent(&self, name: &str) -> MountIntent {
        self.mount_intent.get(name).copied().unwrap_or_default()
    }
    pub fn set_mount_intent(path: &Path, name: &str, intent: MountIntent) -> Result<(), String> {
        Self::update(path, |config| {
            config.find(name)?;
            config.mount_intent.insert(name.into(), intent);
            Ok(())
        })
    }
    pub(crate) fn update(
        path: &Path,
        change: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        let resolved = match path.canonicalize() {
            Ok(resolved) => resolved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
            Err(error) => return Err(format!("resolve configuration: {error}")),
        };
        let path = resolved.as_path();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| format!("config directory: {e}"))?;
        let lock_path = path.with_extension("lock");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let _lock = options.open(&lock_path).map_err(|e| {
            format!(
                "config lock {}: {e}; if a prior write crashed, inspect and remove the stale lock",
                lock_path.display()
            )
        })?;
        let _guard = Lock(lock_path);
        let mut config = Self::load(path)?;
        change(&mut config)?;
        let bytes = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = options
            .open(&temporary)
            .map_err(|e| format!("temporary config: {e}"))?;
        let temp_guard = Lock(temporary);
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("write config: {e}"))?;
        fs::rename(&temp_guard.0, path).map_err(|e| format!("replace config: {e}"))?;
        Ok(())
    }
}
fn validate_mount_state_generation(generation: &str) -> Result<(), String> {
    if generation.is_empty()
        || !generation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("mount-state generation must contain only ASCII letters, digits, hyphens, or underscores".into());
    }
    Ok(())
}
impl MountOptions {
    fn validate(&self) -> Result<(), String> {
        if self
            .sshfs
            .as_ref()
            .is_some_and(|p| !Path::new(p).is_absolute() || p.contains('\0'))
        {
            return Err("saved SSHFS executable must be an absolute path without NUL".into());
        }
        Ok(())
    }
}
fn check_pair(a: &Workspace, b: &Workspace) -> Result<(), String> {
    let a_root = resolve_existing_ancestor(&a.mount_root)?;
    let b_root = resolve_existing_ancestor(&b.mount_root)?;
    if a.name == b.name || a_root.starts_with(&b_root) || b_root.starts_with(&a_root) {
        Err("workspace name already exists or mount roots overlap".into())
    } else {
        Ok(())
    }
}
// Mount points may not exist at registration time. Resolve their existing
// ancestor so aliases such as /tmp and /private/tmp cannot hide overlaps.
pub(crate) fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    match path.canonicalize() {
        Ok(resolved) => Ok(resolved),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(format!("unresolvable mount path: {}", path.display()));
            }
            let parent = path.parent().ok_or("mount path has no existing ancestor")?;
            let name = path.file_name().ok_or("invalid mount path")?;
            Ok(resolve_existing_ancestor(parent)?.join(name))
        }
        Err(e) => Err(format!("resolve mount path: {e}")),
    }
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_intent_roundtrips_and_preserves_other_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        fs::write(&path, br#"{"version":1,"workspaces":[{"name":"demo","host":"host","remote_root":"/srv/demo","mount_root":"/Volumes/demo"}],"mount":{"fskit":true,"sshfs":"/opt/sshfs"},"mount_state_generation":"release"}"#).unwrap();
        assert_eq!(
            Config::load(&path).unwrap().mount_intent("demo"),
            MountIntent::Connected
        );
        Config::set_mount_intent(&path, "demo", MountIntent::Paused).unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.mount_intent("demo"), MountIntent::Paused);
        assert!(config.mount.fskit);
        assert_eq!(config.mount_state_generation.as_deref(), Some("release"));
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("\"demo\": \"paused\"")
        );
        let before = fs::read(&path).unwrap();
        assert!(Config::set_mount_intent(&path, "unknown", MountIntent::Connected).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn rejects_an_unsafe_mount_state_generation() {
        let result =
            Config::parse(br#"{"version":1,"workspaces":[],"mount_state_generation":"../other"}"#);

        assert!(matches!(result, Err(ref error) if error.contains("mount-state generation")));
    }
}
