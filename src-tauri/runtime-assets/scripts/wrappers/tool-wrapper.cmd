@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "TOOL=%~1"
shift /1
set "WRAPPER_DIR=%~dp0"
set "WRAPPER_BIN=%WRAPPER_DIR%bin\"
set "SEARCH_PATH=%PATH%"
call set "SEARCH_PATH=%%SEARCH_PATH:%WRAPPER_BIN%;=%%"
call set "SEARCH_PATH=%%SEARCH_PATH:%WRAPPER_BIN%=%%"

set "REAL_BIN="
for /f "delims=" %%P in ('where "%TOOL%.cmd" 2^>nul') do (
  if not defined REAL_BIN if /I not "%%~dpP"=="%WRAPPER_BIN%" set "REAL_BIN=%%P"
)
if not defined REAL_BIN (
  for /f "delims=" %%P in ('where "%TOOL%.exe" 2^>nul') do (
    if not defined REAL_BIN if /I not "%%~dpP"=="%WRAPPER_BIN%" set "REAL_BIN=%%P"
  )
)
if not defined REAL_BIN (
  echo wrapper: failed to locate real binary for %TOOL% 1>&2
  exit /b 127
)

set "DMUX_ACTIVE_AI_TOOL=%TOOL%"
if "%DMUX_ACTIVE_AI_MODEL%"=="" set "DMUX_ACTIVE_AI_MODEL="
call "%WRAPPER_DIR%dmux-ai-state.cmd" session-start codux-tauri %TOOL%
set "PATH=%SEARCH_PATH%"
call "%REAL_BIN%" %*
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%"=="130" (
  call "%WRAPPER_DIR%dmux-ai-state.cmd" stop codux-tauri %TOOL%
) else (
  call "%WRAPPER_DIR%dmux-ai-state.cmd" stop codux-tauri %TOOL%
)
exit /b %EXIT_CODE%
