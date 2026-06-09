use super::*;

#[derive(Clone, Copy)]
pub(in crate::app) struct ScheduledWorkPolicy {
    pub(in crate::app) recent_seconds: f64,
    pub(in crate::app) debounce_seconds: f64,
}

impl ScheduledWorkPolicy {
    pub(in crate::app) const fn new(recent_seconds: f64, debounce_seconds: f64) -> Self {
        Self {
            recent_seconds,
            debounce_seconds,
        }
    }
}

impl CoduxApp {
    pub(in crate::app) fn scheduled_work_busy_or_recent(
        &self,
        key: &str,
        policy: ScheduledWorkPolicy,
    ) -> bool {
        let now = app_now_seconds();
        self.scheduled_work_in_flight.contains(key)
            || self
                .scheduled_work_last_finished_at
                .get(key)
                .is_some_and(|finished| now - finished < policy.recent_seconds)
            || self
                .scheduled_work_last_started_at
                .get(key)
                .is_some_and(|started| now - started < policy.debounce_seconds)
    }

    pub(in crate::app) fn begin_scheduled_work(
        &mut self,
        key: impl Into<String>,
        policy: ScheduledWorkPolicy,
    ) -> bool {
        let key = key.into();
        if self.scheduled_work_busy_or_recent(&key, policy) {
            self.record_ui_scheduler_event("skip_busy", &key);
            return false;
        }
        self.scheduled_work_in_flight.insert(key.clone());
        self.scheduled_work_last_started_at
            .insert(key.clone(), app_now_seconds());
        self.record_ui_scheduler_event("begin", &key);
        true
    }

    pub(in crate::app) fn finish_scheduled_work(&mut self, key: &str) {
        self.scheduled_work_in_flight.remove(key);
        self.scheduled_work_last_finished_at
            .insert(key.to_string(), app_now_seconds());
        self.record_ui_scheduler_event("finish", key);
    }

    pub(in crate::app) fn record_ui_scheduler_event(&mut self, state: &str, key: &str) {
        let label = format!("{state}:{key}");
        self.record_ui_performance_dynamic_event("scheduler", &label);
    }
}
