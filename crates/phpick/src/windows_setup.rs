// Windows install step: copy phpick.exe alongside itself as php.exe and composer.exe,
// then prepend our bin dir to the user PATH via the registry (with a broadcast so
// newly-started shells pick it up without reboot).

#![cfg(windows)]

use std::io;
use std::path::{Path, PathBuf};

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

/// Copy phpick.exe as php.exe and composer.exe beside it, and prepend the dir to user PATH.
pub fn install_shims(exe_path: &Path) -> io::Result<PathBuf> {
    let dir = exe_path
        .parent()
        .ok_or_else(|| io::Error::other("phpick.exe has no parent dir"))?
        .to_path_buf();

    for name in ["php.exe", "composer.exe"] {
        let dest = dir.join(name);
        // Overwrite unconditionally so `phpick update` keeps the shims fresh.
        if dest.exists() {
            let _ = std::fs::remove_file(&dest);
        }
        std::fs::copy(exe_path, &dest)?;
    }

    prepend_user_path(&dir)?;
    broadcast_env_change();
    Ok(dir)
}

/// Read HKCU\Environment\Path, prepend `dir` if not already first, and write back.
/// Emulates `setx PATH` but without the 1024-char truncation bug in the old setx.
fn prepend_user_path(dir: &Path) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
    let current: String = env.get_value("Path").unwrap_or_default();

    let dir_str = dir.to_string_lossy().to_string();
    let already_first = current
        .split(';')
        .next()
        .map(|s| eq_path(s, &dir_str))
        .unwrap_or(false);
    if already_first {
        return Ok(());
    }

    // Remove any existing occurrence and prepend.
    let filtered: Vec<&str> = current
        .split(';')
        .filter(|seg| !seg.is_empty() && !eq_path(seg, &dir_str))
        .collect();
    let new_path = if filtered.is_empty() {
        dir_str.clone()
    } else {
        format!("{};{}", dir_str, filtered.join(";"))
    };

    env.set_value("Path", &new_path)?;
    Ok(())
}

fn eq_path(a: &str, b: &str) -> bool {
    a.trim()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(b.trim().trim_end_matches('\\'))
}

/// Tell other processes (Explorer, running shells) that the environment changed.
/// Without this, a fresh cmd.exe won't see the new PATH until the user logs out.
fn broadcast_env_change() {
    unsafe {
        let env: Vec<u16> = "Environment\0".encode_utf16().collect();
        let mut result: usize = 0;
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result as *mut _,
        );
    }
}

const HWND_BROADCAST: isize = 0xFFFF;
const WM_SETTINGCHANGE: u32 = 0x001A;
const SMTO_ABORTIFHUNG: u32 = 0x0002;

#[link(name = "user32")]
extern "system" {
    fn SendMessageTimeoutW(
        hwnd: isize,
        msg: u32,
        wparam: usize,
        lparam: isize,
        flags: u32,
        timeout: u32,
        result: *mut usize,
    ) -> isize;
}
