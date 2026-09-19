use clap::{Parser, Subcommand};
use rws::{
    config::Config,
    transport::{remote_command, remote_shell},
    workspace::Workspace,
};
use std::{path::PathBuf, process::Command};

#[derive(Parser)]
#[command(
    version,
    about = "Remote workspaces: files and commands stay on their owning host"
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Execute remotely; without --workspace, infer from the current directory.
    Exec {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Open the remote user's default shell interactively.
    Shell {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Mount a registered workspace (requires SSHFS/macFUSE on macOS).
    Mount {
        workspace: String,
        #[arg(long)]
        dry_run: bool,
        /// Use the macFUSE FSKit backend; requires a direct child of /Volumes.
        #[arg(long)]
        fskit: bool,
        /// Disable UTF-8 NFC remote / NFD local filename conversion.
        #[arg(long)]
        raw_names: bool,
    },
    Unmount {
        workspace: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Check local tools; optionally probe a workspace's SSH connection.
    Doctor {
        #[arg(long)]
        workspace: Option<String>,
    },
}
#[derive(Subcommand)]
enum WorkspaceAction {
    Add {
        name: String,
        #[arg(long)]
        ssh: String,
        #[arg(long)]
        remote: String,
        #[arg(long)]
        mount: PathBuf,
    },
    List,
}
fn main() {
    match run(Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("rws: {e}");
            std::process::exit(1);
        }
    }
}
fn default_config() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is unset; supply --config")?;
    let base = if cfg!(target_os = "macos") {
        "Library/Application Support/RWS"
    } else {
        ".config/rws"
    };
    Ok(PathBuf::from(home).join(base).join("config.json"))
}
fn resolve(config: &Config, name: Option<&str>) -> Result<(Workspace, String), String> {
    if let Some(name) = name {
        let w = config.find(name)?;
        return Ok((w.clone(), w.remote_root.clone()));
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let matches: Vec<_> = config
        .workspaces
        .iter()
        .filter_map(|w| w.remote_path(&cwd).ok().map(|p| (w.clone(), p)))
        .collect();
    if matches.len() != 1 {
        return Err(
            "current directory must match exactly one workspace; use --workspace NAME".into(),
        );
    }
    Ok(matches.into_iter().next().unwrap())
}
fn available(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|p| p.join(program).is_file()))
}
fn sshfs_program() -> String {
    std::env::var("RWS_SSHFS").unwrap_or_else(|_| "sshfs".into())
}
fn check_sshfs() -> Result<String, String> {
    let program = sshfs_program();
    if !available(&program) {
        return Err("SSHFS is missing. Install macFUSE and SSHFS; see docs/prototype.md".into());
    }
    let output = Command::new(&program)
        .arg("--version")
        .output()
        .map_err(|e| format!("SSHFS cannot start: {e}. Check the macFUSE/SSHFS installation"))?;
    if !output.status.success() {
        return Err(format!(
            "SSHFS is installed but unusable. Check the macFUSE/SSHFS installation: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}
fn mounted_filesystem(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(mount), Some(Ok(parent))) = (
            std::fs::metadata(path),
            path.parent().map(std::fs::metadata),
        ) {
            return mount.dev() != parent.dev();
        }
    }
    false
}
fn invoke(program: &str, args: &[String], dry_run: bool, replace: bool) -> Result<i32, String> {
    if dry_run {
        println!("{}", serde_json::json!({"program": program, "args": args}));
        return Ok(0);
    }
    let mut command = Command::new(program);
    command.args(args);
    #[cfg(unix)]
    if replace {
        use std::os::unix::process::CommandExt;
        return Err(format!("start {program}: {}", command.exec()));
    }
    let _ = replace;
    let status = command
        .status()
        .map_err(|e| format!("start {program}: {e}"))?;
    Ok(status.code().unwrap_or(1))
}
fn ssh_args(host: &str, script: String, tty: bool) -> Vec<String> {
    vec![
        if tty { "-t" } else { "-T" }.into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "--".into(),
        host.into(),
        script,
    ]
}
fn run(cli: Cli) -> Result<i32, String> {
    let path = match cli.config {
        Some(p) => p,
        None => default_config()?,
    };
    if let Action::Workspace {
        action:
            WorkspaceAction::Add {
                name,
                ssh,
                remote,
                mount,
            },
    } = cli.command
    {
        Config::add(
            &path,
            Workspace {
                name: name.clone(),
                host: ssh,
                remote_root: remote,
                mount_root: mount,
            },
        )?;
        println!("Registered {name}");
        return Ok(0);
    }
    let config = Config::load(&path)?;
    match cli.command {
        Action::Workspace {
            action: WorkspaceAction::List,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?
            );
            Ok(0)
        }
        Action::Workspace { .. } => unreachable!(),
        Action::Exec {
            workspace,
            dry_run,
            command,
        } => {
            let (w, remote) = resolve(&config, workspace.as_deref())?;
            let script = remote_command(&remote, &command)?;
            if !dry_run {
                eprintln!("RWS remote: {}:{}", w.host, remote);
            }
            invoke("ssh", &ssh_args(&w.host, script, false), dry_run, true)
        }
        Action::Shell { workspace, dry_run } => {
            let (w, remote) = resolve(&config, workspace.as_deref())?;
            let script = remote_shell(&remote)?;
            if !dry_run {
                eprintln!("RWS remote shell: {}:{}", w.host, remote);
            }
            invoke("ssh", &ssh_args(&w.host, script, true), dry_run, true)
        }
        Action::Mount {
            workspace,
            dry_run,
            fskit,
            raw_names,
        } => {
            let w = config.find(&workspace)?;
            if !cfg!(target_os = "macos") {
                return Err("mount is currently supported on macOS only".into());
            }
            if fskit && w.mount_root.parent() != Some(std::path::Path::new("/Volumes")) {
                return Err("FSKit requires a mount point directly under /Volumes".into());
            }
            if !dry_run {
                let version = check_sshfs()?;
                if fskit && !raw_names && !version.contains("3.7.5-rws-fskit2") {
                    return Err("FSKit Unicode support requires the RWS SSHFS build: run scripts/build-sshfs-fskit.sh and set RWS_SSHFS to its output. Use --raw-names only for intentional unconverted filename access".into());
                }
                if mounted_filesystem(&w.mount_root) {
                    return Err("a filesystem is already mounted at this path".into());
                }
                if std::fs::symlink_metadata(&w.mount_root)
                    .is_ok_and(|m| m.file_type().is_symlink())
                {
                    return Err("mount point must not be a symlink".into());
                }
                // macFUSE creates and assigns ownership of FSKit mount points
                // under /Volumes; creating them here would require sudo.
                if !fskit {
                    std::fs::create_dir_all(&w.mount_root)
                        .map_err(|e| format!("mount directory: {e}"))?;
                }
                if w.mount_root.exists()
                    && std::fs::read_dir(&w.mount_root)
                        .map_err(|e| e.to_string())?
                        .next()
                        .is_some()
                {
                    return Err(
                        "mount directory is not empty; refusing to hide existing files".into(),
                    );
                }
            }
            let mut args = vec![
                format!("{}:{}", w.host, w.remote_root),
                w.mount_root.to_string_lossy().into_owned(),
                "-o".into(),
                "ConnectTimeout=10,ServerAliveInterval=15,ServerAliveCountMax=3,BatchMode=yes"
                    .into(),
            ];
            if fskit {
                args.extend(["-o".into(), "backend=fskit".into()]);
            }
            if fskit && !raw_names {
                args.extend(["-o".into(), "rws_unicode".into()]);
            }
            args.extend(["-o".into(), format!("volname=RWS-{}", w.name)]);
            args.push("-f".into());
            let program = sshfs_program();
            if dry_run {
                return invoke(&program, &args, true, false);
            }
            let logs = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new("."))
                .join("mount-logs");
            std::fs::create_dir_all(&logs).map_err(|e| format!("create log directory: {e}"))?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let log = logs.join(format!("{}-{}-{stamp}.log", w.name, std::process::id()));
            let mut command = Command::new(&program);
            command.args(&args);
            let _server = rws::mount::start(
                &mut command,
                &log,
                std::time::Duration::from_secs(30),
                || mounted_filesystem(&w.mount_root),
            )?;
            // SSHFS continues independently and exits when the OS unmounts its volume.
            println!("Mounted {} at {}", w.name, w.mount_root.display());
            eprintln!("SSHFS log: {}", log.display());
            Ok(0)
        }
        Action::Unmount { workspace, dry_run } => {
            let w = config.find(&workspace)?;
            if !cfg!(target_os = "macos") {
                return Err("unmount is currently supported on macOS only".into());
            }
            invoke(
                "/sbin/umount",
                &[w.mount_root.to_string_lossy().into_owned()],
                dry_run,
                false,
            )
        }
        Action::Doctor { workspace } => {
            let mut missing = false;
            for tool in ["ssh", "sftp"] {
                let found = available(tool);
                println!("{tool}: {}", if found { "available" } else { "missing" });
                missing |= !found;
            }
            match check_sshfs() {
                Ok(_) => println!("sshfs: available and runnable"),
                Err(error) => {
                    println!("sshfs: unusable — {error}");
                    missing = true;
                }
            }
            if let Some(name) = workspace {
                let w = config.find(&name)?;
                let mut args = vec!["-o".into(), "BatchMode=yes".into()];
                args.extend(ssh_args(
                    &w.host,
                    remote_command(&w.remote_root, &["pwd".into()])?,
                    false,
                ));
                let code = invoke("ssh", &args, false, false)?;
                if code != 0 {
                    return Ok(code);
                }
            }
            Ok(if missing { 1 } else { 0 })
        }
    }
}
