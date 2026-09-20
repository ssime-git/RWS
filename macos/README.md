# RWS for macOS

This SwiftUI executable is the native front end for the RWS Rust CLI. Normal app operations pass user values as separate process arguments to
`RWS.app/Contents/Resources/bin/rws`. The interactive agent action is different:
`AgentLauncher` writes a private, shell-quoted `.command` file to open Terminal
and invoke the bundled `rws agent`. See [architecture](../docs/architecture.md).

## Build and test

The package targets macOS 13 or later and pins Sparkle 2.10.0 exactly.

For the complete bundle and prerequisite procedure, follow the
[developer guide](../docs/development.md). From the repository root:

```sh
swift test --package-path macos
scripts/build-macos-app.sh development
open dist/development/RWS.app
```

The bundle script resolves the Swift release output directory, embeds the Rust
CLI and Sparkle framework, signs the development bundle and safely replaces its
canonical location. `swift build` alone does not produce this assembled app.

## Local state and prerequisites

New settings live at `~/Library/Application Support/RWS/config.json`. Startup
reuses an existing default/remembered configuration or a scoped adjacent
`.rws-local` configuration. Selection validates through the CLI and uses the
original file without copying mount receipts. Startup does not rewrite settings
or switch mount backends. Automatically discovered SSHFS is passed as a
process-scoped executable override; explicit Save is needed to persist changes.
See [discovery details](../docs/macos-app.md#configuration-discovery-at-startup).

macFUSE and the external patched RWS SSHFS must already be installed. Startup
checks them and shows actionable errors; it does not install system software.

The optional Delta action adds rules that tell Delta agents to forward explicit
commands through RWS. Delta, Finder, editors, Git integrations, and other native
processes still execute locally unless they explicitly use that forwarding.

## Updates

Sparkle starts only when the production bundle has all three values:

- `RWSUpdatesEnabled` is true;
- `SUPublicEDKey` is nonempty and is not a placeholder;
- `SUFeedURL` is HTTPS.

Development bundles therefore do not contact an update feed. Automatic update
checks remain opt-in. Automatic downloads are disabled (`SUAllowsAutomaticUpdates`
must be false in the bundle) so an ordinary Quit can never install a staged
update while a mount is active. Before an explicit update relaunch,
or quitting for an update, the app reruns `rws status --no-probe`; installation
is postponed when an operation or registered mount is active, status is
uncertain, or status output was truncated. The Check for Updates action retries
a postponed install after a safe status refresh. Ordinary Quit remains available
while a volume is mounted and never unmounts it.
