# Windows PowerShell cannot parse \\?\-verbatim paths in Join-Path; normalize first.
$root = $PSScriptRoot
if (-not $root) { $root = Split-Path -Parent $MyInvocation.MyCommand.Path }
if ($root.StartsWith('\\?\UNC\')) { $root = '\\' + $root.Substring(8) }
elseif ($root.StartsWith('\\?\')) { $root = $root.Substring(4) }
& (Join-Path $root "..\tool-wrapper.ps1") "omp" @args
exit $LASTEXITCODE
