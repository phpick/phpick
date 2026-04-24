// PHP binary discovery across installers, on Windows and Unix.

use std::env;
use std::path::{Path, PathBuf};

/// Find any executable named `name` on PATH, skipping entries that resolve to `self_exe`.
/// Handles Windows PATHEXT (.exe/.bat/.cmd) automatically.
pub fn find_real_binary(name: &str, self_exe: &Path) -> Option<PathBuf> {
    let self_canon = canonicalize(self_exe);
    let path = env::var_os("PATH")?;
    let pathext: Vec<String> = if cfg!(windows) {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };

    for dir in env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let base = dir.join(name);
        let candidates: Vec<PathBuf> = if cfg!(windows) {
            let has_ext = Path::new(name).extension().is_some();
            if has_ext {
                vec![base.clone()]
            } else {
                pathext
                    .iter()
                    .map(|ext| {
                        let mut p = base.clone().into_os_string();
                        p.push(ext);
                        PathBuf::from(p)
                    })
                    .collect()
            }
        } else {
            vec![base]
        };

        for c in candidates {
            if !c.is_file() {
                continue;
            }
            if canonicalize(&c) == self_canon {
                continue;
            }
            return Some(c);
        }
    }
    None
}

/// Find a PHP binary matching major.minor (e.g. "8.4").
pub fn find_php(version: &str) -> Option<PathBuf> {
    let (major, minor) = split_version(version)?;

    // 1. Env override wins: PHPICK_PHP_8_4=C:\path\to\php.exe
    let env_key = format!("PHPICK_PHP_{major}_{minor}");
    if let Ok(val) = env::var(&env_key) {
        let p = PathBuf::from(val);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Named binaries on PATH: php8.4, php84
    let self_exe = env::current_exe().ok();
    let self_ref: &Path = self_exe.as_deref().unwrap_or_else(|| Path::new(""));
    for name in [format!("php{major}.{minor}"), format!("php{major}{minor}")] {
        if let Some(p) = find_real_binary(&name, self_ref) {
            return Some(p);
        }
    }

    // 3. Platform-specific scans.
    if cfg!(windows) {
        if let Some(p) = scan_windows(major, minor) {
            return Some(p);
        }
    } else if let Some(p) = scan_unix(major, minor) {
        return Some(p);
    }

    None
}

/// Return `(major, minor)` as u32 from a version string like "8.4" or "8.4.15".
fn split_version(v: &str) -> Option<(u32, u32)> {
    let mut it = v.split('.').map(|s| s.parse::<u32>().ok());
    let major = it.next().flatten()?;
    let minor = it.next().flatten()?;
    Some((major, minor))
}

/// Walk installed PHP roots on Windows for a `php-{X.Y}.*` directory containing php.exe.
fn scan_windows(major: u32, minor: u32) -> Option<PathBuf> {
    let prefix = format!("php-{major}.{minor}.");
    let prefix_no_dash = format!("{major}.{minor}.");

    for root in laragon_php_roots() {
        if let Some(p) = scan_dir_prefix(&root, &[&prefix, &prefix_no_dash]) {
            return Some(p);
        }
    }

    // Scoop: %USERPROFILE%\scoop\apps\php{major}{minor}\current\php.exe
    if let Ok(home) = env::var("USERPROFILE") {
        let home = PathBuf::from(home);
        let scoop_apps = home.join("scoop").join("apps");
        for name in [
            format!("php{major}{minor}"),
            format!("php{major}.{minor}"),
            "php".to_string(),
        ] {
            let p = scoop_apps.join(&name).join("current").join("php.exe");
            if p.is_file() && php_matches(&p, major, minor) {
                return Some(p);
            }
        }
        // Also scan scoop persisted/current of generic `php` app to match major.minor.
        let generic = scoop_apps.join("php");
        if generic.is_dir() {
            if let Some(p) = scan_dir_prefix(&generic, &[&prefix_no_dash]) {
                let bin = p.join("php.exe");
                if bin.is_file() {
                    return Some(bin);
                }
            }
        }
    }

    // Chocolatey: C:\tools\php{XY}\php.exe or C:\ProgramData\chocolatey\lib\php{XY}\tools\php.exe
    let choco_candidates = [
        PathBuf::from(format!(r"C:\tools\php{major}{minor}\php.exe")),
        PathBuf::from(format!(r"C:\tools\php{major}.{minor}\php.exe")),
        PathBuf::from(format!(
            r"C:\ProgramData\chocolatey\lib\php{major}{minor}\tools\php.exe"
        )),
    ];
    for c in choco_candidates {
        if c.is_file() && php_matches(&c, major, minor) {
            return Some(c);
        }
    }

    // Laravel Herd for Windows: %LOCALAPPDATA%\Herd\bin\php{X.Y}\php.exe
    if let Ok(local) = env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(&local)
            .join("Herd")
            .join("bin")
            .join(format!("php{major}.{minor}"))
            .join("php.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Generic: C:\Program Files\PHP\X.Y\php.exe
    let program_files = env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let pf_candidate = PathBuf::from(&program_files)
        .join("PHP")
        .join(format!("{major}.{minor}"))
        .join("php.exe");
    if pf_candidate.is_file() {
        return Some(pf_candidate);
    }

    None
}

/// Laragon lives at `C:\laragon` or `D:\laragon` by default, but users move it.
fn laragon_php_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(r) = env::var("LARAGON_ROOT") {
        roots.push(PathBuf::from(r).join("bin").join("php"));
    }
    for drive in ['C', 'D', 'E', 'F'] {
        let p = PathBuf::from(format!(r"{drive}:\laragon\bin\php"));
        if p.is_dir() {
            roots.push(p);
        }
    }
    roots
}

/// In `dir`, find the subdirectory whose name starts with any of `prefixes`
/// and contains `php.exe` (Windows) or `bin/php` (Unix). Returns the
/// highest-version match, comparing patch/build numbers numerically so that
/// `8.4.14` > `8.4.9`.
fn scan_dir_prefix(dir: &Path, prefixes: &[&str]) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    let mut matches: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if !prefixes.iter().any(|p| name_str.starts_with(p)) {
            continue;
        }
        let sub = entry.path();
        let bin = if cfg!(windows) {
            sub.join("php.exe")
        } else {
            sub.join("bin").join("php")
        };
        if !bin.is_file() {
            continue;
        }
        matches.push((numeric_key(&name_str), sub));
    }
    // Highest version first.
    matches.sort_by(|a, b| b.0.cmp(&a.0));
    matches.into_iter().next().map(|(_, p)| {
        if cfg!(windows) {
            p.join("php.exe")
        } else {
            p.join("bin").join("php")
        }
    })
}

