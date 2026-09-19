# Global Delta forwarding

User-authorized scope: all registered RWS mounts, not a project-specific rule.
Keep Delta's built-in local execution model explicit: personal rules route agent
commands through RWS; this is not interception of every Delta internal process.

- [x] Add directory context reporting: local outside registered mounts, remote
  inside a registered verified mount; reject missing paths and symlink escapes.
- [x] Add `exec --cwd PATH --git-context`: exact remote subdirectory, command-scoped
  Git metadata/remote URL mapping without editing Delta-managed `.git` files.
  Restrict metadata and mapped local remotes to the same workspace. Do not switch
  repositories inside one forwarded invocation because its Git context is scoped.
- [x] Generate/install a managed block in Delta's personal rules. Preserve other
  instructions, use a private backup and atomic write, and make installation
  idempotent. Include local-project behavior and stop-on-SSH-failure rules.
- [x] Run automated path, quoting, Git metadata and installer regressions;
  validate real Linux execution and Git on at least two disposable projects.
- [x] Install the final binary and personal rule globally; inspect Delta rule
  loading if available. Document exactly what was and was not tested in Delta.

Evidence: 46 local automated tests pass; format and Clippy clean. Two real disposable projects, including accented path and Delta metadata, passed Linux Python/Git tests. Active Delta Git metadata read-only probe passed. Global personal rules installed; direct Delta-turn compliance unverified because the app was unavailable through UI.
