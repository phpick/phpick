// phpick — per-project PHP version routing for Windows (and Unix, as a fallback).
//
// Pin per project:
//   composer config platform.php 8.4.15
//
// One binary, dispatched by argv[0]: `phpick`, `php`, `composer`.
// https://github.com/phpick/phpick

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod discover;
mod http;
mod project;
mod update;

mod windows_setup;

const PHPICK_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REPO: &str = "phpick/phpick";

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from(&args[0]));
    let invoked_as = exe_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    // Clean up a previous in-place update if present.
    let _ = update::reap_old_exe(&exe_path);

    let rest: Vec<OsString> = args.iter().skip(1).cloned().collect();

    // Internal: background update-check child spawned by the shim path.
    if update::handle_background_arg(&rest) {
        return ExitCode::SUCCESS;
    }

    match invoked_as.as_str() {
        "phpick" => run_phpick_cli(&rest, &exe_path),
        "php" => run_shim("php", &rest, &exe_path),
        "composer" => run_shim("composer", &rest, &exe_path),
        other => {
            eprintln!(
                "phpick: invoked as unknown name {:?} (expected php, composer, or phpick)",
                other
            );
            ExitCode::from(1)
        }
    }
}

fn run_phpick_cli(args: &[OsString], exe_path: &Path) -> ExitCode {
    let first = args.first().and_then(|s| s.to_str()).unwrap_or("");
    match first {
        "" | "-h" | "--help" => {
            print_help();
            ExitCode::SUCCESS
        }
        "-v" | "--version" => {
            println!("phpick {}", PHPICK_VERSION);
            ExitCode::SUCCESS
        }
        "check-update" => match update::check(PHPICK_VERSION) {
            Ok(Some(remote)) => {
                println!(
                    "phpick {} -> {} available (run 'phpick update' to upgrade)",
                    PHPICK_VERSION, remote
                );
                ExitCode::SUCCESS
            }
            Ok(None) => {
                println!("phpick {} is up to date", PHPICK_VERSION);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("phpick: could not determine remote version: {e}");
                ExitCode::from(1)
            }
        },
        "update" | "self-update" => match update::self_update(exe_path) {
            Ok(new_version) => {
                println!("phpick: updated to {new_version}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("phpick: update failed: {e}");
                ExitCode::from(1)
            }
        },
        "which" => {
            // `phpick which [php-version]` — resolve and print the matching PHP binary.
            let ver = args.get(1).and_then(|s| s.to_str());
            let ver = match ver {
                Some(v) => v.to_string(),
                None => match resolve_pinned_version() {
                    Some(v) => v,
                    None => {
                        eprintln!("phpick: no version pinned in composer.json, and none given");
                        return ExitCode::from(1);
                    }
                },
            };
            match discover::find_php(&ver) {
                Some(p) => {
                    println!("{}", p.display());
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("phpick: no PHP {ver} found");
                    ExitCode::from(1)
                }
            }
        }
        "list" | "ls" => {
            for (ver, path) in discover::list_all() {
                println!("{:<8} {}", ver, path.display());
            }
            ExitCode::SUCCESS
        }
        "setup" => {
            #[cfg(windows)]
            {
                match windows_setup::install_shims(exe_path) {
                    Ok(dir) => {
                        println!("phpick: shims installed in {}", dir.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("phpick: setup failed: {e}");
                        ExitCode::from(1)
                    }
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!("phpick: 'setup' is Windows-only; use install.sh on Unix");
                ExitCode::from(1)
            }
        }
        other => {
            eprintln!("phpick: unknown argument '{}'", other);
            ExitCode::from(2)
        }
    }
}

fn run_shim(tool: &str, args: &[OsString], exe_path: &Path) -> ExitCode {
    // 1. Find a working real PHP (not us).
    let real_php = match discover::find_real_binary("php", exe_path) {
        Some(p) => p,
        None => {
            eprintln!("phpick: no 'php' binary found on PATH");
            return ExitCode::from(127);
        }
    };

    // 2. Walk up from CWD for composer.json; read config.platform.php.
    let mut pinned_php = real_php.clone();
    if let Some(root) = project::find_project_root() {
        let composer_json = root.join("composer.json");
        if let Some(ver) = project::read_pinned_version(&composer_json) {
            match discover::find_php(&ver) {
                Some(bin) => pinned_php = bin,
                None => eprintln!(
                    "phpick: composer.json pins php {ver} but no matching binary was found; using default php"
                ),
            }
        }
    }

    // 3. Daily background update check (fire and forget).
    update::maybe_background_check(exe_path, PHPICK_VERSION);

    // 4. Exec.
    match tool {
        "php" => exec_replacing(&pinned_php, args),
        "composer" => {
            let real_composer = match discover::find_real_binary("composer", exe_path) {
                Some(p) => p,
                None => {
                    eprintln!("phpick: no 'composer' binary found on PATH");
                    return ExitCode::from(127);
                }
            };
            // Run: <pinned_php> <composer.phar-or-exe> <args...>
            // On Windows `composer.bat` / `composer` is typically a wrapper. Prefer the .phar if we can find it.
            let composer_target = resolve_composer_target(&real_composer).unwrap_or(real_composer);
            let mut full: Vec<OsString> = Vec::with_capacity(args.len() + 1);
            full.push(composer_target.into_os_string());
            full.extend_from_slice(args);
            exec_replacing(&pinned_php, &full)
        }
        _ => unreachable!(),
    }
}

fn resolve_pinned_version() -> Option<String> {
    let root = project::find_project_root()?;
    project::read_pinned_version(&root.join("composer.json"))
}

/// If `composer` is a `.bat`/`.cmd` wrapper, try to find the sibling `composer.phar`
/// so we can invoke it directly as `php composer.phar <args>` and avoid nested shells.
fn resolve_composer_target(composer_exe: &Path) -> Option<PathBuf> {
    let parent = composer_exe.parent()?;
    for name in ["composer.phar", "composer"] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            // Don't pick up another batch wrapper.
            let ext = candidate
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if ext == "phar" || ext.is_empty() {
                return Some(candidate);
            }
        }
    }
    None
}

fn exec_replacing(program: &Path, args: &[OsString]) -> ExitCode {
    let status = Command::new(program).args(args).status();
    match status {
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => {
            eprintln!("phpick: failed to exec {}: {e}", program.display());
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "phpick {PHPICK_VERSION} — per-project PHP version routing

Usage:
  Run 'composer' or 'php' inside a project; phpick picks the PHP
  version pinned in composer.json under config.platform.php.

Pin a project:
  composer config platform.php 8.4.15

Commands:
  update             reinstall phpick from the latest GitHub release
  check-update       report whether a newer phpick is published
  which [X.Y]        print the PHP binary phpick would pick
  list               list all PHP binaries phpick can discover
  setup              (Windows) install php/composer shims next to phpick.exe

Flags:
  -v, --version      print version
  -h, --help         show this help

Env overrides:
  PHPICK_REPO                  owner/repo (default: {DEFAULT_REPO})
  PHPICK_RELEASE_TAG           release tag to install (default: latest)
  PHPICK_NO_UPDATE_CHECK=1     disable the daily background update check
  PHPICK_UPDATE_STAMP          override the stamp file path
  PHPICK_PHP_<MAJOR>_<MINOR>   full path override, e.g. PHPICK_PHP_8_4=C:\\php84\\php.exe"
    );
}

// Tiny utility re-exported for the submodules.
pub fn warn(msg: impl std::fmt::Display) {
    let _ = writeln!(io::stderr(), "phpick: {msg}");
}
