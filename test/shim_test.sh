#!/usr/bin/env bash
# phpick smoke tests — plain bash, no framework. Exits nonzero on any failure.
# Run:  ./test/shim_test.sh

set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SHIM="$ROOT/bin/phpick"

[ -x "$SHIM" ] || { echo "FAIL: $SHIM not executable"; exit 1; }

# Scratch area with fake php/composer and symlinks to the shim.
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/shims" "$TMP/real" "$TMP/proj-pinned" "$TMP/proj-unpinned" "$TMP/no-project"

# Real "composer" stub — a PHP file (since the shim execs `php composer`),
# printing PHP_BINARY so we can tell which PHP invoked it.
cat > "$TMP/real/composer" <<'EOF'
#!/usr/bin/env php
<?php echo "composer-stub ran under: " . (getenv("PHPICK_TEST_STUB_PATH") ?: PHP_BINARY) . "\n";
EOF
chmod +x "$TMP/real/composer"

# Locate the real system php (absolute path) so the fake stubs can delegate
# to it without going back through PATH (which would hit our shim).
SYSTEM_PHP=$(command -v php 2>/dev/null || true)
[ -x "$SYSTEM_PHP" ] || { echo "FAIL: no real php on PATH to use as test backend"; exit 1; }

# Fake php binaries that identify themselves via -v.
make_fake_php() {
    local name="$1" label="$2"
    cat > "$TMP/real/$name" <<EOF
#!/usr/bin/env bash
# Behave enough like php for the shim:
#  - -v prints label
#  - everything else delegates to the real system php via ABSOLUTE path
#  - leak our stub name via env so child PHP can confirm which fake invoked it
if [ "\$1" = "-v" ]; then echo "$label"; exit 0; fi
PHPICK_TEST_STUB_PATH="\$0" exec "$SYSTEM_PHP" "\$@"
EOF
    chmod +x "$TMP/real/$name"
}
make_fake_php php    "sys-default"
make_fake_php php8.4 "fake-8.4"
make_fake_php php8.5 "fake-8.5"

# Shim symlinks.
ln -sf "$SHIM" "$TMP/shims/phpick"
ln -sf "$SHIM" "$TMP/shims/php"
ln -sf "$SHIM" "$TMP/shims/composer"

# PATH: shims first, then stubs. We deliberately DO NOT include the real
# system /usr/bin so fake php wins as the "default".
export PATH="$TMP/shims:$TMP/real:/usr/bin:/bin"

# Pinned project
cat > "$TMP/proj-pinned/composer.json" <<'EOF'
{
    "name": "test/pinned",
    "config": { "platform": { "php": "8.4.15" } }
}
EOF

# Unpinned project (no platform.php)
cat > "$TMP/proj-unpinned/composer.json" <<'EOF'
{ "name": "test/unpinned" }
EOF

pass=0
fail=0
check() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        printf "  ok  %s\n" "$desc"
        pass=$((pass+1))
    else
        printf "  FAIL %s\n    expected: %s\n    got:      %s\n" "$desc" "$expected" "$actual"
        fail=$((fail+1))
    fi
}

echo "--- phpick self ---"
out=$(phpick --version)
check "phpick --version" "phpick 0.1.0" "$out"

echo "--- php shim: pinned project uses php8.4 ---"
cd "$TMP/proj-pinned"
out=$(php -v 2>/dev/null)
check "php -v in pinned project" "fake-8.4" "$out"

echo "--- php shim: unpinned project uses default php ---"
cd "$TMP/proj-unpinned"
out=$(php -v 2>/dev/null)
check "php -v in unpinned project" "sys-default" "$out"

echo "--- php shim: no composer.json uses default php ---"
cd "$TMP/no-project"
out=$(php -v 2>/dev/null)
check "php -v outside any project" "sys-default" "$out"

echo "--- pin to an uninstalled version falls back with warning ---"
# 9.9 is deliberately chosen — no phpX.Y binary, no brew formula, no asdf/phpenv build.
cat > "$TMP/proj-pinned/composer.json" <<'EOF'
{ "config": { "platform": { "php": "9.9.0" } } }
EOF
cd "$TMP/proj-pinned"
out=$(php -v 2>/dev/null)
err=$(php -v 2>&1 >/dev/null)
check "php -v falls back to default"             "sys-default" "$out"
case "$err" in
    *"no matching binary"*) printf "  ok  warning printed on missing binary\n"; pass=$((pass+1)) ;;
    *)                      printf "  FAIL warning missing:\n    got: %s\n" "$err"; fail=$((fail+1)) ;;
esac

echo "--- composer shim routes to pinned php ---"
cat > "$TMP/proj-pinned/composer.json" <<'EOF'
{ "config": { "platform": { "php": "8.5.0" } } }
EOF
cd "$TMP/proj-pinned"
out=$(composer 2>/dev/null | head -1)
check "composer stub invoked under fake-8.5 path" "composer-stub ran under: $TMP/real/php8.5" "$out"

echo
printf "passed: %d   failed: %d\n" "$pass" "$fail"
[ "$fail" -eq 0 ]
