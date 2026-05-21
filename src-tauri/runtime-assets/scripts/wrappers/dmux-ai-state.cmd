@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "SCRIPT=%~dp0dmux-ai-state.ps1"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT%" %*
exit /b 0
