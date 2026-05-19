use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelConfig {
    pub id: String,
    pub endpoint: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDispatchRequest {
    pub channels: Vec<NotificationChannelConfig>,
    pub title: String,
    pub body: String,
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDispatchResult {
    pub sent: usize,
    pub failed: Vec<NotificationChannelFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelFailure {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenericNotificationPayload<'a> {
    title: &'a str,
    body: &'a str,
    group: &'a str,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct FeishuMessage<'a> {
    msg_type: &'static str,
    content: FeishuText<'a>,
}

#[derive(Debug, Serialize)]
struct FeishuText<'a> {
    text: String,
    #[serde(skip)]
    _marker: std::marker::PhantomData<&'a ()>,
}

#[derive(Debug, Serialize)]
struct DingTalkMessage {
    msgtype: &'static str,
    text: DingTalkText,
}

#[derive(Debug, Serialize)]
struct DingTalkText {
    content: String,
}

#[derive(Debug, Serialize)]
struct WeComMessage {
    msgtype: &'static str,
    text: WeComText,
}

#[derive(Debug, Serialize)]
struct WeComText {
    content: String,
}

#[derive(Debug, Serialize)]
struct TelegramMessage<'a> {
    chat_id: &'a str,
    text: String,
    disable_web_page_preview: bool,
}

#[derive(Debug, Serialize)]
struct DiscordMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct SlackMessage {
    text: String,
}

#[derive(Debug, Serialize)]
struct WxPusherMessage<'a> {
    content: String,
    summary: &'a str,
    content_type: u8,
    spt: &'a str,
}

pub async fn dispatch_notification_channels(
    request: NotificationDispatchRequest,
) -> NotificationDispatchResult {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return NotificationDispatchResult {
                sent: 0,
                failed: request
                    .channels
                    .iter()
                    .map(|channel| NotificationChannelFailure {
                        id: channel.id.clone(),
                        message: error.to_string(),
                    })
                    .collect(),
            };
        }
    };
    let mut sent = 0;
    let mut failed = Vec::new();

    for channel in &request.channels {
        if channel.endpoint.trim().is_empty() {
            continue;
        }
        match dispatch_channel(&client, &request, &channel).await {
            Ok(()) => sent += 1,
            Err(message) => failed.push(NotificationChannelFailure {
                id: channel.id.clone(),
                message,
            }),
        }
    }

    NotificationDispatchResult { sent, failed }
}

async fn dispatch_channel(
    client: &reqwest::Client,
    request: &NotificationDispatchRequest,
    channel: &NotificationChannelConfig,
) -> Result<(), String> {
    let id = channel.id.as_str();
    let endpoint = channel.endpoint.trim();
    let token = channel.token.trim();

    let response = match id {
        "ntfy" => {
            let mut builder = client
                .post(endpoint)
                .header("Title", request.title.as_str())
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(if request.body.is_empty() {
                    request.title.clone()
                } else {
                    request.body.clone()
                });
            if !token.is_empty() {
                builder = builder.bearer_auth(token);
            }
            builder.send().await
        }
        "bark" => {
            let url = bark_url(endpoint, token, &request.title, &request.body);
            client.post(url).send().await
        }
        "feishu" => {
            client
                .post(with_token_query(endpoint, token, "access_token"))
                .json(&FeishuMessage {
                    msg_type: "text",
                    content: FeishuText {
                        text: notification_text(&request.title, &request.body),
                        _marker: std::marker::PhantomData,
                    },
                })
                .send()
                .await
        }
        "dingtalk" | "dingTalk" => {
            client
                .post(with_token_query(endpoint, token, "access_token"))
                .json(&DingTalkMessage {
                    msgtype: "text",
                    text: DingTalkText {
                        content: notification_text(&request.title, &request.body),
                    },
                })
                .send()
                .await
        }
        "wecom" | "weCom" => {
            client
                .post(with_token_query(endpoint, token, "key"))
                .json(&WeComMessage {
                    msgtype: "text",
                    text: WeComText {
                        content: notification_text(&request.title, &request.body),
                    },
                })
                .send()
                .await
        }
        "telegram" => {
            if token.is_empty() {
                return Err("Telegram bot token is required.".to_string());
            }
            let url = format!(
                "https://api.telegram.org/bot{}/sendMessage",
                percent_encoding::utf8_percent_encode(token, percent_encoding::NON_ALPHANUMERIC)
            );
            client
                .post(url)
                .json(&TelegramMessage {
                    chat_id: endpoint,
                    text: notification_text(&request.title, &request.body),
                    disable_web_page_preview: true,
                })
                .send()
                .await
        }
        "discord" => {
            let mut builder = client.post(endpoint).json(&DiscordMessage {
                content: notification_text(&request.title, &request.body),
            });
            if !token.is_empty() {
                builder = builder.bearer_auth(token);
            }
            builder.send().await
        }
        "slack" => {
            let mut builder = client.post(endpoint).json(&SlackMessage {
                text: notification_text(&request.title, &request.body),
            });
            if !token.is_empty() {
                builder = builder.bearer_auth(token);
            }
            builder.send().await
        }
        "wxpusher" => {
            let spt = if endpoint.starts_with("SPT") {
                endpoint
            } else {
                token
            };
            if spt.is_empty() {
                return Err("WxPusher SPT token is required.".to_string());
            }
            client
                .post("https://wxpusher.zjiecode.com/api/send/message/spt")
                .json(&WxPusherMessage {
                    content: notification_text(&request.title, &request.body),
                    summary: &request.title,
                    content_type: 1,
                    spt,
                })
                .send()
                .await
        }
        _ => {
            let payload = GenericNotificationPayload {
                title: &request.title,
                body: &request.body,
                group: &request.group,
                source: "codux",
            };
            let mut builder = client.post(endpoint).json(&payload);
            if !token.is_empty() {
                builder = builder.bearer_auth(token);
            }
            builder.send().await
        }
    }
    .map_err(|error| error.to_string())?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", response.status()))
    }
}

fn bark_url(endpoint: &str, token: &str, title: &str, body: &str) -> String {
    if token.is_empty() {
        return endpoint.to_string();
    }
    let base = endpoint.trim_end_matches('/');
    format!(
        "{}/{}/{}/{}",
        base,
        percent_encoding::utf8_percent_encode(token, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(body, percent_encoding::NON_ALPHANUMERIC)
    )
}

fn notification_text(title: &str, body: &str) -> String {
    if body.trim().is_empty() {
        title.to_string()
    } else {
        format!("{title}\n{body}")
    }
}

fn with_token_query(endpoint: &str, token: &str, key: &str) -> String {
    if token.is_empty() || endpoint.contains(&format!("{key}=")) {
        return endpoint.to_string();
    }
    let joiner = if endpoint.contains('?') { '&' } else { '?' };
    format!(
        "{endpoint}{joiner}{key}={}",
        percent_encoding::utf8_percent_encode(token, percent_encoding::NON_ALPHANUMERIC)
    )
}
