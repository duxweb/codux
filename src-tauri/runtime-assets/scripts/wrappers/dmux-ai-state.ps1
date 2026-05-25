$ErrorActionPreference = "SilentlyContinue"

$Action = if ($args.Count -ge 1) { [string]$args[0] } else { "" }
$Owner = if ($args.Count -ge 2) { [string]$args[1] } else { "" }
$Tool = if ($args.Count -ge 3) { [string]$args[2] } else { [string]$env:DMUX_ACTIVE_AI_TOOL }
$HookPayload = [Console]::In.ReadToEnd()

if (-not [string]::IsNullOrWhiteSpace($Owner) -and
    -not [string]::IsNullOrWhiteSpace($env:DMUX_RUNTIME_OWNER) -and
    -not [string]::Equals($Owner, $env:DMUX_RUNTIME_OWNER, [StringComparison]::OrdinalIgnoreCase)) {
  exit 0
}

if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_ID) -or
    [string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_ID) -or
    [string]::IsNullOrWhiteSpace($Tool) -or
    [string]::IsNullOrWhiteSpace($env:DMUX_RUNTIME_EVENT_DIR)) {
  exit 0
}

function Write-LiveLog([string]$Category, [string]$Message) {
  if ([string]::IsNullOrWhiteSpace($env:DMUX_LOG_FILE)) { return }
  try {
    $parent = Split-Path -Parent $env:DMUX_LOG_FILE
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
      New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $stamp = Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"
    Add-Content -LiteralPath $env:DMUX_LOG_FILE -Value "[$stamp] [$Category] $Message"
  } catch {
  }
}

function ConvertFrom-HookPayload([string]$Payload) {
  if ([string]::IsNullOrWhiteSpace($Payload)) { return $null }
  try {
    return $Payload.TrimStart([char]0xFEFF) | ConvertFrom-Json -ErrorAction Stop
  } catch {
    return $null
  }
}

function Get-ObjectProperty($Node, [string]$Name) {
  if ($null -eq $Node) { return $null }
  if ($Node -is [System.Collections.IDictionary]) {
    if ($Node.Contains($Name)) { return $Node[$Name] }
    return $null
  }
  $property = $Node.PSObject.Properties[$Name]
  if ($property) { return $property.Value }
  return $null
}

function Get-NodeKey($Node) {
  if ($null -eq $Node) { return "" }
  if ($Node -is [System.ValueType] -or $Node -is [string]) { return "" }
  return [System.Runtime.CompilerServices.RuntimeHelpers]::GetHashCode($Node).ToString()
}

function Get-NodeChildren($Node) {
  if ($null -eq $Node -or $Node -is [string]) { return @() }
  if ($Node -is [System.Collections.IDictionary]) { return @($Node.Values) }
  if ($Node -is [System.Collections.IEnumerable]) { return @($Node) }
  return @($Node.PSObject.Properties | ForEach-Object { $_.Value })
}

function Find-FirstString($Node, [string[]]$Names) {
  if ($null -eq $Node) { return "" }
  $queue = [System.Collections.Generic.Queue[object]]::new()
  $seen = [System.Collections.Generic.HashSet[string]]::new()
  $queue.Enqueue($Node)
  while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    if ($null -eq $current) { continue }
    $key = Get-NodeKey $current
    if ($key -ne "" -and -not $seen.Add($key)) { continue }
    foreach ($name in $Names) {
      $value = Get-ObjectProperty $current $name
      if ($value -is [string] -and -not [string]::IsNullOrWhiteSpace($value)) {
        return $value
      }
    }
    foreach ($child in (Get-NodeChildren $current)) {
      if ($null -ne $child) { $queue.Enqueue($child) }
    }
  }
  return ""
}

function Find-FirstNumber($Node, [string[]]$Names) {
  if ($null -eq $Node) { return $null }
  $queue = [System.Collections.Generic.Queue[object]]::new()
  $seen = [System.Collections.Generic.HashSet[string]]::new()
  $queue.Enqueue($Node)
  while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    if ($null -eq $current) { continue }
    $key = Get-NodeKey $current
    if ($key -ne "" -and -not $seen.Add($key)) { continue }
    foreach ($name in $Names) {
      $value = Get-ObjectProperty $current $name
      if ($value -is [bool]) { continue }
      if ($value -is [int] -or $value -is [long]) { return [int64]$value }
      if ($value -is [double] -or $value -is [decimal]) {
        if ([Math]::Floor([double]$value) -eq [double]$value) { return [int64]$value }
      }
    }
    foreach ($child in (Get-NodeChildren $current)) {
      if ($null -ne $child) { $queue.Enqueue($child) }
    }
  }
  return $null
}

