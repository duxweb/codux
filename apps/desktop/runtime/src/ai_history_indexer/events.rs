use super::types::AIHistoryEvent;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub(super) fn push_history_event(
    events: &Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    event: AIHistoryEvent,
) {
    if let Ok(mut events) = events.lock() {
        events.push_back(event);
        while events.len() > 256 {
            events.pop_front();
        }
    }
}
