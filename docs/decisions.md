# Decisions and prototype scope

Recorded on 2026-09-19. This document distinguishes confirmed user decisions from implementation proposals. The supplied product and technical specification v0.1 is a design input, not authorization to implement every feature it describes.

## Confirmed direction

- Start the prototype quickly; do not delay it to fully design the future applications.
- Organize the repository for open-source development from the beginning. This does not expand the prototype's functional scope.
- Start on macOS.
- Plan a graphical desktop application for macOS first, then Windows and Linux.
- Plan an iOS application supporting both remote file browsing/transfers and remote command/coding-agent execution and monitoring.
- The iOS application must connect independently; it must not require an awake Mac as a relay.
- The future experience must support remote sessions that survive disconnections and can be resumed from another device.
- Revisit desktop and iOS application details later, once the core is sound.

## Proposed first prototype

Deliver a narrow end-to-end workflow:

1. Use an existing SSH destination to reach a Unix-like host.
2. Mount one remote workspace on macOS using existing SSHFS/macFUSE tooling, subject to local feasibility checks.
3. Open, edit, and save a file through the mounted path; verify its contents remotely.
4. Map a mounted local directory to its owning host and remote directory.
5. Run an explicit command and an interactive shell remotely in that directory, with useful errors and exit status.

After that workflow works, validate a remotely launched coding-agent CLI and a second host. Agent installation and authentication remain explicit prerequisites.

Success means verified remote saves, correct execution host and directory, and explicit failures on connection loss. Filesystem behavior must be measured: the prototype must not claim atomic saves, corruption resistance, a unified Finder volume, or bounded caching without evidence.

## Architecture boundaries to preserve

Keep the workspace/host model and path resolution separate from the CLI, SSH process launching, macOS mount management, and future graphical interfaces. A remote workspace must be identifiable without a local mount, so future mobile clients can address it directly.

Use system OpenSSH for the macOS prototype as proposed in v0.1. Treat this as a platform integration, not a requirement that every future client runs the same executable or shares the Mac's SSH configuration.

Share logic where there is a concrete need. Avoid speculative mobile code, empty platform packages, a custom filesystem, or a general plugin framework at this stage. Future session persistence must remain possible without becoming a prerequisite for the first mount-and-execute proof of concept.

## Deferred decisions and features

- Remote session mechanism: optional RWS component versus an existing session tool. The user has not selected either approach or authorized remote installation.
- iOS transport, authentication, file integration, and UI.
- Desktop UI framework and Windows/Linux filesystem integration.
- Persistent session reattachment across devices.
- Advanced transfers, remote watchers, custom filesystem, offline behavior, installer, updater, and signing.
- License selection. The public repository has since been created and published with user authorization; licensing remains open.

## Repository organization

Start with root-level project documentation, `docs/` for decisions and technical notes, and fictitious examples. Add source packages and meaningful tests when implementation begins, with boundaries driven by the prototype rather than by the entire roadmap.

Before a public release, provide a license, setup and test instructions, contribution guidance, security reporting instructions, and CI appropriate to the implemented code. Never commit credentials, personal host details, runtime state, or logs.
