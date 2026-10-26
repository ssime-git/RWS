//! End-to-end behavior of `rws onboard`, driven by fake `ssh` and `tailscale`
//! executables placed first on PATH.
#![cfg(unix)]
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

const TAILSCALE_OK: &str = r#"{
  "BackendState": "Running",
  "Self": {"HostName": "mac", "DNSName": "mac.tail1234.ts.net.", "Online": true, "OS": "macOS"},
  "Peer": {
    "nodekey:c": {"HostName": "razer-1", "DNSName": "razer-1.tail1234.ts.net.", "Online": true, "OS": "linux", "TailscaleIPs": ["100.106.23.6"]},
    "nodekey:p": {"HostName": "localhost", "DNSName": "phone.tail1234.ts.net.", "Online": true, "OS": "iOS", "TailscaleIPs": ["100.94.78.8"]}
  }
}"#;

/// SFTP version reply, optionally advertising posix-rename.
fn sftp_hello(posix_rename: bool) -> Vec<u8> {
    let mut body = vec![2u8, 0, 0, 0, 3];
    if posix_rename {
        for text in ["posix-rename@openssh.com", "1"] {
            body.extend((text.len() as u32).to_be_bytes());
            body.extend(text.as_bytes());
        }
    }
    let mut packet = (body.len() as u32).to_be_bytes().to_vec();
    packet.extend(body);
    packet
}

struct Fake {
    dir: tempfile::TempDir,
}

impl Fake {
    fn new() -> Self {
        let fake = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        let script = |name: &str, body: &str| {
            let path = fake.dir.path().join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        // Like OpenSSH, options only count before `--`; everything after the
        // host is the remote command (a misplaced `-s` reaches the remote shell,
        // as it did against a real host).
        script(
            "ssh",
            r#"#!/bin/sh
D=$(dirname "$0")
case " $* " in
  *" -V "*) echo "OpenSSH_9.9p1 fake" >&2; exit 0;;
  *" -G "*) echo "hostname $(cat "$D/resolved" 2>/dev/null || echo unknown.example)"; exit 0;;
esac
echo "$@" >> "$D/ssh.log"
SUBSYS=0; AFTER=0; HOST=; CMD=
for a in "$@"; do
  if [ $AFTER = 0 ]; then
    case "$a" in --) AFTER=1;; -s) SUBSYS=1;; esac
  elif [ -z "$HOST" ]; then HOST=$a
  else CMD="$CMD $a"
  fi
done
if [ -f "$D/ssh_fail" ]; then cat "$D/ssh_fail" >&2; exit 255; fi
case "$CMD" in
  " -s"*) echo "bash: - : invalid option" >&2; exit 2;;
esac
if [ $SUBSYS = 1 ]; then cat "$D/hello.bin"; sleep 1; exit 0; fi
case "$CMD" in
  *"d='"*) cat "$D/dir_state"; exit 0;;
esac
exit 0
"#,
        );
        script(
            "tailscale",
            "#!/bin/sh\nD=$(dirname \"$0\")\ncat \"$D/ts.json\"\n",
        );
        fake.put("ts.json", TAILSCALE_OK);
        fake.put("dir_state", "ok\n");
        std::fs::write(fake.dir.path().join("hello.bin"), sftp_hello(true)).unwrap();
        fake
    }

    fn put(&self, name: &str, content: &str) {
        std::fs::write(self.dir.path().join(name), content).unwrap();
    }

    fn rws(&self, args: &[&str]) -> (i32, String, String) {
        self.rws_in(Path::new("/"), args)
    }

    fn rws_in(&self, home: &Path, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_rws"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.dir.path().display()),
            )
            .env("RWS_TAILSCALE", self.dir.path().join("tailscale"))
            .env("HOME", home)
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

fn parse(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"))
}

fn find<'a>(report: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
}

