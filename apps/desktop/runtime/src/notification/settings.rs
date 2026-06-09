fn summary_from_raw(raw: &Map<String, Value>) -> NotificationSummary {
    let channels = raw
        .get("notificationChannels")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut summaries = KNOWN_CHANNELS
        .iter()
        .map(|(id, label)| {
            let channel = channels.get(*id).and_then(Value::as_object);
            let endpoint = channel
                .and_then(|channel| channel.get("endpoint"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let has_token = channel
                .and_then(|channel| channel.get("token"))
                .and_then(Value::as_str)
                .map(|token| !token.trim().is_empty())
                .unwrap_or(false);
            let token = channel
                .and_then(|channel| channel.get("token"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            NotificationChannelSummary {
                id: (*id).to_string(),
                label: (*label).to_string(),
                enabled: channel
                    .and_then(|channel| channel.get("enabled"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                configured: !endpoint.is_empty(),
                endpoint,
                has_token,
                token,
            }
        })
        .collect::<Vec<_>>();
    summaries.extend(
        channels
            .iter()
            .filter(|(id, _)| !KNOWN_CHANNELS.iter().any(|(known, _)| known == id))
            .filter_map(|(id, value)| {
                let channel = value.as_object()?;
                let endpoint = channel
                    .get("endpoint")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                Some(NotificationChannelSummary {
                    id: id.clone(),
                    label: id.clone(),
                    enabled: channel
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    configured: !endpoint.is_empty(),
                    endpoint,
                    has_token: channel
                        .get("token")
                        .and_then(Value::as_str)
                        .map(|token| !token.trim().is_empty())
                        .unwrap_or(false),
                    token: channel
                        .get("token")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
            }),
    );
    let enabled_count = summaries.iter().filter(|channel| channel.enabled).count();
    let configured_count = summaries
        .iter()
        .filter(|channel| channel.configured)
        .count();
    NotificationSummary {
        channel_count: summaries.len(),
        enabled_count,
        configured_count,
        channels: summaries,
        error: None,
    }
}

fn notification_channels_mut(
    raw: &mut Map<String, Value>,
) -> Result<&mut Map<String, Value>, String> {
    raw.entry("notificationChannels".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Notification channels are invalid.".to_string())
}

fn notification_channel_mut<'a>(
    raw: &'a mut Map<String, Value>,
    channel_id: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let channel_id = channel_id.trim();
    if channel_id.is_empty() {
        return Err("Notification channel id is empty.".to_string());
    }
    notification_channels_mut(raw)?
        .entry(channel_id.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Notification channel settings are invalid.".to_string())
}
