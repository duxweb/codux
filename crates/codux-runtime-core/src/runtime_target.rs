use serde::{Deserialize, Serialize};

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
}
