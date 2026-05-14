param(
    [string]$CdpUrl = "http://127.0.0.1:19222",
    [string]$ScreenshotPath = "D:\Projects\l3dg3rr\target\viz-cdp-inspect.png"
)

$ErrorActionPreference = "Stop"

$pages = Invoke-RestMethod "$CdpUrl/json" -TimeoutSec 5
$wsUrl = ($pages | Where-Object { $_.type -eq "page" } | Select-Object -First 1).webSocketDebuggerUrl
if (-not $wsUrl) { throw "No CDP page target found at $CdpUrl" }

$ws = New-Object System.Net.WebSockets.ClientWebSocket
$cts = New-Object System.Threading.CancellationTokenSource
$ws.ConnectAsync([Uri]$wsUrl, $cts.Token).Wait(5000) | Out-Null

function WsSend([string]$msg) {
    $buf = [System.Text.Encoding]::UTF8.GetBytes($msg)
    $seg = [System.ArraySegment[byte]]::new($buf)
    $ws.SendAsync($seg, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $cts.Token).Wait(3000) | Out-Null
}

function WsRecv([int]$id) {
    while ($true) {
        $chunks = New-Object System.Collections.Generic.List[byte]
        do {
            $buf = New-Object byte[] 1048576
            $seg = [System.ArraySegment[byte]]::new($buf)
            $res = $ws.ReceiveAsync($seg, $cts.Token).GetAwaiter().GetResult()
            for ($i = 0; $i -lt $res.Count; $i++) { $chunks.Add($buf[$i]) }
        } while (-not $res.EndOfMessage)
        $text = [System.Text.Encoding]::UTF8.GetString($chunks.ToArray())
        $json = $text | ConvertFrom-Json
        if ($json.id -eq $id) { return $json }
    }
}

function EvalJson([int]$id, [string]$expr) {
    $payload = @{
        id = $id
        method = "Runtime.evaluate"
        params = @{
            expression = $expr
            returnByValue = $true
        }
    } | ConvertTo-Json -Depth 8 -Compress
    WsSend $payload
    return WsRecv $id
}

EvalJson 1 "(function(){var btns=document.querySelectorAll('.nav-item[data-panel-index]');btns[btns.length-1].click();return 'viz';})()" | Out-Null
Start-Sleep -Milliseconds 1200

$expr = @"
(function(){
  var q=function(sel){var e=document.querySelector(sel);return e?e.getBoundingClientRect():null};
  var rect=function(sel){var r=q(sel);return r?{x:r.x,y:r.y,w:r.width,h:r.height}:null};
  var cy=window._cy;
  var visibleNodes=cy?cy.nodes(':visible').length:0;
  var visibleEdges=cy?cy.edges(':visible').length:0;
  return {
    window:{w:window.innerWidth,h:window.innerHeight},
    panel:rect('#panel-viz'),
    toolbar:rect('.viz-toolbar'),
    body:rect('.viz-body'),
    canvas:rect('#cy'),
    detail:rect('#viz-detail'),
    cy:{
      exists:!!cy,
      width:cy?cy.width():0,
      height:cy?cy.height():0,
      zoom:cy?cy.zoom():0,
      nodes:cy?cy.nodes().length:0,
      edges:cy?cy.edges().length:0,
      visibleNodes:visibleNodes,
      visibleEdges:visibleEdges
    },
    text:document.body.innerText.slice(0,1200)
  };
})()
"@

$metrics = EvalJson 2 $expr
$value = $metrics.result.result.value
$value | ConvertTo-Json -Depth 8

WsSend (@{ id = 3; method = "Page.captureScreenshot"; params = @{ format = "png" } } | ConvertTo-Json -Depth 4 -Compress)
$shot = WsRecv 3
if ($shot.result.data) {
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($ScreenshotPath)) | Out-Null
    [System.IO.File]::WriteAllBytes($ScreenshotPath, [Convert]::FromBase64String($shot.result.data))
    Write-Host "screenshot=$ScreenshotPath"
}

$ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "done", $cts.Token).Wait(2000) | Out-Null
