# Build a portable Windows archive: a single folder with the executable and
# its About-panel artwork, zipped so extraction never scatters files into the
# caller's working directory.
# Usage: .\bundle-windows.ps1 [output dir]   (default: .\dist)
param(
    [string]$Out = "dist"
)

$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    # cargo writes build progress to stderr; keep that from tripping the
    # ErrorActionPreference. A real failure is caught via $LASTEXITCODE.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"

    cargo build --release -p lumiere --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }

    # Cargo.toml is the release-version authority; read it without a second
    # value a maintainer has to keep in lockstep at release time.
    $meta = (cargo metadata --no-deps --format-version 1) -join " "
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }

    # Back to strict mode before touching the staging directory, so a failed
    # copy or archive never leaves a half-built zip behind.
    $ErrorActionPreference = $prev

    if ($meta -match '"name"\s*:\s*"lumiere"\s*,\s*"version"\s*:\s*"([^"]+)"') {
        $Version = $Matches[1]
    }
    if (-not $Version) { throw "could not determine the lumiere version" }

    switch ($env:PROCESSOR_ARCHITECTURE) {
        "AMD64" { $Arch = "x86_64" }
        "ARM64" { $Arch = "aarch64" }
        default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
    }

    $Name = "lumiere-v${Version}-windows-${Arch}"
    $Stage = Join-Path $Out $Name
    $Archive = Join-Path $Out "${Name}.zip"

    if (Test-Path $Stage)   { Remove-Item $Stage -Recurse -Force }
    if (Test-Path $Archive) { Remove-Item $Archive -Force }
    New-Item -ItemType Directory -Path $Stage | Out-Null

    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class LumiereIconCheck {
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern uint ExtractIconEx(string file, int index, System.IntPtr large, System.IntPtr small, uint count);
}
'@

    # The executable icon is embedded at compile time; keep the
    # About-panel artwork beside the executable, the way the Linux bundle
    # does.
    $exe  = Join-Path $PSScriptRoot "target\release\lumiere.exe"
    $icon = Join-Path $PSScriptRoot "assets\AppIcon.png"
    if (-not (Test-Path $exe)) { throw "release executable not found: $exe" }
    $iconCount = [LumiereIconCheck]::ExtractIconEx($exe, -1, [IntPtr]::Zero, [IntPtr]::Zero, 0)
    if ($iconCount -eq 0 -or $iconCount -eq [uint32]::MaxValue) {
        throw "release executable has no application icon resource: $exe"
    }
    Copy-Item $exe  (Join-Path $Stage "lumiere.exe") -Force
    Copy-Item $icon (Join-Path $Stage "AppIcon.png") -Force

    $Readme = @"
Lumiere for Windows
===================

Run lumiere.exe directly, or pass a folder to open it:

    .\lumiere.exe C:\path\to\folder

No installation is required: extract the archive and run the executable.
"@
    Set-Content -Path (Join-Path $Stage "README.txt") -Value $Readme -Encoding ASCII

    Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Archive -Force
    Remove-Item $Stage -Recurse -Force

    Write-Host "built $Archive"
}
finally {
    Pop-Location
}
