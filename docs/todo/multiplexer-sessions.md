# Multiplexer-backed sessions

| | |
|---|---|
| Author | _draft_ |
| Date | 2026-11-11 |
| Status | Draft / Todo |
| Scope | rune-term session concept; first target rmux, applicable to tmux-class backends |

## Overview

rune-term today owns ephemeral, in-process terminals: one Rune window with tabs and an integer-cell split tree, where each leaf is a local `librio::Surface` (`RenderState`, PTY, VT) created by a single in-process engine (see `docs/todo/rune-term.md`, `rune-term/src/{model,surfaces,tabs,split,delegate}.rs`). There is no detach/reattach, no named persistent workspace, and no remote access.

This document defines a **session concept** and the **user-facing use flows** needed to support multiplexer-backed terminals. The primary backend is **rmux** (daemon-owned sessions/windows/panes over local IPC, typed SDK/protocol, and E2EE web-share remoting). The model is intended to be rich enough to apply, with backend-specific mapping, to **tmux** or other tmux-like multiplexers.

This is a design/todo note, not implemented behavior. When parts land, update `docs/todo/status.md` (and this file as needed) in the same change.

## Goals

- Make “session” an explicit concept: a named, persistent terminal workspace a user can create, attach, detach, list, and resume.
- Keep the fast **local ephemeral shell** as the zero-setup default/fallback (current behavior).
- Allow a pane/workspace to be backed by a **multiplexer daemon** (rmux first) so process/screen lifetime can outlive a window, tab, or app restart.
- Support **multi-client** semantics where the backend allows it (observe, and define resize/input/view policy).
- Reuse rmux **Web Share** as the remote access/remoting path for rmux-backed sessions (operator/spectator browser access over an untrusted tunnel/transport) rather than inventing a parallel sharing stack.
- Define flows and state clearly enough to drive a backend abstraction and UX without committing to a single UI layout prematurely.

## Non-goals (for this concept pass)

- Implementing a tmux backend now; tmux is a mapping target, not a required first delivery.
- A native rune-term-to-rune-term encrypted remote-attach transport (distinct from rmux web-share).
- Full state restoration of rune-term-local UI (exact split geometry, local search/selection view state) unless explicitly specified later.
- Live migration of an already-running local `librio` PTY into a multiplexer session; promotion may start a fresh process unless a backend supports migration.

## Concepts

| Concept | User meaning | rune-term today | rmux mapping | tmux mapping (conceptual) |
|---|---|---|---|---|
| **Window** | One OS window with Rune chrome. | Yes. | Client-local (not daemon-owned). | Client-local. |
| **Workspace tab** | A tab in the strip containing one pane or a split tree. | `Tab { title, root, focused }`. | Closest to an attached view of an rmux session/window; exact binding is an open decision. | Attached session/window view. |
| **Pane** | One terminal view (grid, cursor, scrollback, I/O). | `Leaf` + `HostSurface` (`librio::Surface`, `RenderState`). | rmux pane (`PaneId`, daemon PTY/screen state). | tmux pane. |
| **Session** | Named persistent workspace that can outlive the UI. | None (implicit/ephemeral). | rmux `Session` (`SessionId`/name, windows/panes, recency). | tmux session. |
| **Attach** | Bind UI to a running session/pane. | N/A. | Client attach / SDK handle + subscriptions. | `attach-session` / client attach. |
| **Detach** | Stop viewing without stopping the workload. | N/A. | Detach; session remains in daemon. | `detach-client` semantics. |
| **Share / remote access** | Let another user view/control remotely. | N/A. | rmux Web Share (E2EE browser, operator/spectator, tunnel/static frontend). | Typically external (e.g. tmux + ssh); out of scope except as contrast. |

**Invariant to preserve:** rune-term’s rendering/winsize discipline (exact cell-pixel winsize, integer-cell layout, dirty/upload budgets per `docs/todo/rune-term.md`) must hold regardless of backend; the backend supplies state/I/O, not a second windowing model.

## Use flows

### F1. Quick local shell (default)
1. User opens rune-term or creates a new local tab.
2. App creates a local workspace tab + local pane (current path).
3. Closing the tab/window ends that pane’s lifetime.
- **Properties:** no daemon required, no persistence, no session identity, no sharing.

### F2. Create a persistent session
1. User invokes “New Session…” and provides/accepts a name (and optional cwd/profile/command).
2. App connects to (or starts, per policy) the local multiplexer daemon and creates/ensures the session + initial pane/window.
3. App opens a tab attached to that session and renders it live.
4. UI shows session identity and backend state (name, attached).
- **Variants:** name exists → attach (F3) or choose another; daemon unavailable → actionable error and offer local tab (F1) or retry/start.

### F3. Attach to an existing session
1. User invokes “Attach Session…” and sees discoverable sessions (name, activity, counts, attached-elsewhere indicator, metadata as available).
2. User selects a target.
3. App attaches and binds/syncs the view to current screen state.
- **Already attached elsewhere:** policy must be explicit (multi-client allowed, notify, or exclusive/takeover) because input and resize fanout differ.

### F4. Detach vs close
1. **Detach:** user leaves the session running in the daemon; UI unbinds (tab closes or becomes a disconnected placeholder by design).
2. **Close/kill (scoped):** user terminates pane/window/session; destructive and confirmed when it affects a persistent named session.
3. **Close app window:** per policy/prompt — detach attached sessions, kill them, or ask (optionally remembered).
- **Rule:** detach is never process termination; verbs and confirmations must make the distinction unambiguous.

