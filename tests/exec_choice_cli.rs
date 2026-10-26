use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const AGENT_ENV: [&str; 9] = [
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "PI_CODING_AGENT_DIR",
    "CLINE_DIR",
    "DELTA_CONFIG_DIR",
    "OneDrive",
    "OneDriveConsumer",
    "OneDriveCommercial",
    "RWS_EXEC_CHOICE_TTL_SECS",
];

/// Run with a private HOME and no agent overrides, so nothing reaches the real home.
fn rws(home: &Path, config: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rws"));
    for name in AGENT_ENV {
        command.env_remove(name);
    }
    command
        .env("HOME", home)
        .envs(envs.iter().copied())
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
}

struct Setup {
    temp: tempfile::TempDir,
    config: PathBuf,
    project: PathBuf,
}

fn setup(mount: Option<&Path>) -> Setup {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("state/config.json");
    let root = mount
        .map(Path::to_path_buf)
        .unwrap_or(temp.path().join("mount"));
    let project = temp.path().join("mount/project");
    std::fs::create_dir_all(&project).unwrap();
    let out = rws(
        temp.path(),
        &config,
        &[],
        &[
            "workspace",
            "add",
            "demo",
            "--ssh",
            "dev@vm",
            "--remote",
            "/srv/demo",
            "--mount",
            root.to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Setup {
        temp,
        config,
        project,
    }
}

impl Setup {
    fn run(&self, envs: &[(&str, &str)], args: &[&str]) -> Output {
        rws(self.temp.path(), &self.config, envs, args)
    }
    fn choice(&self, envs: &[(&str, &str)], action: &str, extra: &[&str]) -> Output {
        let mut args = vec!["choice", action, "--cwd", self.project.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.run(envs, &args)
    }
    fn store(&self) -> PathBuf {
        self.config.parent().unwrap().join("exec-choices.json")
    }
}

#[test]
fn choice_set_get_clear_is_per_workspace_and_stored_privately_outside_the_mount() {
    let s = setup(None);
    let empty = ok(&s.choice(&[], "get", &[]));
    assert_eq!(empty["workspace"], "demo");
    assert_eq!(empty["host"], "dev@vm");
    assert!(empty["choice"].is_null() && empty["choice_expires_at"].is_null());

    let set = ok(&s.choice(&[], "set", &["--mode", "remote"]));
    assert_eq!(set["choice"], "remote");
    let expires = set["choice_expires_at"].as_u64().unwrap();
    assert_eq!(expires, set["chosen_at"].as_u64().unwrap() + 86_400);
    assert_eq!(ok(&s.choice(&[], "get", &[]))["choice"], "remote");
    assert_eq!(
        std::fs::metadata(s.store()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read_dir(&s.project).unwrap().count(), 0);

    // Switching overwrites the previous choice.
    assert_eq!(
        ok(&s.choice(&[], "set", &["--mode", "local"]))["choice"],
        "local"
    );
    assert_eq!(ok(&s.choice(&[], "get", &[]))["choice"], "local");

    assert_eq!(ok(&s.choice(&[], "clear", &[]))["cleared"], true);
    assert!(ok(&s.choice(&[], "get", &[]))["choice"].is_null());
    assert_eq!(ok(&s.choice(&[], "clear", &[]))["cleared"], false);

    assert!(!s.choice(&[], "set", &["--mode", "vm"]).status.success());
    let outside = s.run(
        &[],
        &["choice", "get", "--cwd", s.temp.path().to_str().unwrap()],
    );
    assert!(!outside.status.success());
    assert!(
        String::from_utf8_lossy(&outside.stderr).contains("not inside a registered RWS mount"),
        "{}",
        String::from_utf8_lossy(&outside.stderr)
    );
}

#[test]
fn expired_choices_read_as_absent_and_are_pruned_on_write() {
    let s = setup(None);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::create_dir_all(s.config.parent().unwrap()).unwrap();
    std::fs::write(
        s.store(),
        format!(
            r#"{{"demo":{{"mode":"remote","chosen_at":{}}},"gone":{{"mode":"local","chosen_at":{}}}}}"#,
            now - 100,
            now - 100
        ),
    )
    .unwrap();
    let short = [("RWS_EXEC_CHOICE_TTL_SECS", "50")];
    let long = [("RWS_EXEC_CHOICE_TTL_SECS", "1000")];
    assert!(ok(&s.choice(&short, "get", &[]))["choice"].is_null());
    let live = ok(&s.choice(&long, "get", &[]));
    assert_eq!(live["choice"], "remote");
    assert_eq!(live["choice_expires_at"].as_u64().unwrap(), now + 900);
    // Default TTL (24h) keeps a 100-second-old choice.
    assert_eq!(ok(&s.choice(&[], "get", &[]))["choice"], "remote");
    assert!(
        !s.choice(&[("RWS_EXEC_CHOICE_TTL_SECS", "soon")], "get", &[])
            .status
            .success()
    );

    ok(&s.choice(&short, "set", &["--mode", "local"]));
    let text = std::fs::read_to_string(s.store()).unwrap();
    assert!(!text.contains("\"gone\""), "{text}");
    assert!(text.contains("\"demo\""), "{text}");
}

/// The macOS data volume is a real mount, so a workspace registered there can
/// be verified without SSHFS and exercise the remote `context` output.
#[cfg(target_os = "macos")]
#[test]
fn remote_context_reports_the_current_choice_and_its_expiry() {
    let data = Path::new("/System/Volumes/Data");
    let s = setup(Some(data));
    let project = s.project.canonicalize().unwrap();
    let cwd = data.join(project.strip_prefix("/").unwrap());
    let cwd = cwd.to_str().unwrap();
    let config = rws::config::Config::load_existing(&s.config).unwrap();
    let workspace = config.find("demo").unwrap();
    let identity = rws::lifecycle::identity(data).unwrap().unwrap();
    rws::lifecycle::record(&s.config, &config, workspace, identity).unwrap();
    let before = ok(&s.run(&[], &["context", "--cwd", cwd]));
    assert_eq!(before["mode"], "remote");
    assert_eq!(before["workspace"], "demo");
    assert_eq!(before["host"], "dev@vm");
    assert_eq!(before["mount_verified"], true);
    assert!(
        before["remote_cwd"]
            .as_str()
            .unwrap()
            .starts_with("/srv/demo/")
    );
    assert!(before["choice"].is_null() && before["choice_expires_at"].is_null());
    let set = ok(&s.run(&[], &["choice", "set", "--cwd", cwd, "--mode", "local"]));
    let after = ok(&s.run(&[], &["context", "--cwd", cwd]));
    assert_eq!(after["choice"], "local");
    assert_eq!(after["choice_expires_at"], set["choice_expires_at"]);
}

fn agent_dirs(home: &Path) {
    for dir in [
        ".claude",
        ".codex",
        ".pi/agent",
        ".cline",
        ".config/devin",
        ".config/delta",
    ] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
}

const DEFAULTS: [(&str, &str); 6] = [
    ("claude", ".claude/CLAUDE.md"),
    ("codex", ".codex/AGENTS.md"),
    ("pi", ".pi/agent/AGENTS.md"),
    ("cline", "Documents/Cline/Rules/rws-remote-execution.md"),
    ("devin", ".config/devin/AGENTS.md"),
    ("delta", ".config/delta/AGENT.md"),
];

#[test]
fn agent_rules_install_for_each_agent_preserving_content_idempotently() {
    let s = setup(None);
    let home = s.temp.path();
    agent_dirs(home);
    for (agent, relative) in DEFAULTS {
        let target = home.join(relative);
        if agent != "cline" {
            std::fs::write(&target, format!("My {agent} rules.\n")).unwrap();
        }
        let out = s.run(&[], &["agent-rules", "--agent", agent]);
        assert!(
            out.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(target.to_str().unwrap()),
            "{agent}"
        );
        let first = std::fs::read_to_string(&target).unwrap();
        if agent != "cline" {
            assert!(
                first.starts_with(&format!("My {agent} rules.\n")),
                "{first}"
            );
        }
        assert_eq!(first.matches("BEGIN RWS REMOTE EXECUTION").count(), 1);
        assert!(first.contains("ask the user exactly one question"));
        assert!(first.contains("choice set --cwd"));
        assert!(first.contains(env!("CARGO_BIN_EXE_rws")));
        assert!(
            s.run(&[], &["agent-rules", "--agent", agent])
                .status
                .success()
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), first, "{agent}");
    }
    // One backup per agent whose file had prior content; none for idempotent reruns.
    let backups = s.config.parent().unwrap().join("agent-rule-backups");
    assert_eq!(std::fs::read_dir(backups).unwrap().count(), 5);
}

#[test]
fn agent_rules_follow_override_files_and_environment() {
    let s = setup(None);
    let home = s.temp.path();
    agent_dirs(home);
    std::fs::write(home.join(".codex/AGENTS.override.md"), "override\n").unwrap();
    std::fs::write(home.join(".pi/agent/CLAUDE.md"), "pi claude\n").unwrap();
    let custom = home.join("claude-config");
    let custom_str = custom.to_str().unwrap();
    for agent in ["codex", "pi"] {
        ok(&s.run(&[], &["agent-rules", "--agent", agent]));
    }
    ok(&s.run(
        &[("CLAUDE_CONFIG_DIR", custom_str)],
        &["agent-rules", "--agent", "claude"],
    ));
    for (path, prefix) in [
        (home.join(".codex/AGENTS.override.md"), "override\n"),
        (home.join(".pi/agent/CLAUDE.md"), "pi claude\n"),
        (custom.join("CLAUDE.md"), ""),
    ] {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(prefix), "{}", path.display());
        assert!(text.contains("BEGIN RWS REMOTE EXECUTION"));
    }
    assert!(!home.join(".codex/AGENTS.md").exists());
    assert!(!home.join(".pi/agent/AGENTS.md").exists());
    assert!(!home.join(".claude/CLAUDE.md").exists());
}

#[test]
fn agent_rules_all_list_target_and_if_installed() {
    let s = setup(None);
    let home = s.temp.path();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::create_dir_all(home.join(".pi/agent")).unwrap();
    let out = s.run(&[], &["agent-rules", "--all"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for skipped in ["claude", "cline", "devin", "delta"] {
        assert!(stdout.contains(&format!("Skipped {skipped}")), "{stdout}");
    }
    assert!(home.join(".codex/AGENTS.md").exists());
    assert!(home.join(".pi/agent/AGENTS.md").exists());
    assert!(!home.join(".claude").exists());
    assert!(!home.join("Documents").exists());

    let list = ok(&s.run(&[], &["agent-rules", "--list"]));
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 6);
    let codex = rows.iter().find(|r| r["agent"] == "codex").unwrap();
    assert_eq!(codex["detected"], true);
    assert_eq!(codex["installed"], true);
    let claude = rows.iter().find(|r| r["agent"] == "claude").unwrap();
    assert_eq!(claude["detected"], false);
    assert_eq!(claude["installed"], false);

    let target = home.join("elsewhere/rules.md");
    let target_str = target.to_str().unwrap();
    assert!(
        !s.run(&[], &["agent-rules", "--target", target_str])
            .status
            .success()
    );
    assert!(
        !s.run(
            &[],
            &[
                "agent-rules",
                "--agent",
                "pi",
                "--agent",
                "codex",
                "--target",
                target_str
            ]
        )
        .status
        .success()
    );
    assert!(!s.run(&[], &["agent-rules"]).status.success());
    assert!(
        !s.run(&[], &["agent-rules", "--agent", "nope"])
            .status
            .success()
    );
    ok(&s.run(
        &[],
        &[
            "agent-rules",
            "--agent",
            "devin",
            "--target",
            target_str,
            "--if-installed",
        ],
    ));
    assert!(!target.exists());
    ok(&s.run(
        &[],
        &["agent-rules", "--agent", "devin", "--target", target_str],
    ));
    assert!(target.exists());
}

#[test]
fn delta_rules_alias_installs_like_agent_rules_delta() {
    let s = setup(None);
    let home = s.temp.path();
    std::fs::create_dir_all(home.join(".config/delta")).unwrap();
    let out = s.run(&[], &["delta-rules"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Global Delta RWS rules installed"));
    let via_alias = std::fs::read_to_string(home.join(".config/delta/AGENT.md")).unwrap();
    std::fs::remove_file(home.join(".config/delta/AGENT.md")).unwrap();
    ok(&s.run(&[], &["agent-rules", "--agent", "delta"]));
    assert_eq!(
        std::fs::read_to_string(home.join(".config/delta/AGENT.md")).unwrap(),
        via_alias
    );
    let output = home.join("custom/AGENT.md");
    ok(&s.run(&[], &["delta-rules", "--output", output.to_str().unwrap()]));
    assert!(output.exists());
}
