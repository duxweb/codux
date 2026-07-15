#[cfg(target_os = "windows")]
use super::install::run_runtime_installer;
use super::{
    WslInstallOperation, WslInstallProgress, WslRuntimeClient,
    discovery::{discover_wsl_distributions, discover_wsl_online_distributions},
    install::{ProgressCallback, install_distribution},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslDistributionStatus {
    pub distribution: String,
    pub display_name: String,
    pub distribution_installed: bool,
    pub runtime: Option<WslRuntimeInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslRuntimeInfo {
    pub version: Option<String>,
    pub protocol_version: Option<u32>,
    pub required_protocol_version: u32,
}

impl WslRuntimeInfo {
    pub fn is_compatible(&self) -> bool {
        self.protocol_version == Some(self.required_protocol_version)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslDistributionCatalog {
    pub distributions: Vec<WslDistributionStatus>,
    pub installed_error: Option<String>,
    pub online_error: Option<String>,
}

#[derive(Default)]
pub struct WslRuntimeManager {
    clients: Mutex<HashMap<String, Arc<WslRuntimeClient>>>,
    distributions: Mutex<Option<Vec<super::WslDistribution>>>,
    runtime_starting: Mutex<()>,
    installing: Mutex<()>,
}

impl WslRuntimeManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn distributions(&self) -> Result<Vec<super::WslDistribution>, String> {
        let mut cached = self
            .distributions
            .lock()
            .map_err(|_| "WSL distribution state is unavailable".to_string())?;
        if let Some(distributions) = cached.as_ref() {
            return Ok(distributions.clone());
        }
        let distributions = discover_wsl_distributions()?;
        *cached = Some(distributions.clone());
        Ok(distributions)
    }

    pub fn client_for(&self, distribution: &str) -> Result<Arc<WslRuntimeClient>, String> {
        let distribution = distribution.trim();
        if distribution.is_empty() {
            return Err("WSL distribution cannot be empty".to_string());
        }
        if let Some(client) = self
            .clients
            .lock()
            .ok()
            .and_then(|clients| clients.get(distribution).cloned())
            .filter(|client| client.is_alive())
        {
            return Ok(client);
        }
        let _starting = self
            .runtime_starting
            .lock()
            .map_err(|_| "WSL runtime startup state is unavailable".to_string())?;
        if let Some(client) = self
            .clients
            .lock()
            .ok()
            .and_then(|clients| clients.get(distribution).cloned())
            .filter(|client| client.is_alive())
        {
            return Ok(client);
        }
        if let Ok(mut clients) = self.clients.lock() {
            clients.remove(distribution);
        }
        require_wsl_sidecar(distribution)?;
        let client = WslRuntimeClient::start(distribution)?;
        self.clients
            .lock()
            .map_err(|_| "WSL runtime manager is unavailable".to_string())?
            .insert(distribution.to_string(), Arc::clone(&client));
        Ok(client)
    }

    pub fn current_client(&self, distribution: &str) -> Option<Arc<WslRuntimeClient>> {
        self.clients
            .lock()
            .ok()
            .and_then(|clients| clients.get(distribution.trim()).cloned())
            .filter(|client| client.is_alive())
    }

    pub fn catalog(&self) -> Result<WslDistributionCatalog, String> {
        let (installed, installed_error) = match discover_wsl_distributions() {
            Ok(installed) => {
                if let Ok(mut cached) = self.distributions.lock() {
                    *cached = Some(installed.clone());
                }
                (installed, None)
            }
            Err(error) => (Vec::new(), Some(error)),
        };
        let (online, online_error) = match discover_wsl_online_distributions() {
            Ok(online) => (online, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        if installed_error.is_some() && online_error.is_some() {
            return Err(format!(
                "Unable to inspect WSL distributions: {}; {}",
                installed_error.as_deref().unwrap_or_default(),
                online_error.as_deref().unwrap_or_default()
            ));
        }
        Ok(WslDistributionCatalog {
            distributions: build_distribution_statuses(installed, online, |distribution| {
                inspect_wsl_sidecar(distribution)
            })?,
            installed_error,
            online_error,
        })
    }

    pub fn install_distribution_with_progress(
        &self,
        distribution: &str,
        version: &str,
        progress: impl Fn(WslInstallProgress) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let distribution = validate_distribution(distribution)?;
        let version = validate_release_version(version)?;
        let _installing = self.acquire_install_lock()?;
        let online = discover_wsl_online_distributions()?;
        if !online
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(distribution))
        {
            return Err("WSL distribution is not available for installation".to_string());
        }
        if discover_wsl_distributions().is_ok_and(|installed| {
            installed
                .iter()
                .any(|installed| installed.name.eq_ignore_ascii_case(distribution))
        }) {
            return Err("WSL distribution is already installed".to_string());
        }
        let progress: ProgressCallback = Arc::new(progress);
        install_distribution(distribution, Arc::clone(&progress))?;
        if let Ok(mut cached) = self.distributions.lock() {
            *cached = None;
        }
        progress(WslInstallProgress {
            distribution: distribution.to_string(),
            operation: WslInstallOperation::Runtime,
            percent: None,
            message: String::new(),
        });
        install_wsl_sidecar(distribution, version, progress)
    }

    pub fn install_runtime_with_progress(
        &self,
        distribution: &str,
        version: &str,
        progress: impl Fn(WslInstallProgress) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let distribution = validate_distribution(distribution)?;
        let version = validate_release_version(version)?;
        let _installing = self.acquire_install_lock()?;
        if let Ok(mut clients) = self.clients.lock() {
            clients.remove(distribution);
        }
        install_wsl_sidecar(distribution, version, Arc::new(progress))
    }

    fn acquire_install_lock(&self) -> Result<MutexGuard<'_, ()>, String> {
        match self.installing.try_lock() {
            Ok(installing) => Ok(installing),
            Err(TryLockError::WouldBlock) => {
                Err("Another WSL installation is already in progress".to_string())
            }
            Err(TryLockError::Poisoned(_)) => Err("WSL installer state is unavailable".to_string()),
        }
    }
}

fn validate_distribution(distribution: &str) -> Result<&str, String> {
    let distribution = distribution.trim();
    if distribution.is_empty()
        || distribution.starts_with('-')
        || distribution.chars().any(char::is_control)
    {
        Err("WSL distribution name is invalid".to_string())
    } else {
        Ok(distribution)
    }
}

fn validate_release_version(version: &str) -> Result<&str, String> {
    let version = version.trim().trim_start_matches('v');
    semver::Version::parse(version)
        .map(|_| version)
        .map_err(|_| "WSL runtime release version is invalid".to_string())
}

fn build_distribution_statuses(
    installed: Vec<super::WslDistribution>,
    online: Vec<super::WslOnlineDistribution>,
    runtime: impl Fn(&str) -> Result<Option<WslRuntimeInfo>, String>,
) -> Result<Vec<WslDistributionStatus>, String> {
    let mut statuses = Vec::new();
    for distribution in &installed {
        let display_name = online
            .iter()
            .find(|online| online.name.eq_ignore_ascii_case(&distribution.name))
            .map(|online| online.display_name.clone())
            .unwrap_or_else(|| distribution.name.clone());
        statuses.push(WslDistributionStatus {
            distribution: distribution.name.clone(),
            display_name,
            distribution_installed: true,
            runtime: runtime(&distribution.name)?,
        });
    }
    for distribution in online {
        if statuses
            .iter()
            .any(|status| status.distribution.eq_ignore_ascii_case(&distribution.name))
        {
            continue;
        }
        statuses.push(WslDistributionStatus {
            distribution: distribution.name,
            display_name: distribution.display_name,
            distribution_installed: false,
            runtime: None,
        });
    }
    Ok(statuses)
}

#[cfg(target_os = "windows")]
fn inspect_wsl_sidecar(distribution: &str) -> Result<Option<WslRuntimeInfo>, String> {
    let output = super::command()
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            &sidecar_probe_script(),
        ])
        .output()
        .map_err(|error| format!("Unable to inspect WSL runtime: {error}"))?;
    Ok(parse_sidecar_version_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(not(target_os = "windows"))]
fn inspect_wsl_sidecar(_distribution: &str) -> Result<Option<WslRuntimeInfo>, String> {
    Ok(None)
}

fn require_wsl_sidecar(distribution: &str) -> Result<(), String> {
    match inspect_wsl_sidecar(distribution)? {
        Some(runtime) if runtime.is_compatible() => Ok(()),
        Some(_) => Err(super::WSL_RUNTIME_PROTOCOL_MISMATCH_ERROR.to_string()),
        None => Err(super::WSL_RUNTIME_NOT_INSTALLED_ERROR.to_string()),
    }
}

#[cfg(target_os = "windows")]
fn install_wsl_sidecar(
    distribution: &str,
    version: &str,
    progress: ProgressCallback,
) -> Result<(), String> {
    run_runtime_installer(distribution, &sidecar_install_script(version), progress)
}

#[cfg(any(target_os = "windows", test))]
fn sidecar_probe_script() -> String {
    "runtime=/usr/local/bin/codux; test -e \"$runtime\" || exit 0; printf '__CODUX_RUNTIME_PRESENT__\\n'; \"$runtime\" version 2>/dev/null || true"
        .to_string()
}

#[cfg(any(target_os = "windows", test))]
fn parse_sidecar_version_output(output: &str) -> Option<WslRuntimeInfo> {
    output
        .lines()
        .any(|line| line.trim() == "__CODUX_RUNTIME_PRESENT__")
        .then(|| WslRuntimeInfo {
            version: output.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("codux ")
                    .map(str::trim)
                    .filter(|version| !version.is_empty())
                    .map(str::to_string)
            }),
            protocol_version: output.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("runtime-stdio ")
                    .and_then(|version| version.trim().parse().ok())
            }),
            required_protocol_version:
                codux_runtime_core::runtime_stdio::RUNTIME_STDIO_PROTOCOL_VERSION,
        })
}

