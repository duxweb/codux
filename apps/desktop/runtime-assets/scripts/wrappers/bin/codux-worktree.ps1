$root = $PSScriptRoot
if (-not $root) { $root = Split-Path -Parent $MyInvocation.MyCommand.Path }
if ($root.StartsWith('\\?\UNC\')) { $root = '\\' + $root.Substring(8) }
elseif ($root.StartsWith('\\?\')) { $root = $root.Substring(4) }
$helper = Join-Path $root "..\codux-wrapper-helper.exe"
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) {
  Write-Error "codux-worktree: bundled helper is missing"
  exit 127
}
& $helper --codux-wrapper-helper agent-worktree @args
exit $LASTEXITCODE
