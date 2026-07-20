use super::TerminalStatusEvent;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TerminalActivityEvent {
    Status(TerminalStatusEvent),
    Exit {
        terminal_id: String,
        exit_code: Option<i32>,
    },
    Error {
        terminal_id: String,
        message: String,
    },
}

impl TerminalActivityEvent {
    fn terminal_id(&self) -> &str {
        match self {
            Self::Status(status) => &status.terminal_id,
            Self::Exit { terminal_id, .. } | Self::Error { terminal_id, .. } => terminal_id,
        }
    }
}

#[derive(Default)]
pub(crate) struct TerminalActivityHub {
    subscribers: Arc<Mutex<HashMap<String, HashMap<u64, Sender<TerminalActivityEvent>>>>>,
    next_id: AtomicU64,
}

pub(crate) struct TerminalActivitySubscription {
    terminal_id: String,
    subscription_id: u64,
    receiver: Receiver<TerminalActivityEvent>,
    subscribers: Weak<Mutex<HashMap<String, HashMap<u64, Sender<TerminalActivityEvent>>>>>,
}

impl TerminalActivityHub {
    pub(crate) fn subscribe(&self, terminal_id: &str) -> TerminalActivitySubscription {
        let terminal_id = terminal_id.to_string();
        let subscription_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = channel();
        self.subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(terminal_id.clone())
            .or_default()
            .insert(subscription_id, sender);
        TerminalActivitySubscription {
            terminal_id,
            subscription_id,
            receiver,
            subscribers: Arc::downgrade(&self.subscribers),
        }
    }

    pub(crate) fn publish(&self, event: TerminalActivityEvent) {
        let terminal_id = event.terminal_id().to_string();
        let mut subscribers = self
            .subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(terminal_subscribers) = subscribers.get_mut(&terminal_id) else {
            return;
        };
        terminal_subscribers.retain(|_, sender| sender.send(event.clone()).is_ok());
        if terminal_subscribers.is_empty() {
            subscribers.remove(&terminal_id);
        }
    }
}

impl TerminalActivitySubscription {
    pub(crate) fn recv(&self) -> Result<TerminalActivityEvent, std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }
}

impl Drop for TerminalActivitySubscription {
    fn drop(&mut self) {
        let Some(subscribers) = self.subscribers.upgrade() else {
            return;
        };
        let mut subscribers = subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(terminal_subscribers) = subscribers.get_mut(&self.terminal_id) else {
            return;
        };
        terminal_subscribers.remove(&self.subscription_id);
        if terminal_subscribers.is_empty() {
            subscribers.remove(&self.terminal_id);
        }
    }
}
