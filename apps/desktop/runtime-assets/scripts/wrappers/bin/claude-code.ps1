& (Join-Path $PSScriptRoot "..\tool-wrapper.ps1") "claude-code" @args
exit $LASTEXITCODE
