# RWS macOS app and automatic releases

The native app is the graphical frontend for the existing Rust CLI. It stores
new workspace configuration in `~/Library/Application Support/RWS/`, outside
the app bundle. Existing detected configurations are reused in place. Updating the app must not replace that directory.

## User workflow

1. Install RWS in Applications and open it.
2. On launch RWS detects its existing configuration and checks macFUSE and SSHFS.
   A single detected configuration is reused automatically; multiple candidates
   are presented for selection. Missing or incompatible dependencies display an
   actionable error. On a new setup, add the SSH host, remote directory and a
   workspace name. SSH authentication and FSKit activation remain prerequisites.
3. Select the workspace and choose **Open**: RWS connects, verifies the mount and
   opens Finder. Choose **Disconnect** when finished; busy mounts are preserved.

This first app does not install macFUSE, provision SSH credentials or package the
experimental SSHFS dependencies. These prerequisites still follow the
[setup guide](prototype.md). A new Mac is not yet a zero-setup installation.
Finder pinning is implemented in the app with a manual fallback; full lifecycle
consistency across entry points remains [RWS-008](../FEATURES.md#rws-008).
Transparent execution from Delta/ordinary terminals is not implemented; see
[the remaining product scope](../FEATURES.md).

## Developer test builds

The `macOS app build` workflow builds an Apple silicon app on pushes/PRs and
provides a downloadable development ZIP in Actions artifacts. It runs the Swift
and release metadata tests. Development bundles are ad-hoc signed and have no
update URL or enabled updater. They are not notarized public installers.

For a local developer build with Rust and Swift installed:

```sh
scripts/build-macos-app.sh development
```

The script creates `dist/development/RWS.app` and a development ZIP. Rebuilding
uses a temporary staging directory, then replaces that same canonical app only
after compilation, packaging and signing succeed. The previous app is preserved
in `dist/archives/` as a ZIP, so it cannot appear as another launchable app.
Quit the canonical app before rebuilding; the script refuses to replace a running
bundle. Failed publication restores the previous build. Open the canonical path
with `open dist/development/RWS.app` to avoid selecting unrelated old copies.
Each bundle records its short Git revision in `RWSBuildRevision` (with `-dirty`
for tracked local changes), while release version numbers remain unchanged. The CLI and Sparkle framework are bundled; personal configuration,
mount receipts, SSH keys and diagnostics are not.

Initial packaging targets Apple silicon, macOS 13+. This deployment target is
not a claim that the experimental FSKit mount backend works on macOS 13; its
compatibility requirements and tested versions remain in the filesystem docs.
Intel packages and clean-machine compatibility require separate validation.

## One-time production setup

No Apple Developer account or production signing keys were available when this
pipeline was introduced. A signed public release and installation of an update
from one signed version to another must pass before calling auto-updates
operational.

Create a GitHub environment named `release` and configure these **secrets** using
the GitHub settings UI, never a tracked configuration file or chat:

| Secret | Value |
| --- | --- |
| `RWS_CERTIFICATE_P12` | Base64-encoded exported Developer ID Application certificate and private key |
| `RWS_CERTIFICATE_PASSWORD` | Password protecting that P12 |
| `RWS_SIGNING_IDENTITY` | Exact Developer ID Application signing identity |
| `RWS_APPLE_ID` | Apple account used for notarization |
| `RWS_APPLE_TEAM_ID` | Developer team identifier |
| `RWS_APPLE_APP_PASSWORD` | App-specific password for notarization |
| `RWS_SPARKLE_PRIVATE_KEY` | Sparkle private key exported using `generate_keys -x` (new-format 32-byte seed, base64 text) |

Set environment **variable** `RWS_SPARKLE_PUBLIC_KEY` to the matching Sparkle
public key. Generate and back up the Sparkle keys using the official
[signing instructions](https://sparkle-project.org/documentation/). CI verifies
the pair without displaying it. Keep recovery copies of signing keys securely.
The certificate is imported only into an ephemeral CI keychain, deleted in a
finally block; the runner is disposable.

Review the project's licensing decision before public distribution. The
repository does not yet declare an overall open-source license.

## Publishing a stable version

Update the version in `Cargo.toml` in a reviewed commit, then create/push the
matching tag `vMAJOR.MINOR.PATCH`. Tagging is the maintainer's release decision;
ordinary development commits do not become public releases.

The `Signed macOS release` workflow then:

1. Runs Rust checks on macOS/Linux and metadata tests, checks tag/version equality,
   and refuses missing signing credentials.
2. Runs Swift tests and builds the app/embedded CLI with matching versions.
3. Signs nested code inside out, enables hardened runtime, notarizes, staples,
   and checks Gatekeeper acceptance.
4. Builds the final ZIP, verifies the Sparkle key pair and generates an update
   appcast with signed archives using the pinned official Sparkle tools.
5. Uploads ZIP, appcast and checksums to a draft GitHub Release, then publishes it
   only after all assets are uploaded. Existing releases are not overwritten.

Users' apps read `https://github.com/ssime-git/RWS/releases/latest/download/appcast.xml`.
Each feed points to its immutable version-specific archive. The first production
release is downloaded manually; subsequent compatible stable versions are
detected automatically through Sparkle. Installation is explicit and waits for
all registered workspaces to be disconnected. Silent installation on quit is
disabled (`SUAllowsAutomaticUpdates=false`) so a downloaded update cannot
interrupt mounted workspaces or trap ordinary Quit. Failed CI runs must not change the published feed.
RWS configuration remains external to the bundle. macFUSE is updated separately.

Before broad distribution, exercise two consecutively signed versions on a
clean Mac: update availability, download/signature verification, deferred
installation with a mounted workspace, installation after disconnect, preserved
configuration and successful reconnect. Recheck a busy mount and failed download.
Do not declare those checks passed solely because a GitHub Release exists.

## Current limits

The native app and build scripts can be tested without an Apple Developer account.
Production signing/notarization and Sparkle delivery require the setup above.
A development build deliberately cannot fetch production updates; replace it
with the first signed release once available. The generic Delta routing rule is
still an agent convention, not interception of native Delta processes; see
[the Delta guide](connection.md#delta-and-linux-commands).

## Configuration discovery at startup

Precedence: existing default Application Support config, remembered chosen source,
then legacy `~/.config/rws/config.json` and `.rws-local/config.json` in at most seven
app-bundle parent directories. This finds a repository development build without
hardcoded personal paths or scanning the disk. An app copied to Applications
cannot discover arbitrary old checkouts; use **Choisir une configuration** once
if no known candidate exists. No configurations are merged or overwritten.

The CLI validates the selected source and returns normalized JSON for the UI.
Errors identify the selected path. Using the original file retains its mount
receipts and avoids changing existing Delta rules. The chosen path is remembered
for future launches. Keep that file and the configured SSHFS executable in place.

Startup checks the installed macFUSE package/runtime and runs the selected SSHFS
`--version` with a five-second bound. Without an explicit SSHFS path, detection
checks adjacent experimental build directories and standard installation paths.
A discovered SSHFS path is used only as an environment override for this app’s
CLI calls; startup never rewrites the configuration or changes its backend.
Use the explicit Save action to persist a changed path/backend.
FSKit requires the RWS fskit3 fixes or later. Detection of installed files does
not prove FSKit activation or remote connectivity; actual mount verification
still runs on **Ouvrir dans le Finder**. Dependency failures disable Open, while
Disconnect remains available. The installation link opens the official macFUSE
site; RWS does not silently install packages or change system extensions.

### Finder sidebar

Ajouter now saves, connects and pins the workspace under Finder Favourites. Opening
an existing workspace also refreshes its favorite. Pin failures are reported without
hiding a successful mount. Disconnect removes the managed favorite after successful unmount; remote data is unchanged.
See [Finder behavior and limits](finder-macos.md).

### Launch a remote agent

Select a workspace, enter `claude`, `codex`, `opencode`, `gemini` or an absolute
remote executable path, then click **Lancer sur la VM**. Terminal opens an SSH
session and displays the remote identity before starting the agent. The executable
is resolved in the VM's login environment, never on the Mac. Missing agents and
SSH failures remain visible in Terminal. Authentication must already be configured
on the VM. This does not redirect agents independently launched by local IDEs.
The private launcher files live under `Application Support/RWS/AgentLaunchers`;
they contain paths/workspace names, not credentials, and are retained so an open
Terminal can safely finish launching. They may be removed once the session starts.
