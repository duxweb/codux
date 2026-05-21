@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ACTION=%~1"
set "OWNER=%~2"
set "TOOL=%~3"
if "%TOOL%"=="" set "TOOL=%DMUX_ACTIVE_AI_TOOL%"
if "%DMUX_RUNTIME_OWNER%" neq "" if "%OWNER%" neq "" if /I not "%DMUX_RUNTIME_OWNER%"=="%OWNER%" exit /b 0
if "%DMUX_SESSION_ID%"=="" exit /b 0
if "%DMUX_PROJECT_ID%"=="" exit /b 0
if "%TOOL%"=="" exit /b 0
if "%DMUX_RUNTIME_EVENT_DIR%"=="" exit /b 0

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='SilentlyContinue';" ^
  "function Write-LiveLog([string]$Message) { if (-not [string]::IsNullOrWhiteSpace($env:DMUX_LOG_FILE)) { $parent=Split-Path -Parent $env:DMUX_LOG_FILE; if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }; $stamp=Get-Date -Format 'yyyy-MM-ddTHH:mm:sszzz'; Add-Content -LiteralPath $env:DMUX_LOG_FILE -Value ('[' + $stamp + '] [hook-file] ' + $Message) } };" ^
  "$dir=$env:DMUX_RUNTIME_EVENT_DIR; New-Item -ItemType Directory -Force -Path $dir | Out-Null;" ^
  "$kind = switch ('%ACTION%') { 'session-start' {'sessionStarted'} 'codex-session-start' {'sessionStarted'} 'prompt-submit' {'promptSubmitted'} 'codex-prompt-submit' {'promptSubmitted'} 'permission-request' {'needsInput'} 'codex-permission-request' {'needsInput'} 'stop' {'turnCompleted'} 'codex-stop' {'turnCompleted'} 'session-end' {'sessionEnded'} default { $null } };" ^
  "if (-not $kind) { Write-LiveLog 'skip action=%ACTION% tool=%TOOL% reason=unsupported'; exit 0 };" ^
  "$meta = @{ transcriptPath=$null; notificationType=$null; source=$null; reason='%ACTION%'; cwd=$env:PWD; targetToolName=$null; message=$null };" ^
  "if ($kind -eq 'needsInput') { $meta.notificationType='permission-request' };" ^
  "$projectName = if ($env:DMUX_PROJECT_NAME) { $env:DMUX_PROJECT_NAME } else { 'Workspace' };" ^
  "$sessionTitle = if ($env:DMUX_SESSION_TITLE) { $env:DMUX_SESSION_TITLE } else { 'Terminal' };" ^
  "$payload = @{ kind=$kind; terminalID=$env:DMUX_SESSION_ID; terminalInstanceID=$env:DMUX_SESSION_INSTANCE_ID; projectID=$env:DMUX_PROJECT_ID; projectName=$projectName; projectPath=$env:DMUX_PROJECT_PATH; sessionTitle=$sessionTitle; tool='%TOOL%'; aiSessionID=$env:DMUX_EXTERNAL_SESSION_ID; model=$env:DMUX_ACTIVE_AI_MODEL; totalTokens=$null; updatedAt=([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()/1000.0); metadata=$meta };" ^
  "$envelope = @{ kind='ai-hook'; payload=$payload } | ConvertTo-Json -Depth 8 -Compress;" ^
  "$name = ('{0}-{1}.json' -f ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()), ([guid]::NewGuid().ToString('N')));" ^
  "$path=Join-Path $dir $name; $utf8NoBom = New-Object System.Text.UTF8Encoding($false); [System.IO.File]::WriteAllText($path, $envelope, $utf8NoBom);" ^
  "Write-LiveLog ('hook written action=%ACTION% tool=%TOOL% kind=' + $kind + ' file=' + $name + ' session=' + $env:DMUX_SESSION_ID)"
exit /b 0
