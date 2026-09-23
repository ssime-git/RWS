use clap::{Parser, Subcommand, ValueEnum};
use rws::{
    config::Config,
    transport::{remote_agent, remote_command, remote_shell},
    workspace::Workspace,
};
use std::{path::PathBuf, process::Command};
mod maintenance;

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
    /// Copy this configuration and its executables into durable macOS state.
    Install {
        /// Let the app reconcile integrations according to its preferences.
        #[arg(long)]
        skip_integrations: bool,
    },
    /// Describe whether a filesystem directory belongs to a verified RWS mount.
    Context {
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Install conditional forwarding rules in Delta's personal instructions.
    DeltaRules {
        #[arg(long)]
        output: Option<PathBuf>,
        /// Only refresh rules that already exist; never install them anew.
        #[arg(long)]
        if_installed: bool,
    },
    /// Install per-workspace macOS login agents that remount after reboot.
    Autostart {
        #[command(subcommand)]
        action: AutostartAction,
    },
    /// Remember this machine's mount backend and SSHFS executable.
    Settings {
        #[arg(long)]
        sshfs: Option<String>,
        #[arg(long, value_enum)]
        backend: Option<Backend>,
    },
    /// Show mount identity separately from SSH reachability and execution.
    Status {
        workspace: Option<String>,
        #[arg(long)]
        no_probe: bool,
    },
    /// Create Finder shortcuts, including an explicitly remote terminal.
    Shortcuts {
        workspace: String,
        #[arg(long)]
        directory: PathBuf,
    },
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Execute remotely; without --workspace, infer from the current directory.
    Exec {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long, conflicts_with = "workspace")]
        cwd: Option<PathBuf>,
        /// Map this repository's Git metadata for this invocation only.
        #[arg(long, conflicts_with = "workspace")]
        git_context: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Launch an installed agent in the remote login environment. Never runs locally.
    Agent {
        #[arg(long)]
        workspace: Option<String>,
        /// Start in the remote directory mapped from this mounted path.
        #[arg(long, conflicts_with = "workspace")]
        cwd: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        /// Disable PTY allocation for noninteractive diagnostics.
        #[arg(long)]
        no_tty: bool,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Shell integration that auto-switches into `rws shell` in mounts.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Open the remote user's default shell interactively.
    Shell {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Mount a registered workspace (requires SSHFS/macFUSE on macOS).
    #[command(name = "connect", visible_alias = "mount")]
    Mount {
        workspace: String,
        /// Verify and adopt an existing volume using a disposable remote file.
        #[arg(long)]
        verify_existing: bool,
        /// Eject a verified mount whose I/O fails (zombie after link loss),
        /// then mount again. Refused while the mount answers normally.
        #[arg(long)]
        repair: bool,
        #[arg(long)]
        dry_run: bool,
        /// Use the macFUSE FSKit backend; requires a direct child of /Volumes.
        #[arg(long)]
        fskit: bool,
        /// Disable UTF-8 NFC remote / NFD local filename conversion.
        #[arg(long)]
        raw_names: bool,
    },
    #[command(name = "disconnect", visible_alias = "unmount")]
    Unmount {
        workspace: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Check local tools; optionally probe a workspace's SSH connection.
    Doctor {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        json: bool,
        /// Save a private report to a new file (never overwrite an existing file).
        #[arg(long)]
        report: Option<PathBuf>,
        /// Refresh existing managed integrations, then diagnose again.
        #[arg(long)]
        repair: bool,
        /// Also reconnect missing or repair verified unresponsive mounts.
        #[arg(long, requires = "repair")]
        mounts: bool,
    },
    /// Diagnose, repair existing managed integrations, and diagnose again.
    Repair {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        report: Option<PathBuf>,
        /// May force-eject a verified dead mount; unsaved writes can be lost.
        #[arg(long)]
        mounts: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Backend {
    Default,
    Fskit,
}
#[derive(Subcommand)]
enum HookAction {
    /// Print the zsh snippet for eval in ~/.zshrc.
    Zsh,
    /// Write the eval line into ~/.zshrc (or --zshrc PATH), replacing any
    /// previously installed RWS line.
    Install {
        #[arg(long)]
        zshrc: Option<PathBuf>,
        /// Refresh an active managed hook only; preserve absent/commented hooks.
        #[arg(long)]
        if_installed: bool,
    },
}
#[derive(Subcommand)]
enum AutostartAction {
    /// Write the generic LaunchAgent for the durable installation.
    Install {
        #[arg(long)]
        directory: Option<PathBuf>,
    },
    /// Mount every workspace from the canonical durable configuration.
    Run,
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
fn canonical_layout() -> Result<rws::installation::Layout, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is unset")?;
    Ok(rws::installation::Layout::macos(std::path::Path::new(
        &home,
    )))
}

fn install_autostart(directory: Option<PathBuf>) -> Result<i32, String> {
    if !cfg!(target_os = "macos") {
        return Err("autostart is supported on macOS only".into());
    }
    let layout = canonical_layout()?;
    let binary = layout.require_binary()?;
    let config_path = layout.config_path();
    let config = Config::load_existing(&config_path)?;
    let home = std::env::var_os("HOME").ok_or("HOME is unset; supply --directory")?;
    let directory = directory.unwrap_or_else(|| PathBuf::from(home).join("Library/LaunchAgents"));
    let paths = rws::autostart::install(&directory, &binary, &config_path, &config.workspaces)?;
    for path in paths {
        println!("Installed {}", path.display());
    }
    println!(
        "LaunchAgents retry only failed mounts every 30 seconds; successful mounts are not restarted."
    );
    Ok(0)
}

fn run_autostart() -> Result<i32, String> {
    if !cfg!(target_os = "macos") {
        return Err("autostart is supported on macOS only".into());
    }
    let path = canonical_layout()?.config_path();
    let config = Config::load_existing(&path)?;
    let mut failures = Vec::new();
    if let Err(error) = maintenance::refresh_integrations(&path) {
        failures.push(format!("integration refresh: {error}"));
    }
    for workspace in &config.workspaces {
        match run(Cli {
            config: Some(path.clone()),
            command: Action::Mount {
                workspace: workspace.name.clone(),
                verify_existing: false,
                repair: false,
                dry_run: false,
                fskit: false,
                raw_names: false,
            },
        }) {
            Ok(0) => {}
            Ok(code) => failures.push(format!("{} (exit {code})", workspace.name)),
            Err(error) => failures.push(format!("{}: {error}", workspace.name)),
        }
    }
    if failures.is_empty() {
        Ok(0)
    } else {
        Err(format!(
            "autostart could not mount: {}",
            failures.join("; ")
        ))
    }
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
fn require_verified_mount(
    config_path: &std::path::Path,
    config: &Config,
    w: &Workspace,
) -> Result<(), String> {
    let actual = rws::lifecycle::identity(&w.mount_root)?
        .ok_or("RWS workspace is disconnected; connect it before forwarding commands")?;
    if !rws::lifecycle::verified(config_path, config, w, &actual) {
        return Err("RWS mount identity is unverified; use connect NAME --verify-existing before forwarding commands".into());
    }
    Ok(())
}
fn available(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|p| p.join(program).is_file()))
}
fn mount_health_status(name: &str, health: Result<(), String>) -> (String, bool) {
    match health {
        Ok(()) => ("connected (verified RWS mount)".into(), false),
        Err(reason) => (
            format!(
                "connected (unresponsive mount: {reason}); repair with: rws connect {name} --repair"
            ),
            true,
        ),
    }
}
fn sshfs_program(config_path: &std::path::Path, config: &Config) -> Result<String, String> {
    rws::installation::select_sshfs(config_path, config)?
        .into_os_string()
        .into_string()
        .map_err(|_| "configured SSHFS path is not UTF-8".into())
}
fn check_sshfs(config_path: &std::path::Path, config: &Config) -> Result<String, String> {
    let program = sshfs_program(config_path, config)?;
    if !available(&program) {
        return Err("SSHFS is missing. Install macFUSE and SSHFS; see docs/prototype.md".into());
    }
    let output = rws::process::output(
        Command::new(&program).arg("--version"),
        std::time::Duration::from_secs(5),
    )
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
    let status = if matches!(program, "/usr/sbin/diskutil" | "/sbin/umount") {
        rws::process::run(&mut command, std::time::Duration::from_secs(15))
            .map_err(|error| format!("{error}; ejection is NOT confirmed, remount stopped. The OS may still hold the volume. Inspect RWS doctor and close applications using it. A global FSKit restart requires explicit authorization; do not repeat repairs in a loop."))?
    } else {
        command
            .status()
            .map_err(|e| format!("start {program}: {e}"))?
    };
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
    let explicit_config = cli.config.is_some();
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
    match &cli.command {
        Action::Doctor {
            workspace,
            json,
            report,
            repair,
            mounts,
        } => {
            return maintenance::execute(
                &path,
                workspace.as_deref(),
                *json,
                report.as_deref(),
                *repair,
                *mounts,
            );
        }
        Action::Repair {
            workspace,
            json,
            report,
            mounts,
        } => {
            return maintenance::execute(
                &path,
                workspace.as_deref(),
                *json,
                report.as_deref(),
                true,
                *mounts,
            );
        }
        Action::Autostart {
            action: AutostartAction::Install { directory },
        } => {
            if explicit_config {
                return Err(
                    "autostart uses the canonical installation; --config is not allowed".into(),
                );
            }
            return install_autostart(directory.clone());
        }
        Action::Autostart {
            action: AutostartAction::Run,
        } => {
            if explicit_config {
                return Err(
                    "autostart run uses the canonical installation; --config is not allowed".into(),
                );
            }
            return run_autostart();
        }
        _ => {}
    }
    let strict_routing = matches!(
        &cli.command,
        Action::Context { .. }
            | Action::Exec { cwd: Some(_), .. }
            | Action::Exec {
                git_context: true,
                ..
            }
            | Action::Agent { cwd: Some(_), .. }
    );
    let config = if strict_routing {
        Config::load_existing(&path)?
    } else {
        Config::load(&path)?
    };
    match cli.command {
        Action::Install { skip_integrations } => {
            if !cfg!(target_os = "macos") {
                return Err("install is supported on macOS only".into());
            }
            let layout = canonical_layout()?;
            let installed = rws::installation::install_current(&layout, &path)?;
            maintenance::refresh_launch_agent()?;
            println!(
                "Installed durable RWS at {}",
                layout.config_path().display()
            );
            if !skip_integrations {
                maintenance::refresh_integrations(&layout.config_path())?;
                println!(
                    "Existing managed shell and Delta integrations refreshed to {}. New integrations remain opt-in (hook install / delta-rules).",
                    installed.rws.display()
                );
            }
            Ok(0)
        }
        Action::DeltaRules {
            output,
            if_installed,
        } => {
            let target = match output {
                Some(p) => p,
                None => rws::agent_rules::default_delta_rules_path()?,
            };
            if if_installed && !rws::agent_rules::managed_block_present(&target)? {
                println!(
                    "No RWS rules in {}; nothing refreshed. Run delta-rules without --if-installed to install them.",
                    target.display()
                );
                return Ok(0);
            }
            let config = path
                .canonicalize()
                .map_err(|e| format!("config path: {e}"))?;
            if if_installed && !maintenance::rules_for_configuration(&target, &config)? {
                println!("Custom Delta configuration preserved; nothing refreshed.");
                return Ok(0);
            }
            let binary = std::env::current_exe().map_err(|e| e.to_string())?;
            rws::agent_rules::install(&config, &binary, &target)?;
            println!("Global Delta RWS rules installed in {}", target.display());
            println!(
                "Rules guide agent commands; they do not intercept Delta's internal processes."
            );
            Ok(0)
        }
        Action::Autostart { .. } => unreachable!(),
        Action::Context { cwd } => {
            let cwd = match cwd {
                Some(p) => p,
                None => std::env::current_dir().map_err(|e| e.to_string())?,
            };
            match rws::routing::resolve_directory(&config, &cwd)? {
                None => println!("{}", serde_json::json!({"mode":"local","cwd":cwd})),
                Some((w, remote)) => {
                    require_verified_mount(&path, &config, &w)?;
                    println!(
                        "{}",
                        serde_json::json!({"mode":"remote","workspace":w.name,"host":w.host,"cwd":cwd,"remote_cwd":remote,"mount_verified":true})
                    );
                }
            }
            Ok(0)
        }
        Action::Settings { sshfs, backend } => {
            let mut options = config.mount;
            if let Some(sshfs) = sshfs {
                options.sshfs = Some(sshfs);
            }
            if let Some(backend) = backend {
                options.fskit = matches!(backend, Backend::Fskit);
            }
            Config::set_mount_options(&path, options)?;
            println!("Mount settings saved in {}", path.display());
            Ok(0)
        }
        Action::Shortcuts {
            workspace,
            directory,
        } => {
            config.find(&workspace)?;
            rws::shortcuts::create(&path, &workspace, &directory)?;
            println!("Shortcuts created in {}", directory.display());
            Ok(0)
        }
        Action::Status {
            workspace,
            no_probe,
        } => {
            let selected = match workspace {
                Some(name) => vec![config.find(&name)?],
                None => config.workspaces.iter().collect(),
            };
            let mut failed = false;
            for w in selected {
                let state = match rws::lifecycle::identity(&w.mount_root) {
                    Ok(None) => "disconnected".to_string(),
                    Ok(Some(ref actual)) if rws::lifecycle::verified(&path, &config, w, actual) => {
                        let (state, unhealthy) = mount_health_status(
                            &w.name,
                            rws::lifecycle::probe_health(
                                &w.mount_root,
                                std::time::Duration::from_secs(4),
                            ),
                        );
                        failed |= unhealthy;
                        state
                    }
                    Ok(Some(_)) => {
                        failed = true;
                        "mounted (identity unverified; not managed by this configuration)".into()
                    }
                    Err(e) => {
                        failed = true;
                        format!("unavailable: {e}")
                    }
                };
                println!(
                    "{}: {state}\n  VM: {}:{}\n  Mac: {}",
                    w.name,
                    w.host,
                    w.remote_root,
                    w.mount_root.display()
                );
                if !no_probe {
                    let mut command = Command::new("ssh");
                    command.args(["-o", "BatchMode=yes"]);
                    command.args(ssh_args(
                        &w.host,
                        remote_command(&w.remote_root, &["true".into()])?,
                        false,
                    ));
                    match rws::transport::bounded_status(
                        &mut command,
                        std::time::Duration::from_secs(12),
                    ) {
                        Ok(true) => println!("  SSH: reachable; remote directory accessible"),
                        Ok(false) => {
                            println!("  SSH: unavailable or remote directory inaccessible");
                            failed = true;
                        }
                        Err(e) => {
                            println!("  SSH: {e}");
                            failed = true;
                        }
                    }
                } else {
                    println!("  SSH: not checked");
                }
                println!(
                    "  Application commands: not redirected. Use rws exec / rws shell for VM execution."
                );
            }
            Ok(if failed { 1 } else { 0 })
        }
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
            cwd,
            git_context,
            dry_run,
            mut command,
        } => {
            let local = match cwd.as_ref() {
                Some(p) => p.clone(),
                None => std::env::current_dir().map_err(|e| e.to_string())?,
            };
            let (w, remote) = if cwd.is_some() || git_context {
                let selected = rws::routing::resolve_directory(&config, &local)?.ok_or(
                    "directory is not in a registered RWS workspace; refusing local fallback",
                )?;
                if !dry_run {
                    require_verified_mount(&path, &config, &selected.0)?;
                }
                selected
            } else {
                resolve(&config, workspace.as_deref())?
            };
            // Validate the original argv before prepending the environment tool.
            remote_command(&remote, &command)?;
            if git_context {
                let environment = rws::routing::git_environment(&w, &local)?;
                if !environment.is_empty() {
                    let mut wrapped = vec!["env".to_string()];
                    wrapped.extend(
                        environment
                            .into_iter()
                            .map(|(key, value)| format!("{key}={value}")),
                    );
                    wrapped.extend(command);
                    command = wrapped;
                }
            }
            let script = remote_command(&remote, &command)?;
            if !dry_run {
                eprintln!("RWS remote: {}:{}", w.host, remote);
            }
            invoke("ssh", &ssh_args(&w.host, script, false), dry_run, true)
        }
        Action::Agent {
            workspace,
            cwd,
            dry_run,
            no_tty,
            command,
        } => {
            let (w, remote) = match cwd {
                Some(local) => {
                    let selected = rws::routing::resolve_directory(&config, &local)?.ok_or(
                        "directory is not in a registered RWS workspace; refusing local fallback",
                    )?;
                    if !dry_run {
                        require_verified_mount(&path, &config, &selected.0)?;
                    }
                    selected
                }
                None => resolve(&config, workspace.as_deref())?,
            };
            let script = remote_agent(&remote, &command)?;
            if !dry_run {
                eprintln!("RWS agent on VM: {}:{}", w.host, remote);
            }
            invoke("ssh", &ssh_args(&w.host, script, !no_tty), dry_run, true)
        }
        Action::Hook { action } => {
            let binary = std::env::current_exe().map_err(|e| e.to_string())?;
            // Bake an absolute path: the snippet runs from arbitrary directories.
            let absolute = std::path::absolute(&path).map_err(|e| e.to_string())?;
            let baked = explicit_config.then_some(absolute.as_path());
            match action {
                HookAction::Zsh => {
                    print!("{}", rws::shell_hook::zsh_snippet(&binary, baked)?);
                    // Guidance only for a manual invocation; stay silent under
                    // eval "$(rws hook zsh)", which runs at every shell start.
                    use std::io::IsTerminal;
                    if std::io::stdout().is_terminal() {
                        eprintln!(
                            "Add to ~/.zshrc: eval \"$(rws hook zsh)\" — or run: rws hook install"
                        );
                        eprintln!("Opt out per shell with RWS_NO_AUTO_SHELL=1.");
                    }
                }
                HookAction::Install {
                    zshrc,
                    if_installed,
                } => {
                    let zshrc = match zshrc {
                        Some(p) => p,
                        None => {
                            let base = std::env::var_os("ZDOTDIR")
                                .or_else(|| std::env::var_os("HOME"))
                                .ok_or("HOME is unset; supply --zshrc")?;
                            PathBuf::from(base).join(".zshrc")
                        }
                    };
                    if if_installed {
                        let text = match std::fs::read_to_string(&zshrc) {
                            Ok(text) => text,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                            Err(e) => return Err(e.to_string()),
                        };
                        if text
                            .lines()
                            .filter(|line| {
                                line.contains(rws::shell_hook::MARKER)
                                    && !line.trim_start().starts_with('#')
                            })
                            .all(|line| maintenance::manages_configuration(line, &absolute))
                        {
                            rws::shell_hook::refresh_existing(&zshrc, &binary, baked)?;
                        }
                        println!(
                            "Existing eligible shell hook refreshed; disabled and custom hooks preserved."
                        );
                        return Ok(0);
                    }
                    let line = rws::shell_hook::install(&zshrc, &binary, baked)?;
                    println!("Installed in {}: {line}", zshrc.display());
                    println!("Open a new terminal, or run: source {}", zshrc.display());
                }
            }
            Ok(0)
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
            verify_existing,
            repair,
            dry_run,
            fskit,
            raw_names,
        } => {
            let w = config.find(&workspace)?;
            let fskit = fskit || config.mount.fskit;
            if !cfg!(target_os = "macos") {
                return Err("mount is currently supported on macOS only".into());
            }
            let program = sshfs_program(&path, &config)?;
            if fskit && w.mount_root.parent() != Some(std::path::Path::new("/Volumes")) {
                return Err("FSKit requires a mount point directly under /Volumes".into());
            }
            let _operation = if dry_run {
                None
            } else {
                Some(rws::lifecycle::lock(&path, &config, w)?)
            };
            if !dry_run {
                if let Some(actual) = rws::lifecycle::identity(&w.mount_root)? {
                    if rws::lifecycle::verified(&path, &config, w, &actual) {
                        let health = rws::lifecycle::probe_health(
                            &w.mount_root,
                            std::time::Duration::from_secs(4),
                        );
                        match (health, repair) {
                            (Ok(()), false) => {
                                println!(
                                    "Already connected: {} at {}",
                                    w.name,
                                    w.mount_root.display()
                                );
                                return Ok(0);
                            }
                            (Ok(()), true) => {
                                return Err(
                                    "mount answers normally; nothing to repair. Use disconnect to unmount deliberately".into(),
                                );
                            }
                            (Err(reason), false) => {
                                return Err(format!(
                                    "mount is unresponsive ({reason}). Close files using the volume, then run: rws connect {} --repair. Unsaved writes on the dead mount may be lost",
                                    w.name
                                ));
                            }
                            (Err(reason), true) => {
                                eprintln!(
                                    "Unresponsive mount ({reason}); ejecting {} before remounting.",
                                    w.mount_root.display()
                                );
                                let code = invoke(
                                    "/usr/sbin/diskutil",
                                    &[
                                        "unmount".into(),
                                        "force".into(),
                                        w.mount_root.to_string_lossy().into_owned(),
                                    ],
                                    false,
                                    false,
                                )?;
                                if code != 0 || rws::lifecycle::identity(&w.mount_root)?.is_some() {
                                    return Err(
                                        "forced ejection failed; the volume is still mounted. Close programs using it and retry".into(),
                                    );
                                }
                                rws::lifecycle::forget(&path, &config, w)?;
                                // The dead volume's SSHFS server can outlive
                                // the ejection and wedge the next mount.
                                let ended = rws::lifecycle::terminate_stale_servers(
                                    std::path::Path::new(&program),
                                    &format!("{}:{}", w.host, w.remote_root),
                                    &w.mount_root,
                                    std::time::Duration::from_secs(10),
                                )?;
                                if ended > 0 {
                                    eprintln!("Terminated {ended} stale SSHFS server process(es).");
                                }
                                rws::lifecycle::mountpoint_answers(
                                    &w.mount_root,
                                    std::time::Duration::from_secs(4),
                                )
                                .map_err(|reason| {
                                    format!(
                                        "{reason}; the FSKit service appears wedged, so mounting again would hang. Run: sudo pkill -9 fskitd (launchd restarts it), then retry; reboot as the fallback"
                                    )
                                })?;
                                // Fall through to the normal mount sequence below.
                            }
                        }
                    } else if verify_existing {
                        rws::lifecycle::attest(w, &actual)?;
                        rws::lifecycle::record(&path, &config, w, actual)?;
                        println!("Existing volume verified and connected: {}", w.name);
                        return Ok(0);
                    } else {
                        return Err("a filesystem is already mounted here, but its identity is unverified; leaving it untouched. Use connect NAME --verify-existing to verify it against SSH without disconnecting".into());
                    }
                }
                let version = check_sshfs(&path, &config)?;
                if fskit && !raw_names && !version.contains("3.7.5-rws-fskit3") {
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
            let actual = rws::lifecycle::identity(&w.mount_root)?
                .ok_or("mount disappeared before recording its identity")?;
            rws::lifecycle::attest(w, &actual).map_err(|e| {
                format!("volume exists but verification failed: {e}; no ownership receipt saved")
            })?;
            rws::lifecycle::record(&path, &config, w, actual).map_err(|e| format!("volume is mounted, but its identity could not be saved: {e}; eject through Finder before reconnecting"))?;
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
            let _operation = if dry_run {
                None
            } else {
                Some(rws::lifecycle::lock(&path, &config, w)?)
            };
            if !dry_run {
                match rws::lifecycle::identity(&w.mount_root)? {
                    None => {
                        rws::lifecycle::forget(&path, &config, w)?;
                        println!("Already disconnected: {}", w.name);
                        return Ok(0);
                    },
                    Some(actual) if rws::lifecycle::verified(&path, &config, w, &actual) => {},
                    Some(_) => return Err("refusing to unmount a filesystem with an unverified identity; close your work and eject it through Finder".into()),
                }
            }
            let code = invoke(
                "/sbin/umount",
                &[w.mount_root.to_string_lossy().into_owned()],
                dry_run,
                false,
            )?;
            if !dry_run {
                if code != 0 {
                    return Err("disconnect failed; close files and terminals using the volume, then retry (no forced unmount performed)".into());
                }
                if rws::lifecycle::identity(&w.mount_root)?.is_some() {
                    return Err("disconnect returned but the volume is still mounted".into());
                }
                rws::lifecycle::forget(&path, &config, w)?;
                println!("Disconnected: {}. Remote files are preserved.", w.name);
            }
            Ok(code)
        }
        Action::Doctor { .. } | Action::Repair { .. } => unreachable!(),
    }
}

#[cfg(test)]
mod maintenance_status_tests {
    #[test]
    fn unhealthy_mount_never_reports_success_even_without_ssh_probe() {
        let (text, failed) = super::mount_health_status("demo", Err("deadline exceeded".into()));
        assert!(failed);
        assert!(text.contains("unresponsive") && text.contains("demo --repair"));
        assert!(!super::mount_health_status("demo", Ok(())).1);
    }
}
