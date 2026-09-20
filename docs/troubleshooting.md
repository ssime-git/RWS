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
| Finder frozen; processes stuck in state `U` (`ps` STAT) on the mount; repair or remount hangs | `pgrep -fl sshfs` shows the old server; `ps -o stat` shows `U` on git/mount/umount | The FSKit service itself is wedged with uninterruptible operations — beyond RWS's reach. Kill the stale workspace `sshfs` process, then restart the service: `sudo pkill -9 fskitd` (launchd respawns it, aborting the stuck operations). Reconnect afterwards. A reboot is the guaranteed fallback. Heavy Git worktree activity over the mount is a known trigger. |
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
