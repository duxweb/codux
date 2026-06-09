#[cfg(test)]
mod tests;
mod types;

use serde::Serialize;
use serde_json::{Map, Value};
use std::{path::PathBuf, time::Duration};

pub use types::*;

const KNOWN_CHANNELS: &[(&str, &str)] = &[
    ("bark", "Bark"),
    ("ntfy", "ntfy"),
    ("wxpusher", "WxPusher"),
    ("feishu", "Feishu"),
    ("dingtalk", "DingTalk"),
    ("wecom", "WeCom"),
    ("telegram", "Telegram"),
    ("discord", "Discord"),
    ("slack", "Slack"),
    ("webhook", "Webhook"),
];

include!("notification/payloads.rs");
include!("notification/settings.rs");
include!("notification/dispatch.rs");
include!("notification/native.rs");
include!("notification/service.rs");
