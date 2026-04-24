#!/usr/bin/env bash
# phpick installer — https://github.com/phpick/phpick
#
#   curl -fsSL https://raw.githubusercontent.com/phpick/phpick/main/install.sh | bash
#
# Env overrides:
#   PHPICK_REF=main           git ref to download from
#   PHPICK_HOME=~/.phpick     install prefix
#   PHPICK_NO_PATH=1          do not modify shell rc files

set -eu

: "${PHPICK_REF:=main}"
: "${PHPICK_HOME:=$HOME/.phpick}"
: "${PHPICK_NO_PATH:=0}"

REPO_RAW="https://raw.githubusercontent.com/phpick/phpick/${PHPICK_REF}"
BIN_DIR="$PHPICK_HOME/bin"
SHIM_PATH="$BIN_DIR/phpick"

say()  { printf "phpick: %s\n" "$*"; }
die()  { printf "phpick: %s\n" "$*" >&2; exit 1; }

fetch() {
    local url="$1" dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        die "need curl or wget to download phpick"
    fi
}

# Check for a working php (needed by the shim at runtime).
command -v php >/dev/null 2>&1 || die "no 'php' binary on PATH — install PHP first"

# Check for composer (not strictly required for php shim, but the main use case).
command -v composer >/dev/null 2>&1 || say "warning: 'composer' not found on PATH (install it if you want the composer shim)"

mkdir -p "$BIN_DIR"

say "downloading shim to $SHIM_PATH"
fetch "$REPO_RAW/bin/phpick" "$SHIM_PATH"
chmod +x "$SHIM_PATH"

# Create/refresh symlinks for composer and php next to phpick.
for name in composer php; do
    ln -sfn "$SHIM_PATH" "$BIN_DIR/$name"
done
say "installed shims: composer, php, phpick → $BIN_DIR"

# PATH wiring.
add_path_line() {
    local rc="$1" line="$2"
    [ -f "$rc" ] || return 0
    grep -Fq "$line" "$rc" && return 0
    printf '\n# phpick\n%s\n' "$line" >> "$rc"
    say "added phpick to PATH in $rc"
}

if [ "$PHPICK_NO_PATH" != "1" ]; then
    bash_line='export PATH="$HOME/.phpick/bin:$PATH"'
    fish_line='set -gx PATH $HOME/.phpick/bin $PATH'
    add_path_line "$HOME/.bashrc"       "$bash_line"
    add_path_line "$HOME/.zshrc"        "$bash_line"
    add_path_line "$HOME/.bash_profile" "$bash_line"
    [ -f "$HOME/.config/fish/config.fish" ] && add_path_line "$HOME/.config/fish/config.fish" "$fish_line"
fi

# Sanity: warn if BIN_DIR isn't on current PATH (user needs to reload shell).
case ":$PATH:" in
    *":$BIN_DIR:"*) : ;;
    *) say "open a new shell or run:  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

cat <<EOF

phpick installed.

Pin a project's PHP version:
  cd /path/to/project
  composer config platform.php 8.4.15

Verify:
  php -v       # inside project -> 8.4
  composer -V  # runs on 8.4

Uninstall:
  rm -rf "$PHPICK_HOME"
  # and remove the '# phpick' PATH line from your shell rc
EOF
