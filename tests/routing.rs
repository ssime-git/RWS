use rws::{config::Config, workspace::Workspace};
use std::{fs, path::Path, process::Command};

fn setup() -> (tempfile::TempDir, Config, Workspace) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("mount");
    fs::create_dir(&root).unwrap();
    let workspace = Workspace {
        name: "demo".into(),
        host: "dev@vm".into(),
        remote_root: "/srv/remote root".into(),
        mount_root: root,
    };
    let config_path = temp.path().join("config.json");
    Config::add(&config_path, workspace.clone()).unwrap();
    let config = Config::load(&config_path).unwrap();
    (temp, config, workspace)
}

#[test]
fn directory_context_is_generic_and_rejects_escape_and_missing_path() {
    let (temp, config, w) = setup();
    let nested = w.mount_root.join("project one/src");
    fs::create_dir_all(&nested).unwrap();
    let selected = rws::routing::resolve_directory(&config, &nested)
        .unwrap()
        .unwrap();
    assert_eq!(selected.1, "/srv/remote root/project one/src");
    assert!(
        rws::routing::resolve_directory(&config, temp.path())
            .unwrap()
            .is_none()
    );
    assert!(rws::routing::resolve_directory(&config, &w.mount_root.join("missing")).is_err());
    std::os::unix::fs::symlink(temp.path(), w.mount_root.join("escape")).unwrap();
    assert!(rws::routing::resolve_directory(&config, &w.mount_root.join("escape")).is_err());
}

#[test]
fn git_context_maps_delta_metadata_and_local_remotes_without_editing_files() {
    let (_temp, _config, w) = setup();
    let checkout = w.mount_root.join("project/.delta/worktrees/run/project");
    let gitdir = w.mount_root.join("project/.delta/clones/run/project.git");
    fs::create_dir_all(checkout.join("src")).unwrap();
    fs::create_dir_all(gitdir.parent().unwrap()).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--bare"])
            .arg(&gitdir)
            .output()
            .unwrap()
            .status
            .success()
    );
    let gitfile = format!("gitdir: {}\n", gitdir.display());
    fs::write(checkout.join(".git"), &gitfile).unwrap();
    let config_path = gitdir.join("config");
    assert!(
        Command::new("git")
            .arg("config")
            .arg("--file")
            .arg(&config_path)
            .args(["remote.local.url"])
            .arg(w.mount_root.join("project/.git"))
            .status()
            .unwrap()
            .success()
    );
    // The local source repo exists, as in a Delta-managed clone.
    fs::create_dir_all(w.mount_root.join("project/.git")).unwrap();
    let before = fs::read(&config_path).unwrap();
    let env = rws::routing::git_environment(&w, &checkout.join("src")).unwrap();
    assert!(env.contains(&(
        "GIT_DIR".into(),
        "/srv/remote root/project/.delta/clones/run/project.git".into()
    )));
    assert!(env.contains(&(
        "GIT_WORK_TREE".into(),
        "/srv/remote root/project/.delta/worktrees/run/project".into()
    )));
    assert!(
        env.iter()
            .any(|(key, value)| key.starts_with("GIT_CONFIG_KEY_")
                && value == "url./srv/remote root/.insteadOf")
    );
    assert_eq!(fs::read_to_string(checkout.join(".git")).unwrap(), gitfile);
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[test]
fn git_metadata_outside_workspace_is_refused() {
    let (temp, _config, w) = setup();
    let outside = temp.path().join("other.git");
    fs::create_dir(&outside).unwrap();
    fs::write(
        w.mount_root.join(".git"),
        format!("gitdir: {}", outside.display()),
    )
    .unwrap();
    assert!(rws::routing::git_environment(&w, &w.mount_root).is_err());
    fs::remove_file(w.mount_root.join(".git")).unwrap();
    assert!(
        rws::routing::git_environment(&w, &w.mount_root)
            .unwrap()
            .is_empty()
    );
    assert!(
        rws::routing::resolve_directory(
            &Config::load(&temp.path().join("empty.json")).unwrap(),
            Path::new("relative")
        )
        .is_err()
    );
}

#[test]
fn git_url_translation_and_linked_worktree_context_work_for_real_git() {
    let (_temp, _config, mut w) = setup();
    // Use the same local path as remote root to execute the generated context locally.
    w.remote_root = w.mount_root.to_string_lossy().into_owned();
    let repo = w.mount_root.join("original");
    assert!(
        Command::new("git")
            .args(["init"])
            .arg(&repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    let commit = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "-c",
            "user.name=RWS Test",
            "-c",
            "user.email=rws@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    let linked = w.mount_root.join("linked");
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "--detach"])
            .arg(&linked)
            .output()
            .unwrap()
            .status
            .success()
    );
    let environment = rws::routing::git_environment(&w, &linked).unwrap();
    let out = Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain"])
        .current_dir(&linked)
        .envs(environment.clone())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    assert!(
        environment
            .iter()
            .any(|(key, value)| key == "GIT_COMMON_DIR" && value.ends_with("original/.git"))
    );
    let source = repo.join(".git");
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&linked)
            .args(["remote", "add", "local"])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    w.remote_root = "/srv/mapped".into();
    let mapping = rws::routing::git_environment(&w, &linked).unwrap();
    let out = Command::new("git")
        .arg("-C")
        .arg(&linked)
        .args(["remote", "get-url", "local"])
        .envs(
            mapping
                .into_iter()
                .filter(|(key, _)| key.starts_with("GIT_CONFIG_")),
        )
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "/srv/mapped/original/.git"
    );
}

#[test]
fn alias_then_escape_does_not_become_a_local_project() {
    let (temp, config, w) = setup();
    let alias = temp.path().join("alias");
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&w.mount_root, &alias).unwrap();
    std::os::unix::fs::symlink(&outside, w.mount_root.join("escape")).unwrap();
    assert!(rws::routing::resolve_directory(&config, &alias.join("escape")).is_err());
}

#[test]
fn linux_absolute_gitdir_and_commondir_are_mapped_back_for_inspection() {
    let (_temp, _config, w) = setup();
    let project = w.mount_root.join("project");
    let metadata = w.mount_root.join("metadata");
    let common = w.mount_root.join("common");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&metadata).unwrap();
    fs::create_dir(&common).unwrap();
    fs::write(project.join(".git"), "gitdir: /srv/remote root/metadata\n").unwrap();
    fs::write(metadata.join("commondir"), "/srv/remote root/common\n").unwrap();
    let environment = rws::routing::git_environment(&w, &project).unwrap();
    assert!(environment.contains(&("GIT_DIR".into(), "/srv/remote root/metadata".into())));
    assert!(environment.contains(&("GIT_COMMON_DIR".into(), "/srv/remote root/common".into())));
}

#[test]
fn different_registered_mount_selects_its_own_host_and_root() {
    let (temp, _config, _w) = setup();
    let root = temp.path().join("second");
    fs::create_dir_all(root.join("project/src")).unwrap();
    let path = temp.path().join("config.json");
    Config::add(
        &path,
        Workspace {
            name: "other".into(),
            host: "another-vm".into(),
            remote_root: "/opt/projects".into(),
            mount_root: root.clone(),
        },
    )
    .unwrap();
    let config = Config::load(&path).unwrap();
    let (selected, remote) = rws::routing::resolve_directory(&config, &root.join("project/src"))
        .unwrap()
        .unwrap();
    assert_eq!(selected.host, "another-vm");
    assert_eq!(remote, "/opt/projects/project/src");
}
