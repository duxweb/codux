@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\codux-ssh.ps1" %*
exit /b %ERRORLEVEL%
