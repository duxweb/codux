use serde_json::{Value, json};

pub const REMOTE_PROTOCOL_VERSION: &str = "v3.1";
pub const REMOTE_TERMINAL_BUFFER_MAX_CHARS: usize = 200_000;
pub const REMOTE_TERMINAL_BUFFER_CHUNK_CHARS: usize = 16_384;

pub struct RemoteTerminalBufferWindow {
    pub data: String,
    pub offset: usize,
    pub total_characters: usize,
    pub truncated: bool,
    pub output_seq: Option<i64>,
    pub request_id: Option<String>,
    pub tail: bool,
    pub screen_snapshot: bool,
    pub has_previous: bool,
}

pub fn host_capabilities() -> Value {
    json!({
        "domains": {
            "project": true,
            "terminal": true,
            "worktree": true,
            "file": true,
            "git": true,
            "aiStats": true,
        },
        "terminalBuffer": {
            "chunking": true,
            "maxChars": REMOTE_TERMINAL_BUFFER_MAX_CHARS,
            "chunkChars": REMOTE_TERMINAL_BUFFER_CHUNK_CHARS,
            "requestId": true,
            "tailSnapshot": true,
            "screenSnapshot": true,
        },
        "terminalViewport": {
            "ownership": true,
            "state": true,
        },
    })
}

pub fn terminal_buffer_payloads(
    window: &RemoteTerminalBufferWindow,
    output_seq: i64,
    chunk_chars: Option<usize>,
) -> Vec<Value> {
    let max_chunk_chars = chunk_chars.unwrap_or(REMOTE_TERMINAL_BUFFER_CHUNK_CHARS);
    let total_chars = window.data.chars().count();
    if total_chars <= max_chunk_chars {
        return vec![terminal_buffer_payload(
            window,
            output_seq,
            window.data.clone(),
            window.offset,
            None,
        )];
    }

    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let chunks = split_text_chunks(&window.data, max_chunk_chars);
    let chunk_count = chunks.len();
    let mut offset = window.offset;
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, data)| {
            let payload = terminal_buffer_payload(
                window,
                output_seq,
                data.clone(),
                offset,
                Some((&snapshot_id, index, chunk_count)),
            );
            offset += data.chars().count();
            payload
        })
        .collect()
}

fn terminal_buffer_payload(
    window: &RemoteTerminalBufferWindow,
    output_seq: i64,
    data: String,
    offset: usize,
    chunk: Option<(&str, usize, usize)>,
) -> Value {
    let mut payload = json!({
        "data": data,
        "buffer": true,
        "offset": offset,
        "startOffset": window.offset,
        "bufferLength": window.total_characters,
        "truncated": window.truncated,
        "outputSeq": output_seq,
        "tail": window.tail,
        "screenSnapshot": window.screen_snapshot,
        "hasPrevious": window.has_previous,
    });
    if let Some(request_id) = window.request_id.as_deref() {
        payload["requestId"] = json!(request_id);
    }
    if let Some((snapshot_id, chunk_index, chunk_count)) = chunk {
        payload["snapshotId"] = json!(snapshot_id);
        payload["chunkIndex"] = json!(chunk_index);
        payload["chunkCount"] = json!(chunk_count);
        payload["chunked"] = json!(true);
    }
    payload
}

fn split_text_chunks(text: &str, chunk_chars: usize) -> Vec<String> {
    let chunk_chars = chunk_chars.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for ch in text.chars() {
        current.push(ch);
        current_chars += 1;
        if current_chars >= chunk_chars {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_buffer_payloads_are_chunked_on_character_boundaries() {
        let window = RemoteTerminalBufferWindow {
            data: "ab你好cd".to_string(),
            offset: 10,
            total_characters: 16,
            truncated: true,
            output_seq: None,
            request_id: Some("request-1".to_string()),
            tail: true,
            screen_snapshot: true,
            has_previous: true,
        };

        let payloads = terminal_buffer_payloads(&window, 7, Some(2));

        assert_eq!(payloads.len(), 3);
        let snapshot_id = payloads[0]["snapshotId"]
            .as_str()
            .expect("snapshot id")
            .to_string();
        assert!(!snapshot_id.is_empty());
        let data = payloads
            .iter()
            .map(|payload| payload["data"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(data, vec!["ab", "你好", "cd"]);
        assert_eq!(payloads[0]["offset"], 10);
        assert_eq!(payloads[1]["offset"], 12);
        assert_eq!(payloads[2]["offset"], 14);
        for (index, payload) in payloads.iter().enumerate() {
            assert_eq!(payload["snapshotId"], snapshot_id);
            assert_eq!(payload["chunkIndex"], index);
            assert_eq!(payload["chunkCount"], 3);
            assert_eq!(payload["startOffset"], 10);
            assert_eq!(payload["bufferLength"], 16);
            assert_eq!(payload["outputSeq"], 7);
            assert_eq!(payload["truncated"], true);
            assert_eq!(payload["requestId"], "request-1");
            assert_eq!(payload["tail"], true);
            assert_eq!(payload["screenSnapshot"], true);
            assert_eq!(payload["hasPrevious"], true);
        }
    }

    #[test]
    fn host_capabilities_advertise_runtime_domains() {
        let capabilities = host_capabilities();
        assert_eq!(capabilities["domains"]["project"], true);
        assert_eq!(capabilities["domains"]["terminal"], true);
        assert_eq!(capabilities["domains"]["worktree"], true);
        assert_eq!(capabilities["domains"]["file"], true);
        assert_eq!(capabilities["domains"]["git"], true);
        assert_eq!(capabilities["domains"]["aiStats"], true);
        assert_eq!(capabilities["terminalBuffer"]["chunking"], true);
        assert_eq!(capabilities["terminalBuffer"]["requestId"], true);
        assert_eq!(capabilities["terminalBuffer"]["tailSnapshot"], true);
        assert_eq!(capabilities["terminalBuffer"]["screenSnapshot"], true);
        assert_eq!(capabilities["terminalViewport"]["ownership"], true);
    }
}
