use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeTarget {
    #[default]
    Local,
    Wsl {
        distribution: String,
    },
    Remote {
        device_id: String,
    },
}

impl RuntimeTarget {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn is_hosted(&self) -> bool {
        !self.is_local()
    }

    pub fn remote_device_id(&self) -> Option<&str> {
        match self {
            Self::Remote { device_id } => Some(device_id),
            Self::Local | Self::Wsl { .. } => None,
        }
    }

    pub fn identity(&self) -> Option<String> {
        match self {
            Self::Local => None,
            Self::Wsl { distribution } => Some(format!("wsl:{distribution}")),
            Self::Remote { device_id } => Some(format!("remote:{device_id}")),
        }
    }

    pub fn paths_equal(&self, left: &str, right: &str) -> bool {
        match self {
            Self::Local => crate::path::local_paths_equal(Path::new(left), Path::new(right)),
            Self::Wsl { .. } | Self::Remote { .. } => crate::path::paths_equal(left, right),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_targets_compare_the_path_platform() {
        let target = RuntimeTarget::Remote {
            device_id: "windows-host".to_string(),
        };
        assert!(target.paths_equal(r"\\?\F:\Projects\Codux", "f:/projects/codux"));
    }

    #[cfg(unix)]
    #[test]
    fn local_targets_follow_the_current_filesystem() {
        assert!(!RuntimeTarget::Local.paths_equal(r"/repo/project\name", "/repo/project/name"));
    }
}
