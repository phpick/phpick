// Tiny HTTP fetcher that shells out to curl.exe (ships with Windows 10+ since 2018)
// or falls back to PowerShell's Invoke-WebRequest. Keeps the binary dependency-free.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Fetch `url` into memory and return the body bytes.
pub fn get(url: &str) -> io::Result<Vec<u8>> {
    if let Some(bytes) = try_curl(url)? {
        return Ok(bytes);
    }
    try_powershell(url)
}

/// Download `url` to `dest` atomically-ish (write to `dest.part`, then rename).
pub fn download(url: &str, dest: &Path) -> io::Result<()> {
    let tmp = dest.with_extension("part");
    let body = get(url)?;
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

fn try_curl(url: &str) -> io::Result<Option<Vec<u8>>> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "--retry",
            "2",
            "-A",
            concat!("phpick/", env!("CARGO_PKG_VERSION")),
            url,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(Some(o.stdout)),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(io::Error::other(format!(
                "curl failed ({}): {err}",
                o.status
            )))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn try_powershell(url: &str) -> io::Result<Vec<u8>> {
    // System.Net.WebRequest copies directly into a MemoryStream — faster than
    // Invoke-WebRequest piping bytes one at a time, and works on PS 5.1.
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         $ErrorActionPreference='Stop'; \
         $r=[System.Net.WebRequest]::Create('{}'); \
         $r.UserAgent='phpick/{}'; \
         $resp=$r.GetResponse(); \
         $s=$resp.GetResponseStream(); \
         $out=[System.IO.MemoryStream]::new(); \
         $s.CopyTo($out); \
         $stdout=[Console]::OpenStandardOutput(); \
         $bytes=$out.ToArray(); \
         $stdout.Write($bytes,0,$bytes.Length); \
         $stdout.Flush();",
        url.replace('\'', "''"),
        env!("CARGO_PKG_VERSION"),
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(io::Error::other(format!(
            "powershell fetch failed ({}): {err}",
            out.status
        )));
    }
    Ok(out.stdout)
}