#[cfg(any(target_os = "windows", test))]
fn sidecar_install_script(version: &str) -> String {
    format!(
        r#"set -eu
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) asset=codux-linux-x86_64 ;;
  aarch64|arm64) asset=codux-linux-aarch64 ;;
  *) echo "Unsupported WSL architecture: $arch" >&2; exit 64 ;;
esac
target=/usr/local/bin/codux
dir="$(dirname "$target")"
tmp="$target.tmp"
trap 'rm -f "$tmp"' EXIT
mkdir -p "$dir"
url="https://github.com/duxweb/codux/releases/download/v{version}/$asset"
if command -v curl >/dev/null 2>&1; then
  curl -fL --progress-bar "$url" -o "$tmp"
elif command -v wget >/dev/null 2>&1; then
  wget --progress=bar:force:noscroll -O "$tmp" "$url"
else
  echo "curl or wget is required to install the WSL runtime" >&2
  exit 69
fi
chmod 755 "$tmp"
version_output="$("$tmp" version 2>/dev/null)"
printf '%s\n' "$version_output" | grep -Fxq 'runtime-stdio {}' || {{
  echo "{}" >&2
  exit 65
}}
mv "$tmp" "$target"
trap - EXIT"#,
        codux_runtime_core::runtime_stdio::RUNTIME_STDIO_PROTOCOL_VERSION,
        super::WSL_RUNTIME_PROTOCOL_MISMATCH_ERROR
    )
}

