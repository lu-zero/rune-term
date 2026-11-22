# rune-term-ctl

Control and inspect rune-term (profiles, tabs, runtime state).

## Usage

```
rune-term-ctl [OPTIONS] <COMMAND>
```

## Commands

* [`info`](rune-term-ctl-info.md) — Print basic CLI/protocol information.
* [`profiles`](rune-term-ctl-profiles.md) — Inspect configured profiles.
* [`tabs`](rune-term-ctl-tabs.md) — Inspect live tabs/panes (requires a running rune-term control endpoint).
* [`handover`](rune-term-ctl-handover.md) — Flip the running instance's handover on: its exit hands its shells to keepers.

## Options

* `--json` — Output machine-readable JSON.
* `--socket <PATH>` — Path to the control socket.
* `-h, --help` — Print help (see a summary with '-h')
* `-V, --version` — Print version
