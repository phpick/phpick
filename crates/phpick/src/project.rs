// Walk up the filesystem looking for composer.json, then read
// config.platform.php and reduce to major.minor.

use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};

pub fn find_project_root() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;
    loop {
        if dir.join("composer.json").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn read_pinned_version(composer_json: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(composer_json).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let pinned = v
        .get("config")?
        .get("platform")?
        .get("php")?
        .as_str()?
        .to_string();
    reduce_to_major_minor(&pinned)
}

fn reduce_to_major_minor(s: &str) -> Option<String> {
    let mut it = s.split('.');
    let major: u32 = it.next()?.parse().ok()?;
    let minor: u32 = it.next()?.parse().ok()?;
    Some(format!("{major}.{minor}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_patch_to_major_minor() {
        assert_eq!(reduce_to_major_minor("8.4.15"), Some("8.4".into()));
        assert_eq!(reduce_to_major_minor("8.4"), Some("8.4".into()));
        assert_eq!(reduce_to_major_minor("8"), None);
        assert_eq!(reduce_to_major_minor(""), None);
    }
}
