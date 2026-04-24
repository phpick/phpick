# phpick

Per-project PHP version routing for `composer` and `php` — like [Volta](https://volta.sh) but for PHP.

Pin a PHP version in your project's `composer.json`, and `composer`/`php` automatically run on that version when you're inside the project. Outside, they fall back to your system default.

## Why

You have multiple PHP versions installed (`php8.2`, `php8.4`, `php8.5`) and your projects need different ones. You don't want to `update-alternatives` every time you `cd`, and you don't want a `./bin/composer` wrapper in every repo.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/phpick/phpick/main/install.sh | bash
```

This drops a single shim at `~/.phpick/bin/phpick`, symlinks it as `composer` and `php`, and puts `~/.phpick/bin` first on your `PATH`.

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
- Windows native — not yet; use WSL

## Environment overrides for install

| Variable | Default | Purpose |
|---|---|---|
| `PHPICK_REF` | `main` | Git ref to install from |
| `PHPICK_HOME` | `$HOME/.phpick` | Install prefix |
| `PHPICK_NO_PATH` | `0` | Skip modifying shell rc files |

## Uninstall

```bash
rm -rf ~/.phpick
# then remove the '# phpick' PATH line from ~/.bashrc / ~/.zshrc
```

## License

MIT
