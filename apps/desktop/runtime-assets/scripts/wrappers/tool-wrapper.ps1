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
    $normalized = (Normalize-Directory $part).Replace('\', '/')
    if ($normalized.EndsWith("/runtime-root/scripts/wrappers/bin", [StringComparison]::OrdinalIgnoreCase)) { continue }
    $filtered += $part
  }
  return Join-PathList $filtered
}

function Find-Real-Binary([string]$Name, [string]$SearchPath) {
  $previousPath = $env:PATH
  try {
    $env:PATH = $SearchPath
    $candidateNames = switch ($Name) {
      "claude" { @("claude.ps1", "claude.exe", "claude-code.ps1", "claude-code.exe", "reclaude.ps1", "reclaude.exe"); break }
      "claude-code" { @("claude-code.ps1", "claude-code.exe", "claude.ps1", "claude.exe", "reclaude.ps1", "reclaude.exe"); break }
      "reclaude" { @("reclaude.ps1", "reclaude.exe", "claude.ps1", "claude.exe", "claude-code.ps1", "claude-code.exe"); break }
      default { @("$Name.ps1", "$Name.exe") }
    }
    foreach ($candidate in $candidateNames) {
      $commands = @(Get-Command $candidate -CommandType Application,ExternalScript -ErrorAction SilentlyContinue)
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

function Tool-Memory-Injection-Strategy([string]$Name) {
  $path = Join-Path $wrapperDir "tool-drivers.json"
  if (-not (Test-Path -LiteralPath $path)) { return "" }
  try {
    $payload = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    foreach ($driver in @($payload.tools)) {
      $aliases = @($driver.aliases | ForEach-Object { [string]$_ })
      if ($aliases -contains $Name.ToLowerInvariant()) {
        return [string]$driver.memoryInjection
      }
    }
  } catch {
  }
  return ""
}

function Apply-Managed-Lifecycle-Env([string]$Name) {
  $path = Join-Path $wrapperDir "managed-env\$Name.ps1"
  if (Test-Path -LiteralPath $path) {
    . $path
    Write-Live-Log "managed lifecycle env tool=$Name path=$path"
  }
}

function Tool-Config-Key([string]$Name) {
  switch ($Name) {
    "codex" { "codex" }
    "claude" { "claudeCode" }
    "claude-code" { "claudeCode" }
    "reclaude" { "claudeCode" }
    "agy" { "agy" }
    "kimi" { "kimi" }
    "kimi-code" { "kimi" }
    "opencode" { "opencode" }
    "mimo" { "mimo" }
    "kiro-cli" { "kiro" }
    "codewhale" { "codewhale" }
    default { "" }
  }
}

function Tool-Model-Key([string]$Name) {
  switch ($Name) {
    "codex" { "codexModel" }
    "claude" { "claudeCodeModel" }
    "claude-code" { "claudeCodeModel" }
    "reclaude" { "claudeCodeModel" }
    "agy" { "agyModel" }
    "kimi" { "kimiModel" }
    "kimi-code" { "kimiModel" }
    "opencode" { "opencodeModel" }
    "mimo" { "mimoModel" }
    "kiro-cli" { "kiroModel" }
    "codewhale" { "codewhaleModel" }
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

function Extract-Resume-Target([string[]]$Args) {
  for ($index = 0; $index -lt $Args.Count; $index++) {
    $arg = $Args[$index]
    if (($arg -eq "--resume" -or
          $arg -eq "-r" -or
          $arg -eq "--resume-id" -or
          $arg -eq "--session" -or
          $arg -eq "--session-id") -and
        $index + 1 -lt $Args.Count -and
        -not $Args[$index + 1].StartsWith("-", [StringComparison]::Ordinal)) {
      return $Args[$index + 1]
    }
    if ($arg -eq "resume" -and
        $index + 1 -lt $Args.Count -and
        -not $Args[$index + 1].StartsWith("-", [StringComparison]::Ordinal)) {
      return $Args[$index + 1]
    }
    if ($arg.StartsWith("--resume=", [StringComparison]::Ordinal)) { return $arg.Substring("--resume=".Length) }
    if ($arg.StartsWith("--resume-id=", [StringComparison]::Ordinal)) { return $arg.Substring("--resume-id=".Length) }
    if ($arg.StartsWith("--session=", [StringComparison]::Ordinal)) { return $arg.Substring("--session=".Length) }
    if ($arg.StartsWith("--session-id=", [StringComparison]::Ordinal)) { return $arg.Substring("--session-id=".Length) }
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

function Codex-Profile-Name([string]$Seed) {
  if ([string]::IsNullOrWhiteSpace($Seed)) {
    $Seed = [Guid]::NewGuid().ToString("N")
  }
  try {
    $bytes = [Text.Encoding]::UTF8.GetBytes($Seed)
    $hash = [Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
    $hex = -join ($hash | ForEach-Object { $_.ToString("x2") })
    return "codux-runtime-$($hex.Substring(0, 16))"
  } catch {
    return "codux-runtime-$([Guid]::NewGuid().ToString("N").Substring(0, 16))"
  }
}

function Write-Codex-Developer-Instructions-Profile([string]$Content, [string]$Seed) {
  if ([string]::IsNullOrWhiteSpace($Content)) { return "" }
  try {
    $codexHome = $env:CODEX_HOME
    if ([string]::IsNullOrWhiteSpace($codexHome)) {
      $codexHome = Join-Path $env:USERPROFILE ".codex"
    }
    New-Item -ItemType Directory -Force -Path $codexHome | Out-Null
    $profileName = Codex-Profile-Name $Seed
    $profilePath = Join-Path $codexHome "$profileName.config.toml"
    $tomlString = $Content | ConvertTo-Json -Compress
    Set-Content -LiteralPath $profilePath -Value "developer_instructions = $tomlString" -Encoding UTF8
    return $profileName
  } catch {
    Write-Live-Log "failed to write codex developer instructions profile: $($_.Exception.Message)"
    return ""
  }
}

function Get-Memory-Prompt-File {
  $promptFile = $env:DMUX_AI_MEMORY_PROMPT_FILE
  if ([string]::IsNullOrWhiteSpace($promptFile) -or -not (Test-Path -LiteralPath $promptFile)) {
    return ""
  }
  return $promptFile
}

function Apply-Kimi-Memory-Agent-File([string[]]$Args) {
  if ($memoryInjectionStrategy -ne "kimiAgentFile") { return $Args }
  if ($Args.Count -gt 0) {
    switch -Regex ($Args[0]) {
      '^(login|logout|info|export|mcp|plugin|vis|web|term|acp|__background-task-worker|__web-worker)$' {
        Write-Live-Log "kimi instructions skipped: subcommand=$($Args[0])"
        return $Args
      }
    }
  }
  if ($Args.Count -gt 0 -and ($Args[0] -eq "--help" -or $Args[0] -eq "-h" -or $Args[0] -eq "--version" -or $Args[0] -eq "-V")) {
    Write-Live-Log "kimi instructions skipped: metadata invocation"
    return $Args
  }
  if ((Has-Option-Value $Args @("--agent-file")) -or (Has-Option-Value $Args @("--agent"))) {
    Write-Live-Log "kimi instructions skipped: agent override already provided"
    return $Args
  }
  $promptFile = Get-Memory-Prompt-File
  if ([string]::IsNullOrWhiteSpace($promptFile)) {
    Write-Live-Log "kimi instructions skipped: prompt file missing"
    return $Args
  }
  try {
    $prompt = Get-Content -LiteralPath $promptFile -Raw
    if ([string]::IsNullOrWhiteSpace($prompt)) {
      Write-Live-Log "kimi instructions skipped: prompt empty path=$promptFile"
      return $Args
    }
    $agentKey = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_ID)) { "default" } else { $env:DMUX_SESSION_ID }
    $agentKey = [Regex]::Replace($agentKey, "[^A-Za-z0-9_.-]", "_")
    $agentDir = Join-Path (Join-Path $wrapperDir "managed-kimi-agent") $agentKey
    $agentFile = Join-Path $agentDir "agent.yaml"
    New-Item -ItemType Directory -Force -Path $agentDir | Out-Null
    $lines = @(
      "version: 1",
      "agent:",
      "  extend: default",
      "  name: `"`"",
      "  system_prompt_args:",
      "    ROLE_ADDITIONAL: |"
    )
    foreach ($line in ($prompt -split "`r?`n", -1)) {
      $lines += "      $line"
    }
    [System.IO.File]::WriteAllText($agentFile, ($lines -join "`n"), [System.Text.UTF8Encoding]::new($false))
    Write-Live-Log "kimi instructions injected path=$promptFile agent=$agentFile chars=$($prompt.Length)"
    return @("--agent-file", $agentFile) + $Args
  } catch {
    Write-Live-Log "kimi instructions skipped: failed to write agent file error=$($_.Exception.Message)"
    return $Args
  }
}

function Apply-Append-System-Prompt([string[]]$Args, [string]$Strategy, [string]$Label) {
  if ($memoryInjectionStrategy -ne $Strategy) { return $Args }
  if (Has-Option-Value $Args @("--append-system-prompt")) {
    Write-Live-Log "$Label instructions skipped: append-system-prompt already provided"
    return $Args
  }
  $promptFile = Get-Memory-Prompt-File
  if ([string]::IsNullOrWhiteSpace($promptFile)) {
    Write-Live-Log "$Label instructions skipped: prompt file missing"
    return $Args
  }
  try {
    $prompt = Get-Content -LiteralPath $promptFile -Raw
    if ([string]::IsNullOrWhiteSpace($prompt)) {
      Write-Live-Log "$Label instructions skipped: prompt empty path=$promptFile"
      return $Args
    }
    Write-Live-Log "$Label instructions injected path=$promptFile chars=$($prompt.Length)"
    return @("--append-system-prompt", $prompt) + $Args
  } catch {
    Write-Live-Log "$Label instructions skipped: failed to read prompt path=$promptFile error=$($_.Exception.Message)"
    return $Args
  }
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

function Emit-Wrapper-SessionEnd {
  if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_ID) -or
      [string]::IsNullOrWhiteSpace($env:DMUX_RUNTIME_EVENT_DIR)) {
    return
  }
  $helper = Join-Path $wrapperDir "dmux-ai-state.ps1"
  if (-not (Test-Path -LiteralPath $helper)) { return }
  try {
    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $helper "session-end" "$env:DMUX_RUNTIME_OWNER" "$Tool" *> $null
  } catch {
  }
}

function Write-Runtime-Binding([string]$ExternalSessionId, [string]$Model, [string]$TranscriptPath) {
  if ([string]::IsNullOrWhiteSpace($env:DMUX_AI_RUNTIME_BINDING_DIR) -or
      [string]::IsNullOrWhiteSpace($env:DMUX_SESSION_ID) -or
      [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_ID) -or
      [string]::IsNullOrWhiteSpace($Tool)) {
    return
  }
  try {
    New-Item -ItemType Directory -Force -Path $env:DMUX_AI_RUNTIME_BINDING_DIR | Out-Null
    $bindingIdSeed = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_INSTANCE_ID)) { $env:DMUX_SESSION_ID } else { $env:DMUX_SESSION_INSTANCE_ID }
    $payload = [ordered]@{
      runtimeBindingId = "$bindingIdSeed-$Tool"
      terminalId = $env:DMUX_SESSION_ID
      terminalInstanceId = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_INSTANCE_ID)) { $null } else { $env:DMUX_SESSION_INSTANCE_ID }
      tool = $Tool
      projectId = $env:DMUX_PROJECT_ID
      projectName = if ([string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_NAME)) { "Workspace" } else { $env:DMUX_PROJECT_NAME }
      projectPath = if ([string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH)) { $null } else { $env:DMUX_PROJECT_PATH }
      sessionTitle = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_TITLE)) { "Terminal" } else { $env:DMUX_SESSION_TITLE }
      launchStartedAt = ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0)
      externalSessionId = if ([string]::IsNullOrWhiteSpace($ExternalSessionId)) { $null } else { $ExternalSessionId }
      transcriptPath = if ([string]::IsNullOrWhiteSpace($TranscriptPath)) { $null } else { $TranscriptPath }
      model = if ([string]::IsNullOrWhiteSpace($Model)) { $null } else { $Model }
      updatedAt = ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0)
    }
    $path = Join-Path $env:DMUX_AI_RUNTIME_BINDING_DIR "$($env:DMUX_SESSION_ID)-$Tool.json"
    $tmp = "$path.tmp"
    $json = $payload | ConvertTo-Json -Depth 8 -Compress
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($tmp, $json, $utf8NoBom)
    Move-Item -Force -LiteralPath $tmp -Destination $path
  } catch {
    Write-Live-Log "failed to write runtime binding: $($_.Exception.Message)"
  }
}

