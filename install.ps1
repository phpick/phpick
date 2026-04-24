# phpick installer for Windows — https://github.com/phpick/phpick
#
#   irm https://raw.githubusercontent.com/phpick/phpick/main/install.ps1 | iex
#
# Env overrides:
#   $env:PHPICK_HOME            install prefix (default: %LOCALAPPDATA%\phpick)
#   $env:PHPICK_RELEASE_TAG     release tag (default: latest)
#   $env:PHPICK_REPO            owner/repo   (default: phpick/phpick)
#   $env:PHPICK_NO_PATH         '1' to skip PATH modification

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = if ($env:PHPICK_REPO) { $env:PHPICK_REPO } else { 'phpick/phpick' }
$tag  = if ($env:PHPICK_RELEASE_TAG) { $env:PHPICK_RELEASE_TAG } else { 'latest' }
$home = if ($env:PHPICK_HOME) { $env:PHPICK_HOME } else { Join-Path $env:LOCALAPPDATA 'phpick' }
$binDir = Join-Path $home 'bin'

function Say($msg)  { Write-Host "phpick: $msg" }
function Die($msg)  { Write-Error "phpick: $msg"; exit 1 }

# Pick the right asset for this machine's architecture.
$arch = $env:PROCESSOR_ARCHITECTURE
if (-not $arch) { $arch = 'AMD64' }
$asset = switch ($arch.ToUpper()) {
    'AMD64' { 'phpick-windows-x86_64.exe' }
    'ARM64' { 'phpick-windows-arm64.exe' }
    default { Die "unsupported processor architecture: $arch" }
}

# Resolve the release and download URL via the GitHub API so we always get the
# latest published asset without hard-coding a version.
$apiUrl = if ($tag -eq 'latest') {
    "https://api.github.com/repos/$repo/releases/latest"
} else {
    "https://api.github.com/repos/$repo/releases/tags/$tag"
}

Say "looking up release from $apiUrl"
try {
    $release = Invoke-RestMethod -UseBasicParsing -Uri $apiUrl -Headers @{ 'User-Agent' = 'phpick-installer' }
} catch {
    Die "could not reach GitHub Releases: $($_.Exception.Message)"
}

$downloadUrl = ($release.assets | Where-Object { $_.name -eq $asset } | Select-Object -First 1).browser_download_url
if (-not $downloadUrl) {
    Die "no '$asset' asset on release $($release.tag_name)"
}

# Prepare target directory.
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$exePath = Join-Path $binDir 'phpick.exe'

# If phpick.exe is currently running, we can't overwrite it — rename it out of the way first.
if (Test-Path $exePath) {
    $old = "$exePath.old"
    if (Test-Path $old) { Remove-Item -Force $old -ErrorAction SilentlyContinue }
    try { Rename-Item -Path $exePath -NewName 'phpick.exe.old' -ErrorAction Stop } catch {
        Say "note: could not rename old phpick.exe ($($_.Exception.Message)) — continuing"
    }
}

Say "downloading $asset -> $exePath"
Invoke-WebRequest -UseBasicParsing -Uri $downloadUrl -OutFile $exePath

# Mirror as php.exe / composer.exe so they're on PATH next to phpick.
foreach ($name in 'php.exe', 'composer.exe') {
    $dest = Join-Path $binDir $name
    if (Test-Path $dest) { Remove-Item -Force $dest -ErrorAction SilentlyContinue }
    Copy-Item -Path $exePath -Destination $dest -Force
}
Say "installed shims: phpick, php, composer -> $binDir"

# Prepend to user PATH unless the user opted out.
if ($env:PHPICK_NO_PATH -ne '1') {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { $userPath = '' }

    $parts = $userPath -split ';' | Where-Object { $_ -and ($_ -ne $binDir) }
    $newPath = (@($binDir) + $parts) -join ';'

    if ($userPath -ne $newPath) {
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Say "prepended $binDir to user PATH"
    } else {
        Say "$binDir already at front of user PATH"
    }

    # Update this session too — so the user can run `phpick` immediately.
    if (-not ($env:Path -split ';' -contains $binDir)) {
        $env:Path = "$binDir;$env:Path"
    }
}

Say "installed phpick $($release.tag_name). run:  phpick --version"

Write-Host ""
Write-Host "Pin a project's PHP version:"
Write-Host "  cd C:\path\to\project"
Write-Host "  composer config platform.php 8.4.15"
Write-Host ""
Write-Host "Verify:"
Write-Host "  php -v        # inside project -> 8.4"
Write-Host "  composer -V   # runs on 8.4"
Write-Host ""
Write-Host "Uninstall:"
Write-Host "  Remove-Item -Recurse -Force '$home'"
Write-Host "  # then remove '$binDir' from your user PATH"
