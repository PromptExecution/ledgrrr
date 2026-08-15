[CmdletBinding()]
param(
    [ValidateSet("Build", "TestInstall")]
    [string]$Action = "Build",
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")),
    [string]$Version = "0.1.0",
    [string]$OutputDir,
    [string]$CertificateStorePath,
    [switch]$KeepCertificate
)

$ErrorActionPreference = "Stop"
if (-not $CertificateStorePath) {
    $CertificateStorePath = if (Test-Path "P:\") {
        "P:\ledgrrr\test-signing"
    } else {
        Join-Path $env:LOCALAPPDATA "ledgrrr\test-signing"
    }
}
# WSL-mounted checkouts can inherit `build.rustc-wrapper = "sccache"` from
# Linux configuration even when Windows does not have sccache. Do not use a
# batch-file pass-through: Cargo's generated rustc invocation can exceed cmd's
# command-line limit. A temporary Cargo config is length-safe and avoids
# PowerShell/native argument quoting differences around an empty TOML string.
$cargoConfig = @()
$cargoConfigPath = $null
if (-not (Get-Command sccache -ErrorAction SilentlyContinue)) {
    $cargoConfigPath = Join-Path $env:TEMP "ledgrrr-cargo-no-sccache.toml"
    @"
[build]
rustc-wrapper = ""
"@ | Set-Content -NoNewline -Encoding ascii $cargoConfigPath
    $cargoConfig = @("--config", $cargoConfigPath)
}
if (-not $OutputDir) { $OutputDir = Join-Path $RepoRoot "dist\\windows" }
$stage = Join-Path $OutputDir "stage"
$payload = Join-Path $OutputDir "payload"
$msix = Join-Path $OutputDir "ledgrrr-$Version-test-signed.msix"
$certificatePath = Join-Path $OutputDir "ledgrrr-test.cer"
$payloadArchive = Join-Path $OutputDir "ledgrrr-$Version-external-payload.zip"
$checksumPath = "$msix.sha256"
$pfxPath = Join-Path $CertificateStorePath "ledgrrr-test-signing.pfx"
$persistentCertificatePath = Join-Path $CertificateStorePath "ledgrrr-test.cer"

function Find-WindowsSdkTool {
    param([string]$Name)
    $candidates = Get-ChildItem "${env:ProgramFiles(x86)}\\Windows Kits\\10\\bin" -Filter $Name -Recurse -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending
    if (-not $candidates) { throw "$Name was not found. Install the Windows 10/11 SDK." }
    return $candidates[0].FullName
}

function Assert-MsvcLinker {
    if (Get-Command link.exe -ErrorAction SilentlyContinue) { return }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    $installPaths = @()
    if (Test-Path $vswhere) {
        $installPaths += @(& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    }
    $devCommands = @()
    if ($env:LEDGRRR_VSDEVCMD) { $devCommands += $env:LEDGRRR_VSDEVCMD }
    foreach ($installPath in $installPaths) {
        if ($installPath) { $devCommands += (Join-Path $installPath "Common7\Tools\VsDevCmd.bat") }
    }
    # `vswhere` can be absent or stale after a repaired/offline Build Tools
    # install. Probe the conventional and documented clean-install locations.
    $devCommands += (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat")
    $devCommands += "C:\BuildTools\Common7\Tools\VsDevCmd.bat"

    foreach ($devCmd in ($devCommands | Select-Object -Unique)) {
        if (Test-Path $devCmd -PathType Leaf) {
            # `VsDevCmd.bat` changes a cmd.exe child environment. Re-import it
            # into this PowerShell process so ordinary PowerShell/WSL callers
            # do not have to know or launch a special Developer prompt.
            $environment = & cmd.exe /d /s /c "call `"$devCmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
            foreach ($line in $environment) {
                if ($line -match '^(?<name>[^=]+)=(?<value>.*)$') {
                    Set-Item -Path "env:$($Matches.name)" -Value $Matches.value
                }
            }
            if (Get-Command link.exe -ErrorAction SilentlyContinue) { return }
        }
    }
    throw "MSVC Build Tools with the Desktop development with C++ workload are required (link.exe was not found or could not be activated). Set LEDGRRR_VSDEVCMD to VsDevCmd.bat if installed in a custom location."
}

function Copy-RequiredFile {
    param([string]$Source, [string]$Destination)
    if (-not (Test-Path $Source -PathType Leaf)) { throw "required build output missing: $Source" }
    Copy-Item -Force $Source $Destination
}

function New-PayloadArchive {
    param([string]$PayloadDirectory, [string]$ArchivePath)
    # Defender/indexing can retain a newly copied PE file for a moment. The
    # release archive is required, so retry a short, bounded number of times
    # instead of emitting a half-built MSIX directory without its payload zip.
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            Remove-Item -Force $ArchivePath -ErrorAction SilentlyContinue
            Compress-Archive -Path (Join-Path $PayloadDirectory "*") -DestinationPath $ArchivePath -CompressionLevel Optimal -ErrorAction Stop
            return
        } catch {
            if ($attempt -eq 5) { throw "could not archive external payload after $attempt attempts: $($_.Exception.Message)" }
            Start-Sleep -Seconds $attempt
        }
    }
}

