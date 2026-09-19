use rws::{transport::remote_command, workspace::Workspace};
use std::{path::PathBuf, process::Command};
fn workspace(root: PathBuf) -> Workspace {
    Workspace {
        name: "demo".into(),
        host: "dev@server".into(),
        remote_root: "/srv/my project".into(),
        mount_root: root,
    }
}
#[test]
fn maps_nested_paths_but_rejects_sibling_prefix_and_symlink_escape() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    let w = workspace(root.clone());
    assert_eq!(
        w.remote_path(&root.join("src")).unwrap(),
        "/srv/my project/src"
    );
    let sibling = dir.path().join("project-other");
    std::fs::create_dir(&sibling).unwrap();
    assert!(w.remote_path(&sibling).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&sibling, root.join("escape")).unwrap();
        assert!(w.remote_path(&root.join("escape")).is_err());
    }
}
#[test]
fn rejects_option_injection_and_relative_or_traversing_roots() {
    let mut w = workspace("/tmp/rws-demo".into());
    assert!(w.validate().is_ok());
    for host in [
        "-oProxyCommand=bad",
        "host name",
        "host;id",
        "",
        "user@@host",
    ] {
        w.host = host.into();
        assert!(w.validate().is_err(), "{host}");
    }
    w.host = "dev@server".into();
    for path in ["relative", "/srv/../etc", "/srv\0bad"] {
        w.remote_root = path.into();
        assert!(w.validate().is_err(), "{path}");
    }
}
#[test]
fn command_round_trips_literal_arguments_through_posix_shell() {
    let dir = tempfile::tempdir().unwrap();
    let special = dir.path().join("space ' quote");
    std::fs::create_dir(&special).unwrap();
    let values = [
        "two words",
        "a'b",
        "$(touch NEVER)",
        "*",
        "line\nbreak",
        "日本語",
        "",
    ];
    let mut argv = vec!["printf".into(), "%s\\000".into()];
    argv.extend(values.map(String::from));
    let script = remote_command(special.to_str().unwrap(), &argv).unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected = values
        .iter()
        .flat_map(|s| s.as_bytes().iter().copied().chain([0]))
        .collect::<Vec<_>>();
    assert_eq!(output.stdout, expected);
    let script = remote_command(special.to_str().unwrap(), &["pwd".into()]).unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    assert_eq!(
        PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
            .canonicalize()
            .unwrap(),
        special.canonicalize().unwrap()
    );
    assert!(remote_command("/tmp", &[]).is_err());
    assert!(remote_command("/tmp", &["bad\0arg".into()]).is_err());
}
