use super::*;

#[test]
fn memory_read_error_reply_preserves_request_id() {
    let support_dir = temp_support_dir("codux-remote-memory-error");
    let runtime = RemoteHostRuntime::new(support_dir.clone());
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    runtime.handle_memory_read(&RemoteEnvelope {
        kind: REMOTE_MEMORY_READ.to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: Some("request-memory-error".to_string()),
        seq: None,
        payload: json!({ "op": "unsupported" }),
    });

    let messages = transport.take_messages();
    assert_eq!(messages.len(), 1);
    let envelope: Value = serde_json::from_slice(&messages[0].1).expect("error envelope");
    assert_eq!(envelope["type"], REMOTE_ERROR);
    assert_eq!(envelope["requestId"], "request-memory-error");
    assert_eq!(
        envelope["payload"]["message"],
        "Unsupported memory read operation: unsupported"
    );

    fs::remove_dir_all(support_dir).ok();
}