#[cfg(not(target_os = "windows"))]
fn install_wsl_sidecar(
    _distribution: &str,
    _version: &str,
    _progress: ProgressCallback,
) -> Result<(), String> {
    Err("WSL runtimes are available on Windows only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_probe_requires_stdio_protocol() {
        let script = sidecar_probe_script();
        assert!(script.contains("__CODUX_RUNTIME_PRESENT__"));
        assert!(script.contains("\"$runtime\" version"));
        assert!(!script.contains("grep"));
    }

    #[test]
    fn sidecar_version_output_reports_version_and_protocol_compatibility() {
        let current = parse_sidecar_version_output(&format!(
            "__CODUX_RUNTIME_PRESENT__\ncodux 2.0.1\nprotocol v3.2\nruntime-stdio {}\n",
            codux_runtime_core::runtime_stdio::RUNTIME_STDIO_PROTOCOL_VERSION
        ))
        .expect("installed runtime");
        assert_eq!(current.version.as_deref(), Some("2.0.1"));
        assert!(current.is_compatible());

        let old = parse_sidecar_version_output(
            "__CODUX_RUNTIME_PRESENT__\ncodux 2.0.0\nprotocol v3.2\nruntime-stdio 2\n",
        )
        .expect("old runtime");
        assert_eq!(old.protocol_version, Some(2));
        assert!(!old.is_compatible());

        assert_eq!(parse_sidecar_version_output(""), None);
    }

    #[test]
    fn sidecar_install_uses_exact_release_and_atomic_move() {
        let script = sidecar_install_script("2.0.0-rc.10");
        assert!(script.contains("releases/download/v2.0.0-rc.10/$asset"));
        assert!(script.contains("trap 'rm -f \"$tmp\"' EXIT"));
        assert!(script.contains("mv \"$tmp\" \"$target\""));
        assert!(script.contains("target=/usr/local/bin/codux"));
        assert!(script.contains(crate::wsl::WSL_RUNTIME_PROTOCOL_MISMATCH_ERROR));
        assert!(!script.contains("releases/latest"));
    }

    #[test]
    fn distribution_catalog_merges_installed_and_online_entries() {
        let statuses = build_distribution_statuses(
            vec![super::super::WslDistribution {
                name: "Ubuntu".to_string(),
            }],
            vec![
                super::super::WslOnlineDistribution {
                    name: "Ubuntu".to_string(),
                    display_name: "Ubuntu".to_string(),
                },
                super::super::WslOnlineDistribution {
                    name: "Debian".to_string(),
                    display_name: "Debian GNU/Linux".to_string(),
                },
            ],
            |distribution| {
                Ok((distribution == "Ubuntu").then_some(WslRuntimeInfo {
                    version: Some("2.0.1".to_string()),
                    protocol_version: Some(
                        codux_runtime_core::runtime_stdio::RUNTIME_STDIO_PROTOCOL_VERSION,
                    ),
                    required_protocol_version:
                        codux_runtime_core::runtime_stdio::RUNTIME_STDIO_PROTOCOL_VERSION,
                }))
            },
        )
        .unwrap();

        assert_eq!(statuses.len(), 2);
        assert!(statuses[0].distribution_installed);
        assert!(
            statuses[0]
                .runtime
                .as_ref()
                .is_some_and(WslRuntimeInfo::is_compatible)
        );
        assert_eq!(statuses[1].display_name, "Debian GNU/Linux");
        assert!(!statuses[1].distribution_installed);
    }

    #[test]
    fn rejects_distribution_names_that_can_be_parsed_as_options() {
        assert_eq!(
            validate_distribution("--no-launch").unwrap_err(),
            "WSL distribution name is invalid"
        );
        assert_eq!(
            validate_distribution("Debian\n--no-launch").unwrap_err(),
            "WSL distribution name is invalid"
        );
    }

    #[test]
    fn rejects_invalid_release_versions() {
        assert!(validate_release_version("2.0.0-rc.10").is_ok());
        assert!(validate_release_version("v2.0.0").is_ok());
        assert_eq!(
            validate_release_version("../../latest").unwrap_err(),
            "WSL runtime release version is invalid"
        );
    }

    #[test]
    fn concurrent_install_attempt_fails_without_waiting() {
        let manager = WslRuntimeManager::new();
        let _installing = manager.installing.lock().unwrap();

        assert_eq!(
            manager.acquire_install_lock().unwrap_err(),
            "Another WSL installation is already in progress"
        );
    }
}