#[test]
fn a_ready_mac_and_vm_pass_every_check_and_list_tailscale_machines() {
    let fake = Fake::new();
    fake.put("resolved", "razer-1.tail1234.ts.net");
    let (code, stdout, stderr) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@razer-1",
        "--remote",
        "/srv/p",
        "--json",
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let report = parse(&stdout);
    assert_eq!(report["ok"], true);
    for id in [
        "mac.ssh",
        "mac.tailscale",
        "vm.reachable",
        "vm.sftp",
        "vm.atomic_save",
        "vm.directory",
        "vm.tailscale_peer",
    ] {
        let check = find(&report, id).unwrap_or_else(|| panic!("missing {id}: {stdout}"));
        assert_eq!(check["status"], "ok", "{id}: {check}");
    }
    let machines = report["machines"].as_array().unwrap();
    let names: Vec<_> = machines
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["phone", "razer-1"]);
    // The ssh invocations are key-only and bounded.
    let log = std::fs::read_to_string(fake.dir.path().join("ssh.log")).unwrap();
    assert!(
        log.contains("BatchMode=yes") && log.contains("ConnectTimeout="),
        "{log}"
    );
}

#[test]
fn without_a_host_only_the_mac_is_checked_and_machines_are_offered() {
    let fake = Fake::new();
    let (code, stdout, _) = fake.rws(&["onboard", "check", "--json"]);
    assert_eq!(code, 0, "{stdout}");
    let report = parse(&stdout);
    assert!(find(&report, "mac.tailscale").is_some());
    assert!(find(&report, "vm.reachable").is_none());
    assert_eq!(report["machines"].as_array().unwrap().len(), 2);
}

#[test]
fn a_rejected_key_fails_with_the_exact_fix_and_stops_before_vm_probes() {
    let fake = Fake::new();
    fake.put("ssh_fail", "dev@razer-1: Permission denied (publickey).\n");
    let (code, stdout, _) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@razer-1",
        "--remote",
        "/srv/p",
        "--json",
    ]);
    assert_eq!(code, 1);
    let report = parse(&stdout);
    assert_eq!(report["ok"], false);
    let reach = find(&report, "vm.reachable").unwrap();
    assert_eq!(
        (reach["status"].as_str(), reach["code"].as_str()),
        (Some("fail"), Some("key_denied"))
    );
    assert!(
        reach["fix"]
            .as_str()
            .unwrap()
            .contains("ssh-copy-id dev@razer-1")
    );
    for later in ["vm.sftp", "vm.directory", "vm.atomic_save"] {
        assert!(
            find(&report, later).is_none(),
            "{later} must not run: {stdout}"
        );
    }
}

#[test]
fn tailscale_turned_off_only_warns_without_a_host() {
    let fake = Fake::new();
    fake.put("ts.json", r#"{"BackendState": "Stopped", "Peer": {}}"#);
    let (code, stdout, _) = fake.rws(&["onboard", "check", "--json"]);
    assert_eq!(code, 0, "{stdout}");
    let ts = find(&parse(&stdout), "mac.tailscale").cloned().unwrap();
    assert_eq!(
        (ts["status"].as_str(), ts["code"].as_str()),
        (Some("warn"), Some("tailscale_stopped"))
    );
    assert!(ts["fix"].as_str().unwrap().contains("plain SSH"));
}

#[test]
fn an_ssh_reachable_vm_is_not_blocked_by_tailscale_being_off() {
    let fake = Fake::new();
    fake.put("ts.json", r#"{"BackendState": "Stopped", "Peer": {}}"#);
    fake.put("resolved", "files.example.org");
    let (code, stdout, _) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@files.example.org",
        "--remote",
        "/srv/p",
        "--json",
    ]);
    assert_eq!(code, 0, "{stdout}");
    let report = parse(&stdout);
    assert_eq!(report["ok"], true);
    assert_eq!(find(&report, "mac.tailscale").unwrap()["status"], "warn");
    for id in ["vm.reachable", "vm.sftp", "vm.directory"] {
        assert_eq!(find(&report, id).unwrap()["status"], "ok", "{id}: {stdout}");
    }
}

