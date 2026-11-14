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

TOML, serde-typed, split across a directory. Declarative only: no Lua,
no `import`/include stacks. Discovery is fixed: `config.toml`,
`profiles/<name>.toml`, and `themes/<name>.toml`. A theme file is a
WezTerm color scheme. `settings.adaptive_theme` names a dark scheme and
a light scheme; a profile theme wins over that pair.

iTerm Dynamic Profiles (watched JSON merged by GUID, often
machine-generated) are a possible interop format later. They are not the
native format.

## Paths

The XDG config directory:

`${XDG_CONFIG_HOME:-~/.config}/rune-term/`

`config.toml` holds `version` and `[settings]`. Each profile and each
custom theme is its own file. The stem is the name.

The XDG base directories apply on every platform. `etcetera`'s XDG
strategy reads `XDG_CONFIG_HOME` and falls back to `~/.config` under the
home directory, on macOS and Windows too; there is no native macOS or
Windows config path.

Missing directory: built-in defaults. Parse/load errors: keep running with
defaults and log/report the error (do not overwrite the user's files).

## Schema sketch

`config.toml`:

```toml
version = 1

[settings]
default_profile = "Default"
font_size = 14.0
theme = "Flexoki Black"

[settings.adaptive_theme]
dark = "Flexoki Black"
light = "Flexoki Light"
```

`profiles/Default.toml`:

```toml
font_family = "Cascadia Code NF"
font_size = 14.0
# theme = "Flexoki Black"
# shell = { program = "/bin/zsh", args = [] }
# cwd = "~"
```

The field list and the theme file are in [`docs/config.md`](../config.md).

Rules to keep:
- `version` is required in `config.toml`. A newer number is still loaded;
  there is no migration step yet.
- Unknown keys are ignored.
- Profile resolution: built-in defaults → selected profile fields.
  Inheritance between named profiles is out of scope for v1 unless a real
  need appears.
- Values that affect PTY/grid sizing (font size, metrics) go through the
  existing resize path (`ResizeRequest`/`Surface::resize` semantics). Do
  not add a second sizing path.

## Precedence

1. Built-in defaults.
2. Config file.
3. The settings window writes the directory when a field is finished, on
   Save, and when the window closes. The format and the pictures are in
   [`docs/config.md`](../config.md).

Command-line overrides are a later, separate concern.

## Out of scope

- Programmable config (Lua, plugins, expressions).
- Arbitrary config imports/stacks.
- WezTerm-style workspaces/mux runtime model.
- iTerm Dynamic Profiles JSON as the primary/native format.
- Window arrangement/session restore, CWD/process restore.
- Settings beyond the profile and theme editors (environment, scrollback,
  titles, window chrome).

## Implementation order

Steps 1–4 are in the tree.

1. Typed `Settings`/`Profile` structs + TOML load with defaults (no write
   yet, or atomic write only when explicitly saving).
2. Wire one real setting end-to-end: font size (Cmd+, control → profile/
   settings value → cell metrics/resize). Theme/palette selection follows
   the same path.
3. Persist on explicit save or on change, with atomic write.
4. Add named profiles/default profile selection only when creating a
   session can choose a profile.