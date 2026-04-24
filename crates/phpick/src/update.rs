// Self-update via GitHub Releases.
//
// On Windows, the current .exe is locked while running. We work around this
// with the standard "rename yourself, write new" trick:
//   1. Download the new exe to `<exe>.new`.
//   2. Rename the running `<exe>` to `<exe>.old` (allowed even while locked).
//   3. Rename `<exe>.new` to `<exe>`.
//   4. On the next run, `reap_old_exe` deletes `<exe>.old`.
//
// We also track a per-day stamp so the shim's background check only runs once
// per UTC day, matching the bash shim's behavior.

use crate::http;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const DEFAULT_REPO: &str = "phpick/phpick";

#[derive(Debug)]
pub struct UpdateError(pub String);

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpdateError {}

impl From<io::Error> for UpdateError {
    fn from(e: io::Error) -> Self {
        UpdateError(e.to_string())
    }
}

fn repo() -> String {
    env::var("PHPICK_REPO").unwrap_or_else(|_| DEFAULT_REPO.into())
}

fn release_tag() -> String {
    env::var("PHPICK_RELEASE_TAG").unwrap_or_else(|_| "latest".into())
}

fn asset_name() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "phpick-windows-x86_64.exe"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "phpick-windows-arm64.exe"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "phpick-macos-arm64"
        } else {
            "phpick-macos-x86_64"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            "phpick-linux-arm64"
        } else {
            "phpick-linux-x86_64"
        }
    } else {
        "phpick"
    }
}

fn release_api_url() -> String {
    let tag = release_tag();
    if tag == "latest" {
        format!("https://api.github.com/repos/{}/releases/latest", repo())
    } else {
        format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo(),
            tag
        )
    }
}

/// Hit the GitHub Releases API and return `(tag_name, asset_download_url)` for our platform.
fn lookup_release() -> Result<(String, String), UpdateError> {
    let body = http::get(&release_api_url()).map_err(|e| UpdateError(e.to_string()))?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| UpdateError(format!("bad release JSON: {e}")))?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| UpdateError("release JSON missing tag_name".into()))?
        .to_string();
    let want = asset_name();
    let url = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|asset| {
                let name = asset.get("name")?.as_str()?;
                if name == want {
                    asset
                        .get("browser_download_url")?
                        .as_str()
                        .map(String::from)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| UpdateError(format!("no '{want}' asset on release {tag}")))?;
    Ok((tag, url))
}

/// Return `Some(remote)` if a newer version is published, `None` if we're current.
pub fn check(current: &str) -> Result<Option<String>, UpdateError> {
    let (tag, _) = lookup_release()?;
    let remote = tag.trim_start_matches('v');
    if remote == current {
        return Ok(None);
    }
    if is_newer(remote, current) {
        Ok(Some(remote.to_string()))
    } else {
        // We're ahead of remote (pre-release dev build) — don't nag.
        Ok(None)
    }
}

/// Naive semver comparison: compares components as numbers, padding missing parts with 0.
fn is_newer(remote: &str, local: &str) -> bool {
    fn parts(v: &str) -> Vec<u32> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    }
    let r = parts(remote);
    let l = parts(local);
    for i in 0..r.len().max(l.len()) {
        let a = r.get(i).copied().unwrap_or(0);
        let b = l.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// Download the latest platform asset and install it over our own exe.
pub fn self_update(current_exe: &Path) -> Result<String, UpdateError> {
    let (tag, url) = lookup_release()?;
    let remote = tag.trim_start_matches('v').to_string();

    println!("phpick: downloading {} from {}", asset_name(), url);
    let new_exe = current_exe.with_extension("new");
    http::download(&url, &new_exe).map_err(|e| UpdateError(e.to_string()))?;

    // Swap. On Windows you can rename a running exe but can't delete/overwrite it.
    let old_exe = current_exe.with_extension("old");
    // Best-effort: clean any stale .old from a previous update.
    let _ = std::fs::remove_file(&old_exe);

    if cfg!(windows) {
        std::fs::rename(current_exe, &old_exe)
            .map_err(|e| UpdateError(format!("rename current exe: {e}")))?;
    }

    if let Err(e) = std::fs::rename(&new_exe, current_exe) {
        // Put the old one back if we can — don't leave the user with no phpick.
        if cfg!(windows) {
            let _ = std::fs::rename(&old_exe, current_exe);
        }
        return Err(UpdateError(format!("install new exe: {e}")));
    }

    // Mirror to sibling shim names (php.exe, composer.exe) if they exist.
    if cfg!(windows) {
        if let Some(dir) = current_exe.parent() {
            for name in ["php.exe", "composer.exe"] {
                let sibling = dir.join(name);
                if sibling.exists() {
                    let _ = std::fs::remove_file(&sibling);
                    let _ = std::fs::copy(current_exe, &sibling);
                }
            }
        }
    }

    Ok(remote)
}

/// Delete `<exe>.old` if it exists — runs on every startup to clean up after updates.
pub fn reap_old_exe(current_exe: &Path) -> io::Result<()> {
    let old = current_exe.with_extension("old");
    if old.exists() {
        // Might still be locked on the very next run if parent process hadn't exited; ignore failures.
        let _ = std::fs::remove_file(&old);
    }
    Ok(())
}

// --- daily background check -------------------------------------------------

fn update_stamp_path() -> PathBuf {
    if let Ok(p) = env::var("PHPICK_UPDATE_STAMP") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("phpick").join("last-check");
        }
    }
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("phpick").join("last-check");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("phpick")
            .join("last-check");
    }
    PathBuf::from(".phpick-last-check")
}