/// Extract numeric components from a string for version-aware ordering.
/// `"php-8.4.14-nts-Win32-vs17-x64"` → `[8, 4, 14, 32, 17, 64]` — good enough,
/// because for a given prefix `"php-8.4."` only the leading components differ
/// in ways that matter (patch number dominates).
fn numeric_key(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Cheap sanity check: run `php -r 'echo PHP_MAJOR_VERSION.".".PHP_MINOR_VERSION;'`
/// and see that it matches what we want. Silently passes on error (better to try and fail
/// at exec time than to skip a working binary here).
fn php_matches(php: &Path, major: u32, minor: u32) -> bool {
    let out = std::process::Command::new(php)
        .args(["-r", "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.trim() == format!("{major}.{minor}")
        }
        _ => true, // don't reject on transient failure
    }
}

fn scan_unix(major: u32, minor: u32) -> Option<PathBuf> {
    // Homebrew prefix
    if let Ok(out) = std::process::Command::new("brew")
        .args(["--prefix", &format!("php@{major}.{minor}")])
        .output()
    {
        if out.status.success() {
            let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !prefix.is_empty() {
                let p = PathBuf::from(prefix).join("bin").join("php");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    let home = env::var("HOME").ok()?;
    let home = PathBuf::from(home);

    // asdf
    let asdf = home.join(".asdf").join("installs").join("php");
    if asdf.is_dir() {
        if let Some(p) = scan_dir_prefix(&asdf, &[&format!("{major}.{minor}.")]) {
            return Some(p);
        }
    }

    // phpenv
    let phpenv = home.join(".phpenv").join("versions");
    if phpenv.is_dir() {
        if let Some(p) = scan_dir_prefix(&phpenv, &[&format!("{major}.{minor}.")]) {
            return Some(p);
        }
    }

    // Laravel Herd (macOS)
    let herd = home
        .join("Library")
        .join("Application Support")
        .join("Herd")
        .join("bin")
        .join(format!("php{major}.{minor}"));
    if herd.is_file() {
        return Some(herd);
    }

    None
}

/// Enumerate all PHPs phpick can see, as (major.minor, path).
pub fn list_all() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for major in 7..=9 {
        for minor in 0..=9 {
            if let Some(p) = find_php(&format!("{major}.{minor}")) {
                out.push((format!("{major}.{minor}"), p));
            }
        }
    }
    out
}

fn canonicalize(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_key_sorts_versions() {
        // 8.4.14 must rank higher than 8.4.9 — the bug that prompted this helper.
        let a = numeric_key("php-8.4.14-nts-Win32-vs17-x64");
        let b = numeric_key("php-8.4.9-nts-Win32-vs17-x64");
        assert!(a > b, "expected {a:?} > {b:?}");
    }

    #[test]
    fn split_version_handles_patch() {
        assert_eq!(split_version("8.4"), Some((8, 4)));
        assert_eq!(split_version("8.4.15"), Some((8, 4)));
        assert_eq!(split_version("not a version"), None);
    }
}
