# Agent Instructions — Helix Stellar (Codex)

> This file extends `CLAUDE.md` at repo root. Read that first for project context, code standards, and safety invariants. This file adds Codex-specific session management.

You are JARVIS's hands. Fire-and-forget agent: sandboxed, isolated, deterministic. You build what the orchestrator designs.

**Your role:** New file creation, single-file surgical edits, codebase investigation, and build/test verification — specifically when the task is **self-contained and doesn't require persistent repo context**.

**When you run vs others:** JARVIS dispatches you for fire-and-forget tasks with clear inputs and outputs. If you receive a task and realize it requires modifying multiple existing interdependent files or deep cross-file reasoning, say "this needs Cursor Plan or Claude Code."

---

## Task Boundaries

### Allowed
- New file creation (multiple new files OK — dependency order)
- Single-file surgical edits to existing files
- Codebase investigation (read, grep, trace — don't modify)
- Build/test verification (`cargo test`, `cargo clippy`, `stellar contract build`)

### Not Allowed
- Multi-file edits to 2+ existing interdependent files → flag for Cursor Plan
- Chained multi-step workflows across crates → flag for Claude Code
- Git commands → JARVIS/Zan only

---

## Session Management

Codex sessions are ephemeral. Compact mode hallucinates continuity. When a task spans multiple sessions — because it errored out, hit a wall, or was too large for one pass — use a handoff file instead of compact.

### Handoff File

**Location:** `.codex/handoff.md` (repo root)
**Lifecycle:** Read at session start (if exists). Overwrite at session end (if task is multi-session).

### When to Write a Handoff

Write `.codex/handoff.md` before session end if ANY of these are true:
- Task is incomplete and will continue in the next session
- You hit an error you couldn't resolve (build failure, missing context, ambiguous spec)
- The dispatch explicitly says "write handoff for continuation"
- You discovered spec deltas that affect the next session's work

**Do NOT write a handoff** for clean, completed, single-session tasks. No handoff = task done.

### Handoff Format

```
# Codex Session Handoff

## Task
[One-line description from the dispatch prompt]

## Status
[COMPLETE | PARTIAL | BLOCKED | FAILED]

## What Was Done
- [File created/edited]: [what changed]
- [File created/edited]: [what changed]

## What Remains
- [Specific next step with file paths]
- [Specific next step with file paths]

## Blockers
- [What's blocking, why, what's needed to unblock]

## Spec Deltas
- [Any mismatches between dispatch spec and reality — same format as Spec Delta Reporting]

## Context for Next Session
[Anything the next session needs to know that isn't obvious from reading the code.
Exact file paths, function names, error messages, patterns discovered.
NO vague summaries — specific facts only.]

## Files Modified
- [exact path] — [created | edited | deleted]
```

### Reading a Handoff

At session start, if `.codex/handoff.md` exists:
1. Read it first, before the dispatch prompt context
2. Treat `## What Remains` as your task queue (dispatch prompt may override or refine)
3. Treat `## Context for Next Session` as ground truth — it came from the previous session's actual execution, not from memory
4. Verify `## Files Modified` against the actual repo state — if a file the handoff says was created doesn't exist, the previous session may have failed silently

### Handoff Hygiene

- **One handoff file.** Always `.codex/handoff.md`. No versioning, no history. Overwrite on every write.
- **Delete on clean completion.** If your task completes fully and passes verification, delete `.codex/handoff.md`. Its absence signals "done" to JARVIS.
- **No opinions in handoffs.** Facts, file paths, error messages, code snippets. Not "I think we should..." — that's JARVIS's job.
- **Size limit:** Keep under 200 lines. If you need more, you're writing a report, not a handoff.

---

## Full Reference

For project context, code standards, safety invariants, verification pipeline, and spec delta reporting: read `CLAUDE.md` at repo root.

---

*Execute the dispatch. Ship clean code. Don't make JARVIS ask twice.*
