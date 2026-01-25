# Changelog

This file tracks changes made during development. Subagents: **read this before starting work** to understand recent changes and current project state.

For format guidelines, see `developer_guidelines.md` Section 9.

---

## [2024-01-24] — Project initialization

**What changed:**
- Created `prd.md` with product requirements
- Created `spec.md` with technical specifications
- Created `developer_guidelines.md` with coding standards and workflow

**Why:**
- Establish foundation documents for subagent implementation

**Files affected:**
- prd.md (new)
- spec.md (new)
- developer_guidelines.md (new)
- changelog.md (new)

**Testing notes:**
- N/A (documentation only)

**Dependencies/blockers:**
- Unblocks: All implementation work

---

## [2024-01-24] — Spec revision: reduce over-specification

**What changed:**
- Removed exact project directory structure from spec.md Section 1; replaced with component responsibility table
- Simplified dependencies from pinned Cargo.toml to suggested crate names table
- Converted Rust struct definitions to language-agnostic field descriptions (Sections 4, 5, 6)
- Marked systemd unit template as example rather than exact specification

**Why:**
- Allow implementation flexibility while preserving essential contracts
- Spec should define "what" (interfaces, protocols), not "how" (code structure)
- IPC protocol and config schema remain precise for component compatibility

**Files affected:**
- spec.md (revised Sections 1, 2, 4, 5, 6, 8)

**Testing notes:**
- N/A (documentation only)

**Dependencies/blockers:**
- None

---

## [2024-01-24] — Developer guidelines enhancements

**What changed:**
- Added Section 0: Subagent Workflow with explicit 7-step process
- Strengthened testing requirements: must write tests for new features, must manually verify, must leave system in working state after each commit
- Added Section 9: Changelog format with template and example entry
- Updated pre-commit checklist with new verification steps

**Why:**
- Ensure subagents follow consistent workflow
- Prevent broken builds and partial features from being committed
- Changelog provides context for agents starting new tasks

**Files affected:**
- developer_guidelines.md (added Sections 0 and 9, updated Section 8)
- changelog.md (new)

**Testing notes:**
- N/A (documentation only)

**Dependencies/blockers:**
- None

---

## [2024-01-24] — Task list creation

**What changed:**
- Created task_list.md with 28 implementation tasks across 5 phases
- Phase 1 (Foundation): scaffolding, config, types, paths
- Phase 2 (Daemon Core): IPC server, watcher, debounce, restic execution
- Phase 3 (CLI): all 8 CLI commands
- Phase 4 (TUI): layout, live updates, sparklines, interactivity
- Phase 5 (Polish): error messages, logging, e2e tests, docs
- Each task includes acceptance criteria and dependency markers

**Why:**
- Enable parallel work by multiple agents
- Clear dependencies prevent agents from getting blocked
- Acceptance criteria make completion verifiable

**Files affected:**
- task_list.md (new)

**Testing notes:**
- N/A (documentation only)

**Dependencies/blockers:**
- Unblocks: Task #1 (Project scaffolding) can begin immediately