fn today_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64;
    // Days since epoch. Good enough — we just want a daily bucket.
    let days = secs.div_euclid(86_400);
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert days-since-1970-01-01 to (year, month, day). Proleptic Gregorian.
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Shift to 0000-03-01 epoch. Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn should_check() -> bool {
    if env::var("PHPICK_NO_UPDATE_CHECK").ok().as_deref() == Some("1") {
        return false;
    }
    let stamp = update_stamp_path();
    let today = today_utc();
    if let Ok(prev) = std::fs::read_to_string(&stamp) {
        if prev.trim() == today {
            return false;
        }
    }
    // Stamp first — a failing network shouldn't cause repeat checks all day.
    if let Some(parent) = stamp.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&stamp, &today).is_ok()
}

/// Spawn a detached child that prints a single-line hint to stderr if a newer
/// version is published. On Windows we re-exec ourselves with a hidden marker
/// arg so we don't need a separate helper binary.
pub fn maybe_background_check(current_exe: &Path, current_version: &str) {
    if !should_check() {
        return;
    }
    // Only notify when stderr is a terminal — skip scripts/pipes/CI.
    if !stderr_is_tty() {
        return;
    }
    // Sync path for tests / CI.
    if env::var("PHPICK_UPDATE_CHECK_SYNC").ok().as_deref() == Some("1") {
        run_check_quiet(current_version);
        return;
    }

    // Fire-and-forget: spawn ourselves with an internal marker and let it run detached.
    // The child writes to stderr (inherited) and exits; stdout/stdin are nulled so it
    // never blocks the exec'd php/composer.
    let _ = Command::new(current_exe)
        .arg("__phpick_bg_check")
        .arg(current_version)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn();
}

/// Internal entry point: called when this binary is spawned with `__phpick_bg_check`.
/// Returns true if handled so main() can exit quickly.
pub fn handle_background_arg(args: &[std::ffi::OsString]) -> bool {
    let first = args.first().and_then(|s| s.to_str());
    if first == Some("__phpick_bg_check") {
        if let Some(v) = args.get(1).and_then(|s| s.to_str()) {
            run_check_quiet(v);
        }
        return true;
    }
    false
}

fn run_check_quiet(current: &str) {
    if let Ok(Some(remote)) = check(current) {
        eprintln!("phpick: new version available: {current} -> {remote} (run `phpick update`)");
    }
}

fn stderr_is_tty() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let h = std::io::stderr().as_raw_handle();
        let mut mode = 0u32;
        // GetConsoleMode returns 0 when the handle isn't a console.
        unsafe { GetConsoleMode(h as _, &mut mode as *mut _) != 0 }
    }
    #[cfg(not(windows))]
    {
        // Good enough without the `isatty` crate: check FD 2.
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stderr().as_raw_fd();
        unsafe { libc_isatty(fd) != 0 }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
}

#[cfg(not(windows))]
extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_compare() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn epoch_date_conversion() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        assert_eq!(days_to_ymd(31), (1970, 2, 1));
        assert_eq!(days_to_ymd(365), (1971, 1, 1));
        // 2000-02-29 — leap-year edge case.
        assert_eq!(days_to_ymd(11_016), (2000, 2, 29));
        assert_eq!(days_to_ymd(19_461), (2023, 4, 14));
    }
}
