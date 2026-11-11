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
    if (-not $env:MPV_LIBRARY_DIR -or -not (Test-Path (Join-Path $env:MPV_LIBRARY_DIR 'mpv.lib')) -or -not (Test-Path (Join-Path $env:MPV_LIBRARY_DIR 'libmpv-2.dll'))) {
        throw 'Set MPV_LIBRARY_DIR to a directory containing mpv.lib and libmpv-2.dll'
    }

    # cargo writes build progress to stderr; keep that from tripping the
    # ErrorActionPreference. A real failure is caught via $LASTEXITCODE.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"

    cargo build --release -p lumiere --locked --features video,models
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
    Copy-Item (Join-Path $env:MPV_LIBRARY_DIR 'libmpv-2.dll') (Join-Path $Stage 'libmpv-2.dll') -Force
    Copy-Item (Join-Path $PSScriptRoot 'LICENSE') (Join-Path $Stage 'LICENSE') -Force
    Copy-Item (Join-Path $PSScriptRoot 'THIRD_PARTY_NOTICES.md') (Join-Path $Stage 'THIRD_PARTY_NOTICES.md') -Force
    Copy-Item (Join-Path $PSScriptRoot 'third-party\mpv-windows') (Join-Path $Stage 'mpv-windows') -Recurse -Force

    # Authenticode-sign the executable that goes into the archive. This matches
    # how prior Windows releases were signed: the default certificate is the
    # self-signed FETAG.NET code-signing cert in the local store, and an RFC 3161
    # timestamp is embedded so the signature stays valid past the cert's expiry.
    # (A self-signed root still triggers Windows "unknown publisher" for end
    # users — it establishes a verifiable signature, not publisher trust.)
    # Override with LUMIERE_SIGN_CERT (cert thumbprint) or LUMIERE_SIGN_TIMESTAMP
    # (RFC 3161 server) if the defaults do not fit. When no certificate or no
    # signtool is available the bundle is still produced, but left unsigned with
    # a loud warning — do not ship it.
    $StageExe = Join-Path $Stage "lumiere.exe"

    # signtool.exe is part of the Windows SDK and is usually NOT on PATH, so
    # resolve it explicitly. Prefer the x64 binary to match the x64 executable.
    $Signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue).Source
    if (-not $Signtool) {
        $SdkBins = @("${env:ProgramFiles(x86)}\Windows Kits\10\bin", "${env:ProgramFiles}\Windows Kits\10\bin") |
            Where-Object { $_ -and (Test-Path $_) }
        foreach ($root in $SdkBins) {
            $x64 = Get-ChildItem -Path $root -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -like '*\x64\*' } |
                Sort-Object FullName -Descending
            if ($x64) { $Signtool = $x64[0].FullName; break }
        }
        if (-not $Signtool) {
            foreach ($root in $SdkBins) {
                $any = Get-ChildItem -Path $root -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue
                if ($any) { $Signtool = $any[0].FullName; break }
            }
        }
    }

    $SignThumbprint = $env:LUMIERE_SIGN_CERT
    if (-not $SignThumbprint) {
        $SignCert = Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue |
            Where-Object { $_.Subject -eq "CN=FETAG.NET" } |
            Select-Object -First 1
        if ($SignCert) { $SignThumbprint = $SignCert.Thumbprint }
    }

    if ($Signtool -and $SignThumbprint) {
        $TimestampServer = $env:LUMIERE_SIGN_TIMESTAMP
        if (-not $TimestampServer) { $TimestampServer = "http://timestamp.digicert.com" }
        Write-Host "Signing $StageExe with certificate $SignThumbprint ..."

        # signtool writes progress and errors to stderr; under the script's
        # $ErrorActionPreference = "Stop" a native stderr line trips a
        # NativeCommandError. Relax it around the native calls (same approach as
        # the cargo build above) and judge success by $LASTEXITCODE + the
        # verify output, not by a stderr-driven exception.
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = "Continue"

        & $Signtool sign /fd sha256 /td sha256 /tr $TimestampServer /sha1 $SignThumbprint $StageExe
        if ($LASTEXITCODE -ne 0) {
            $ErrorActionPreference = $prevEAP
            throw "signtool sign failed (exit $LASTEXITCODE)"
        }

        # Verify. The signing certificate is self-signed (FETAG.NET), so
        # `signtool verify` always reports its root as "not trusted" and exits
        # non-zero even for a correct, timestamped signature — the exit code is
        # therefore NOT the pass/fail signal here. Instead confirm from the
        # verbose output that the primary signature is present, belongs to the
        # expected certificate, and carries an RFC 3161 timestamp.
        $verifyOut = & $Signtool verify /v /pa $StageExe 2>&1 | ForEach-Object { "$_" }
        $ErrorActionPreference = $prevEAP
        $blob = $verifyOut -join "`n"
        $hasPrimary = $blob -match 'Signature Index: 0 \(Primary Signature\)'
        $isCert     = $blob -match [regex]::Escape($SignThumbprint)
        $isStamped  = $blob -match 'The signature is timestamped'
        if (-not ($hasPrimary -and $isCert -and $isStamped)) {
            $verifyOut | ForEach-Object { Write-Host $_ }
            throw "signtool verify could not confirm a primary signature for $SignThumbprint with an embedded timestamp"
        }
        Write-Host "Signature verified (self-signed FETAG.NET; the 'root not trusted' warning is expected)."
    }
    elseif (-not $Signtool) {
        Write-Warning "signtool.exe not found (Windows SDK not installed or not on PATH); $StageExe will be left UNSIGNED. Install the Windows SDK or add its bin\x64 dir to PATH before shipping."
    }
    else {
        Write-Warning "NO code-signing certificate available; $StageExe will be left UNSIGNED. Import the FETAG.NET cert (or set LUMIERE_SIGN_CERT) and re-run before shipping."
    }

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
