$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$binary = Join-Path $root "target\release\lm-resizer.exe"
if (-not (Test-Path $binary)) {
  throw "missing release binary: $binary"
}

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()

$bind = "127.0.0.1:$port"
$proxy = Start-Process -FilePath $binary -ArgumentList @("serve", "--bind", $bind) -PassThru -WindowStyle Hidden

try {
  $health = "http://$bind/health"
  $deadline = (Get-Date).AddSeconds(10)
  do {
    try {
      $ok = Invoke-RestMethod -Method Get -Uri $health -TimeoutSec 2
      if ($ok.ok -eq $true) { break }
    } catch {
      Start-Sleep -Milliseconds 150
    }
  } while ((Get-Date) -lt $deadline)

  if ((Get-Date) -ge $deadline) {
    throw "proxy did not become healthy at $health"
  }

  $body = @{
    model = "gpt-test"
    messages = @(
      @{
        role = "user"
        content = "Summarize this repeated build output: error: compile failed`nerror: compile failed`nerror: compile failed"
      }
    )
  } | ConvertTo-Json -Depth 8

  $response = Invoke-RestMethod `
    -Method Post `
    -Uri "http://$bind/v1/chat/completions" `
    -ContentType "application/json" `
    -Body $body `
    -TimeoutSec 10

  if ($response.mode -ne "preview") {
    throw "expected preview response"
  }
  if ($response.message -notmatch "set --upstream") {
    throw "expected upstream guidance message"
  }
  if ($null -eq $response.compression) {
    throw "missing compression stats"
  }
  if ($null -eq $response.request.messages) {
    throw "missing compressed request envelope"
  }

  Write-Host "proxy preview smoke passed at http://$bind"
} finally {
  if ($proxy -and -not $proxy.HasExited) {
    Stop-Process -Id $proxy.Id -Force
    $proxy.WaitForExit()
  }
}
