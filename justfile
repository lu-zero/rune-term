# Common development tasks for rune-term.

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# The workspace with default features (rune-term spawns shells), then rune-term without them.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p rune-term --all-targets --no-default-features --features graphics -- -D warnings

check:
    cargo check --workspace --all-targets
    cargo check -p rune-term --all-targets --no-default-features --features graphics

check-lib:
    cargo check -p rune-term --no-default-features --lib
    cargo check -p term-control -p rune-term-ctl

# rune-term's tests differ by feature: the shell link with `pty`, the window without it.
test-lib:
    cargo test --workspace --lib
    cargo test -p rune-term --no-default-features --lib

test-grid:
    cargo test -p rune-term --test grid_pipeline

test-headless:
    cargo test -p rune-term --no-default-features --test headless

test: test-lib

# Pre-commit gate: formatting, clippy (no warnings), and lib tests.
check-all: fmt-check clippy test-lib

run:
    cargo run -p rune-term

# Headless pictures. Settings follow the desktop appearance. The terminal picture is a 24-bit script.
shots:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    cargo build -p rune-term --no-default-features --offline
    mkdir -p "$root/docs/images"
    bin="$root/target/debug/rune-term"
    settings() {
        local home
        home="$(mktemp -d)"
        cp -R "$root/docs/shots/config/rune-term" "$home/rune-term"
        XDG_CONFIG_HOME="$home" "$bin" --rune-args \
            --window settings --size 880x560 --scale 2 \
            --script "$root/docs/shots/$2" \
            --snap "$root/docs/images/$1.png"
        rm -rf "$home"
    }
    settings settings-general settle.json
    settings settings-profiles profiles.json
    settings settings-theme-menu theme-menu.json
    settings settings-themes themes.json
    settings settings-color color.json
    # 960×560 at scale 2, Cascadia 14pt (9×17). The pane is 106×30 cells.
    home="$(mktemp -d)"
    cp -R "$root/docs/shots/term-config/rune-term" "$home/rune-term"
    python3 "$root/docs/shots/truecolor.py" 106 30 > "$home/truecolor.bin"
    XDG_CONFIG_HOME="$home" RUNE_TERM_INJECT="$home/truecolor.bin" "$bin" --rune-args \
        --size 960x560 --scale 2 \
        --script "$root/docs/shots/tabs.json" \
        --snap "$root/docs/images/terminal-tabs.png"
    rm -rf "$home"

ci: check-all

# The prompt marks as files to source, for every shell.
install-shell-integration dir: (install-shell-integration-bash dir) (install-shell-integration-zsh dir)

# rune-term.sh, for a profile directory such as /etc/profile.d.
install-shell-integration-bash dir:
    install -d "{{dir}}"
    install -m 644 assets/shell/rune-term.bash "{{dir}}/rune-term.sh"

# rune-term.zsh, to source from a zshrc.
install-shell-integration-zsh dir:
    install -d "{{dir}}"
    install -m 644 assets/shell/rune-term.zsh "{{dir}}/rune-term.zsh"

ctl *args:
    cargo run -p rune-term-ctl -- {{args}}

# rune-term-agent into ~/.local/bin, where rune-term looks for it over ssh.
agent-install:
    cargo install --force --path rune-term-agent --root "$HOME/.local"

