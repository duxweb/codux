use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSummary {
    pub channel_count: usize,
    pub enabled_count: usize,
    pub configured_count: usize,
    pub channels: Vec<NotificationChannelSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelSummary {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub configured: bool,
    pub endpoint: String,
    pub has_token: bool,
    #[serde(skip_serializing)]
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct NotificationChannelConfig {
    pub id: String,
    pub endpoint: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct NotificationDispatchRequest {
    pub channels: Vec<NotificationChannelConfig>,
    pub title: String,
    pub body: String,
    pub group: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDispatchResult {
    pub sent: usize,
    pub failed: Vec<NotificationChannelFailure>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelFailure {
    pub id: String,
    pub message: String,
}
