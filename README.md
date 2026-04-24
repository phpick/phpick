# phpick

Per-project PHP version routing for `composer` and `php` — like [Volta](https://volta.sh) but for PHP.

Pin a PHP version in your project's `composer.json`, and `composer`/`php` automatically run on that version when you're inside the project. Outside, they fall back to your system default.

## Why

You have multiple PHP versions installed (`php8.2`, `php8.4`, `php8.5`) and your projects need different ones. You don't want to `update-alternatives` every time you `cd`, and you don't want a `./bin/composer` wrapper in every repo.

## Install

**macOS / Linux / WSL** — bash shim:

```bash
curl -fsSL https://raw.githubusercontent.com/phpick/phpick/main/install.sh | bash
```

This drops a single shim at `~/.phpick/bin/phpick`, symlinks it as `composer` and `php`, and puts `~/.phpick/bin` first on your `PATH`.

**Windows** — native Rust binary (~300 KB):

```powershell
irm https://raw.githubusercontent.com/phpick/phpick/main/install.ps1 | iex
```

Downloads `phpick.exe` to `%LOCALAPPDATA%\phpick\bin`, copies it as `php.exe` and `composer.exe`, and prepends the directory to your user `PATH`.

Open a new shell (or `source ~/.bashrc`) and you're done.

## Pin a project

Use composer's built-in config:

```bash
composer config platform.php 8.4.15
```

This writes to your `composer.json`:

```json
{
  "config": {
    "platform": {
      "php": "8.4.15"
    }
  }
}
```

phpick honors the major.minor (`8.4`) and routes to a matching installed PHP binary.

Bonus: composer already uses `config.platform.php` for dependency resolution, so pinning here keeps both runtime and lockfile in sync.

## How it works

When you run `composer` or `php`, the shim:

1. Walks up from `$PWD` looking for the nearest `composer.json`.
2. Reads `config.platform.php` (via `php -r` — no `jq` needed).
3. Resolves the matching PHP binary in this order:
   - `phpX.Y` on `PATH` (Debian/Ubuntu)
   - `phpXY` on `PATH` (some RHEL-family distros)
   - Homebrew: `brew --prefix php@X.Y`
   - asdf: `~/.asdf/installs/php/X.Y.*/bin/php`
   - phpenv: `~/.phpenv/versions/X.Y.*/bin/php`
   - Laravel Herd: `~/Library/Application Support/Herd/bin/phpX.Y`
4. Execs the real `composer` or `php` under that PHP.

No pin, no `composer.json`, or the pinned version isn't installed → falls back to your system default with a warning in the last case.

## Supported platforms

- Linux (Debian/Ubuntu, RHEL-family, Arch) ✓
- macOS (system PHP, Homebrew `php@X.Y`, Herd, asdf, phpenv) ✓
- WSL ✓
- Windows native (Laragon, Scoop, Chocolatey, Herd for Windows, `C:\Program Files\PHP\X.Y`) ✓

## Updating

```bash
phpick check-update    # is a newer version published?
phpick update          # reinstall in place
```

`update` re-runs the installer from `$PHPICK_REPO@$PHPICK_REF` (defaults: `phpick/phpick@main`), overwriting the shim at `$PHPICK_HOME/bin/phpick`. Pin a specific release with `PHPICK_REF=v0.2.0 phpick update`.

The shim also runs a quiet background check once per UTC day when you invoke `php` or `composer` interactively, and prints a single-line hint to stderr if a newer version is published. The check runs detached from the foreground command so it never adds latency, and it's skipped when stderr isn't a TTY (scripts, pipes, CI). Disable with `PHPICK_NO_UPDATE_CHECK=1`.

## Environment overrides

Install (`install.sh`):

| Variable | Default | Purpose |
|---|---|---|
| `PHPICK_REF` | `main` | Git ref to install from |
| `PHPICK_HOME` | `$HOME/.phpick` | Install prefix |
| `PHPICK_NO_PATH` | `0` | Skip modifying shell rc files |

Update / version check (`phpick update`, `phpick check-update`, daily shim check):

| Variable | Default | Purpose |
|---|---|---|
| `PHPICK_REF` | `main` | Git ref to fetch from |
| `PHPICK_REPO` | `phpick/phpick` | `owner/repo` on GitHub |
| `PHPICK_INSTALL_URL` | derived | Full URL to `install.sh` (overrides `REF`/`REPO`) |
| `PHPICK_REMOTE_VERSION_URL` | derived | Full URL to `bin/phpick` used for the version check |
| `PHPICK_NO_UPDATE_CHECK` | `0` | Set to `1` to disable the daily background update check |
| `PHPICK_UPDATE_STAMP` | `${XDG_CACHE_HOME:-~/.cache}/phpick/last-check` | Rate-limit stamp file |

## Uninstall

**Unix:**

```bash
rm -rf ~/.phpick
# then remove the '# phpick' PATH line from ~/.bashrc / ~/.zshrc
```

**Windows:**

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\phpick"
# then remove "$env:LOCALAPPDATA\phpick\bin" from your user PATH
```

## Building from source

```bash
cargo build --release --locked
# -> target/release/phpick (or phpick.exe on Windows)
```

To install into your existing shim directory on Windows, drop the resulting `phpick.exe` into `%LOCALAPPDATA%\phpick\bin\` and run `phpick setup` to refresh the `php.exe` / `composer.exe` copies.

## License

MIT