#[test]
fn an_unreachable_vm_still_fails_with_tailscale_as_one_possible_cause() {
    let fake = Fake::new();
    fake.put("ts.json", r#"{"BackendState": "Stopped", "Peer": {}}"#);
    fake.put(
        "ssh_fail",
        "ssh: connect to host 10.0.0.9 port 22: Operation timed out\n",
    );
    let (code, stdout, _) = fake.rws(&["onboard", "check", "--host", "dev@10.0.0.9", "--json"]);
    assert_eq!(code, 1);
    let reach = find(&parse(&stdout), "vm.reachable").cloned().unwrap();
    assert_eq!(
        (reach["status"].as_str(), reach["code"].as_str()),
        (Some("fail"), Some("unreachable"))
    );
    assert!(
        reach["fix"]
            .as_str()
            .unwrap()
            .contains("If the VM is on your tailnet")
    );
}

#[test]
fn a_missing_remote_folder_fails_with_the_mkdir_fix() {
    let fake = Fake::new();
    fake.put("dir_state", "missing\n");
    let (code, stdout, _) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@razer-1",
        "--remote",
        "/srv/new",
        "--json",
    ]);
    assert_eq!(code, 1);
    let dir = find(&parse(&stdout), "vm.directory").cloned().unwrap();
    assert_eq!(dir["code"], "directory_missing");
    assert!(dir["fix"].as_str().unwrap().contains("mkdir -p '/srv/new'"));
}

#[test]
fn warnings_do_not_fail_the_check() {
    let fake = Fake::new();
    std::fs::write(fake.dir.path().join("hello.bin"), sftp_hello(false)).unwrap();
    // A host outside the tailnet and an SFTP server without posix-rename.
    fake.put("resolved", "files.example.org");
    let (code, stdout, _) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@files.example.org",
        "--remote",
        "/srv/p",
        "--json",
    ]);
    assert_eq!(code, 0, "{stdout}");
    let report = parse(&stdout);
    assert_eq!(report["ok"], true);
    assert_eq!(find(&report, "vm.atomic_save").unwrap()["status"], "warn");
    assert_eq!(
        find(&report, "vm.tailscale_peer").unwrap()["status"],
        "warn"
    );
}

#[test]
fn a_remote_command_that_is_not_sftp_fails_the_sftp_check() {
    let fake = Fake::new();
    std::fs::write(fake.dir.path().join("hello.bin"), b"Welcome to a shell\r\n").unwrap();
    let (code, stdout, _) = fake.rws(&[
        "onboard",
        "check",
        "--host",
        "dev@razer-1",
        "--remote",
        "/srv/p",
        "--json",
    ]);
    assert_eq!(code, 1);
    let sftp = find(&parse(&stdout), "vm.sftp").cloned().unwrap();
    assert_eq!(sftp["status"], "fail");
    assert!(sftp["fix"].as_str().unwrap().contains("Subsystem sftp"));
}

#[test]
fn human_output_shows_each_failure_with_its_fix_and_a_verdict() {
    let fake = Fake::new();
    fake.put("ssh_fail", "Host key verification failed.\n");
    let (code, stdout, _) = fake.rws(&["onboard", "check", "--host", "dev@razer-1"]);
    assert_eq!(code, 1);
    assert!(stdout.contains("vm.reachable"), "{stdout}");
    assert!(
        stdout.contains("fix:") && stdout.contains("ssh dev@razer-1"),
        "{stdout}"
    );
    assert!(stdout.to_lowercase().contains("not ready"), "{stdout}");
}

fn add_args<'a>(config: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "--config",
        config,
        "onboard",
        "add",
        "demo",
        "--host",
        "dev@razer-1",
        "--remote",
        "/srv/p",
    ];
    args.extend(extra);
    args
}

