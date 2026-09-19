//! Resolve registered mounts and scope Git's Mac paths to one remote command.
use crate::{
    config::{Config, resolve_existing_ancestor},
    workspace::Workspace,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn resolve_directory(
    config: &Config,
    directory: &Path,
) -> Result<Option<(Workspace, String)>, String> {
    if !directory.is_absolute() {
        return Err(
            "working directory must be an absolute filesystem path, not a worktree:// URI".into(),
        );
    }
    let physical = directory
        .canonicalize()
        .map_err(|e| format!("working directory {}: {e}", directory.display()))?;
    if !physical.is_dir() {
        return Err("working directory must be a directory".into());
    }
    let ancestors: Vec<PathBuf> = directory
        .ancestors()
        .skip(1)
        .map(|p| {
            p.canonicalize()
                .map_err(|e| format!("resolve directory ancestor: {e}"))
        })
        .collect::<Result<_, _>>()?;
    let mut matches = Vec::new();
    for w in &config.workspaces {
        let root = resolve_existing_ancestor(&w.mount_root)?;
        if (directory.starts_with(&w.mount_root) || ancestors.iter().any(|p| p.starts_with(&root)))
            && !physical.starts_with(&root)
        {
            return Err(
                "working directory escapes its registered RWS workspace through a symlink".into(),
            );
        }
        if physical.starts_with(&root) {
            matches.push((w.clone(), w.remote_path(&physical)?));
        }
    }
    match matches.len() {
        0 => {
            if directory.starts_with("/Volumes")
                && directory
                    .components()
                    .nth(2)
                    .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with("RWS-"))
            {
                return Err("unregistered RWS volume; register it before running commands; no local fallback".into());
            }
            Ok(None)
        }
        1 => Ok(matches.pop()),
        _ => Err("working directory matches multiple RWS workspaces".into()),
    }
}

fn git_path(w: &Workspace, base: &Path, value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err("unsupported Git metadata path".into());
    }
    let path = Path::new(value);
    if path.is_absolute()
        && !path.starts_with(&w.mount_root)
        && let Ok(suffix) = path.strip_prefix(&w.remote_root)
    {
        return Ok(w.mount_root.join(suffix));
    }
    Ok(base.join(path))
}

/// Override Git only for this command's repository. No metadata is rewritten.
/// A caller must start a new invocation when switching to another repository.
pub fn git_environment(w: &Workspace, directory: &Path) -> Result<Vec<(String, String)>, String> {
    let root = w.mount_root.canonicalize().map_err(|e| e.to_string())?;
    let mut current = directory.canonicalize().map_err(|e| e.to_string())?;
    if !current.starts_with(&root) {
        return Err("Git directory is outside workspace".into());
    }
    loop {
        let dotgit = current.join(".git");
        let metadata = match fs::metadata(&dotgit) {
            Ok(metadata) => Some(metadata),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("read Git metadata: {e}")),
        };
        if let Some(metadata) = metadata {
            let gitdir = if metadata.is_dir() {
                dotgit
            } else if metadata.is_file() {
                let bytes = fs::read_to_string(&dotgit).map_err(|e| e.to_string())?;
                let value = bytes
                    .strip_prefix("gitdir: ")
                    .ok_or("unsupported .git file (expected gitdir:)")?;
                git_path(w, &current, value.trim_end_matches(['\r', '\n']))?
            } else {
                return Err("unsupported .git file type".into());
            };
            let gitdir = gitdir
                .canonicalize()
                .map_err(|e| format!("Git directory: {e}"))?;
            let remote_git=w.remote_path(&gitdir).map_err(|_|"Git metadata is outside the registered workspace; refusing to invent a remote path")?;
            let common = match fs::read_to_string(gitdir.join("commondir")) {
                Ok(value) => git_path(w, &gitdir, value.trim_end_matches(['\r', '\n']))?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => gitdir.clone(),
                Err(e) => return Err(format!("Git common directory: {e}")),
            };
            let remote_common = w
                .remote_path(&common)
                .map_err(|_| "Git common directory is outside the registered workspace")?;
            let mut env = vec![
                ("GIT_DIR".into(), remote_git),
                ("GIT_WORK_TREE".into(), w.remote_path(&current)?),
                ("GIT_COMMON_DIR".into(), remote_common),
            ];
            // Git remote URLs are multi-valued: remote.NAME.url overrides do not
            // replace the first URL. insteadOf translates mounted path prefixes.
            let mut prefixes = vec![w.mount_root.clone()];
            if root != w.mount_root {
                prefixes.push(root);
            }
            env.push(("GIT_CONFIG_COUNT".into(), prefixes.len().to_string()));
            for (i, prefix) in prefixes.iter().enumerate() {
                env.push((
                    format!("GIT_CONFIG_KEY_{i}"),
                    format!("url.{}/.insteadOf", w.remote_root.trim_end_matches('/')),
                ));
                env.push((
                    format!("GIT_CONFIG_VALUE_{i}"),
                    format!(
                        "{}/",
                        prefix
                            .to_str()
                            .ok_or("mount path must be UTF-8")?
                            .trim_end_matches('/')
                    ),
                ));
            }
            return Ok(env);
        }
        if current == root {
            return Ok(vec![]);
        }
        current.pop();
    }
}
