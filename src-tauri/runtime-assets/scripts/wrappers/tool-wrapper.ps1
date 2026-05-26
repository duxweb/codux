if ($args.Count -lt 1) {
  Write-Error "Missing tool name."
  exit 64
}

$Tool = [string]$args[0]
$ToolArgs = @()
if ($args.Count -gt 1) {
  $ToolArgs = @($args[1..($args.Count - 1)] | ForEach-Object { [string]$_ })
}

$ErrorActionPreference = "SilentlyContinue"
$wrapperDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$wrapperBin = Join-Path $wrapperDir "bin"

function Write-Live-Log([string]$Message) {
  if ([string]::IsNullOrWhiteSpace($env:DMUX_LOG_FILE)) { return }
  try {
    $parent = Split-Path -Parent $env:DMUX_LOG_FILE
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
      New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $stamp = Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"
    Add-Content -LiteralPath $env:DMUX_LOG_FILE -Value "[$stamp] [wrapper] $Message"
  } catch {
  }
}

function Split-PathList([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return @() }
  return $Value -split [Regex]::Escape([IO.Path]::PathSeparator)
}

function Join-PathList([string[]]$Values) {
  return ($Values | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [IO.Path]::PathSeparator
}

function Normalize-Directory([string]$Value) {
  try {
    return [IO.DirectoryInfo]::new($Value).FullName.TrimEnd('\', '/')
  } catch {
    return $Value.TrimEnd('\', '/')
  }
}

function Same-Directory([string]$A, [string]$B) {
  $left = Normalize-Directory $A
  $right = Normalize-Directory $B
  return [string]::Equals($left, $right, [StringComparison]::OrdinalIgnoreCase)
}

function Filter-Wrapper-Path([string]$Value) {
  $parts = Split-PathList $Value
  $filtered = @()
  foreach ($part in $parts) {
    if ([string]::IsNullOrWhiteSpace($part)) { continue }
    if (Same-Directory $part $wrapperBin) { continue }
    $filtered += $part
  }
  return Join-PathList $filtered
}

function Find-Real-Binary([string]$Name, [string]$SearchPath) {
  $previousPath = $env:PATH
  try {
    $env:PATH = $SearchPath
    $candidateNames = switch ($Name) {
      "claude" { @("claude.cmd", "claude-code.cmd", "claude.exe", "claude-code.exe"); break }
      "claude-code" { @("claude-code.cmd", "claude.cmd", "claude-code.exe", "claude.exe"); break }
      default { @("$Name.cmd", "$Name.exe") }
    }
    foreach ($candidate in $candidateNames) {
      $commands = @(Get-Command $candidate -CommandType Application -ErrorAction SilentlyContinue)
      foreach ($command in $commands) {
        if ($command -and $command.Source -and -not (Same-Directory (Split-Path -Parent $command.Source) $wrapperBin)) {
          return $command.Source
        }
      }
    }
  } finally {
    $env:PATH = $previousPath
  }
  return $null
}

function Read-Tool-Settings {
  $path = $env:DMUX_TOOL_PERMISSION_SETTINGS_FILE
  if ([string]::IsNullOrWhiteSpace($path) -or -not (Test-Path -LiteralPath $path)) { return $null }
  try {
    return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
  } catch {
    return $null
  }
}

function Tool-Config-Key([string]$Name) {
  switch ($Name) {
    "codex" { "codex" }
    "claude" { "claudeCode" }
    "claude-code" { "claudeCode" }
    "gemini" { "gemini" }
    "agy" { "gemini" }
    "opencode" { "opencode" }
    default { "" }
  }
}

function Tool-Model-Key([string]$Name) {
  switch ($Name) {
    "codex" { "codexModel" }
    "claude" { "claudeCodeModel" }
    "claude-code" { "claudeCodeModel" }
    "gemini" { "geminiModel" }
    "agy" { "geminiModel" }
    "opencode" { "opencodeModel" }
    default { "" }
  }
}

function Has-Arg([string[]]$Args, [string]$Name) {
  return $Args -contains $Name
}

function Has-Prefix-Arg([string[]]$Args, [string]$Prefix) {
  foreach ($arg in $Args) {
    if ($arg.StartsWith($Prefix, [StringComparison]::Ordinal)) { return $true }
  }
  return $false
}

function Has-Option-Value([string[]]$Args, [string[]]$Names) {
  for ($index = 0; $index -lt $Args.Count; $index++) {
    if ($Names -contains $Args[$index]) { return $true }
    foreach ($name in $Names) {
      if ($Args[$index].StartsWith("$name=", [StringComparison]::Ordinal)) { return $true }
    }
  }
  return $false
}

function Codex-Allows-Additional-Writable-Roots([string[]]$Args) {
  for ($index = 0; $index -lt $Args.Count; $index++) {
    $arg = $Args[$index]
    if ($arg -eq "--dangerously-bypass-approvals-and-sandbox" -or $arg -eq "--full-auto") {
      return $true
    }
    if ($arg -eq "--sandbox" -or $arg -eq "-s") {
      if ($index + 1 -lt $Args.Count) {
        $value = $Args[$index + 1]
        if ($value -eq "workspace-write" -or $value -eq "danger-full-access") { return $true }
      }
    }
    if ($arg -eq "--sandbox=workspace-write" -or
        $arg -eq "--sandbox=danger-full-access" -or
        $arg -eq "-sworkspace-write" -or
        $arg -eq "-sdanger-full-access") {
      return $true
    }
  }
  return $false
}

function Codex-Has-Sandbox-Mode-Arg([string[]]$Args) {
  foreach ($arg in $Args) {
    if ($arg -eq "--dangerously-bypass-approvals-and-sandbox" -or
        $arg -eq "--full-auto" -or
        $arg -eq "--sandbox" -or
        $arg -eq "-s" -or
        $arg.StartsWith("--sandbox=", [StringComparison]::Ordinal) -or
        $arg.StartsWith("-s", [StringComparison]::Ordinal)) {
      return $true
    }
  }
  return $false
}

function Has-Config-Key([string[]]$Args, [string]$Key) {
  for ($index = 0; $index -lt $Args.Count; $index++) {
    $arg = $Args[$index]
    if (($arg -eq "-c" -or $arg -eq "--config") -and $index + 1 -lt $Args.Count) {
      if ($Args[$index + 1].StartsWith("$Key=", [StringComparison]::Ordinal)) { return $true }
    }
    if ($arg.StartsWith("-c$Key=", [StringComparison]::Ordinal) -or
        $arg.StartsWith("--config=$Key=", [StringComparison]::Ordinal)) {
      return $true
    }
  }
  return $false
}

function Extract-Model([string[]]$Args) {
  for ($index = 0; $index -lt $Args.Count; $index++) {
    $arg = $Args[$index]
    if (($arg -eq "--model" -or $arg -eq "-m") -and $index + 1 -lt $Args.Count) { return $Args[$index + 1] }
    if ($arg.StartsWith("--model=", [StringComparison]::Ordinal)) { return $arg.Substring("--model=".Length) }
  }
  return ""
}

function Is-Metadata-Invocation([string[]]$CommandArgs) {
  if ($CommandArgs.Count -eq 0) { return $false }
  foreach ($arg in $CommandArgs) {
    switch -Regex ($arg) {
      '^(--version|-V|version)$' { return $true }
      '^(--help|-h|help)$' { return $true }
      '^(features|--features)$' { return $true }
      '^(auth|login|logout|doctor|update|upgrade|config)$' { return $true }
    }
  }
  return $false
}

function Codex-Hooks-Feature-Flag([string]$Binary, [string]$SearchPath) {
  $previousPath = $env:PATH
  try {
    $env:PATH = $SearchPath
    $features = & $Binary features list 2>$null
    if ($features -match "(?m)^hooks\\s") { return "hooks" }
    if ($features -match "(?m)^codex_hooks\\s") { return "codex_hooks" }
  } finally {
    $env:PATH = $previousPath
  }
  return "hooks"
}

function Invoke-Real-Binary([string]$Binary, [string[]]$CommandArgs, [string]$SearchPath, [string]$LaunchDir) {
  $previousPath = $env:PATH
  try {
    $env:PATH = $SearchPath
    if (-not [string]::IsNullOrWhiteSpace($LaunchDir) -and (Test-Path -LiteralPath $LaunchDir)) {
      Push-Location -LiteralPath $LaunchDir
      try {
        & $Binary @CommandArgs
        $script:DMUX_WRAPPER_EXIT_CODE = $LASTEXITCODE
        return
      } finally {
        Pop-Location
      }
    }
    & $Binary @CommandArgs
    $script:DMUX_WRAPPER_EXIT_CODE = $LASTEXITCODE
  } finally {
    $env:PATH = $previousPath
  }
}

$searchPath = Filter-Wrapper-Path $env:PATH
if ([string]::IsNullOrWhiteSpace($searchPath)) {
  $searchPath = Filter-Wrapper-Path $env:DMUX_ORIGINAL_PATH
}

$realBin = Find-Real-Binary $Tool $searchPath
if ([string]::IsNullOrWhiteSpace($realBin)) {
  Write-Live-Log "launch failed tool=$Tool reason=missing-binary"
  Write-Error "$Tool is not installed or not available in PATH."
  exit 127
}

$settings = Read-Tool-Settings
$permissionKey = Tool-Config-Key $Tool
$modelKey = Tool-Model-Key $Tool
$permissionMode = if ($settings -and $permissionKey) { [string]$settings.$permissionKey } else { "" }
$configuredModel = if ($settings -and $modelKey) { [string]$settings.$modelKey } else { "" }
$codexEffort = if ($settings) { [string]$settings.codexEffort } else { "" }

$launchArgs = @($ToolArgs)
if (-not [string]::IsNullOrWhiteSpace($configuredModel) -and -not (Has-Option-Value $launchArgs @("--model", "-m"))) {
  if ($Tool -eq "codex") {
    $launchArgs = @("--model=$configuredModel") + $launchArgs
  } else {
    $launchArgs = @("--model", $configuredModel) + $launchArgs
  }
}

if ($Tool -eq "codex" -and -not [string]::IsNullOrWhiteSpace($codexEffort) -and -not (Has-Option-Value $launchArgs @("-c", "--config"))) {
  $launchArgs = @("-c", "model_reasoning_effort=`"$codexEffort`"") + $launchArgs
}

if ($Tool -eq "codex" -and
    $permissionMode -ne "fullAccess" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH) -and
    (Test-Path -LiteralPath $env:DMUX_PROJECT_PATH) -and
    -not (Codex-Has-Sandbox-Mode-Arg $launchArgs)) {
  $launchArgs = @("--sandbox", "workspace-write") + $launchArgs
}

if ($Tool -eq "codex" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH) -and
    (Test-Path -LiteralPath $env:DMUX_PROJECT_PATH) -and
    ($permissionMode -eq "fullAccess" -or (Codex-Allows-Additional-Writable-Roots $launchArgs)) -and
    -not (Has-Option-Value $launchArgs @("-C", "--cd"))) {
  $launchArgs = @("-C", $env:DMUX_PROJECT_PATH, "--add-dir", $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) + $launchArgs
}

if ($Tool -eq "codex" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not (Has-Config-Key $launchArgs "developer_instructions")) {
  $memoryAgents = Join-Path $env:DMUX_AI_MEMORY_WORKSPACE_ROOT "AGENTS.md"
  if (Test-Path -LiteralPath $memoryAgents) {
    try {
      $content = Get-Content -LiteralPath $memoryAgents -Raw
      if (-not [string]::IsNullOrWhiteSpace($content)) {
        $tomlString = $content | ConvertTo-Json -Compress
        $launchArgs = @("-c", "developer_instructions=$tomlString") + $launchArgs
      }
    } catch {
    }
  }
}

if ($Tool -eq "codex" -and -not (Is-Metadata-Invocation $launchArgs) -and -not (Has-Arg $launchArgs "--enable")) {
  $hooksFeature = Codex-Hooks-Feature-Flag $realBin $searchPath
  $launchArgs = @("--enable", $hooksFeature) + $launchArgs
} else {
  $hooksFeature = ""
}

if ($permissionMode -eq "fullAccess") {
  if ($Tool -eq "codex") {
    if (-not (Has-Arg $launchArgs "--dangerously-bypass-approvals-and-sandbox") -and
        -not (Has-Arg $launchArgs "--full-auto") -and
        -not (Has-Option-Value $launchArgs @("--sandbox", "--ask-for-approval", "-s", "-a"))) {
      $launchArgs = @("--dangerously-bypass-approvals-and-sandbox") + $launchArgs
    }
  } elseif ($Tool -eq "claude" -or $Tool -eq "claude-code") {
    if (-not (Has-Arg $launchArgs "--dangerously-skip-permissions") -and
        -not (Has-Arg $launchArgs "--allow-dangerously-skip-permissions") -and
        -not (Has-Option-Value $launchArgs @("--permission-mode"))) {
      $launchArgs = @("--dangerously-skip-permissions") + $launchArgs
    }
  } elseif ($Tool -eq "gemini" -or $Tool -eq "agy") {
    if (-not (Has-Option-Value $launchArgs @("--approval-mode")) -and
        -not (Has-Arg $launchArgs "--yolo") -and
        -not (Has-Arg $launchArgs "-y")) {
      $launchArgs = @("--approval-mode", "yolo") + $launchArgs
    }
  } elseif ($Tool -eq "opencode") {
    if (-not (Has-Arg $launchArgs "--dangerously-skip-permissions")) {
      $launchArgs = @("--dangerously-skip-permissions") + $launchArgs
    }
  }
}

if (($Tool -eq "claude" -or $Tool -eq "claude-code") -and
    -not (Has-Option-Value $launchArgs @("--append-system-prompt"))) {
  $promptFile = $env:DMUX_AI_MEMORY_PROMPT_FILE
  if (-not [string]::IsNullOrWhiteSpace($promptFile) -and (Test-Path -LiteralPath $promptFile)) {
    $prompt = Get-Content -LiteralPath $promptFile -Raw
    if (-not [string]::IsNullOrWhiteSpace($prompt)) {
      $launchArgs = @("--append-system-prompt", $prompt) + $launchArgs
    }
  }
}

$launchModel = Extract-Model $launchArgs
$env:DMUX_ACTIVE_AI_MODEL = $launchModel

if ($Tool -eq "opencode") {
  $env:OPENCODE_CONFIG_DIR = Join-Path $wrapperDir "opencode-config"
}

$launchDir = ""
if ($Tool -eq "codex") {
  Write-Live-Log "launch codex managed session=$env:DMUX_SESSION_ID project=$env:DMUX_PROJECT_ID binary=$realBin hooks=$hooksFeature"
} else {
  Write-Live-Log "launch managed tool=$Tool session=$env:DMUX_SESSION_ID project=$env:DMUX_PROJECT_ID binary=$realBin"
}
Invoke-Real-Binary $realBin $launchArgs $searchPath $launchDir
$exitCode = if ($null -eq $script:DMUX_WRAPPER_EXIT_CODE) { 0 } else { $script:DMUX_WRAPPER_EXIT_CODE }
Write-Live-Log "process exit tool=$Tool session=$env:DMUX_SESSION_ID code=$exitCode"
exit $exitCode
