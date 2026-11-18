# rune-term-agent: roaming sessions, and shells that outlive the UI

| | |
|---|---|
| Author | _draft_ |
| Date | 2026-11-18 |
| Status | Decided in outline; phases 0 and 1 built |
| Scope | A lean process that holds terminals for the UI: on an ssh host, over a roaming link; here, across UI restarts |

## Goal

- A session on a host survives a change of network (Wi-Fi to tethering, a new
  address, a laptop asleep for an hour) without the user doing anything, as
  mosh's does.
- After a long drop the client resumes where it was: the exact bytes it missed,
  or the current screen when those are gone.
- Later: a local shell survives the UI quitting, crashing or being updated. The
  UI comes back and finds its tabs, as iTerm2's do.

Not a goal: a replacement for mosh's predictive echo in the first phases, or a
transport for tmux (tmux over ssh already works and stays).

## What rmux gives

Checked against rmux `main` at 1f4571e.

- `rmux-sdk` reaches a daemon only over a Unix socket or a Windows pipe. The
  only network path is Web Share, an end-to-end-encrypted WebSocket for a
  browser. Nothing in rmux roams or reconnects, so a remote client needs a
  process on the host that talks to the daemon there.
- `Pane::recover_output` is a recoverable raw stream: a rebase keyframe (ANSI
  bytes that reset and rebuild an emulator, with bounded history), then the
  pane's exact bytes with a per-pane sequence. The daemon sends a new rebase on
  resize, history clear, reset, generation change, and when the reader lags.
  Rebase on lag is mosh's skip to the latest screen, given a reader that drains
  no faster than the network.
- A stream cannot resume from a sequence: each one opens with a rebase. The
  agent outlives the client, so it holds what the client has not acknowledged.

## Shape

```
rune-term ──QUIC, roams──▶ rune-term-agent (HOST) ──rmux -C, rmux-sdk──▶ rmux daemon
    │                             ▲
    └──── ssh HOST, once ─────────┘  prints port, certificate fingerprint, token; ssh exits
```

- **Bootstrap.** `ssh -T HOST rune-term-agent serve --session S` starts the agent
  detached. It binds a UDP port, makes a self-signed certificate, and prints one
  line: port, certificate fingerprint, a random token. ssh exits. ssh is the
  authentication; the agent trusts nothing it did not print over it.
- **Transport.** QUIC (`quinn`, TLS 1.3). QUIC migrates a connection to a new
  client address by itself, which covers most roaming. When the connection
  times out the client dials again with the token and, per pane, the last
  sequence it applied.
- **Resume.** The agent keeps each pane's bytes since the last acknowledged
  sequence, up to a bound. Within it, it replays; past it, or after a rebase,
  it sends the current keyframe.
- **Lifetime.** The agent exits when its session ends, or when no client has
  come back for a time a profile sets. The rmux session outlives it either way.

## Threat model

- The agent listens on a UDP port reachable by anyone who can reach the host.
  It accepts a connection only with the TLS certificate the client pinned and
  the token it was given over ssh. Both are fresh per agent.
- The token authenticates one client. A second client with the same token
  replaces the first (that is the roam), so the token is a credential: it is
  never logged, written to disk on the client, or passed on a command line.
- What a connected client can do is what the user's own shell on the host can
  do. The agent adds no privilege, runs as the user, and talks only to that
  user's rmux daemon over its owner-scoped socket.
- A replayed or reordered packet is QUIC's to refuse. A client that loses the
  token loses the session's link, not the session: ssh can start a new agent.

## Phases

0. **Spike.** Build `rmux-sdk` in this workspace (it brings tokio; the
   dependency rules apply). Check that a lagging recovery stream rebases rather
   than queues, and that a keyframe replayed into librio draws the pane as
   rmux has it. Report either result before phase 1.

   Done against rmux 0.10.0, outside the workspace. `rmux-sdk` with default
   features off builds with 46 crates, tokio among them. An opening keyframe of
   an 80x24 pane, 1.9 KB with all 20 history rows, rebuilt a librio surface that
   matched rmux's snapshot on every row. Bytes after it kept the two in step. A
   reader that stopped while the pane printed three million lines got one
   `Lag` rebase and no backlog, and matched again after it. librio keeps a tab
   as a `\t` cell where rmux has spaces, so the rows compare with tabs as
   spaces.
1. **Control mode over QUIC.** The agent bridges `rmux -C` and the client is the
   existing `mux` client over a new link kind. A session survives a change of
   network, and a long drop reattaches on its own rather than through
   Reconnect.
2. **Exact resume.** Pane output comes from recovery streams; control mode
   still carries windows and layout. A reattach is a replay or a keyframe, so
   this path drops the `capture-pane` redraw.
3. **Predictive echo.** An overlay of guessed characters, confirmed or rolled
   back by the stream. It lives in the emulator, so it is a librio change,
   reported and kept on the fork.
4. **Local PTY host.** The agent runs here as well, holding the shells the UI
   used to own, so the UI can restart, crash or update without ending them. The
   UI reattaches over a local socket.

## Open decisions

- **Local host (phase 4): what holds the screen while the UI is gone.** Either
  the agent hands the PTY descriptors back over the socket (`SCM_RIGHTS`) and
  the UI restores the screen from state it saved, or the agent runs an emulator
  and sends keyframes with the remote wire format. A third choice runs local
  shells as panes of a private rmux server, which gives keyframes for free but
  makes rmux a requirement for a plain tab.
- **Wire format.** One crate (`term-roam`) for the frames both sides speak, so
  the local host and the remote agent share it if phase 4 takes the keyframe
  route.
- **Install.** Decided: the host builds its own checkout, `just
  agent-install`, as [`../agent.md`](../agent.md) describes. Copying a binary over ssh
  on first use is later, if at all.
- **Packaging.** `rune-term-agent` is a binary without GUI dependencies, the
  lean half of status item 24.
