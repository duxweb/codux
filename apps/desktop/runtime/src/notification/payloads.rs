#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenericNotificationPayload<'a> {
    title: &'a str,
    body: &'a str,
    group: &'a str,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct FeishuMessage {
    msg_type: &'static str,
    content: FeishuText,
}

#[derive(Debug, Serialize)]
struct FeishuText {
    text: String,
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
