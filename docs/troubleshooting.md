# Troubleshooting

[Documentation](README.md) · [App setup](install.md) · [CLI guide](prototype.md)

Start with the app's build revision/date and **Détails** output. Keep logs private;
remove real usernames, hosts, paths and tokens before sharing them in an issue.

| Symptom | Check | Next step |
| --- | --- | --- |
| Old UI, missing agent button | Displayed build revision and bundle you opened | Quit RWS and open the current canonical bundle. Source builds replace `dist/development/RWS.app`; do not open archived copies. |
| App opens but no folder appears | Is a workspace selected? | **Ouvrir dans le Finder** initiates the mount. Launching the app alone does not mount all spaces. |
| No configuration discovered | Did you move the app to Applications? | Choose the existing `config.json` in Advanced Configuration. Adjacent checkout discovery is deliberately limited. |
| Invalid configuration file | Selected file and error path | Select an actual RWS JSON config, not the directory, ZIP or template README. See [sample](../.rws-local.template/README.md). |
| macFUSE missing/incomplete | Runtime installation | Install the official package; then recheck. This is separate from SSHFS. |
| SSHFS missing or cannot start | Exact executable and its libraries | Select the compatible patched binary. `Library not loaded` means the runtime dependency is missing, not an SSH failure. |
| FSKit mount fails on a fresh setup | System extension activation and saved backend | Complete [setup steps](install.md#3-configure-the-app-once); explicitly enable FSKit and save. Presence on disk does not prove activation. |
| SSH permission denied / host unavailable | Normal SSH access and required VPN | Fix host reachability, trust and authentication first. RWS mounting is noninteractive. |
| Mounted path opens but favorite fails | Is this the current mount? | Reopen via RWS to renew the favorite. Use the [manual fallback](finder-macos.md) if pinning reports an error. |
| Favorite fails while disconnected | Mount status | Reconnect from RWS first; a favorite is not an auto-mount shortcut. |
| Commands show Darwin/local tools | How was the command started? | Ordinary terminals/IDE processes remain local. Use **Lancer sur la VM** or explicit RWS SSH commands. Automatic routing is still [#1–3](https://github.com/ssime-git/RWS/issues/1). |
| Agent not found on VM | Agent executable in remote login environment | Install/configure it remotely or use its absolute remote path. No local fallback occurs. |
| Disconnect says busy | Open files/terminals using the mounted directory | Close them or leave the directory, then retry normal disconnect. Do not force-unmount by default. |
| Volume still mounted but every operation fails (`Input/output error`) after a network loss | `rws status` reports `connected (unresponsive mount: …)` | The mount is a zombie: close files using it, then click **Réparer** in the app or run `rws connect NAME --repair` — it ejects the dead volume and mounts again. The forced ejection is refused while the mount answers normally; unsaved writes on the dead mount may be lost. **Actualiser** only re-reads state and repairs nothing. |
| Finder frozen; processes stuck in state `U` (`ps` STAT) on the mount; repair or remount hangs | Inspect the saved diagnostic report; distinguish responsive SSH from an unresponsive filesystem | Recovery bounds diskutil/umount execution and stops without remounting if ejection fails or exceeds 15 seconds. A timeout does not prove kernel resources were released. Do not repeat repairs in a loop. A global FSKit restart can disrupt every FSKit volume and needs explicit consent after closing active work; RWS never performs it automatically. The underlying FSKit hang is not established to be fixed. |

## Diagnosis and repair

Use the durable binary, typically `~/Library/Application Support/RWS/bin/rws`:

```sh
rws doctor --workspace demo
rws doctor --json --report /absolute/new-report.json
rws repair
rws repair --workspace demo --mounts
# Equivalent combined form:
rws doctor --workspace demo --repair --mounts
```

`doctor` is read-only unless you request a report or repair. It separates SSH
reachability, registered mount identity and actual filesystem responsiveness.
`status --no-probe` skips SSH only: it still checks mount I/O, and an unhealthy
mount causes a nonzero result. A mounted device entry alone is not success.

Repair always saves a mode-0600 JSON report, by default under
`~/Library/Application Support/RWS/diagnostics/`. Its path is printed (or returned
as `report_path` in JSON). A separate `.before.json` snapshot is synced before
mutation and never overwritten, preserving evidence even if repair fails.
Reports include local paths, workspace names and SSH output: review before sharing.

Without `--mounts`, repair only reconciles existing managed shell/Delta rules
and the recognized RWS LaunchAgent. It preserves custom configurations, disabled
hooks, user text and unrelated launch agents. Duplicate active hooks require an
explicit choice: keep exactly one managed line, then retry. New integration
enablement stays explicit (`hook install`, `delta-rules`, `autostart install`).

With `--mounts`, repair attempts disconnected or verified unresponsive volumes
only when SSH is reachable. Unknown mounts are refused; healthy mounts are left
alone. Forced ejection of a dead volume can lose unsaved writes: close applications
using it first. Each mount repair subprocess has a 120-second deadline; dependency
checks have 5 seconds, SSH probes 12 seconds, and mount health probes 4 seconds.
The whole report can take longer with several workspaces. OS process creation or
kernel I/O may still block below these user-space deadlines; killing a command
does not establish that the filesystem service recovered.

The app's **Réparer** action uses this workflow and keeps its report in the output.
New LaunchAgent log settings are only verified on disk and apply at next login.
No operation changes macFUSE approval, restarts FSKit or redirects Delta's
internal native Git/file operations. A successful CLI repair is not evidence
that a Delta prompt completed; test that workflow separately.
| RWS operation timed out | Diagnostic message and real mount status | Inspect status before retrying. Restart the app if it marks operation state uncertain. Do not assume timeout means no side effect occurred. |
| No update button / no updates | Development versus production bundle | Development updates are disabled. Follow [release readiness](macos-app.md); rebuilding is not production auto-update delivery. |

## Builder problems

- **Rust edition/compiler error:** use current stable, matching CI. No older MSRV
  is promised for the locked dependency graph.
- **Swift compiler/SDK mismatch:** use one compatible developer-tool installation.
  Check `xcode-select -p` and `swift --version`; use a per-command `DEVELOPER_DIR`
  override when appropriate instead of changing global settings blindly.
- **Running-bundle refusal:** quit the canonical RWS app and retry the build. The
  builder deliberately avoids replacing its files while running.
- **GLib/header error building SSHFS:** install the documented development dependency
  or set `RWS_GLIB_PREFIX`; macFUSE headers must be present separately.
- **Filesystem security/activation issue:** consult the [setup skill](../.agents/skills/rws-macos-setup/SKILL.md).
  Historical Full Disk Access/service-restart experiments are not normal installation steps.

## Report a reproducible problem

Use [GitHub issues](https://github.com/ssime-git/RWS/issues) and include:

1. RWS build revision, macOS/architecture, dependency versions if relevant.
2. Entry point: app button, CLI, Finder favorite, Delta or ordinary terminal.
3. Expected versus observed behavior and a short reproduction.
4. Sanitized error text; say whether the actual mounted path opens.
5. Whether the process ran locally or remotely, if known — do not infer it from the path alone.

Never attach SSH private keys, personal configuration, full shell environments or
unreviewed diagnostic screenshots. Network interruption tests should use disposable
remote data, not a working project.
