use crate::ai_runtime::probe::usage::UsageTotalsFields;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

#[derive(Deserialize)]
pub(super) struct CodexTranscriptRow<'a> {
    #[serde(borrow)]
    pub(super) timestamp: Option<Cow<'a, str>>,
    #[serde(rename = "type", borrow)]
    pub(super) row_type: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) payload: Option<&'a RawValue>,
}

#[derive(Default, Deserialize)]
pub(super) struct CodexPayloadFields<'a> {
    #[serde(rename = "type", borrow)]
    pub(super) payload_type: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) phase: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) role: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) cwd: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) model: Option<Cow<'a, str>>,
    pub(super) started_at: Option<f64>,
    pub(super) completed_at: Option<f64>,
    #[serde(borrow)]
    pub(super) info: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) message: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) text: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) content: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) summary: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) summary_text: Option<&'a RawValue>,
    #[serde(borrow)]
    pub(super) name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub(super) arguments: Option<Cow<'a, str>>,
}

#[derive(Default, Deserialize)]
pub(super) struct CodexTokenInfo {
    pub(super) total_token_usage: Option<UsageTotalsFields>,
    pub(super) last_token_usage: Option<UsageTotalsFields>,
}
