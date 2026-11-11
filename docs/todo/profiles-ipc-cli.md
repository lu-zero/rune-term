# Profiles, IPC, and CLI

| | |
|---|---|
| Author | _draft_ |
| Date | 2026-11-11 |
| Status | Draft / Todo |
| Scope | Profile-driven creation, rune-term IPC/control surface, CLI/docs tooling |
| Related | `settings-profiles.md`, `multiplexer-sessions.md`, `status.md` (16) |

## Overview

Profiles are the insertion point for “open a tab/pane with different settings” and, by extension, for selecting how that terminal is provided (local, multiplexer-backed, remote). This document stages that work into:

1. **Profiles as creation/attach templates** (building on `settings-profiles.md`),
2. an **IPC/control surface** for rune-term so sessions/tabs can be scripted (an `it2`/iTerm2-like automation workflow),
3. a **CLI** (and generated docs/man pages) that exposes the same operations, and
4. **developer/packaging ergonomics** (`just`) to build/install the growing set of binaries/helpers.

This is a design/todo note, not implemented behavior.

## 1. Profiles: from settings to creation targets

`settings-profiles.md` defines profiles as session templates applied when creating a terminal session/tab/pane, with declarative TOML config and bounded merge/resolution. `multiplexer-sessions.md` defines persistent/multiplexer-backed session flows (local, rmux, tmux-class, etc.).

### 1.1 Near-term (align with status 16)

- Complete named profiles, default profile, and profile selection at creation time (“New Tab/Pane”, “New Tab with Profile…”).
- Apply-on-create covers existing knobs first (font/metrics, theme/palette) through the existing resize/render paths, then shell/argv/cwd/env as supported.
- Keep resolution explicit and bounded: built-in defaults → selected profile fields (no profile inheritance in v1 unless required). Persist per `settings-profiles.md`.

### 1.2 Profile as a backend/attach template

Extend the profile schema (versioned, with a clear compatibility story) so a profile can describe *what* to open, not only *how it looks*:

- `backend = "local"` (default): spawn a local PTY/shell (current path).
- `backend = "ssh"` / `"tmux"` / `"rmux"` / `"osewa"` (and later `rmux-web`, roaming), carrying backend-specific, declarative parameters (target, session name, attach/create policy, command, transport hints) as those backends land.
- Capability-gated validation: a profile requesting an unavailable backend produces a clear, actionable error (and/or a defined fallback) rather than silent misbehavior.

This makes “open with profile” the uniform entry point for local shells, wrappers (run local `ssh`/`tmux` binary), and native multiplexer clients described in `multiplexer-sessions.md`.

### 1.3 Boundaries

- Profiles template creation/attach; they do not replace multiplexer-owned persistence or runtime arrangement restore (out of scope unless specified later).
- Values affecting PTY/grid sizing continue to flow through `ResizeRequest`/`Surface::resize` semantics; no parallel sizing path.

## 2. IPC / control surface (automation, `it2`-like)

To enable scripting and external control (iTerm2 `it2`-like workflows), rune-term should expose a local control plane.

### 2.1 Goals

- Programmatic operations over app objects: windows, tabs, panes/leaves, and their bound sessions/backends.
- Create/attach from a **profile** (the stable, high-level primitive) as well as lower-level actions.
- Query state and subscribe to events (focus, title, bell/progress, exit, resize, backend state) for tooling and integrations.

### 2.2 Shape (direction)

- **Transport:** owner-scoped local IPC (Unix domain socket / Windows named pipe), consistent with the trust posture used by rmux; no network exposure by default.
- **Protocol:** versioned, capability-gated request/response + event stream, with a stable object model/ids (window/tab/pane/profile/session where applicable). Reuse proven framing/discipline patterns from rmux where sensible, but treat rune-term as its own authority (distinct from controlling an rmux daemon).
- **Security:** least-privilege by default; explicit gating for sensitive/mutating actions (input injection, spawning/attach, reading screen contents) as the surface grows.
- **Phasing:**
  1. Read-only: list/query state, stream events.
  2. Mutating basics: create (from profile), focus/activate, close/detach, resize.
  3. I/O and richer control: send text/keys, snapshots/text capture, profile/backend management, subject to capability/policy.

