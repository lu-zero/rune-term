# Settings and profiles

Prep note for status **16 (Settings)**. "Session settings" here means
iTerm-style **profiles**: templates applied when creating a terminal
session/tab/pane. It does not mean saved window arrangements or WezTerm
mux workspaces.

## Split

| Kind | Purpose | Persistence |
|---|---|---|
| **Settings** | Global app preferences (UI defaults, default profile, window/chrome behavior as added). | User config file. |
| **Profile** | Session template (font, colors, shell/cwd/env/title behavior, scrollback/input options as added). | Named entries in config, resolved when spawning a `Surface`/tab. |
| **Session/arrangement state** | Open tabs, split tree, focus, cwd/process restore. | Separate, versioned state file later. Out of scope here. |

## Format

TOML, serde-typed, one file for v1. Declarative only: no Lua, no
`import`/include stacks. If multi-file composition is needed later, add a
fixed discovery rule (e.g. `themes/` or `profiles/` directory) rather
than arbitrary imports.

iTerm Dynamic Profiles (watched JSON merged by GUID, often
machine-generated) are a possible interop format later. They are not the
native format.

## Paths (v1)

Platform config directory, file name `rune-term.toml`:

- macOS: `~/Library/Application Support/rune-term/rune-term.toml`
- Linux/BSD: `${XDG_CONFIG_HOME:-~/.config}/rune-term/rune-term.toml`
- Windows: `%APPDATA%\rune-term\rune-term.toml`

Missing file: built-in defaults. Parse/load errors: keep running with
defaults and log/report the error (do not overwrite the user's file).

## Schema sketch (v1)

```toml
version = 1

[settings]
# Name of [profiles.*] used for new sessions when none is specified.
default_profile = "Default"

# Minimal v1 knobs; extend as implementation lands.
font_size = 14.0
# theme = "flexoki-dark"

[profiles.Default]
# Omitted fields inherit from built-in defaults (and, if added later,
# from an explicit base profile). Keep merge rules bounded and explicit.
font_family = "Cascadia Code NF"
font_size = 14.0
# theme = "flexoki-dark"
# shell = { program = "/bin/zsh", args = [] }
# cwd = "~"
```

Rules to keep:
- `version` is required once the file exists; unknown future versions need
  an explicit compatibility decision (reject with a clear error, or
  migrate).
- Unknown keys: warn/ignore in v1 for forward tolerance, or reject if that
  proves safer; choose one behavior and test it.
- Profile resolution: built-in defaults → selected profile fields.
  Inheritance between named profiles is out of scope for v1 unless a real
  need appears.
- Values that affect PTY/grid sizing (font size, metrics) go through the
  existing resize path (`ResizeRequest`/`Surface::resize` semantics). Do
  not add a second sizing path.

## Precedence

1. Built-in defaults.
2. Config file.
3. Runtime/UI changes (Cmd+,) update in-memory state and, when persistence
   is implemented, write the config file or a clearly defined subset of it.

Command-line overrides are a later, separate concern.

## Out of scope

- Programmable config (Lua, plugins, expressions).
- Arbitrary config imports/stacks.
- WezTerm-style workspaces/mux runtime model.
- iTerm Dynamic Profiles JSON as the primary/native format.
- Window arrangement/session restore, CWD/process restore.
- Full settings UI parity; Cmd+, remains a minimal Rune surface until a
  specific setting needs editing.

## Implementation order

1. Typed `Settings`/`Profile` structs + TOML load with defaults (no write
   yet, or atomic write only when explicitly saving).
2. Wire one real setting end-to-end: font size (Cmd+, control → profile/
   settings value → cell metrics/resize). Theme/palette selection follows
   the same path.
3. Persist on explicit save or on change, with atomic write.
4. Add named profiles/default profile selection only when creating a
   session can choose a profile.