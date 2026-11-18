# Shell integration

A multi-line bash prompt keeps its layout through a resize only with OSC 133
marks, which tell the terminal where the prompt starts and ends.

## Evaluated at start

In `~/.bashrc`, after `eval "$(starship init bash)"` if you use starship:

```
command -v rune-term >/dev/null && eval "$(rune-term shell-integration bash)"
```

For zsh, `eval "$(rune-term shell-integration zsh)"` in `~/.zshrc`.

## As files

To source a file rather than run `rune-term` at every shell start:

```
just install-shell-integration /etc/profile.d   # or any directory a profile sources
```

That writes `rune-term.sh` for bash and `rune-term.zsh` to source from a zshrc.
`install-shell-integration-bash` and `-zsh` write one.

## Pitfalls

- Starship sets its own prompt. Read after it, the marks wrap that prompt from
  the first one on. Read before it, as a file in `/etc/profile.d` is, the hook
  moves itself after starship's, so only bash's first prompt goes without marks.
- The marks need bash 4.4 or later (`PS0`); an older bash gets none.
