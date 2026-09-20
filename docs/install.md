# Install and open RWS

[Documentation](README.md) · [Build it yourself](development.md) · [Troubleshooting](troubleshooting.md)

This guide is for using the macOS app. You do not need Rust to run a built bundle.
However, the current preview is **not a self-contained, zero-setup installer**:
macFUSE, compatible SSHFS and SSH access must already be configured.

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

Moving the app into Applications does not copy configuration or SSHFS. If the old
configuration was in a checkout's `.rws-local/`, choose that existing `config.json`
once in **Configuration avancée → Choisir une configuration…**. Do not select the
folder, template README or ZIP. The original config remains in place and is remembered.

## 2. Prepare the Mac and the remote host

| Requirement | Where / how |
| --- | --- |
| Apple silicon Mac | Current app packaging is arm64. Other packages are not supplied. |
| Compatible macFUSE installation | Install from the [official macFUSE site](https://macfuse.github.io/). macFUSE and SSHFS are separate components. |
| Compatible SSHFS | The tested FSKit route requires the [experimental RWS SSHFS build](sshfs-fskit.md), version `3.7.5-rws-fskit3` or a compatible correction. This executable and its libraries are not bundled in RWS. |
| FSKit activation | Enable the applicable macFUSE FSKit extension in macOS System Settings → General → Login Items & Extensions. Labels depend on macOS. Presence of macFUSE files does not prove activation. |
| Reachable Unix-like SSH host | Working SSH/SFTP and a POSIX-compatible login shell. A VPN such as Tailscale is optional; connect it if your host requires it. |
| Prepared SSH authentication | Establish normal SSH access and known-host trust first. Mounting runs without an interactive password prompt. |
| Existing remote folder | Choose an absolute remote path you own. Initial verification requires a writable root for a disposable proof file. |
| Remote filenames | The patched FSKit mode expects UTF-8 NFC names. NFD, invalid UTF-8 and canonically equivalent name collisions are not supported; review the [Unicode contract](sshfs-fskit.md#unicode-filenames) before mounting an existing project. |

The RWS mount tests used macOS 27.0/macFUSE 5.4.0. macFUSE documents its FSKit
backend for macOS 26; RWS's app deployment target alone does not establish mount
compatibility on older systems. See [upstream macFUSE](https://macfuse.github.io/)
and [our tested scope](validation.md).

For the person preparing SSH access, replace `devbox` with your configured SSH alias:

```sh
ssh devbox
```

Verify the destination, finish any normal authentication/trust steps, then `exit`.
RWS does not create SSH keys, change `~/.ssh/config`, install remote agents or
configure their credentials. Never place a password in the workspace form.

**If you have no compatible SSHFS binary:** follow the separate
[SSHFS build guide](sshfs-fskit.md) or have your administrator prepare it and its
runtime dependencies. Downloading RWS alone cannot complete mounting on a new Mac.
Do not copy a build path or a binary tied to another person's libraries blindly.

## 3. Configure the app once

On startup the app checks known configuration locations, macFUSE's runtime and
SSHFS execution. Missing/incompatible prerequisites produce a message.

For a new setup using the tested FSKit route:

1. Expand **Configuration avancée**.
2. Choose the absolute path of the compatible `sshfs` executable.
3. Enable **Utiliser le moteur macFUSE FSKit** and click **Enregistrer**.
4. Click **Vérifier à nouveau** if the dependency message is still present.

Do this before adding the first workspace. The stored backend defaults to the
system/default backend; automatic discovery does **not** silently change it to FSKit.
Keep the chosen SSHFS file and its dynamic libraries at valid locations.

Existing valid settings do not need to be re-entered. Discovery uses the default
Application Support config, a remembered source, or a bounded set of known paths;
it does not scan every project. [Discovery details](macos-app.md#configuration-discovery-at-startup).

## 4. Add or select a workspace

Example values — replace these with your own:

| Field | Example | Meaning |
| --- | --- | --- |
| **Nom** | `demo` | Workspace name; local mount becomes `/Volumes/RWS-demo` |
| **Hôte SSH** | `devbox` | SSH alias or `user@host`; configuration already works outside RWS |
| **Dossier distant** | `/home/dev/projects/demo` | Absolute directory on the remote host |

Click **Ajouter**. RWS saves the workspace, attempts the connection and opens Finder.
If prerequisites or the connection fail, the saved workspace may still exist:
fix the reported problem and select it instead of adding a duplicate.

For an existing workspace, select it and click **Ouvrir dans le Finder**. That
button **connects/mounts as well as opening Finder**; merely launching RWS does
not automatically mount every registered space.

RWS attempts to add a favorite. If it cannot, follow the displayed manual
instructions or the [Finder guide](finder-macos.md). A favorite is a shortcut to a
mounted directory; it does not reconnect an offline workspace when clicked.

Once a configuration is active, the app also installs the
[zsh terminal integration](prototype.md#automatic-terminal-switch-zsh)
automatically: in every **new** interactive zsh, `cd` into a connected
workspace asks `RWS: switch to <host>? [Y/n]` once per shell — Enter opens the
remote shell on the matching remote directory, `n` stays local, and the answer
is remembered for that shell. `exit` returns to the local shell. Terminals already open before installation need one
`source ~/.zshrc`. The integration is one marked line in `~/.zshrc`; remove
that line or set `RWS_NO_AUTO_SHELL=1` to opt out, and set the
`installShellHook` app preference to `false` to stop automatic installation.

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
