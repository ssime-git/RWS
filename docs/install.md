# Install and open RWS

[Documentation](README.md) · [Build it yourself](development.md) · [Troubleshooting](troubleshooting.md)

This guide is for using the macOS app. You do not need Rust to run a built bundle.
The current preview is **not a zero-setup installer**: you need Tailscale, a VM
reachable over key-based SSH, and an app bundle (see below). macFUSE is **not**
required: RWS mounts files through its own [NFS bridge](nfs-bridge.md). To connect
a Mac to a VM step by step, read [Connect a Mac to a VM](onboarding.md).

## Durable macOS installation

`rws onboard add --install` (see [Connect a Mac to a VM](onboarding.md)) performs
the steps below for you. After validating a configuration from a checkout, you can
also install it into the macOS user state by hand before enabling login remounting:

```sh
/absolute/path/to/rws --config /absolute/path/to/setup.json install
/Users/you/Library/Application\ Support/RWS/bin/rws autostart install
```

RWS keeps its canonical configuration in `~/Library/Application Support/RWS/`,
uses a managed executable there (and a versioned SSHFS release with the legacy
backend), and preserves the source configuration. The single `io.rws.mounts` LaunchAgent reads that canonical
configuration at login and every 30 seconds, including after successful passes.
Only workspaces whose saved intent is `connected` are eligible; missing intent
entries preserve the old default of connecting. `disconnect` saves a persistent
pause, cleared by `connect`. See the [state contract](connection.md#automatic-remount-contract).
With the legacy macFUSE backend, its
privileged helper must be approved once from a real GUI terminal; a
LaunchAgent cannot show that prompt. The NFS bridge needs no approval.

### Put `rws` on your PATH

The durable install does not change your PATH. Link the managed executable into
a directory that is already on it, so that you, your terminals and your agents
can type `rws`. `~/.local/bin` is the usual choice:

```sh
mkdir -p "$HOME/.local/bin"
ln -sfn "$HOME/Library/Application Support/RWS/bin/rws" "$HOME/.local/bin/rws"
zsh -lic 'command -v rws && rws agent-rules --help >/dev/null && echo ok'
```

Point the link at the managed copy, never at a checkout's `target/` build or at
the app bundle. Each later `rws install`, and each relaunch of an updated app,
replaces that file in place, so the link keeps working with no further action. If
`command -v rws` prints nothing, `~/.local/bin` is not on your PATH. Add
`export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`, or link into another PATH
directory that you own. Without a `--config` option, `rws` uses the canonical
configuration in Application Support.

### Activate now and verify

The agent is automatically loaded at the next macOS login. To activate it without
restarting, run this from a normal user terminal (not an isolated automation
environment):

```sh
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/io.rws.mounts.plist"
```

It checks eligible mounts periodically, for example while Tailscale is still
connecting. The old one-shot agent stopped checking after its first successful
pass; an already loaded old job does not gain periodic checks merely because its
plist was updated. At the next login, launchd loads the updated plist. Do not
repeatedly bootstrap an already registered job or treat its registration as
proof of healthy mounts. Check its registration and the volumes with:

```sh
launchctl print "gui/$(id -u)/io.rws.mounts"
mount | grep RWS
```

If the service is absent, rerun the two durable-install commands above. With the
legacy backend, a failed mount does not establish that macFUSE can display a
privilege prompt: approve its helper once in a GUI terminal, then let Launchd retry.

Enable the zsh hook and the agent rules manually after installation. Run them
on the Mac, using the managed executable (`rws` once it is on your PATH):

```sh
rws hook install
rws agent-rules --all
```

These commands explicitly enable the integrations. Later `rws install` runs
refresh already-enabled managed references; repeated installation of the same
binary/configuration does not create another release. The updated app performs
this durable update at relaunch for the canonical configuration. It refreshes
the existing LaunchAgent on disk without enabling absent/custom/disabled agents
or restarting a currently loaded job. Logging and scheduling changes apply at the next login.
Autostart output is saved in `~/Library/Application Support/RWS/logs/` as
`autostart.log` and `autostart.err.log` (private files).

Use `rws doctor` before troubleshooting and `rws repair` to reconcile an existing
setup. See [diagnosis and repair](troubleshooting.md#diagnosis-and-repair).

## 1. Get the app

### Build availability

As checked on 20 September 2026, there is **no published RWS release**. Do not expect
a signed installer or an enabled auto-updater in the current development app.

| What you have | What to do |
| --- | --- |
| An existing trusted `RWS.app` bundle | Follow the double-click instructions below |
| No bundle, and you do not want to compile RWS | Open a successful [macOS app build](https://github.com/ssime-git/RWS/actions/workflows/macos-app.yml), download its `RWS-arm64-development` artifact, and extract it. GitHub sign-in may be required; artifacts expire after 14 days. Extract the inner ZIP if present. |
| A development environment | Follow [Build from source](development.md) |
| You need a signed, supported end-user distribution | Track [release readiness #14](https://github.com/ssime-git/RWS/issues/14); that distribution is not available yet |

Development bundles are ad-hoc signed, not Apple-notarized releases. macOS may
block a downloaded bundle. Verify its origin; do not disable Gatekeeper or copy
blanket quarantine-removal commands from troubleshooting posts. Building locally
is the documented developer route while production signing is pending.

### Double-click launch

1. Keep one active copy of **RWS.app**. For a received bundle, place it in your
   Applications folder. For local development, use `dist/development/RWS.app`.
2. Double-click it. No terminal command is required for daily app use after setup.
3. Check the revision and build date displayed in the window if you suspect an old copy.
4. If you already configured RWS, let discovery finish. Otherwise continue below.

Moving the app into Applications does not copy configuration. If the old
configuration was in a checkout's `.rws-local/`, choose that existing `config.json`
once in **Configuration avancée → Choisir une configuration…**. Do not select the
folder, template README or ZIP. The original config remains in place and is remembered.

## 2. Prepare the Mac and the remote host

| Requirement | Where / how |
| --- | --- |
| Apple silicon Mac | Current app packaging is arm64. Other packages are not supplied. |
| Tailscale | Installed, connected and signed in on the Mac and on the VM. The guided onboarding assumes it; any route that reaches the VM over SSH also works for the CLI. |
| Key-based SSH to the VM | `ssh USER@MACHINE true` must succeed without a password prompt. Mounting runs non-interactively. RWS does not create SSH keys, edit `~/.ssh/config` or type passwords. |
| SFTP server on the VM | OpenSSH's `sftp-server` (the Linux default); its `posix-rename` support lets editors save atomically. |
| Existing remote folder | An absolute path you can write to; the first verification writes a disposable proof file. |

Nothing else: no macFUSE, no SSHFS, no sudo, nothing installed on the VM.
`rws onboard check` (or **Vérifier** in the app) verifies every line above and
gives the exact fix for what is missing; see
[Connect a Mac to a VM](onboarding.md#fixing-a-failure). Verify SSH by hand with
your own alias or `USER@MACHINE`:

```sh
ssh devbox
```

Finish any normal authentication or trust prompt yourself, then `exit`.
Never place a password in the workspace form.

## 3. Configure the app once

With the default NFS bridge there is nothing to configure: a new configuration
selects it automatically the first time you add a VM. On startup the app checks
known configuration locations and discovers an existing one; see
[discovery details](macos-app.md#configuration-discovery-at-startup). Choosing
the legacy macFUSE/SSHFS backend is covered in the
[legacy section](#legacy-backend-macfuse-and-sshfs) below.

## 4. Add or select a workspace

In **Ajouter une VM**:

| Field | Example | Meaning |
| --- | --- | --- |
| **Nom** | `demo` | Workspace name; the folder appears at `~/RWS/demo` |
| **Machine** | `dev@my-vm` | `user@machine`; the **Machines Tailscale** menu fills the machine name from your tailnet |
| **Dossier distant** | `/home/dev/projects/demo` | Absolute directory on the VM |

Press **Vérifier** to list every finding with its fix, then **Ajouter et monter**
(it repeats the checks, so it can be pressed directly). RWS registers the
workspace, mounts it, proves it with a proof file and opens Finder. If the checks
fail, nothing is changed: fix what is reported and try again. Re-adding the same
workspace is harmless.

For an existing workspace, select it and click **Ouvrir dans le Finder**. That
button **connects/mounts as well as opening Finder**; merely launching RWS does
not automatically mount every registered space.

RWS attempts to add a favorite. If it cannot, follow the displayed manual
instructions or the [Finder guide](finder-macos.md). A favorite is a shortcut to a
mounted directory; it does not reconnect an offline workspace when clicked.

Enable the [zsh terminal integration](prototype.md#automatic-terminal-switch-zsh)
once with `rws hook install`. The app subsequently refreshes an existing active
hook, but does not recreate a removed or commented-out hook. In every **new**
interactive zsh, `cd` into a connected
workspace asks `RWS: switch to <host>? [Y/n]` once per shell — Enter opens the
remote shell on the matching remote directory, `n` stays local, and the answer
is remembered for that shell. `exit` returns to the local shell. Terminals already open before installation need one
`source ~/.zshrc`. The integration is one marked line in `~/.zshrc`; remove
that line or set `RWS_NO_AUTO_SHELL=1` to opt out, and set the
`installShellHook` app preference to `false` to stop automatic refresh.
If [agent rules](connection.md#agent-rules-and-linux-commands) were installed,
the app refreshes their embedded paths the same way (`refreshDeltaRules`
preference to opt out); it never installs them uninvited.

## 5. Run an agent on the VM or locally

1. Select the workspace.
2. Enter an executable, such as `claude` or `codex` (the field accepts the
   executable, not a whole shell command), then choose where it runs:
   - **Lancer sur la VM**: macOS Terminal opens an SSH session; the agent
     installed on the host runs there, initially at the workspace root. Check
     the displayed host/OS/directory. `RWS_AUTO_MODE=remote` is exported so
     shells the agent spawns never prompt to switch again.
   - **Lancer en local**: macOS Terminal runs this Mac's executable inside the
     mounted folder, with `RWS_AUTO_MODE=local` exported so the terminal
     integration neither prompts nor switches. Requires the mount to be
     connected; processes execute on the Mac while files stay remote.

An absent agent or failed connection reports an error; the VM launcher never
runs a similarly named local executable as a fallback. Authentication with the
agent's service must already be configured where the agent runs. The VM
launcher can work without a filesystem mount if SSH and configuration are ready.

**Opening Ghostty, Terminal or an IDE inside the mount yourself:** interactive
zsh terminals prompt through the [terminal integration](prototype.md#automatic-terminal-switch-zsh);
IDE-internal processes remain local — see
[architecture](architecture.md) and [issue #3](https://github.com/ssime-git/RWS/issues/3).

## 6. Disconnect, quit and update

- Leave the mounted folder in applications and close files using it, then click
  **Déconnecter**. RWS uses normal unmounting; it does not force a busy volume.
- Successful app disconnection removes the managed Finder favorite. Other favorites
  and remote files are preserved. External ejection follows the OS's behavior.
- Quitting RWS does not automatically unmount a volume or provide persistent remote
  agent sessions. Finish those sessions deliberately.
- For development builds, quit the old app before replacing it. Auto-updates are
  disabled. Source builders use the same [rebuild command](development.md#rebuild-without-old-app-copies).
- Signed distribution and real update delivery are tracked separately in
  [#14](https://github.com/ssime-git/RWS/issues/14).

If anything differs from these steps, open [Troubleshooting](troubleshooting.md)
and include the displayed build revision in a report.

## Legacy backend: macFUSE and SSHFS

Only for an explicit need (the NFS bridge is the default and the tested route).
This backend mounts under `/Volumes/RWS-NAME`, which the bridge refuses, and needs
more setup. On the tested Mac the macFUSE runtime has since been removed.

| Requirement | Where / how |
| --- | --- |
| Compatible macFUSE installation | Install from the [official macFUSE site](https://macfuse.github.io/). macFUSE and SSHFS are separate components. |
| Compatible SSHFS | The tested FSKit route requires the [experimental RWS SSHFS build](sshfs-fskit.md), version `3.7.5-rws-fskit3` or a compatible correction. It is not bundled in RWS. |
| FSKit activation | Enable the applicable macFUSE FSKit extension in macOS System Settings → General → Login Items & Extensions. Presence of macFUSE files does not prove activation. |
| Remote filenames | The patched FSKit mode expects UTF-8 NFC names; review the [Unicode contract](sshfs-fskit.md#unicode-filenames). |

The RWS FSKit tests used macOS 27.0/macFUSE 5.4.0; macFUSE documents its FSKit
backend for macOS 26. To select it: expand **Configuration avancée**, choose the
absolute path of the compatible `sshfs`, enable **Utiliser le moteur macFUSE
FSKit**, press **Enregistrer** and **Vérifier à nouveau**, then register the
workspace with `rws workspace add NAME --ssh HOST --remote DIR --mount /Volumes/RWS-NAME`.
Automatic discovery never silently switches the backend. Without a compatible
SSHFS binary, follow the [SSHFS build guide](sshfs-fskit.md); do not copy a build
tied to another person's libraries.
