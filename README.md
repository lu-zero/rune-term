# rune-term

A terminal emulator. Rune owns the window, librio the VT and render state, and
the cell grid is a wgpu pass on Rune's device. macOS first; tmux and rmux
sessions are tab groups. The plan is in [`docs/todo/`](./docs/todo/).

![Tabs. The pane is a 24-bit hue plane from docs/shots/truecolor.py.](./docs/images/terminal-tabs.png)

Configuration is a directory of TOML files that Cmd+, edits:
[`docs/config.md`](./docs/config.md).

## Building

```
cargo run -p rune-term
just check-all
```

`[patch]` takes rune (and its vendored winit) from
[lu-zero/rune](https://github.com/lu-zero/rune) `physical-key-text` and librio
from [lu-zero/rio](https://github.com/lu-zero/rio) `rune-term-on-main`. On
Linux the window needs `libxkbcommon`, and the headless tests a wgpu adapter.

## Shell integration

A bash or zsh prompt keeps its layout through a resize with OSC 133 marks. In
`~/.bashrc`, after starship:

```
command -v rune-term >/dev/null && eval "$(rune-term shell-integration bash)"
```

For zsh, `shell-integration zsh` in `~/.zshrc`. `just install-shell-integration DIR`
writes the scripts as files to source instead.

## Roaming sessions

`rune-term-agent` keeps a session on an ssh host alive across a change of
network, as mosh does: [`docs/agent.md`](./docs/agent.md).

## License

MIT. Cascadia Code NF, vendored in `term-grid/`, is SIL OFL 1.1.
