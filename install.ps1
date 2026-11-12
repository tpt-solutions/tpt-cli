# Installs tptcli from a GitHub Release, after checking its SHA-256 checksum.
#
#   irm https://raw.githubusercontent.com/TPT-Solutions/tpt-cli/master/install.ps1 | iex
#
# Parameters (or set the environment variables of the same name in upper case):
#   -Version      tag to install, such as v0.1.0 (default: the latest release)
#   -InstallDir   where to put tptcli.exe (default: %LOCALAPPDATA%\tptcli\bin)

[CmdletBinding()]
param(
    [string]$Version = $env:TPTCLI_VERSION,
    [string]$InstallDir = $(if ($env:TPTCLI_INSTALL_DIR) { $env:TPTCLI_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'tptcli\bin' })
)

$ErrorActionPreference = 'Stop'
$repo = 'TPT-Solutions/tpt-cli'

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE. Only x86_64 Windows has a prebuilt binary."
}
$target = 'x86_64-pc-windows-msvc'

if (-not $Version) {
    $Version = (Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest").tag_name
}
if ($Version -notmatch '^v\d') {
    throw "Could not determine the release tag (got '$Version')."
}
$number = $Version.Substring(1)
$asset = "tptcli-v$number-$target.zip"
$base = "https://github.com/$repo/releases/download/$Version"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tptcli-install-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading tptcli $Version for $target"
    Invoke-WebRequest -Uri "$base/$asset" -OutFile (Join-Path $tmp $asset)
    Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')

    # Verify the checksum before extracting anything.
    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -match [regex]::Escape($asset) + '$' } | Select-Object -First 1
    if (-not $line) { throw "$asset is not listed in SHA256SUMS." }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp $asset)).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { throw "Checksum mismatch for $asset (expected $expected, got $actual)." }

    Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath $tmp
    $exe = Get-ChildItem -Path $tmp -Recurse -Filter 'tptcli.exe' | Select-Object -First 1
    if (-not $exe) { throw 'tptcli.exe was not found in the archive.' }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $exe.FullName -Destination (Join-Path $InstallDir 'tptcli.exe') -Force
    Write-Host "Installed $(Join-Path $InstallDir 'tptcli.exe')"

    # Add the install directory to the user PATH, if it is not there yet.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('Path', (@($userPath, $InstallDir) | Where-Object { $_ }) -join ';', 'User')
        Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use it."
    }
    & (Join-Path $InstallDir 'tptcli.exe') --version
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
