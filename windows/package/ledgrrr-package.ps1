[CmdletBinding()]
param(
    [ValidateSet("Plan", "Install", "Repair", "Uninstall", "TestInstall")]
    [string]$Action = "Plan",
    [ValidateSet("PerUser", "Machine")]
    [string]$Scope = "PerUser",
    [string]$PackagePath,
    [string]$PayloadPath,
    [string]$ExternalLocation,
    [string]$CertificatePath,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"
$PackageFamily = "ventures.elastic.ledgrrr"

function Get-LedgrrrExternalLocation {
    param([string]$InstallScope)
    if ($ExternalLocation) { return $ExternalLocation }
    if ($InstallScope -eq "Machine") {
        return (Join-Path ${env:ProgramFiles} "ledgrrr")
    }
    return (Join-Path $env:LOCALAPPDATA "Programs\\ledgrrr")
}

function Assert-AdminWhenRequired {
    if ($Scope -ne "Machine") { return }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        $arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"", "-Action", $Action, "-Scope", $Scope)
        if ($PackagePath) { $arguments += @("-PackagePath", "`"$PackagePath`"") }
        if ($PayloadPath) { $arguments += @("-PayloadPath", "`"$PayloadPath`"") }
        if ($ExternalLocation) { $arguments += @("-ExternalLocation", "`"$ExternalLocation`"") }
        if ($CertificatePath) { $arguments += @("-CertificatePath", "`"$CertificatePath`"") }
        if ($Quiet) { $arguments += "-Quiet" }
        $shell = if (Get-Command pwsh.exe -ErrorAction SilentlyContinue) { "pwsh.exe" } else { "powershell.exe" }
        Start-Process -FilePath $shell -Verb RunAs -ArgumentList $arguments
        exit 0
    }
}

function Get-InstalledPackage {
    Get-AppxPackage -Name $PackageFamily -ErrorAction SilentlyContinue | Select-Object -First 1
}

function Assert-DesktopPrerequisites {
    $build = [Environment]::OSVersion.Version.Build
    if ($build -lt 19041) {
        throw "Windows build $build is unsupported; sparse MSIX external-location packages require Windows 10 2004 (build 19041) or newer."
    }
    if (-not (Get-Command Add-AppxPackage -ErrorAction SilentlyContinue)) {
        throw "The Windows Appx PowerShell module is missing; MSIX package registration is unavailable."
    }
    $webViewCandidates = @(
        (Join-Path ${env:ProgramFiles(x86)} "Microsoft\EdgeWebView\Application\*\msedgewebview2.exe"),
        (Join-Path $env:ProgramFiles "Microsoft\EdgeWebView\Application\*\msedgewebview2.exe")
    )
    $webView = $webViewCandidates | ForEach-Object { Get-Item $_ -ErrorAction SilentlyContinue } | Select-Object -First 1
    if (-not $webView) {
        throw "Microsoft Edge WebView2 Runtime is required for the Tauri tray. Install Evergreen WebView2, then retry."
    }
}

function Install-TestCertificate {
    if (-not $CertificatePath -and $PackagePath) {
        $candidate = Join-Path (Split-Path $PackagePath -Parent) "ledgrrr-test.cer"
        if (Test-Path $candidate -PathType Leaf) { $CertificatePath = $candidate }
    }
    if (-not $CertificatePath) {
        $candidate = Join-Path (Get-LedgrrrStateRoot) "package-cache\\ledgrrr-test.cer"
        if (Test-Path $candidate -PathType Leaf) { $CertificatePath = $candidate }
    }
    if (-not $CertificatePath) { return $null }
    if (-not (Test-Path $CertificatePath -PathType Leaf)) {
        throw "test certificate not found: $CertificatePath"
    }
    $certificate = Get-PfxCertificate -FilePath $CertificatePath
    $existing = Get-ChildItem Cert:\CurrentUser\TrustedPeople |
        Where-Object { $_.Thumbprint -eq $certificate.Thumbprint }
    if (-not $existing) {
        Import-Certificate -FilePath $CertificatePath -CertStoreLocation Cert:\CurrentUser\TrustedPeople | Out-Null
    }
    return $certificate
}

function Get-LedgrrrStateRoot {
    if ($env:LEDGRRR_STATE_DIR) { return $env:LEDGRRR_STATE_DIR }
    return (Join-Path $env:LOCALAPPDATA "ledgrrr")
}

function Get-CachedPackagePath {
    return (Join-Path (Get-LedgrrrStateRoot) "package-cache\\ledgrrr-test-signed.msix")
}

function Cache-PackageMaterial {
    param([string]$SourcePackage)
    $cache = Join-Path (Get-LedgrrrStateRoot) "package-cache"
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    Copy-Item -Force $SourcePackage (Join-Path $cache "ledgrrr-test-signed.msix")
    if ($CertificatePath -and (Test-Path $CertificatePath -PathType Leaf)) {
        Copy-Item -Force $CertificatePath (Join-Path $cache "ledgrrr-test.cer")
    }
}

function Write-PackageInstallRecord {
    param([string]$PayloadDirectory, [string]$CertificateThumbprint)
    $state = Get-LedgrrrStateRoot
    New-Item -ItemType Directory -Force -Path $state | Out-Null
    [ordered]@{
        schema_version = 1
        package_family = $PackageFamily
        external_payload_dir = $PayloadDirectory
        certificate_thumbprint = $CertificateThumbprint
        scope = $Scope
        installed_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $state "package-install.json")
}

function Get-PayloadPath {
    if ($PayloadPath) { return (Resolve-Path $PayloadPath) }
    if ($Action -eq "Repair" -and (Test-Path $location -PathType Container)) {
        return (Resolve-Path $location)
    }
    $candidate = Join-Path (Split-Path $PackagePath -Parent) "payload"
    if (-not (Test-Path $candidate -PathType Container)) {
        throw "external payload directory not found: $candidate"
    }
    return (Resolve-Path $candidate)
}

function Assert-ExternalPayload {
    param([string]$PayloadDirectory)
    $manifestPath = Join-Path $PayloadDirectory "support-manifest.json"
    if (-not (Test-Path $manifestPath -PathType Leaf)) {
        throw "external payload is missing support-manifest.json: $PayloadDirectory"
    }
    $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.schema_version -ne 1 -or $manifest.package_family -ne $PackageFamily) {
        throw "external payload support manifest is incompatible with $PackageFamily"
    }
    foreach ($name in @($manifest.payload.controller, $manifest.payload.runtime, $manifest.payload.tray, $manifest.payload.package_helper)) {
        if (-not $name -or -not (Test-Path (Join-Path $PayloadDirectory $name) -PathType Leaf)) {
            throw "external payload is missing required file: $name"
        }
    }
}

$location = Get-LedgrrrExternalLocation -InstallScope $Scope
$result = [ordered]@{
    action = $Action.ToLowerInvariant()
    scope = $Scope
    package_family = $PackageFamily
    external_location = $location
    uac_required = ($Scope -eq "Machine")
    status = "planned"
}

if ($Action -eq "Plan") {
    $result | ConvertTo-Json -Depth 4
    exit 0
}

Assert-AdminWhenRequired

if ($Action -eq "Uninstall") {
    $recordPath = Join-Path (Get-LedgrrrStateRoot) "package-install.json"
    $certificateThumbprint = $null
    if (Test-Path $recordPath -PathType Leaf) {
        try { $certificateThumbprint = (Get-Content $recordPath -Raw | ConvertFrom-Json).certificate_thumbprint } catch {}
    }
    $installed = Get-InstalledPackage
    if ($Scope -eq "Machine") {
        Get-AppxProvisionedPackage -Online |
            Where-Object { $_.DisplayName -eq $PackageFamily } |
            ForEach-Object { Remove-AppxProvisionedPackage -Online -PackageName $_.PackageName | Out-Null }
        Get-AppxPackage -AllUsers -Name $PackageFamily |
            ForEach-Object { Remove-AppxPackage -AllUsers -Package $_.PackageFullName -ErrorAction Stop }
    } elseif ($installed) {
        Remove-AppxPackage -Package $installed.PackageFullName -ErrorAction Stop
    }
    # The sparse payload is executable code outside the MSIX identity. Stop
    # only our named runtime/tray processes before removing that payload so a
    # partial/failed smoke cannot leave locked binaries behind.
    Get-Process -Name "ledgrrr-service", "ledgrrr-tray" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    if (Test-Path $location) { Remove-Item -Recurse -Force $location }
    Remove-Item -Recurse -Force (Join-Path (Get-LedgrrrStateRoot) "package-cache") -ErrorAction SilentlyContinue
    Remove-Item -Force $recordPath -ErrorAction SilentlyContinue
    if ($certificateThumbprint) {
        Remove-Item -Path "Cert:\CurrentUser\TrustedPeople\$certificateThumbprint" -Force -ErrorAction SilentlyContinue
    }
    $result.status = "uninstalled"
    $result | ConvertTo-Json -Depth 4
    exit 0
}

Assert-DesktopPrerequisites

if (-not $PackagePath -and $Action -eq "Repair") {
    $cached = Get-CachedPackagePath
    if (Test-Path $cached -PathType Leaf) { $PackagePath = $cached }
}
if (-not $PackagePath) {
    throw "-PackagePath is required for $Action. Use the test-signed MSIX release artifact."
}
if (-not (Test-Path $PackagePath -PathType Leaf)) {
    throw "MSIX package not found: $PackagePath"
}

New-Item -ItemType Directory -Force -Path $location | Out-Null
$payload = Get-PayloadPath
Assert-ExternalPayload -PayloadDirectory $payload.Path
# Install and repair may replace the sparse external payload. Stop only this
# package's runtime/tray first so a prior interrupted operation cannot lock a
# payload executable and make the retry fail.
Get-Process -Name "ledgrrr-service", "ledgrrr-tray" -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
if ((Resolve-Path $location).Path -ne $payload.Path) {
    Copy-Item -Recurse -Force (Join-Path $payload "*") $location
}
$testCertificate = Install-TestCertificate
if ($Action -eq "Install") { Cache-PackageMaterial -SourcePackage $PackagePath }
if ($Action -eq "Repair") {
    # Re-register the identity while preserving per-user config, audit, and
    # data outside the external payload location.
    $existing = Get-InstalledPackage
    if ($existing) { Remove-AppxPackage -Package $existing.PackageFullName -ErrorAction Stop }
}
$installArgs = @{
    Path = (Resolve-Path $PackagePath)
    ExternalLocation = (Resolve-Path $location)
    ForceApplicationShutdown = $true
    ForceUpdateFromAnyVersion = $true
    ErrorAction = "Stop"
}
# This is a sparse identity package. The external location contains the
# Win32/Tauri/controller/runtime payload; the MSIX only grants package identity.
if ($Scope -eq "Machine") {
    Add-AppxPackage @installArgs -Stage
    Add-AppxProvisionedPackage -Online -PackagePath $installArgs.Path | Out-Null
} else {
    # Dogfood packages are test-signed. This narrowly scoped per-user bypass
    # permits the generated MSIX without installing a self-signed root CA.
    # It must not be used for a public or machine-wide package.
    $installArgs.AllowUnsigned = $true
    Add-AppxPackage @installArgs
}

$installed = Get-InstalledPackage
if (-not $installed) { throw "Windows did not report the MSIX package after installation." }
Write-PackageInstallRecord -PayloadDirectory $location -CertificateThumbprint $testCertificate.Thumbprint
$result.status = if ($Action -eq "Repair") { "repaired" } else { "installed" }
$result.install_location = $installed.InstallLocation
$result | ConvertTo-Json -Depth 4