function Resolve-Model($Payload) {
  $model = Find-FirstString $Payload @("model", "model_name", "modelName")
  if (-not [string]::IsNullOrWhiteSpace($model)) { return $model }
  if (-not [string]::IsNullOrWhiteSpace($env:DMUX_ACTIVE_AI_MODEL)) { return $env:DMUX_ACTIVE_AI_MODEL }
  return $null
}

function Resolve-SessionId($Payload) {
  $session = Find-FirstString $Payload @("session_id", "sessionId")
  if (-not [string]::IsNullOrWhiteSpace($session)) { return $session }
  if (-not [string]::IsNullOrWhiteSpace($env:DMUX_EXTERNAL_SESSION_ID)) { return $env:DMUX_EXTERNAL_SESSION_ID }
  return $null
}

function Write-ClaudeMemoryAdditionalContext([string]$HookEventName = "UserPromptSubmit") {
  if ([string]::IsNullOrWhiteSpace($env:DMUX_AI_MEMORY_INDEX_FILE)) { return }
  if (-not (Test-Path -LiteralPath $env:DMUX_AI_MEMORY_INDEX_FILE)) { return }
  try {
    $text = Get-Content -LiteralPath $env:DMUX_AI_MEMORY_INDEX_FILE -Raw -Encoding UTF8
  } catch {
    return
  }
  if ([string]::IsNullOrWhiteSpace($text)) { return }
  $prefix = "Codux memory refresh: the conversation may have been compacted, or this is a new user turn. Re-apply relevant durable memory below. Prefer current user instructions and repository state over stale memory. Memory index file: $env:DMUX_AI_MEMORY_INDEX_FILE`n`n"
  $payload = $prefix + $text.Trim()
  $suffix = "`n[Codux memory refresh truncated]"
  if ($payload.Length -gt 9500) {
    $payload = $payload.Substring(0, 9500 - $suffix.Length) + $suffix
  }
  $response = [ordered]@{
    hookSpecificOutput = [ordered]@{
      hookEventName = $HookEventName
      additionalContext = $payload
    }
    suppressOutput = $true
  }
  $response | ConvertTo-Json -Depth 8 -Compress
}

function Get-EventKind([string]$Value) {
  switch ($Value) {
    "session-start" { return "sessionStarted" }
    "codex-session-start" { return "sessionStarted" }
    "prompt-submit" { return "promptSubmitted" }
    "codex-prompt-submit" { return "promptSubmitted" }
    "before-agent" { return "promptSubmitted" }
    "pre-compact" { return "memoryRefreshing" }
    "post-compact" { return "memoryRefreshing" }
    "permission-request" { return "needsInput" }
    "codex-permission-request" { return "needsInput" }
    "permission-denied" { return "needsInput" }
    "elicitation" { return "needsInput" }
    "elicitation-result" { return "promptSubmitted" }
    "notification" { return "needsInput" }
    "stop" { return "turnCompleted" }
    "stop-failure" { return "turnCompleted" }
    "codex-stop" { return "turnCompleted" }
    "idle" { return "turnCompleted" }
    "after-agent" { return "turnCompleted" }
    "session-end" { return "sessionEnded" }
    "codex-session-end" { return "sessionEnded" }
    default { return "" }
  }
}

function Get-Source([string]$Value, $Payload) {
  switch ($Value) {
    "prompt-submit" { return "user-input" }
    "codex-prompt-submit" { return "user-input" }
    "elicitation-result" { return "user-input" }
    "before-agent" { return "user-input" }
    "pre-compact" { return "pre-compact" }
    "post-compact" { return "post-compact" }
    default {
      $source = Find-FirstString $Payload @("source")
      if (-not [string]::IsNullOrWhiteSpace($source)) { return $source }
      return $null
    }
  }
}

function Get-NotificationType([string]$Value, $Payload) {
  switch ($Value) {
    "permission-request" { return "permission-request" }
    "codex-permission-request" { return "permission-request" }
    "permission-denied" { return "permission-denied" }
    "elicitation" { return "elicitation" }
    "notification" {
      $notification = Find-FirstString $Payload @("notification_type", "notificationType", "type", "kind", "reason")
      if (-not [string]::IsNullOrWhiteSpace($notification)) { return $notification }
      return $null
    }
    default { return $null }
  }
}