function Invoke-DocsBuild {
    param([string]$RepositoryRoot)
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\\bin"
    if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
    $bookRoot = Join-Path $RepositoryRoot "book"
    $docsRoot = Join-Path $bookRoot "book"
    if (Test-Path (Join-Path $docsRoot "index.html") -PathType Leaf) {
        return $docsRoot
    }

    $mdbookVersion = if (Get-Command mdbook -ErrorAction SilentlyContinue) { & mdbook --version } else { "" }
    if ($mdbookVersion -notmatch '^mdbook v0\.5\.') {
        & cargo install mdbook --version 0.5.1 --force
        if ($LASTEXITCODE -ne 0) { throw "could not install mdbook; package docs require network access or a preinstalled Rust toolchain" }
    }
    $mermaidVersion = if (Get-Command mdbook-mermaid -ErrorAction SilentlyContinue) { & mdbook-mermaid --version } else { "" }
    if ($mermaidVersion -notmatch '^mdbook-mermaid 0\.17\.') {
        & cargo install mdbook-mermaid --version 0.17.1 --force
        if ($LASTEXITCODE -ne 0) { throw "could not install compatible mdbook-mermaid; package docs require network access or a preinstalled Rust toolchain" }
    }
    $admonishVersion = if (Get-Command mdbook-admonish -ErrorAction SilentlyContinue) { & mdbook-admonish --version } else { "" }
    if ($admonishVersion -notmatch '^mdbook-admonish 1\.20\.') {
        & cargo install --git https://github.com/padamson/mdbook-admonish.git --branch feat/mdbook-0.5-compat --force mdbook-admonish
        if ($LASTEXITCODE -ne 0) { throw "could not install compatible mdbook-admonish; package docs require network access or a preinstalled Rust toolchain" }
    }
    if (-not (Get-Command mdbook-rhai-mermaid -ErrorAction SilentlyContinue)) {
        & cargo install --path (Join-Path $RepositoryRoot "crates\\mdbook-rhai-mermaid") --quiet
        if ($LASTEXITCODE -ne 0) { throw "could not install mdbook-rhai-mermaid" }
    }

    & mdbook build $bookRoot
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $docsRoot "index.html") -PathType Leaf)) {
        throw "mdBook did not produce $docsRoot\\index.html"
    }
    return $docsRoot
}

