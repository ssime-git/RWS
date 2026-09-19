use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub name: String,
    pub host: String,
    pub remote_root: String,
    pub mount_root: PathBuf,
}

impl Workspace {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err("workspace name must contain only letters, digits, _ or -".into());
        }
        if self.host.is_empty()
            || self.host.starts_with('-')
            || self.host.matches('@').count() > 1
            || self.host.split('@').any(str::is_empty)
            || !self
                .host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
        {
            return Err(
                "use an SSH alias or user@host (configure ports and IPv6 in SSH config)".into(),
            );
        }
        for path in [Path::new(&self.remote_root), &self.mount_root] {
            if !path.is_absolute()
                || path.components().any(|c| matches!(c, Component::ParentDir))
                || path.to_str().is_none_or(|s| s.contains('\0'))
            {
                return Err(
                    "workspace roots must be absolute UTF-8 paths without .. or NUL".into(),
                );
            }
        }
        if self.mount_root.parent().is_none() {
            return Err("the filesystem root cannot be a mount point".into());
        }
        Ok(())
    }

    pub fn remote_path(&self, local: &Path) -> Result<String, String> {
        self.validate()?;
        let root = self
            .mount_root
            .canonicalize()
            .map_err(|e| format!("mount path: {e}"))?;
        let local = local
            .canonicalize()
            .map_err(|e| format!("working directory: {e}"))?;
        let suffix = local
            .strip_prefix(root)
            .map_err(|_| "path is outside workspace")?;
        let suffix = suffix.to_str().ok_or("path is not UTF-8")?;
        if suffix.is_empty() {
            Ok(self.remote_root.clone())
        } else {
            Ok(format!(
                "{}/{}",
                self.remote_root.trim_end_matches('/'),
                suffix
            ))
        }
    }
}