function Write-AIHookEvent([string]$Kind, $Payload) {
  $totalTokens = Find-FirstNumber $Payload @("total_tokens", "totalTokenCount", "totalTokens")
  $transcriptPath = Find-FirstString $Payload @("transcript_path", "transcriptPath")
  $model = Resolve-Model $Payload
  $cwd = Find-FirstString $Payload @("cwd", "current_working_directory", "working_directory")
  if ([string]::IsNullOrWhiteSpace($cwd)) { $cwd = [Environment]::CurrentDirectory }
  $reason = Find-FirstString $Payload @("stop_reason", "reason")
  $targetTool = Find-FirstString $Payload @("tool_name", "toolName", "tool")
  $message = Find-FirstString $Payload @("message", "prompt")
  $notification = Get-NotificationType $Action $Payload
  if ($Kind -eq "needsInput" -and [string]::IsNullOrWhiteSpace($reason)) { $reason = $notification }

  $metadata = [ordered]@{
    transcriptPath = if ([string]::IsNullOrWhiteSpace($transcriptPath)) { $null } else { $transcriptPath }
    notificationType = if ([string]::IsNullOrWhiteSpace($notification)) { $null } else { $notification }
    source = Get-Source $Action $Payload
    reason = if ([string]::IsNullOrWhiteSpace($reason)) { $null } else { $reason }
    cwd = if ([string]::IsNullOrWhiteSpace($cwd)) { $null } else { $cwd }
    targetToolName = if ([string]::IsNullOrWhiteSpace($targetTool)) { $null } else { $targetTool }
    message = if ([string]::IsNullOrWhiteSpace($message)) { $null } else { $message }
  }

  $eventPayload = [ordered]@{
    kind = $Kind
    terminalID = $env:DMUX_SESSION_ID
    terminalInstanceID = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_INSTANCE_ID)) { $null } else { $env:DMUX_SESSION_INSTANCE_ID }
    projectID = $env:DMUX_PROJECT_ID
    projectName = if ([string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_NAME)) { "Workspace" } else { $env:DMUX_PROJECT_NAME }
    projectPath = if ([string]::IsNullOrWhiteSpace($env:DMUX_PROJECT_PATH)) { $null } else { $env:DMUX_PROJECT_PATH }
    sessionTitle = if ([string]::IsNullOrWhiteSpace($env:DMUX_SESSION_TITLE)) { "Terminal" } else { $env:DMUX_SESSION_TITLE }
    tool = $Tool
    aiSessionID = Resolve-SessionId $Payload
    model = $model
    totalTokens = $totalTokens
    updatedAt = ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0)
    metadata = $metadata
  }
  $envelope = [ordered]@{
    kind = "ai-hook"
    payload = $eventPayload
  }

  New-Item -ItemType Directory -Force -Path $env:DMUX_RUNTIME_EVENT_DIR | Out-Null
  $name = "{0}-{1}.json" -f ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()), ([guid]::NewGuid().ToString("N"))
  $path = Join-Path $env:DMUX_RUNTIME_EVENT_DIR $name
  $json = $envelope | ConvertTo-Json -Depth 24 -Compress
  $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($path, $json, $utf8NoBom)
  $loggedTranscript = if ([string]::IsNullOrWhiteSpace($transcriptPath)) { "nil" } else { $transcriptPath }
  $loggedModel = if ([string]::IsNullOrWhiteSpace($model)) { "nil" } else { $model }
  $loggedTokens = if ($null -eq $totalTokens) { "nil" } else { $totalTokens }
  Write-LiveLog "hook-file" ("hook written action={0} tool={1} kind={2} file={3} session={4} transcript={5} model={6} tokens={7}" -f $Action, $Tool, $Kind, $name, $env:DMUX_SESSION_ID, $loggedTranscript, $loggedModel, $loggedTokens)
}

$kind = Get-EventKind $Action
if ([string]::IsNullOrWhiteSpace($kind)) {
  Write-LiveLog "hook-file" "skip action=$Action tool=$Tool reason=unsupported"
  exit 0
}

$payload = ConvertFrom-HookPayload $HookPayload
if ($null -eq $payload) {
  $payload = [ordered]@{}
}

Write-AIHookEvent $kind $payload
if (($Tool -eq "claude" -or $Tool -eq "claude-code") -and
    $Action -eq "session-start" -and (Find-FirstString $payload @("source")) -eq "compact") {
  Write-ClaudeMemoryAdditionalContext "SessionStart"
}
exit 0
