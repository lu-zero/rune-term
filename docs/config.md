# rune-term configuration (TOML)

`rune-term` can be configured by editing its TOML config file directly. This is the primary/manual configuration path; the in-app settings UI (when available) edits the same file.

## Location

- **Path:** `$XDG_CONFIG_HOME/rune-term/rune-term.toml` (or the platform XDG config directory as resolved by `etcetera`)
- **Fallback:** `./rune-term/config/rune-term.toml` (relative to the current working directory) if an XDG config directory is unavailable

## Behavior and safety

- If the config file does not exist, `rune-term` starts with defaults.
- If the config file exists but cannot be read or parsed, `rune-term` starts with defaults and **refuses to save** from the app, to avoid overwriting an unreadable/broken file.
- Font sizes are clamped to a supported range (currently `6.0..=96.0`).
- `settings.default_profile` falls back to `Default` (or the first available profile) if it does not exist.

## Schema (v1)

```toml
version = 1

[settings]
default_profile = "Default"
font_size = 14.0
theme = "Flexoki Black"

[profiles.Default]
font_family = "Cascadia Code NF"
font_size = 14.0
# theme = "Flexoki Black"      # optional; inherits settings.theme when omitted
backend = "local"               # currently only "local" is accepted
# cwd = "/path/to/dir"         # optional working directory

# [profiles.Default.shell]     # optional; platform default shell when omitted
# program = "/bin/zsh"
# args = ["-l"]

[custom_themes."My Theme"]
author = ""
url = ""
created_at = 0                  # unix seconds (informational)
bg = 0x0010_0F0F                # 0x00RRGGBB
fg = 0x00CE_CDC3
fg_bright = 0x00FF_FCF0
muted = 0x0087_8580
tab_bg = 0x001C_1B1A
active_border = 0x0028_2826
accent = 0x00DA_702C
ansi16 = [
  [16, 15, 15], [209, 77, 65], [135, 154, 57], [208, 162, 21],
  [67, 133, 190], [206, 93, 151], [58, 169, 159], [135, 133, 128],
  [111, 110, 105], [209, 77, 65], [135, 154, 57], [208, 162, 21],
  [67, 133, 190], [206, 93, 151], [58, 169, 159], [206, 205, 195],
]
```

## Sections and fields

### `[settings]`
Global defaults and app-level selections.

- `default_profile` (string): name of the profile used by default.
- `font_size` (float): global/default font size (points).
- `theme` (string): default theme name (built-in or custom).

### `[profiles.<name>]`
Named launch/runtime profiles. Profiles can override global settings.

- `font_family` (string): font family name. Default: `"Cascadia Code NF"`.
- `font_size` (float, optional): overrides `[settings].font_size` for this profile.
- `theme` (string, optional): overrides `[settings].theme` for this profile.
- `backend` (string): session backend. Default: `"local"`. **Currently only `"local"` is available**; unknown values are rejected by validation.
- `cwd` (string, optional): working directory for new sessions/tabs/panes created with this profile.
- `[profiles.<name>.shell]` (table, optional): shell to launch.
  - `program` (string, optional): executable path/name. If omitted/empty and `args` is empty, the platform default shell is used.
  - `args` (array of strings): arguments passed to `program`.

> Note: profile `font_family` is stored in config and selectable in settings, but terminal font/atlas rendering may not yet apply it in all cases (font size is applied live). See `docs/todo/status.md` for current status.

### `[custom_themes.<name>]`
User-defined themes. A custom theme name can be referenced from `[settings].theme` or `[profiles.<name>].theme`.

- `author` (string), `url` (string): metadata.
- `created_at` (integer): creation timestamp (unix seconds).
- `bg`, `fg`, `fg_bright`, `muted`, `tab_bg`, `active_border`, `accent` (integer): colors as `0x00RRGGBB`.
- `ansi16` (array of 16 `[r, g, b]` entries): ANSI palette colors (`0..255` per channel).

## Built-in themes

Built-in theme names currently include:

- `Flexoki Black` (default)
- `Flexoki Dark`
- `Flexoki Light`
- `Dracula`
- `Nord`
- `Solarized Dark`
- `Solarized Light`
- `Gruvbox Dark`
- `Catppuccin Mocha`

Use these exact names in `settings.theme` or a profile’s `theme`. Custom themes with the same name are treated as custom entries in config storage, but prefer unique names.

## Examples

### Minimal config
```toml
version = 1

[settings]
default_profile = "Default"
font_size = 14.0
theme = "Flexoki Black"

[profiles.Default]
font_family = "Cascadia Code NF"
```

### Multiple profiles (manual editing)
```toml
version = 1

[settings]
default_profile = "Work"
font_size = 14.0
theme = "Flexoki Black"

[profiles.Default]
font_family = "Cascadia Code NF"
font_size = 14.0

[profiles.Work]
font_family = "Cascadia Code NF"
font_size = 16.0
theme = "Nord"
backend = "local"
cwd = "/Users/me/work"

[profiles.Work.shell]
program = "/bin/zsh"
args = ["-l"]
```

## Editing tips

- Keep `version = 1`.
- Use valid TOML; if parsing fails, the app will fall back to defaults and will not overwrite the file on save.
- Prefer editing while the app is not writing settings, or restart after manual edits to ensure the running instance reflects changes.
- Profile/theme names are referenced by string; renaming a profile/theme requires updating references (`settings.default_profile`, profile `theme` fields).