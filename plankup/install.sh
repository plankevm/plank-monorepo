#!/usr/bin/env bash
set -euo pipefail

PLANK_DIR="${PLANK_DIR:-$HOME/.plank}"
PLANK_BIN_DIR="$PLANK_DIR/bin"

REPO="plankevm/plank-monorepo"
PLANKUP_URL="https://raw.githubusercontent.com/$REPO/main/plankup/plankup"

say() { printf "plankup-install: %s\n" "$1"; }
warn() { say "warning: $1" >&2; }
err() { say "error: $1" >&2; exit 1; }

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "need '$1' (command not found)"
    fi
}

add_to_path() {
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)
            SHELL_PROFILE="$HOME/.zshenv"
            ;;
        bash)
            SHELL_PROFILE="$HOME/.bashrc"
            ;;
        fish)
            SHELL_PROFILE="$HOME/.config/fish/config.fish"
            ;;
        *)
            SHELL_PROFILE="$HOME/.profile"
            ;;
    esac

    if [ "$SHELL_NAME" = "fish" ]; then
        echo "fish_add_path $PLANK_BIN_DIR" >> "$SHELL_PROFILE"
    else
        echo "export PATH=\"\$PATH:$PLANK_BIN_DIR\"" >> "$SHELL_PROFILE"
    fi
}

main() {
    need_cmd curl
    need_cmd chmod
    need_cmd mkdir

    say "installing plankup..."

    mkdir -p "$PLANK_BIN_DIR"

    curl -sSf -L "$PLANKUP_URL" -o "$PLANK_BIN_DIR/plankup"
    chmod +x "$PLANK_BIN_DIR/plankup"

    say "plankup installed to $PLANK_BIN_DIR/plankup"

    # Add to PATH if not already there
    case ":$PATH:" in
        *":$PLANK_BIN_DIR:"*) ;;
        *)
            add_to_path
            ;;
    esac

    say ""
    say "detected your preferred shell is $SHELL"
    say "added \"$PLANK_BIN_DIR\" to PATH in your shell profile"
    say ""
    say "run 'source $SHELL_PROFILE' or open a new terminal, then run 'plankup' to install plank"
}

main
