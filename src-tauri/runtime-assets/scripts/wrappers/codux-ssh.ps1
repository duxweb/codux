$ErrorActionPreference = "Stop"

$listProfiles = $false
if ($args.Count -ge 1 -and (
    [string]::Equals([string]$args[0], "list", [StringComparison]::OrdinalIgnoreCase) -or
    [string]::Equals([string]$args[0], "--list", [StringComparison]::OrdinalIgnoreCase) -or
    [string]::Equals([string]$args[0], "profiles", [StringComparison]::OrdinalIgnoreCase))) {
  $listProfiles = $true
  if ($args.Count -gt 1) {
    [Console]::Error.WriteLine("usage: codux-ssh list")
    exit 64
  }
} elseif ($args.Count -lt 1 -or [string]::IsNullOrWhiteSpace([string]$args[0])) {
  [Console]::Error.WriteLine("codux-ssh: missing profile id")
  exit 64
}

$profileId = if ($listProfiles) { "" } else { ([string]$args[0]).ToLowerInvariant() }
$remoteArgs = @()
if (-not $listProfiles -and $args.Count -gt 1) {
  if ([string]$args[1] -ne "--") {
    [Console]::Error.WriteLine("usage: codux-ssh <profile-id> [-- <remote-command>] | codux-ssh list")
    exit 64
  }
  if ($args.Count -lt 3) {
    [Console]::Error.WriteLine("codux-ssh: missing remote command after --")
    exit 64
  }
  $remoteArgs = @($args[2..($args.Count - 1)])
}

$profilesFile = $env:CODUX_SSH_PROFILES_FILE
if ([string]::IsNullOrWhiteSpace($profilesFile)) {
  [Console]::Error.WriteLine("codux-ssh: CODUX_SSH_PROFILES_FILE is not set")
  exit 66
}
if (-not (Test-Path -LiteralPath $profilesFile)) {
  [Console]::Error.WriteLine("codux-ssh: unable to read SSH profile file")
  exit 66
}

try {
  $root = Get-Content -LiteralPath $profilesFile -Raw -Encoding UTF8 | ConvertFrom-Json
} catch {
  [Console]::Error.WriteLine("codux-ssh: failed to read SSH profiles: $($_.Exception.Message)")
  exit 66
}

$profiles = @($root)
if ($root.PSObject.Properties.Name -contains "sshProfiles") {
  $profiles = @($root.sshProfiles)
}

if ($listProfiles) {
  $publicProfiles = @()
  foreach ($profile in $profiles) {
    $id = ([string]$profile.id).Trim()
    $hostName = ([string]$profile.host).Trim()
    $user = ([string]$profile.username).Trim()
    if ([string]::IsNullOrWhiteSpace($id) -or [string]::IsNullOrWhiteSpace($hostName) -or [string]::IsNullOrWhiteSpace($user)) {
      continue
    }
    $port = 22
    if ($profile.port -ne $null) {
      $port = [int]$profile.port
    }
    if ($port -lt 1 -or $port -gt 65535) {
      $port = 22
    }
    $name = ([string]$profile.name).Trim()
    if ([string]::IsNullOrWhiteSpace($name)) {
      $name = "$user@$hostName"
    }
    $publicProfiles += [pscustomobject]@{
      id = $id
      name = $name
      host = $hostName
      port = $port
      username = $user
      endpoint = "$user@$hostName`:$port"
      credential = [string]$profile.credentialKind
    }
  }
  [pscustomobject]@{ profiles = $publicProfiles } | ConvertTo-Json -Depth 4
  exit 0
}

$sshProfile = $profiles |
  Where-Object { [string]$_.id -and ([string]$_.id).ToLowerInvariant() -eq $profileId } |
  Select-Object -First 1
if (-not $sshProfile) {
  [Console]::Error.WriteLine("codux-ssh: SSH profile not found")
  exit 67
}

$hostName = ([string]$sshProfile.host).Trim()
$user = ([string]$sshProfile.username).Trim()
if ([string]::IsNullOrWhiteSpace($hostName) -or [string]::IsNullOrWhiteSpace($user)) {
  [Console]::Error.WriteLine("codux-ssh: SSH profile is missing host or username")
  exit 65
}

$port = 22
if ($sshProfile.port -ne $null) {
  $port = [int]$sshProfile.port
}
if ($port -lt 1 -or $port -gt 65535) {
  $port = 22
}

$sshArgs = @("-p", [string]$port)
if ([string]$sshProfile.credentialKind -eq "privateKey" -and [string]$sshProfile.privateKeyPath) {
  $keyPath = [Environment]::ExpandEnvironmentVariables([string]$sshProfile.privateKeyPath)
  if ($keyPath.StartsWith("~/") -or $keyPath.StartsWith("~\")) {
    $keyPath = Join-Path $HOME $keyPath.Substring(2)
  }
  $sshArgs += @("-i", $keyPath)
}
if ([string]$sshProfile.credentialKind -eq "password") {
  Write-Host "codux-ssh: saved password profiles require an interactive SSH prompt on Windows."
}

$sshArgs += "$user@$hostName"
$sshArgs += $remoteArgs
& ssh @sshArgs
exit $LASTEXITCODE
