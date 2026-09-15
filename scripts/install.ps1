# AgentTx installer for Windows (PowerShell 5.1 or later).
#
#   irm https://agent-tx-protocol.vercel.app/install.ps1 | iex
#
# Downloads the latest release from GitHub, checks its SHA-256 checksum,
# installs agenttx.exe into %USERPROFILE%\.agenttx\bin and adds it to your PATH.

$ErrorActionPreference = "Stop"

$Repo = "harshal050/agent-tx-protocol"
$Guide = "https://agent-tx-protocol.vercel.app/docs/connect-ai-agents"
$InstallDir = if ($env:AGENTTX_INSTALL_DIR) { $env:AGENTTX_INSTALL_DIR } else { Join-Path $env:USERPROFILE ".agenttx\bin" }
$Version = if ($env:AGENTTX_VERSION) { $env:AGENTTX_VERSION } else { "latest" }

if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    throw "There is no ready-made AgentTx download for this Windows computer yet. Build from source instead: $Guide#option-b-build-from-source"
}

$Base = if ($Version -eq "latest") { "https://github.com/$Repo/releases/latest/download" } else { "https://github.com/$Repo/releases/download/$Version" }
$Target = "x86_64-pc-windows-msvc"
$File = "agenttx-$Target.zip"
$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("agenttx-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp | Out-Null

try {
    Write-Host "Downloading $File ($Version)..."
    $ZipPath = Join-Path $Tmp $File
    try {
        Invoke-WebRequest -Uri "$Base/$File" -OutFile $ZipPath -UseBasicParsing
    } catch {
        throw "Download failed. Check that a release exists at https://github.com/$Repo/releases - or build from source: $Guide#option-b-build-from-source"
    }

    $ChecksumPath = Join-Path $Tmp "$File.sha256"
    $HasChecksum = $true
    try {
        Invoke-WebRequest -Uri "$Base/$File.sha256" -OutFile $ChecksumPath -UseBasicParsing
    } catch {
        $HasChecksum = $false
    }
    if ($HasChecksum) {
        $Expected = ((Get-Content $ChecksumPath -Raw).Trim() -split '\s+')[0].ToLower()
        $Actual = (Get-FileHash $ZipPath -Algorithm SHA256).Hash.ToLower()
        if ($Expected -ne $Actual) { throw "Checksum mismatch - the download may be corrupted. Nothing was installed." }
        Write-Host "Checksum verified."
    }

    Expand-Archive -Path $ZipPath -DestinationPath $Tmp -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item (Join-Path $Tmp "agenttx-$Target\agenttx.exe") (Join-Path $InstallDir "agenttx.exe") -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $Entries = if ($UserPath) { $UserPath -split ";" } else { @() }
    if ($Entries -notcontains $InstallDir) {
        $NewPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Host "Added $InstallDir to your PATH."
    }

    Write-Host ""
    Write-Host "AgentTx installed: $(Join-Path $InstallDir 'agenttx.exe')" -ForegroundColor Green
    Write-Host "Next: open a NEW PowerShell window and run:  agenttx connect"
    Write-Host "Guide: $Guide"
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
