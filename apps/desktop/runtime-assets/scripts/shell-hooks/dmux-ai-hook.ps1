# OSC 133 semantic marks for Codux terminals (C=command start, D=command end,
# A=prompt). Mirrors shell-hooks/dmux-ai-hook.zsh; AI TUIs skip C because a
# session-long command mark would fake a permanent spinner.
if ($env:DMUX_PS_HOOK_INSTALLED) { return }
$env:DMUX_PS_HOOK_INSTALLED = "1"

$global:CoduxOsc133Running = $false
$global:CoduxAiTools = @(
  'codex', 'claude', 'claude-code', 'reclaude', 'opencode', 'agy',
  'kiro-cli', 'codewhale', 'kimi', 'kimi-code', 'mimo'
)

function Global:CoduxWriteOsc([string]$Payload) {
  [Console]::Write(("{0}]{1}{2}" -f [char]27, $Payload, [char]7))
}

function Global:CoduxCommandIsAiTool([string]$CommandLine) {
  if ([string]::IsNullOrWhiteSpace($CommandLine)) { return $false }
  $first = $CommandLine.Trim().Split(@(' ', "`t"), 2)[0].Trim('"', "'")
  $leaf = Split-Path -Leaf $first
  $name = [System.IO.Path]::GetFileNameWithoutExtension($leaf).ToLowerInvariant()
  return $global:CoduxAiTools -contains $name
}

# PSReadLine hands back the accepted line; C marks the command as running.
function Global:PSConsoleHostReadLine {
  $line = [Microsoft.PowerShell.PSConsoleReadLine]::ReadLine($Host.Runspace, $ExecutionContext)
  if (-not [string]::IsNullOrWhiteSpace($line)) {
    $global:CoduxOsc133Running = $true
    if (-not (CoduxCommandIsAiTool $line)) {
      CoduxWriteOsc "133;C"
    }
  }
  return $line
}

$global:CoduxPreviousPrompt = $function:prompt
function Global:prompt {
  $lastSucceeded = $?
  if ($global:CoduxOsc133Running) {
    $global:CoduxOsc133Running = $false
    $exitCode = if ($null -ne $global:LASTEXITCODE) { $global:LASTEXITCODE }
    elseif ($lastSucceeded) { 0 } else { 1 }
    CoduxWriteOsc ("133;D;{0}" -f $exitCode)
  }
  CoduxWriteOsc "133;A"
  if ($global:CoduxPreviousPrompt) {
    & $global:CoduxPreviousPrompt
  } else {
    "PS $($ExecutionContext.SessionState.Path.CurrentLocation)> "
  }
}