#[test]
fn add_dry_run_plans_the_nfs_setup_and_changes_nothing() {
    let fake = Fake::new();
    fake.put("resolved", "razer-1.tail1234.ts.net");
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let args = add_args(config.to_str().unwrap(), &["--dry-run", "--json"]);
    let (code, stdout, stderr) = fake.rws_in(home.path(), &args);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let report = parse(&stdout);
    assert_eq!(
        (report["ok"].as_bool(), report["dry_run"].as_bool()),
        (Some(true), Some(true))
    );
    let ids: Vec<_> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["settings", "workspace_add", "connect"]);
    let add = &report["steps"][1]["args"];
    let mount = format!("{}/RWS/demo", home.path().display());
    assert!(
        add.as_array().unwrap().iter().any(|a| a == mount.as_str()),
        "{add}"
    );
    assert!(report["results"].as_array().unwrap().is_empty());
    assert!(
        !config.exists(),
        "a dry run must not create the configuration"
    );
}

#[test]
fn add_stops_on_a_failed_check_without_touching_the_configuration() {
    let fake = Fake::new();
    fake.put("ssh_fail", "dev@razer-1: Permission denied (publickey).\n");
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let args = add_args(config.to_str().unwrap(), &["--json"]);
    let (code, stdout, _) = fake.rws_in(home.path(), &args);
    assert_eq!(code, 1);
    let report = parse(&stdout);
    assert_eq!(report["ok"], false);
    assert_eq!(find(&report, "vm.reachable").unwrap()["code"], "key_denied");
    assert!(report["steps"].as_array().unwrap().is_empty());
    assert!(!config.exists());
}

#[test]
fn add_refuses_a_name_that_exists_with_other_settings_and_keeps_the_file() {
    let fake = Fake::new();
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let existing = serde_json::json!({
        "version": 1,
        "workspaces": [{"name": "demo", "host": "dev@other", "remote_root": "/srv/p",
                        "mount_root": format!("{}/RWS/demo", home.path().display())}],
        "mount": {"nfs": true}
    })
    .to_string();
    std::fs::write(&config, &existing).unwrap();
    let args = add_args(config.to_str().unwrap(), &["--dry-run"]);
    let (code, _, stderr) = fake.rws_in(home.path(), &args);
    assert_eq!(code, 1);
    assert!(stderr.contains("already exists"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), existing);
}

#[test]
fn add_with_install_plans_the_durable_steps_from_the_users_library() {
    let fake = Fake::new();
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let args = add_args(
        config.to_str().unwrap(),
        &["--install", "--dry-run", "--json"],
    );
    let (code, stdout, stderr) = fake.rws_in(home.path(), &args);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let report = parse(&stdout);
    let ids: Vec<_> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "settings",
            "workspace_add",
            "install",
            "autostart",
            "bootstrap",
            "connect"
        ]
    );
    let durable = format!(
        "{}/Library/Application Support/RWS/bin/rws",
        home.path().display()
    );
    assert_eq!(report["steps"][3]["program"], durable.as_str());
    assert_eq!(report["steps"][5]["program"], durable.as_str());
}

#[test]
fn add_rejects_an_invalid_name_before_any_ssh_attempt() {
    let fake = Fake::new();
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let (code, _, stderr) = fake.rws_in(
        home.path(),
        &[
            "--config",
            config.to_str().unwrap(),
            "onboard",
            "add",
            "bad name!",
            "--host",
            "dev@razer-1",
            "--remote",
            "/srv/p",
            "--dry-run",
        ],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("workspace name"), "{stderr}");
    assert!(
        !fake.dir.path().join("ssh.log").exists(),
        "no SSH before validation"
    );
}

#[test]
fn add_human_dry_run_lists_numbered_steps_and_says_nothing_changed() {
    let fake = Fake::new();
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("cfg.json");
    let args = add_args(config.to_str().unwrap(), &["--dry-run"]);
    let (code, stdout, _) = fake.rws_in(home.path(), &args);
    assert_eq!(code, 0);
    assert!(stdout.contains("1. ") && stdout.contains("3. "), "{stdout}");
    assert!(
        stdout.to_lowercase().contains("dry run") && stdout.contains("nothing was changed"),
        "{stdout}"
    );
}
