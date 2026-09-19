# Startup detection implementation plan

**Goal:** Automatically reuse existing RWS configuration and check mount prerequisites on launch, with clear actionable errors.

**Design:** Prefer the app's existing default config; otherwise a remembered source, then scoped candidates (legacy user config and `.rws-local/config.json` in app-bundle ancestors). Reuse a detected file in place so mount receipts and existing Delta routing remain consistent. Validate/load through the bundled CLI's `workspace list` JSON; never merge or overwrite configurations. Multiple candidates require an explicit UI selection. No disk-wide scan, credential access or automatic macFUSE installation.

- [x] Add tested candidate selection and prerequisite classification helpers.
- [x] Replace manual startup load/import with CLI-validated selection; report the selected file on failures and retain prior valid state.
- [x] Check installed macFUSE runtime and configured/discovered patched SSHFS version asynchronously using the bounded runner. Missing/broken dependencies block Open with actionable French guidance; Disconnect stays available. Installed does not mean FSKit enabled: actual connect errors remain authoritative.
- [x] Add a concise startup banner, candidate choices, recheck and official macFUSE installation link. Keep manual paths in advanced setup.
- [x] Test scoped discovery, ambiguity, existing-default precedence, invalid config, valid Rust JSON and missing/incompatible dependencies; build bundle and run read-only local startup probe.
- [x] Update docs, commit/push and verify CI. Preserve running mounts and user configuration.

No automatic configuration rewriting or backend switch: discovered SSHFS is a
process-scoped override. Public signed updates remain a separate pending
credential-dependent workflow. Revised UI relaunch acceptance remains pending.