function Invoke-DesktopTool {
    param([string]$Controller, [string]$Name, [object]$Arguments = @{})
    $request = @{ jsonrpc = "2.0"; id = 1; method = "tools/call"; params = @{ name = $Name; arguments = $Arguments } } |
        ConvertTo-Json -Depth 20 -Compress
    # Use a temporary cmd file so the controller receives literal file-backed
    # handles. We collect the response file rather than waiting for cmd.exe:
    # Windows shell wrappers can retain descendant handles after start_runtime.
    $nonce = [guid]::NewGuid().ToString("N")
    $requestFile = Join-Path $env:TEMP "ledgrrr-mcp-$nonce-request.json"
    $responseFile = Join-Path $env:TEMP "ledgrrr-mcp-$nonce-response.json"
    $errorFile = Join-Path $env:TEMP "ledgrrr-mcp-$nonce-error.log"
    $runnerFile = Join-Path $env:TEMP "ledgrrr-mcp-$nonce.cmd"
    [IO.File]::WriteAllText($requestFile, $request + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    @"
@echo off
cd /d "$env:TEMP"
"$Controller" --once < "$requestFile" > "$responseFile" 2> "$errorFile"
"@ | Set-Content -NoNewline -Encoding ascii $runnerFile
    try {
        $wrapper = Start-Process -FilePath cmd.exe -ArgumentList @("/d", "/c", $runnerFile) -PassThru
        for ($attempt = 0; $attempt -lt 300; $attempt++) {
            if ((Test-Path $responseFile) -and (Get-Item $responseFile).Length -gt 0) { break }
            Start-Sleep -Milliseconds 100
        }
        if (-not (Test-Path $responseFile) -or (Get-Item $responseFile).Length -eq 0) {
            throw "controller $Name timed out after 30 seconds: $(Get-Content $errorFile -Raw -ErrorAction SilentlyContinue)"
        }
        if (-not $wrapper.HasExited) { Stop-Process -Id $wrapper.Id -Force -ErrorAction SilentlyContinue }
        $stdout = Get-Content $responseFile -Raw
    } finally {
        Remove-Item -Force $requestFile, $responseFile, $errorFile, $runnerFile -ErrorAction SilentlyContinue
    }
    $response = ($stdout -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -Last 1) | ConvertFrom-Json
    if ($response.error) { throw "controller $Name failed: $($response.error.message)" }
    return $response.result.structuredContent
}

if ($Action -eq "TestInstall") {
    if (-not (Test-Path $msix -PathType Leaf)) {
        & $PSCommandPath -Action Build -RepoRoot $RepoRoot -Version $Version -OutputDir $OutputDir -CertificateStorePath $CertificateStorePath -KeepCertificate:$KeepCertificate
    }
    Write-Host "[smoke] install"
    & (Join-Path $RepoRoot "windows\\package\\ledgrrr-package.ps1") -Action Install -Scope PerUser -PackagePath $msix -PayloadPath $payload -CertificatePath $certificatePath
    $installed = Get-AppxPackage -Name "ventures.elastic.ledgrrr" -ErrorAction SilentlyContinue
    if (-not $installed) { throw "MSIX install smoke did not discover the installed package." }

    # Sparse packages keep their identity registration separate from the
    # external Win32 payload.  Drive the real controller that was installed
    # into the per-user external location, not the tiny MSIX registration.
    $installRecord = Join-Path $env:LOCALAPPDATA "ledgrrr\package-install.json"
    if (-not (Test-Path $installRecord -PathType Leaf)) { throw "installed package did not write its external-payload record." }
    $externalPayload = (Get-Content $installRecord -Raw | ConvertFrom-Json).external_payload_dir
    $controller = Join-Path $externalPayload "ledgrrr-mcp.exe"
    if (-not (Test-Path $controller -PathType Leaf)) { throw "installed controller missing: $controller" }
    if (-not (Test-Path (Join-Path $externalPayload "docs\\index.html") -PathType Leaf)) {
        throw "installed package is missing the embedded docs playbook"
    }
    Write-Host "[smoke] controller status"
    $status = Invoke-DesktopTool -Controller $controller -Name "ledgrrr_status"
    if (-not $status.claude_controller -or [int]$status.claude_controller.expected_tools -ne 11) {
        throw "controller did not report the 11-tool desktop contract"
    }
    Write-Host "[smoke] start runtime"
    $started = Invoke-DesktopTool -Controller $controller -Name "ledgrrr_start_service"
    if (-not $started.ok) { throw "installed runtime did not start: $($started.message)" }
    $fixture = Get-Content (Join-Path $RepoRoot "crates\\ledgerr-desktop-agent\\tests\\fixtures\\sample-playbook-linear.json") -Raw | ConvertFrom-Json
    Write-Host "[smoke] render diagram"
    $rendered = Invoke-DesktopTool -Controller $controller -Name "ledgrrr_render_diagram" -Arguments @{ playbook = $fixture; format = "mermaid" }
    if ($rendered.content -notmatch "flowchart TD") { throw "installed controller did not render the sample playbook" }
    Write-Host "[smoke] stop runtime"
    $stopped = Invoke-DesktopTool -Controller $controller -Name "ledgrrr_stop_service"
    if (-not $stopped.ok) { throw "installed runtime did not stop: $($stopped.message)" }
    Write-Host "[smoke] repair"
    & (Join-Path $RepoRoot "windows\\package\\ledgrrr-package.ps1") -Action Repair -Scope PerUser
    $repaired = Get-AppxPackage -Name "ventures.elastic.ledgrrr" -ErrorAction SilentlyContinue
    if (-not $repaired) { throw "MSIX repair smoke did not restore package registration." }
    Write-Host "[smoke] uninstall"
    & (Join-Path $RepoRoot "windows\\package\\ledgrrr-package.ps1") -Action Uninstall -Scope PerUser
    if (Get-AppxPackage -Name "ventures.elastic.ledgrrr" -ErrorAction SilentlyContinue) { throw "MSIX uninstall smoke left a package registration." }
    Write-Host "Windows MSIX install/repair/uninstall smoke passed."
    exit 0
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $payload -ErrorAction SilentlyContinue
Remove-Item -Force $payloadArchive -ErrorAction SilentlyContinue
Remove-Item -Force $checksumPath -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $payload "Assets") | Out-Null
$docsRoot = Invoke-DocsBuild -RepositoryRoot $RepoRoot

Push-Location $RepoRoot
try {
    Assert-MsvcLinker
    & cargo @cargoConfig build --release -p ledgerr-desktop-agent --bins
    if ($LASTEXITCODE -ne 0) { throw "desktop-agent Windows build failed (exit $LASTEXITCODE)" }
    & cargo @cargoConfig build --release -p ledgerr-host --bin host-tauri
    if ($LASTEXITCODE -ne 0) { throw "Tauri Windows build failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
    if ($cargoConfigPath) { Remove-Item -Force $cargoConfigPath -ErrorAction SilentlyContinue }
}

Copy-RequiredFile (Join-Path $RepoRoot "target\\release\\ledgrrr-service.exe") (Join-Path $payload "ledgrrr-service.exe")
Copy-RequiredFile (Join-Path $RepoRoot "target\\release\\ledgrrr-mcp.exe") (Join-Path $payload "ledgrrr-mcp.exe")
Copy-RequiredFile (Join-Path $RepoRoot "target\\release\\host-tauri.exe") (Join-Path $payload "ledgrrr-tray.exe")
Copy-Item -Force (Join-Path $RepoRoot "windows\\package\\ledgrrr-package.ps1") (Join-Path $payload "ledgrrr-package.ps1")
Copy-Item -Force (Join-Path $RepoRoot "windows\\package\\support-manifest.json") (Join-Path $payload "support-manifest.json")
New-Item -ItemType Directory -Force -Path (Join-Path $payload "docs") | Out-Null
Copy-Item -Recurse -Force (Join-Path $docsRoot "*") (Join-Path $payload "docs")
Copy-Item -Force (Join-Path $RepoRoot "crates\\ledgerr-host\\icons\\StoreLogo.png") (Join-Path $payload "Assets\\StoreLogo.png")
Copy-Item -Force (Join-Path $RepoRoot "crates\\ledgerr-host\\icons\\Square150x150Logo.png") (Join-Path $payload "Assets\\Square150x150Logo.png")
Copy-Item -Force (Join-Path $RepoRoot "crates\\ledgerr-host\\icons\\Square44x44Logo.png") (Join-Path $payload "Assets\\Square44x44Logo.png")
Copy-Item -Force (Join-Path $RepoRoot "windows\\package\\AppxManifest.xml") (Join-Path $stage "AppxManifest.xml")
@"
<?xml version="1.0" encoding="utf-8"?>
<assembly manifestVersion="1.0" xmlns="urn:schemas-microsoft-com:asm.v1">
  <assemblyIdentity version="0.0.0.0" name="Ledgrrr" />
  <msix xmlns="urn:schemas-microsoft-com:msix.v1"
        publisher="CN=Ledgrrr Test Certificate"
        packageName="ventures.elastic.ledgrrr"
        applicationId="Ledgrrr" />
</assembly>
"@ | Set-Content -NoNewline -Encoding utf8 (Join-Path $payload "ledgrrr-tray.exe.manifest")

$windowsVersion = "$Version.0".Split(".")
while ($windowsVersion.Count -lt 4) { $windowsVersion += "0" }
$windowsVersion = ($windowsVersion | Select-Object -First 4) -join "."
(Get-Content (Join-Path $stage "AppxManifest.xml") -Raw).Replace("0.0.0.0", $windowsVersion) |
    Set-Content -NoNewline -Encoding utf8 (Join-Path $stage "AppxManifest.xml")

$makeAppx = Find-WindowsSdkTool "makeappx.exe"
$signTool = Find-WindowsSdkTool "signtool.exe"
& $makeAppx pack /d $stage /p $msix /o /nv
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE" }

$password = ConvertTo-SecureString -String "ledgrrr-test-only" -Force -AsPlainText
New-Item -ItemType Directory -Force -Path $CertificateStorePath | Out-Null
if (-not (Test-Path $pfxPath -PathType Leaf)) {
    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=Ledgrrr Test Certificate" -CertStoreLocation "Cert:\\CurrentUser\\My" -KeyExportPolicy Exportable
    Export-PfxCertificate -Cert $certificate -FilePath $pfxPath -Password $password | Out-Null
    Export-Certificate -Cert $certificate -FilePath $persistentCertificatePath | Out-Null
}
if (-not (Test-Path $persistentCertificatePath -PathType Leaf)) {
    throw "persistent certificate is missing next to test signer: $persistentCertificatePath"
}
Copy-Item -Force $persistentCertificatePath $certificatePath
& $signTool sign /fd SHA256 /f $pfxPath /p "ledgrrr-test-only" $msix
if ($LASTEXITCODE -ne 0) { throw "SignTool failed with exit code $LASTEXITCODE" }
# Authenticode policy verification of a deliberately self-signed certificate
# requires temporarily changing the user's trusted root store. Do not make a
# build mutate trust roots: TestInstall verifies this signature through the
# real Add-AppxPackage registration path after importing the release CER into
# CurrentUser\\TrustedPeople.
# The PFX remains only in CertificateStorePath (P: on private workstations)
# and is never copied into dist/, MCPB, or release artifacts.

# GitHub release assets are files, whereas sparse MSIX installs require a
# sibling external directory.  Archive its contents without changing the
# installer contract: release consumers extract this archive to .\payload.
New-PayloadArchive -PayloadDirectory $payload -ArchivePath $payloadArchive

$sha256 = (Get-FileHash $msix -Algorithm SHA256).Hash.ToLowerInvariant()
"$sha256 *$([IO.Path]::GetFileName($msix))" | Set-Content -NoNewline -Encoding ascii $checksumPath
[ordered]@{
    package = (Split-Path $msix -Leaf)
    checksum = (Split-Path $checksumPath -Leaf)
    certificate = (Split-Path $certificatePath -Leaf)
    external_payload = "payload"
    external_payload_archive = (Split-Path $payloadArchive -Leaf)
    extract_external_payload = "Expand-Archive -Force .\\$([IO.Path]::GetFileName($payloadArchive)) .\\payload"
    install = ".\\ledgrrr-package.ps1 -Action Install -Scope PerUser -PackagePath .\\$([IO.Path]::GetFileName($msix)) -PayloadPath .\\payload -CertificatePath .\\$([IO.Path]::GetFileName($certificatePath))"
    repair = ".\\ledgrrr-package.ps1 -Action Repair -Scope PerUser -PackagePath .\\$([IO.Path]::GetFileName($msix)) -PayloadPath .\\payload -CertificatePath .\\$([IO.Path]::GetFileName($certificatePath))"
    uninstall = ".\\ledgrrr-package.ps1 -Action Uninstall -Scope PerUser"
} | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutputDir "INSTALL.json")
Copy-Item -Force (Join-Path $RepoRoot "windows\\package\\ledgrrr-package.ps1") (Join-Path $OutputDir "ledgrrr-package.ps1")
[ordered]@{
    artifact = (Resolve-Path $msix).Path
    sha256 = $sha256
    checksum = (Resolve-Path $checksumPath).Path
    certificate = (Resolve-Path $certificatePath).Path
    external_payload_archive = (Resolve-Path $payloadArchive).Path
    version = $windowsVersion
    package_family = "ventures.elastic.ledgrrr"
    provenance = @{ source = $env:GITHUB_SHA; built_at_utc = (Get-Date).ToUniversalTime().ToString("o"); test_signed = $true; external_payload = "payload" }
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $OutputDir "ledgrrr-$Version-test-signed.provenance.json")
Write-Host "Built test-signed MSIX: $msix"
