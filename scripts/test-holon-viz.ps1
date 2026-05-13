<#
.SYNOPSIS
    Build the Tauri host, launch it with CDP, and verify the holon-viz panel
    renders a Cytoscape graph (window._cy has nodes).

.EXAMPLE
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\Projects\l3dg3rr\scripts\test-holon-viz.ps1"
#>
param(
    [switch]$SkipBuild,
    [int]$WaitSeconds = 10,
    [string]$CdpUrl = "http://127.0.0.1:19222"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = "D:\Projects\l3dg3rr"
$Binary      = "$ProjectRoot\target\debug\host-tauri.exe"
$env:PATH    = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH

$pass = 0; $fail = 0
function Check([string]$label, [scriptblock]$test) {
    try {
        if (& $test) { Write-Host "  PASS  $label" -ForegroundColor Green; $script:pass++ }
        else         { Write-Host "  FAIL  $label" -ForegroundColor Red;   $script:fail++ }
    } catch {
        Write-Host "  FAIL  $label  ($_)" -ForegroundColor Red; $script:fail++
    }
}

Write-Host "`n=== holon-viz CDP test ===" -ForegroundColor Cyan

# ── Build ────────────────────────────────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "[build] cargo build -p ledgerr-host --bin host-tauri"
    Push-Location $ProjectRoot
    try {
        cargo build -p ledgerr-host --bin host-tauri 2>&1 | ForEach-Object { "  $_" }
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally { Pop-Location }
}

Check "host-tauri binary exists" { Test-Path $Binary }

# ── Launch with CDP ──────────────────────────────────────────────────────────
Write-Host "`n[launch] starting host-tauri with CDP on port 19222"
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=19222"
$proc = Start-Process -FilePath $Binary -WorkingDirectory "$ProjectRoot\crates\ledgerr-host" -PassThru
Write-Host "  PID: $($proc.Id)"
Start-Sleep -Seconds $WaitSeconds

# ── CDP: navigate to viz panel by evaluating JS ──────────────────────────────
Write-Host "`n[cdp] connecting to $CdpUrl"
$cdpOk = $false
try {
    $ver = (Invoke-RestMethod "$CdpUrl/json/version" -TimeoutSec 5).Browser
    Write-Host "  CDP browser: $ver"
    $cdpOk = $true
} catch {
    Write-Host "  CDP unreachable: $_" -ForegroundColor Yellow
}

Check "CDP reachable on port 19222" { $cdpOk }

if ($cdpOk) {
    # Get the first page websocket URL
    $pages = Invoke-RestMethod "$CdpUrl/json" -TimeoutSec 5
    $wsUrl = ($pages | Where-Object { $_.type -eq "page" } | Select-Object -First 1).webSocketDebuggerUrl
    Write-Host "  WS: $wsUrl"

    if ($wsUrl) {
        # Use CDP Runtime.evaluate to click the Viz nav button and check window._cy
        Add-Type -AssemblyName System.Net.WebSockets.Client -ErrorAction SilentlyContinue
        try {
            $ws = New-Object System.Net.WebSockets.ClientWebSocket
            $cts = New-Object System.Threading.CancellationTokenSource
            $ws.ConnectAsync([Uri]$wsUrl, $cts.Token).Wait(5000) | Out-Null

            function WsSend([string]$msg) {
                $buf = [System.Text.Encoding]::UTF8.GetBytes($msg)
                $seg = [System.ArraySegment[byte]]::new($buf)
                $ws.SendAsync($seg, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $cts.Token).Wait(3000) | Out-Null
            }
            function WsRecv() {
                $buf = New-Object byte[] 65536
                $seg = [System.ArraySegment[byte]]::new($buf)
                $res = $ws.ReceiveAsync($seg, $cts.Token).GetAwaiter().GetResult()
                return [System.Text.Encoding]::UTF8.GetString($buf, 0, $res.Count)
            }

            # Navigate to viz panel (click last nav button = VZ)
            WsSend '{"id":1,"method":"Runtime.evaluate","params":{"expression":"(function(){var btns=document.querySelectorAll(\".nav-item[data-panel-index]\");btns[btns.length-1].click();return \"clicked\";})()" }}'
            $r1 = WsRecv
            Write-Host "  nav click: $($r1.Substring(0,[math]::Min(120,$r1.Length)))"
            Start-Sleep -Milliseconds 1500

            # Check window._cy exists and has nodes
            WsSend '{"id":2,"method":"Runtime.evaluate","params":{"expression":"window._cy ? window._cy.nodes().length : -1"}}'
            $r2 = WsRecv
            Write-Host "  _cy nodes: $($r2.Substring(0,[math]::Min(200,$r2.Length)))"

            $nodeCount = -1
            if ($r2 -match '"value"\s*:\s*(\d+)') { $nodeCount = [int]$Matches[1] }

            Check "window._cy initialized (nodes >= 0)" { $nodeCount -ge 0 }
            Check "graph has nodes (>= 5 holons)" { $nodeCount -ge 5 }
            Write-Host "  node count: $nodeCount"

            $ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "done", $cts.Token).Wait(2000) | Out-Null
        } catch {
            Write-Host "  CDP WS error: $_" -ForegroundColor Yellow
            Check "window._cy via CDP" { $false }
        }
    }
}

# ── Teardown ─────────────────────────────────────────────────────────────────
Write-Host "`n[teardown] stopping host-tauri PID $($proc.Id)"
try { $proc.Kill() } catch {}

# ── Summary ──────────────────────────────────────────────────────────────────
Write-Host ""
$total = $pass + $fail
if ($fail -eq 0) { Write-Host "=== PASSED $pass/$total ===" -ForegroundColor Green }
else             { Write-Host "=== FAILED $fail/$total ===" -ForegroundColor Red }
Write-Host ""

if ($fail -gt 0) { exit 1 }
