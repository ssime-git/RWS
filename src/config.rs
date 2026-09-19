use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub workspaces: Vec<Workspace>,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    workspaces: vec![],
                });
            }
            Err(e) => return Err(format!("read config: {e}")),
        };
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid config: {e}"))?;
        if config.version != 1 {
            return Err("unsupported config version".into());
        }
        for (i, w) in config.workspaces.iter().enumerate() {
            w.validate()?;
            for other in &config.workspaces[..i] {
                check_pair(w, other)?;
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
        for other in &config.workspaces {
            check_pair(&workspace, other)?;
        }
        config.workspaces.push(workspace);
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
fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
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