### F5. Resume later / after restart
1. User opens rune-term with no local terminal state.
2. App offers recent/active daemon sessions and/or auto-attaches last-used (opt-in).
3. User attaches and continues with process/screen state preserved by the daemon.
- **Daemon not running:** present “no daemon/sessions” state with actions to start/connect or open a local tab.

### F6. Multiple clients
1. The same backend session/pane is viewed by more than one client (another rune-term window, CLI, or web-share viewer).
2. Defined behavior is required for: shared vs independent viewport, **resize authority** (e.g. last writer, minimum, or fixed), focus/title/bell propagation, and input interleaving.
3. UI indicates multi-client/contested state and the active policy.

### F7. Remote access / remoting (rmux Web Share)
1. User selects an rmux-backed pane/session and invokes “Share…”.
2. User configures scope and role (operator vs spectator), TTL, PIN/max viewers, tunnel/provider or URL, and frontend URL as applicable.
3. App creates the share via rmux and presents links/QR/status.
4. Remote browser viewers authenticate/handshake per rmux policy and view/control by role; transport/tunnel is untrusted (E2EE between browser and daemon).
5. Owner monitors, copies, and revokes/stops shares; share ends on stop, TTL, or session exit.
- **Notes:** local-only panes are not directly web-shareable without an rmux-backed context. Operator credentials are sensitive; defaulting toward least privilege (spectator) is preferred where sensible.
- **tmux/generalization:** other backends may not provide equivalent built-in remoting; treat sharing as a capability, not a universal assumption.

### F8. Promote local to persistent (optional)
1. User starts local (F1) and later requests persistence/sharing.
2. App creates a backend session/pane and rebinds the workflow as defined.
- **Expectation:** unless true PTY migration exists, promotion likely starts a fresh process/command; UX must state this clearly.

## Pane/tab state model

```text
[Local]
  -> (create/ensure backend session) -> [Attaching]

[Attaching] -> [Attached Live]
[Attaching] -> [Error/Degraded] (retry | fallback to local)

[Attached Live] -> [Detached]        (UI unbound, daemon keeps running)
[Attached Live] -> [Stale/Disconnected] (transport/daemon loss; retry or manual)
[Attached Live] -> [Exited/Gone]     (process/session ended)

[Detached] -> [Attaching]            (reattach)
[Stale/Disconnected] -> [Attaching]  (reconnect)
[Detached|Exited] -> [Gone]          (destroyed/forgotten)

[Attached Live] <-> [Shared]         (rmux web-share active; orthogonal viewer state)
```

Representative UI-visible states: `local`, `attaching/syncing`, `live`, `detached` (listed/persistent), `stale/disconnected`, `contested/multi-client`, `shared (roles/count)`, `exited` (reason/code), `error`.

## Backend abstraction (direction)

To support local and multiplexer-backed panes uniformly, a pane host should separate **view/chrome state** from a **terminal backend**:

- **Local backend:** current `Engine` + `librio::Surface` + `RenderState` + delegate/wakeup/flags (source of truth in-process).
- **Multiplexer backend (rmux first):** daemon-owned session/window/pane accessed via a client surface (prefer `rmux-sdk` as the public integration boundary, or `rmux-proto`/`rmux-ipc` where needed), with snapshots/streams/events mapped into grid updates, resize, and input.
- **Capability-driven behavior:** attach/list, create/kill, resize, snapshots/output/render/state streams, scrollback, clipboard/OSC policy, mouse modes, images, hyperlinks, search, and sharing differ by backend and version; flows must degrade or gate explicitly.

This abstraction is a prerequisite for F2–F7 without regressing F1.

## Open decisions

- **Mapping:** rune-term tab == rmux session, vs tab == window/leaf bound to rmux panes (mixing local and remote in one split tree is higher complexity).
- **Resize authority:** policy for multi-client and for reconciling Rune layout/cell-px with backend terminal geometry.
- **Persistence boundary:** what, if anything, beyond daemon state is restored (tab titles, local split layout, viewports/overlays).
- **Promotion semantics (F8):** fresh process vs migrate; how commands/cwd/env are captured.
- **Default posture:** local-first vs session-first; daemon auto-start/connect behavior.
- **Destructive UX:** confirmations and defaults for detach vs close/kill at pane/window/session/app scopes.
- **Multi-client viewport:** shared cursor/viewport or independent follow modes where supported.
- **tmux mapping details:** session/window/pane identity, attach flags, control/streaming interface, and which features (e.g. sharing) are unavailable.

## Security and trust notes

- rmux local control uses owner-scoped local IPC; the daemon is the authority for persisted terminal state.
- rmux Web Share treats tunnels/relays/frontends as untrusted transport/delivery paths per its model (E2EE browser↔daemon, scoped roles/tokens/PIN/TTL). Any future native non-local transport must define an equivalent explicit threat model and must not weaken local IPC assumptions.
- Operator-level remote control is credential-equivalent; UI must surface active remote viewers/control clearly.

## Suggested implementation order (when scheduled)

1. Decide mapping (tab/session/window) and resize/destructive policies; record them here.
2. Introduce an explicit session/pane-backend abstraction while keeping F1 unchanged and existing tests green.
3. Implement F2–F5 against local rmux daemon (connect/start, ensure, list/attach, detach/resume) with clear state/UI.
4. Add F7 share management for rmux-backed targets.
5. Address F6/F8 and tmux-backend evaluation as follow-ups based on validated abstractions and capabilities.
