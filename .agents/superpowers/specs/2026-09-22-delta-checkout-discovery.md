# Delta Checkout Discovery Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make RWS-generated Delta rules tell agents to discover their checkout with a local `pwd -P` probe instead of asking the user which machine they are on.

**Architecture:** Keep the global managed RWS block and its explicit `rws context`/`rws exec` routing. Add one deterministic local discovery step before context resolution, then assert that generated rules contain it. No terminal-emulator configuration or process redirection changes are included.

**Tech Stack:** Rust, existing unit tests in `src/agent_rules.rs`, Delta `AGENT.md` generation.

---

## Chunk 1: Generated Delta rule

### Task 1: Specify checkout discovery

**Files:**
- Modify: `src/agent_rules.rs:managed_rules`
- Test: `src/agent_rules.rs:tests`

- [x] **Step 1: Write the failing test**

  Extend the existing managed-rule test to require `pwd -P` and a clear instruction not to ask the user to identify the machine when the local path can be probed.

- [x] **Step 2: Run the focused test to verify it fails**

  Run: `cargo test --locked agent_rules::tests::preserves_custom_rules_and_replaces_idempotently_with_private_backup`

  Expected: FAIL because the generated managed block lacks the checkout-discovery wording.

- [x] **Step 3: Implement the minimal generated-rule wording**

  Before the `rws context --cwd` command, direct the agent to run `pwd -P`, use that result as the absolute checkout path, and stop with the actual path error if unavailable rather than asking which machine it is on.

- [x] **Step 4: Run the focused test to verify it passes**

  Run: `cargo test --locked agent_rules::tests::preserves_custom_rules_and_replaces_idempotently_with_private_backup`

  Expected: PASS.

- [x] **Step 5: Run regression checks**

  Run: `cargo test --locked agent_rules::tests`

  Expected: PASS, including preservation, idempotence, and malformed-marker safety.

### Task 2: Refresh local Delta rules and document the boundary

**Files:**
- Generated locally: `/Users/seb/.config/delta/AGENT.md`
- Verify: `docs/connection.md:Delta and Linux commands`

- [x] **Step 1: Regenerate the managed personal Delta block**

  Run the built RWS executable with the active absolute configuration and `delta-rules --output /Users/seb/.config/delta/AGENT.md`.

- [x] **Step 2: Verify the installed block**

  Confirm that the managed block contains `pwd -P`, `rws context`, and `rws exec`, and that no duplicate managed markers exist.

- [x] **Step 3: Record the operational boundary**

  Report that the current Delta conversation must still use an existing checkout inside `/Volumes/RWS-*`; an unassociated conversation cannot be assigned a mount by a global instruction file.

- [ ] **Step 4: Commit**

  Run: `git add src/agent_rules.rs .agents/superpowers/specs/2026-09-22-delta-checkout-discovery.md && git commit -m "Guide Delta agents to discover mounted checkouts"`
