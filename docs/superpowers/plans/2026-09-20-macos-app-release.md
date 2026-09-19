# macOS app and release implementation plan

> **For agentic workers:** Use subagent-driven-development for the independent app task and review; coordinate release integration in the parent task.

**Goal:** Replace routine terminal commands with a native RWS window and prepare signed automatic updates.

**Architecture:** A SwiftUI macOS app calls the bundled Rust CLI using argv, retains configuration in Application Support, and opens verified mounts in Finder. Sparkle handles signed updates. GitHub Actions builds test artifacts, while production publishing requires explicit signing secrets and notarization success.

**Tech Stack:** Rust CLI, Swift Package Manager / SwiftUI, Sparkle 2, GitHub Actions, Apple codesign/notarytool.

## Global constraints

- No Apple Developer account exists yet: do not claim notarization or production update delivery is validated.
- Existing private configuration and active mounts must remain intact.
- Never silently execute remote workloads locally or force unmount.
- No private credentials or machine-specific configuration in published artifacts.
- Normal user workflow: add SSH host/path once, Open connects and opens Finder, Disconnect unmounts.
- Keep external macFUSE/experimental SSHFS prerequisites explicit until portable packaging is validated.
- Retain personal settings outside the app across updates.
- Do not install updates while an RWS operation or registered mount is active.
- Automatic update checks, explicit guarded installation; silent installation on quit is disabled.

## Task 1: native app

- [x] Add macos/Package.swift, Sources/RWSApp and Tests for process/config behavior.
- [x] Use Bundle resources/bin/rws, default Application Support/RWS/config.json; add/open/disconnect/status via subprocess arguments, never shell interpolation.
- [x] Expose SSHFS selection in setup, actionable errors, details and prerequisite help, and optional Delta rule installation using bundled stable executable path.
- [x] Integrate Sparkle only when public key/feed configured; unsigned development bundles must not attempt updating.
- [x] Block updater installation while CLI operations or registered mounts exist; fail closed on uncertain state.
- [x] Build and test with the CommandLineTools toolchain; report limitations.

## Task 2: bundle and release

- [x] Add bundle builder with version derived from Cargo.toml, copied CLI and Sparkle framework, proper Info.plist and stable bundle identifier.
- [x] Add unsigned artifact CI builds on macOS, alongside existing Rust checks; production tags must run checks before publication.
- [x] Add signed release workflow requiring Developer ID certificate/password, notarization credentials, Sparkle key pair; missing credentials must fail rather than publishing unsigned release.
- [x] Sign nested code inside out, notarize/staple bundle, sign update archive and generate appcast; publish immutable versioned archive and latest stable appcast only after successful checks.
- [x] Verify scripts, plist, bundle build and tests locally. No production release tag or public update publication without signing prerequisites.

## Task 3: documentation and review

- [x] Document one-time developer secrets setup, developer test builds versus production releases, and user installation/update flow.
- [x] Review app and release integration for data loss, unsafe shell expansion, unsigned update paths and mount/update races.
- [x] Record exact local test results and outstanding signed-release/second-machine acceptance requirements.

## Acceptance boundary

Implementation and local checks completed. Graphical clicks could not be exercised
because CUA failed to read the app window. Production notarization and actual
Sparkle installation remain pending Apple Developer credentials and a two-version
release test. No production tag or release was published.
