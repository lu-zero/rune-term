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
    cargo test -p rune-term --no-default-features --features font-bundled --test headless

test: test-lib

# Pre-commit gate: formatting, clippy (no warnings), and lib tests.
check-all: fmt-check clippy test-lib

run:
    cargo build -p rune-term-keeper
    cargo run -p rune-term

# Release build, keeper included: the app hands its local shells to the keeper at exit.
run-release:
    cargo build --release -p rune-term-keeper
    cargo run --release -p rune-term

# Headless pictures of the settings and the terminal features. Settings have light and dark captures; the terminal shots use the dark theme from term-config.
shots:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    cargo build -p rune-term --no-default-features --offline
    mkdir -p "$root/docs/images"
    bin="$root/target/debug/rune-term"
    settings() {
        local name="$1" script="$2" mode="$3" home config=config
        if [ "$mode" = dark ]; then config=config-dark; fi
        home="$(mktemp -d)"
        cp -R "$root/docs/shots/$config/rune-term" "$home/rune-term"
        XDG_CONFIG_HOME="$home" "$bin" --rune-args \
            --theme "$mode" \
            --window settings --size 880x560 --scale 2 \
            --script "$root/docs/shots/$script" \
            --snap "$root/docs/images/$name.png"
        rm -rf "$home"
    }
    settings settings-general settle.json light
    settings settings-profiles profiles.json light
    settings settings-theme-menu theme-menu.json light
    settings settings-themes themes.json light
    settings settings-color color.json light
    settings settings-general-dark settle.json dark
    settings settings-profiles-dark profiles.json dark
    settings settings-theme-menu-dark theme-menu.json dark
    settings settings-themes-dark themes.json dark
    settings settings-color-dark color.json dark
    # 960×560 at scale 2, Maple Mono NF 14pt. The pane is 106×30 cells.
    term() {
        local name="$1" source="$2" script="$3" home
        home="$(mktemp -d)"
        cp -R "$root/docs/shots/term-config/rune-term" "$home/rune-term"
        if [ "$source" = truecolor ]; then
            python3 "$root/docs/shots/truecolor.py" 106 30 > "$home/inject.bin"
        else
            python3 "$root/docs/shots/features.py" "$source" 30 > "$home/inject.bin"
        fi
        XDG_CONFIG_HOME="$home" RUNE_TERM_INJECT="$home/inject.bin" "$bin" --rune-args \
            --size 960x560 --scale 2 \
            --script "$root/docs/shots/$script" \
            --snap "$root/docs/images/$name.png"
        rm -rf "$home"
    }
    term terminal-tabs truecolor tabs.json
    term terminal-split truecolor split.json
    term terminal-search truecolor search.json
    term terminal-selection text selection.json
    term terminal-link link link.json
    term terminal-scrollback scroll scrollback.json

# The mdBook site, from README.md and docs/ staged into target/site-src by site/stage.py.
site:
    python3 site/stage.py
    mdbook build site

# Preview the staged copy on port 3000; edits under docs/ need a re-run.
site-serve host="localhost":
    python3 site/stage.py
    mdbook serve site --hostname {{host}}

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

