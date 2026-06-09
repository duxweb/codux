use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct RemoteTerminalSubscriptions {
    viewers_by_session: Mutex<HashMap<String, HashSet<String>>>,
    subscribers_by_project: Mutex<HashMap<String, HashSet<String>>>,
}

impl RemoteTerminalSubscriptions {
    pub(super) fn add_session_viewer(&self, session_id: &str, device_id: &str) {
        if session_id.trim().is_empty() || device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            viewers
                .entry(session_id.to_string())
                .or_default()
                .insert(device_id.to_string());
        }
    }

    pub(super) fn add_project_subscriber(&self, project_id: &str, device_id: &str) {
        if project_id.trim().is_empty() || device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut subscribers) = self.subscribers_by_project.lock() {
            subscribers
                .entry(project_id.to_string())
                .or_default()
                .insert(device_id.to_string());
        }
    }

    pub(super) fn remove_session_viewer(&self, session_id: &str, device_id: &str) {
        if session_id.trim().is_empty() || device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            if let Some(session_viewers) = viewers.get_mut(session_id) {
                session_viewers.remove(device_id);
            }
            viewers.retain(|_, value| !value.is_empty());
        }
    }

    pub(super) fn remove_project_subscriber(&self, project_id: &str, device_id: &str) {
        if project_id.trim().is_empty() || device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut subscribers) = self.subscribers_by_project.lock() {
            if let Some(project_subscribers) = subscribers.get_mut(project_id) {
                project_subscribers.remove(device_id);
            }
            subscribers.retain(|_, value| !value.is_empty());
        }
    }

    pub(super) fn remove_project_session_viewers<'a>(
        &self,
        session_ids: impl IntoIterator<Item = &'a str>,
        device_id: &str,
    ) {
        if device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            for session_id in session_ids {
                if let Some(session_viewers) = viewers.get_mut(session_id) {
                    session_viewers.remove(device_id);
                }
            }
            viewers.retain(|_, value| !value.is_empty());
        }
    }

    pub(super) fn remove_device(&self, device_id: &str) {
        if device_id.trim().is_empty() {
            return;
        }
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            for session_viewers in viewers.values_mut() {
                session_viewers.remove(device_id);
            }
            viewers.retain(|_, value| !value.is_empty());
        }
        if let Ok(mut subscribers) = self.subscribers_by_project.lock() {
            for project_subscribers in subscribers.values_mut() {
                project_subscribers.remove(device_id);
            }
            subscribers.retain(|_, value| !value.is_empty());
        }
    }

    pub(super) fn remove_session(&self, session_id: &str) {
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            viewers.remove(session_id);
        }
    }

    pub(super) fn clear(&self) {
        if let Ok(mut viewers) = self.viewers_by_session.lock() {
            viewers.clear();
        }
        if let Ok(mut subscribers) = self.subscribers_by_project.lock() {
            subscribers.clear();
        }
    }

    pub(super) fn viewers_for_session(
        &self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> HashSet<String> {
        let mut viewers = self
            .viewers_by_session
            .lock()
            .ok()
            .and_then(|viewers| viewers.get(session_id).cloned())
            .unwrap_or_default();
        if let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) {
            if let Ok(subscribers) = self.subscribers_by_project.lock() {
                if let Some(project_subscribers) = subscribers.get(project_id) {
                    viewers.extend(project_subscribers.iter().cloned());
                }
            }
        }
        viewers
    }
}