An `it2`-like client experience (scriptable, discoverable commands over live terminal objects) is the target UX for this surface.

## 3. CLI, man pages, and documentation

The control/automation operations should be available via a CLI that shares the same semantics as the IPC API.

- **Argument parsing:** use `winnow-args` for CLI needs, to provide structured parsing with generated help and documentation artifacts.
- **Man pages & docs:** generate and ship man pages and reference documentation from the CLI definition so commands, flags, profiles/backends, and IPC-driven operations are documented consistently.
- **Surface alignment:** CLI subcommands map onto IPC/capability operations (e.g. list, show, new/open --profile, attach, send, capture, subscribe/watch), keeping one source of truth for behavior.
- **Packaging:** as helpers/backends grow (`rune-term`, protocol helpers/agents per `multiplexer-sessions.md` fat/lean model), the CLI and man pages must cover the installed composition.

(Exact crate/module layout for the CLI binary/binaries — e.g. within `rune-term`, a dedicated `rune-term-cli`, or a multicall/helper — to be decided when implementation starts, but designed so `winnow-args` definitions, IPC client, and doc/man generation stay shared.)

## 4. Tooling: `just`

The repository currently has no `justfile` (`AGENTS.md`). The expanding set of build/install targets (GUI, CLI, helpers/backends, man pages/docs, config/assets) motivates a standard task runner:

- Introduce a `justfile` with recipes for common workflows: build/test/check/fmt, run, and **install** of the selected composition (lean core, fat, or specific helpers/backends).
- Provide recipes to generate/install **man pages and CLI documentation** alongside binaries.
- Keep recipes thin wrappers over `cargo`/workspace commands (and any doc-generation steps), so CI and local use share the same entry points.
- Update contributor/build docs (`README.md`, AGENTS/tooling notes as appropriate) when `just` becomes the recommended interface.

## Staged plan

1. **Profiles foundation:** named profiles + default + “open with profile” for current settings; persist/load per `settings-profiles.md`; wire shell/cwd/env.
2. **Profile schema readiness:** add explicit `backend` (default `local`) and versioned, capability-validated backend parameters as adapters land.
3. **IPC read-only:** local endpoint, protocol framing, list/query/events for windows/tabs/panes/profile state.
4. **CLI bootstrap:** `winnow-args`-based CLI over IPC for read/query and create-from-profile; generate initial man pages/docs.
5. **IPC mutating + automation:** create/focus/close/resize/input/capture with policy; reach `it2`-like scripting parity for core flows.
6. **`just` & packaging:** `justfile` for build/install/docs/man across lean/fat/helper compositions; align release/install guidance.
7. **Backend expansion:** progressively bind profiles/IPC/CLI to `rmux`, `ssh`, `tmux`, `rmux-web`, `osewa`, and roaming experiments per `multiplexer-sessions.md`.

## Open questions

- **Object model/ids:** stable identifiers and naming across windows/tabs/panes and backend-native session ids (esp. when a pane rebinds or a backend session is multiplexed).
- **CLI binary shape:** single multicall, subcommands under `rune-term`, or separate `rune-term-cli`/helper binaries; how it coexists with fat/lean packaging.
- **`winnow-args` integration details:** module layout and the exact man/doc generation pipeline to standardize on.
- **IPC schema evolution:** version negotiation, capability advertisement, and forward/backward compatibility (and relationship to any reused rmux framing ideas).
- **Security policy:** which operations require explicit opt-in/authz, and how remote/agent scenarios are treated.
- **Profile/backend validation UX:** when a requested backend/helper is missing vs misconfigured vs unreachable.

## Acceptance cues (per stage)

- A user can define/select a profile and open a tab/pane that deterministically reflects it.
- A script/CLI can discover state and create a tab from a profile over local IPC with documented, stable commands.
- Man pages and reference docs are generated from the CLI definition; `just` installs the chosen composition reproducibly.
- Adding a backend is primarily “implement adapter + declare capability/params + profile schema entry”, not bespoke UI plumbing.