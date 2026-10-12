$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

function Read-Helper {
  param([string]$Path, [string]$Name)
  $tokens = $null
  $parseErrors = $null
  $ast = [Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$parseErrors)
  if ($parseErrors.Count) { throw "Invalid PowerShell in ${Path}: $parseErrors" }
  $function = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name
  }, $true)
  if (-not $function) { throw "Missing $Name in $Path" }
  return [scriptblock]::Create($function.Extent.Text)
}

foreach ($source in @('install.ps1', 'scripts/test-install-binary.ps1')) {
  & {
    . (Read-Helper (Join-Path $root $source) 'Remove-LmResizerTemporaryPath')
    $script:attempts = 0
    $script:delays = @()
    $script:warnings = @()
    $script:exists = $true
    $script:permanent = $false
    function Test-Path { param($LiteralPath) return $script:exists }
    function Remove-Item {
      param($LiteralPath, [switch]$Recurse, [switch]$Force, $ErrorAction)
      if ($ErrorAction -ne 'Stop') { throw 'Removal must catch nonterminating errors' }
      $script:attempts++
      if ($script:permanent -or $script:attempts -lt 3) { throw 'simulated sharing violation' }
    }
    function Start-Sleep { param($Milliseconds) $script:delays += $Milliseconds }
    function Write-Warning {
      param($Message, $WarningAction)
      if ($WarningAction -ne 'Continue') { throw 'Cleanup warning could become terminating' }
      $script:warnings += $Message
    }
    Remove-LmResizerTemporaryPath 'temporary-test-path'
    if ($script:attempts -ne 3 -or $script:delays.Count -ne 2 -or $script:warnings.Count) {
      throw "Transient lock was not retried correctly: $source"
    }
    if (@($script:delays | Where-Object { $_ -le 0 -or $_ -gt 1000 }).Count) {
      throw "Invalid retry delay: $source"
    }
    $script:attempts = 0
    $script:delays = @()
    $script:permanent = $true
    # A persistent lock must allow successful installation to remain successful.
    Remove-LmResizerTemporaryPath 'temporary-test-path'
    if ($script:attempts -ne 10 -or $script:delays.Count -ne 9 -or $script:warnings.Count -ne 1) {
      throw "Persistent lock did not stop with a warning: $source"
    }
    $caught = $null
    try {
      try { throw 'original installation failure' }
      finally { Remove-LmResizerTemporaryPath 'temporary-test-path' }
    } catch { $caught = $_.Exception.Message }
    if ($caught -ne 'original installation failure') { throw "Cleanup masked the original error: $caught" }
    $script:exists = $false
    $script:attempts = 0
    Remove-LmResizerTemporaryPath 'already-removed'
    if ($script:attempts) { throw 'Absent temporary path should require no removal' }
    Write-Host "PASS cleanup retries, warning and original error: $source"
  }
}

& {
  . (Read-Helper (Join-Path $root 'install.ps1') 'Invoke-LmResizerVersion')
  # Model stdout EOF before process termination, including valid-looking
  # version output followed by a failed process exit.
  function New-Object { param($TypeName) return $script:fakeProcess }
  foreach ($exitCode in @(0, 9)) {
    $reader = [pscustomobject]@{}
    $reader | Add-Member ScriptMethod ReadToEnd { return "lm-resizer test-version`n" }
    $script:fakeProcess = [pscustomobject]@{
      StartInfo = [pscustomobject]@{ FileName = ''; Arguments = ''; UseShellExecute = $true; RedirectStandardOutput = $false }
      StandardOutput = $reader
      ExitCode = $null
      ExpectedExitCode = $exitCode
      Waited = $false
      Disposed = $false
    }
    $script:fakeProcess | Add-Member ScriptMethod Start { return $true }
    $script:fakeProcess | Add-Member ScriptMethod WaitForExit { $this.Waited = $true; $this.ExitCode = $this.ExpectedExitCode }
    $script:fakeProcess | Add-Member ScriptMethod Dispose { $this.Disposed = $true }
    $caught = $null
    try { $version = Invoke-LmResizerVersion 'test-program' } catch { $caught = $_.Exception.Message }
    if (-not $script:fakeProcess.Waited -or -not $script:fakeProcess.Disposed) { throw 'Process was not waited/disposed' }
    if ($exitCode -eq 0 -and ($caught -or $version -ne 'lm-resizer test-version')) { throw 'Successful version command failed' }
    if ($exitCode -ne 0 -and $caught -notlike 'Version command failed (9):*') { throw 'Failed version command was accepted' }
  }
  Write-Host 'PASS version process wait, disposal and exit status'
}