$searchPath = Filter-Wrapper-Path $env:PATH
if ([string]::IsNullOrWhiteSpace($searchPath)) {
  $searchPath = Filter-Wrapper-Path $env:DMUX_ORIGINAL_PATH
}
$runtimePath = Join-PathList @($wrapperBin, $searchPath)

$realBin = Find-Real-Binary $Tool $searchPath
if ([string]::IsNullOrWhiteSpace($realBin)) {
  Write-Live-Log "launch failed tool=$Tool reason=missing-binary"
  Write-Error "$Tool is not installed or not available in PATH."
  exit 127
}

# Seed the console's reported default colors (OSC 10/11 set) with the app
# theme: ConPTY answers color queries itself from its own black palette, so
# TUIs would detect a dark background under a light theme.
if (-not [Console]::IsOutputRedirected) {
  $esc = [char]27
  if (-not [string]::IsNullOrWhiteSpace($env:DMUX_TERMINAL_OSC_FG)) {
    [Console]::Out.Write("$esc]10;$($env:DMUX_TERMINAL_OSC_FG)$esc\")
  }
  if (-not [string]::IsNullOrWhiteSpace($env:DMUX_TERMINAL_OSC_BG)) {
    [Console]::Out.Write("$esc]11;$($env:DMUX_TERMINAL_OSC_BG)$esc\")
  }
}

$settings = Read-Tool-Settings
$memoryInjectionStrategy = Tool-Memory-Injection-Strategy $Tool
$permissionKey = Tool-Config-Key $Tool
$modelKey = Tool-Model-Key $Tool
$permissionMode = if ($settings -and $permissionKey) { [string]$settings.$permissionKey } else { "" }
$configuredModel = if ($settings -and $modelKey) { [string]$settings.$modelKey } else { "" }
$codexEffort = if ($settings) { [string]$settings.codexEffort } else { "" }
if ($codexEffort -notin @("minimal", "low", "medium", "high", "xhigh")) {
  $codexEffort = ""
}

$launchArgs = @($ToolArgs)
if ($Tool -ne "kiro-cli" -and -not [string]::IsNullOrWhiteSpace($configuredModel) -and -not (Has-Option-Value $launchArgs @("--model", "-m"))) {
  if ($Tool -eq "codex") {
    $launchArgs = @("--model=$configuredModel") + $launchArgs
  } else {
    $launchArgs = @("--model", $configuredModel) + $launchArgs
  }
}

if ($Tool -eq "codex" -and -not [string]::IsNullOrWhiteSpace($codexEffort) -and -not (Has-Config-Key $launchArgs "model_reasoning_effort")) {
  $launchArgs = @("-c", "model_reasoning_effort=`"$codexEffort`"") + $launchArgs
}

if ($memoryInjectionStrategy -eq "codexDeveloperInstructions" -and
    $permissionMode -ne "fullAccess" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH) -and
    (Test-Path -LiteralPath $env:DMUX_PROJECT_PATH) -and
    -not (Codex-Has-Sandbox-Mode-Arg $launchArgs)) {
  $launchArgs = @("--sandbox", "workspace-write") + $launchArgs
}

if ($memoryInjectionStrategy -eq "codexDeveloperInstructions" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH) -and
    (Test-Path -LiteralPath $env:DMUX_PROJECT_PATH) -and
    ($permissionMode -eq "fullAccess" -or (Codex-Allows-Additional-Writable-Roots $launchArgs)) -and
    -not (Has-Option-Value $launchArgs @("-C", "--cd"))) {
  $launchArgs = @("-C", $env:DMUX_PROJECT_PATH, "--add-dir", $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) + $launchArgs
}

if ($memoryInjectionStrategy -eq "codexDeveloperInstructions" -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_WORKSPACE_ROOT) -and
    -not (Has-Config-Key $launchArgs "developer_instructions")) {
  $memoryAgents = Join-Path $env:DMUX_AI_MEMORY_WORKSPACE_ROOT "AGENTS.md"
  if (Test-Path -LiteralPath $memoryAgents) {
    try {
      $content = Get-Content -LiteralPath $memoryAgents -Raw
      if (-not [string]::IsNullOrWhiteSpace($content)) {
        $profileName = Write-Codex-Developer-Instructions-Profile $content "$env:DMUX_SESSION_ID|$memoryAgents"
        if (-not [string]::IsNullOrWhiteSpace($profileName) -and -not (Has-Option-Value $launchArgs @("--profile", "--profile-v2"))) {
          $launchArgs = @("--profile", $profileName) + $launchArgs
        } else {
          $tomlString = $content | ConvertTo-Json -Compress
          $launchArgs = @("-c", "developer_instructions=$tomlString") + $launchArgs
        }
        Write-Live-Log "codex instructions injected path=$memoryAgents chars=$($content.Length)"
      } else {
        Write-Live-Log "codex instructions skipped: AGENTS.md empty path=$memoryAgents"
      }
    } catch {
      Write-Live-Log "codex instructions skipped: failed to read AGENTS.md path=$memoryAgents error=$($_.Exception.Message)"
    }
  } else {
    Write-Live-Log "codex instructions skipped: AGENTS.md missing path=$memoryAgents"
  }
} elseif ($memoryInjectionStrategy -eq "codexDeveloperInstructions") {
  if (Has-Config-Key $launchArgs "developer_instructions") {
    Write-Live-Log "codex instructions skipped: developer_instructions already provided"
  } else {
    Write-Live-Log "codex instructions skipped: memory workspace missing"
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
  } elseif ($Tool -eq "claude" -or $Tool -eq "claude-code" -or $Tool -eq "reclaude") {
    if (-not (Has-Arg $launchArgs "--dangerously-skip-permissions") -and
        -not (Has-Arg $launchArgs "--allow-dangerously-skip-permissions") -and
        -not (Has-Option-Value $launchArgs @("--permission-mode"))) {
      $launchArgs = @("--dangerously-skip-permissions") + $launchArgs
    }
  } elseif ($Tool -eq "agy") {
    if (-not (Has-Option-Value $launchArgs @("--approval-mode")) -and
        -not (Has-Arg $launchArgs "--yolo") -and
        -not (Has-Arg $launchArgs "-y")) {
      $launchArgs = @("--approval-mode", "yolo") + $launchArgs
    }
  } elseif ($Tool -eq "kimi" -or $Tool -eq "kimi-code") {
    # Kimi Code uses the generic model flag, but its permission flags differ from agy.
  } elseif ($Tool -eq "opencode" -or $Tool -eq "mimo") {
    if (-not (Has-Arg $launchArgs "--dangerously-skip-permissions")) {
      $launchArgs = @("--dangerously-skip-permissions") + $launchArgs
    }
  } elseif ($Tool -eq "codewhale") {
    if (-not (Has-Arg $launchArgs "--yolo")) {
      $launchArgs = @("--yolo") + $launchArgs
    }
  }
}

if ($Tool -eq "kimi" -or $Tool -eq "kimi-code") {
  $launchArgs = Apply-Kimi-Memory-Agent-File $launchArgs
}

if ($memoryInjectionStrategy -eq "claudeAppendSystemPrompt" -and
    -not (Has-Option-Value $launchArgs @("--append-system-prompt"))) {
  $promptFile = $env:DMUX_AI_MEMORY_PROMPT_FILE
  if (-not [string]::IsNullOrWhiteSpace($promptFile) -and (Test-Path -LiteralPath $promptFile)) {
    $prompt = Get-Content -LiteralPath $promptFile -Raw
    if (-not [string]::IsNullOrWhiteSpace($prompt)) {
      $launchArgs = @("--append-system-prompt", $prompt) + $launchArgs
      Write-Live-Log "claude instructions injected path=$promptFile chars=$($prompt.Length)"
    } else {
      Write-Live-Log "claude instructions skipped: prompt empty path=$promptFile"
    }
  } else {
    Write-Live-Log "claude instructions skipped: prompt file missing"
  }
} elseif ($memoryInjectionStrategy -eq "claudeAppendSystemPrompt") {
  Write-Live-Log "claude instructions skipped: append-system-prompt already provided"
}

$launchModel = if ($Tool -eq "kiro-cli") { $configuredModel } else { Extract-Model $launchArgs }
$resumeTarget = Extract-Resume-Target $launchArgs
$bindingExternalSessionId = if (-not [string]::IsNullOrWhiteSpace($resumeTarget)) { $resumeTarget } else { $env:DMUX_EXTERNAL_SESSION_ID }
$env:DMUX_ACTIVE_AI_MODEL = $launchModel
if (-not [string]::IsNullOrWhiteSpace($resumeTarget)) {
  $env:DMUX_EXTERNAL_SESSION_ID = $resumeTarget
}

if ($Tool -eq "opencode" -or $Tool -eq "mimo") {
  $openCodeConfigDir = Join-Path $wrapperDir "opencode-config"
  if ($Tool -eq "mimo") {
    $env:XDG_CONFIG_HOME = Join-Path $openCodeConfigDir "xdg"
  } else {
    $env:OPENCODE_CONFIG_DIR = $openCodeConfigDir
  }
  $env:DMUX_ACTIVE_AI_TOOL = $Tool
}

Apply-Managed-Lifecycle-Env $Tool

$launchDir = ""
Write-Runtime-Binding $bindingExternalSessionId $launchModel ""
Invoke-Real-Binary $realBin $launchArgs $runtimePath $launchDir
$exitCode = if ($null -eq $script:DMUX_WRAPPER_EXIT_CODE) { 0 } else { $script:DMUX_WRAPPER_EXIT_CODE }
Emit-Wrapper-SessionEnd
exit $exitCode
