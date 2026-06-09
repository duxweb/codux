use super::{RemoteService, RemoteSummary};
use crate::runtime_trace::runtime_trace;
use std::thread;

impl RemoteService {
    pub fn sync_settings_background(&self) -> RemoteSummary {
        let summary = self.summary();
        if !summary.enabled {
            return summary;
        }

        let settings_path = self.settings_path.clone();
        thread::spawn(move || {
            let service = RemoteService { settings_path };
            if let Err(error) = service.reconnect() {
                runtime_trace("remote", &format!("background sync failed: {error}"));
            }
        });

        summary
    }
}
