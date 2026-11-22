# Reflow: our commits on top of Rio main

`lu-zero/rio` branch `for-rune-term` is Rio main at `e7c05e43` plus:

1. `1b80f604` crosswords: between OSC 133 `A` and `C`, a width change keeps
   the prompt's layout at the width the shell drew it. When the shell's
   output resumes, the input is cleared from the last prompt mark and the
   cursor goes back to its old row offset below that mark, so the shell's
   redraw lands on its own rows. A resize back to the shell's width drops
   it: the kernel sends no SIGWINCH for an unchanged size. readline needs a
   mark at the last prompt line, `P;k=s`; zle redraws from the first.
2. `d8b4aad3` performer: the PTY gets its size after resizes pause, when no
   new one has come for 100 ms (kitty's default). The grid still reflows at
   once. A drag sent a SIGWINCH per frame, and bash piled a misplaced redraw
   on the line above for each.

The rest of the old `reflow-cursor-on-2058` branch is upstream: #2058 took
our reflow cases, and #2084's writer thread makes an empty input harmless
(#2082, closed).

## Shell integration

bash and starship send no OSC 133, and without the marks 1 does nothing: a
multi-line bash prompt loses a line per drag. `rune-term shell-integration bash`
prints `assets/shell/rune-term.bash`, which adds them, and `zsh` prints
`rune-term.zsh`; `just install-shell-integration DIR` installs them as files,
and `install-shell-integration-bash` or `-zsh` one of them. They are opt-in.

bash gets `A` before the prompt, `P;k=s` before its last line, and `C` in
`PS0`. starship sets `PS1` at the end of its own `PROMPT_COMMAND`, so the hook
puts itself back last; read before starship, the first prompt has no marks.

zle redraws the whole prompt after a resize, so zsh gets only `A` at the
prompt's first line, put back on `PROMPT` in `precmd`, and `C` from `preexec`.
Without marks a zsh prompt loses a line within a few drags as bash's does.

## Upstreaming

Both can go to Rio as they are. No issue or PR has been filed for either. Ask
Raphael first: 1 depends on prompt marks the shell may not send, and 2 delays
every resize the child sees.
