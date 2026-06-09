use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSummary {
    pub mode: String,
    pub effective_enabled: bool,
    pub power_adapter_connected: Option<bool>,
    pub assertion_active: bool,
    pub error: Option<String>,
}
