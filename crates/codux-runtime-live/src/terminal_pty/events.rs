use super::*;

#[derive(Clone, Debug)]
pub(super) struct TerminalViewportLease {
    pub(super) state: TerminalViewportState,
    pub(super) expires_at: Instant,
    pub(super) explicit_owner: bool,
}

pub type EventSink = Arc<dyn Fn(TerminalEvent) -> bool + Send + Sync + 'static>;
pub(super) type EventSubscriberKey = Arc<str>;
pub(crate) static TERMINAL_EVENT_SUBSCRIBER_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct EventSubscriber {
    key: Option<EventSubscriberKey>,
    generation: u64,
    sink: EventSink,
}

impl EventSubscriber {
    pub(super) fn anonymous(sink: EventSink) -> Self {
        Self {
            key: None,
            generation: 0,
            sink,
        }
    }

    pub(super) fn keyed(key: EventSubscriberKey, sink: EventSink) -> Self {
        Self {
            key: Some(key),
            generation: TERMINAL_EVENT_SUBSCRIBER_GENERATION.fetch_add(1, Ordering::Relaxed),
            sink,
        }
    }
}

/// Resolves the next viewport owner when a remote lease expires. Called with
/// `(session_id, expired_owner)`; returns another owner string to hand off to
/// (e.g. a second phone still watching the same terminal), or `None` to revert
/// to the host desktop.
pub type ViewportOwnerResolver = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

pub fn terminal_viewport_local_owner() -> &'static str {
    "desktop"
}

pub fn terminal_viewport_remote_owner(device_id: &str) -> String {
    format!("remote:{}", device_id.trim())
}

pub(super) fn terminal_viewport_owner(owner: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        terminal_viewport_local_owner().to_string()
    } else {
        owner.to_string()
    }
}

pub(super) fn emit_terminal_event(
    subscribers: &Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    event: TerminalEvent,
) {
    let mut subscribers = subscribers.lock();
    let mut latest_by_key = HashMap::new();
    for subscriber in subscribers.iter() {
        if let Some(key) = subscriber.key.as_ref() {
            latest_by_key.insert(key.clone(), subscriber.generation);
        }
    }
    subscribers.retain(|subscriber| {
        if subscriber
            .key
            .as_ref()
            .and_then(|key| latest_by_key.get(key).copied())
            .is_some_and(|latest| latest != subscriber.generation)
        {
            return false;
        }
        (subscriber.sink)(event.clone())
    });
}

pub(super) fn insert_keyed_event_subscriber(
    subscribers: &Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    key: String,
    sink: EventSink,
) {
    let key = key.trim();
    if key.is_empty() {
        subscribers.lock().push(EventSubscriber::anonymous(sink));
        return;
    }
    subscribers
        .lock()
        .push(EventSubscriber::keyed(Arc::<str>::from(key), sink));
}
