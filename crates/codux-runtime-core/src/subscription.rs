use codux_protocol::{
    REMOTE_RESOURCE_SUBSCRIBE, REMOTE_RESOURCE_UNSUBSCRIBE, RemoteEnvelope,
    RemoteResourceSubscriptionTarget, RemoteResourceSubscriptions,
};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

type ResourceVersionKey = (String, Option<String>, Option<String>);

/// Monotonic version per replicated resource key `(resource, project_id,
/// session_id)`. The host is the sole writer; every outbound state payload
/// carries its key's version so clients apply latest-wins and reconcile on
/// reconnect instead of relying on a single broadcast landing. Bumped only on
/// state CHANGES (cold path), so the per-bump key allocation is irrelevant.
///
/// This generalizes what the terminal viewport already does with its
/// `generation` field to every replicated resource (projects, git, ai, ...).
#[derive(Default)]
pub struct ResourceVersions {
    versions: Mutex<HashMap<ResourceVersionKey, u64>>,
}

impl ResourceVersions {
    fn key(
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> ResourceVersionKey {
        (
            resource.to_string(),
            project_id.map(str::to_string),
            session_id.map(str::to_string),
        )
    }

    /// Bump and return the next version for a key. Call once per state change,
    /// before fan-out, then stamp the returned value into the broadcast payload.
    pub fn next(&self, resource: &str, project_id: Option<&str>, session_id: Option<&str>) -> u64 {
        let mut versions = match self.versions.lock() {
            Ok(versions) => versions,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = versions
            .entry(Self::key(resource, project_id, session_id))
            .or_insert(0);
        *entry += 1;
        *entry
    }

    /// The current version for a key (0 if never bumped). Use for snapshot
    /// replies so a freshly (re)subscribed client knows where it stands.
    pub fn current(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> u64 {
        let versions = match self.versions.lock() {
            Ok(versions) => versions,
            Err(poisoned) => poisoned.into_inner(),
        };
        versions
            .get(&Self::key(resource, project_id, session_id))
            .copied()
            .unwrap_or(0)
    }

    fn remove_project(&self, project_id: &str) {
        let mut versions = match self.versions.lock() {
            Ok(versions) => versions,
            Err(poisoned) => poisoned.into_inner(),
        };
        versions.retain(|(_, current_project_id, _), _| {
            current_project_id.as_deref() != Some(project_id)
        });
    }

    fn remove_session(&self, session_id: &str) {
        let mut versions = match self.versions.lock() {
            Ok(versions) => versions,
            Err(poisoned) => poisoned.into_inner(),
        };
        versions.retain(|(_, _, current_session_id), _| {
            current_session_id.as_deref() != Some(session_id)
        });
    }

    fn clear(&self) {
        match self.versions.lock() {
            Ok(mut versions) => versions.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }
}

#[derive(Default)]
pub struct RuntimeSubscriptionRouter {
    subscriptions: RemoteResourceSubscriptions,
    versions: ResourceVersions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSubscriptionChange {
    pub device_id: String,
    pub resource: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub baseline: bool,
}

impl RuntimeSubscriptionRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe_envelope(
        &self,
        envelope: &RemoteEnvelope,
    ) -> Result<RuntimeSubscriptionChange, String> {
        let change = Self::parse_subscribe_envelope(envelope)?;
        self.subscribe_change(&change);
        Ok(change)
    }

    pub fn parse_subscribe_envelope(
        envelope: &RemoteEnvelope,
    ) -> Result<RuntimeSubscriptionChange, String> {
        if envelope.kind != REMOTE_RESOURCE_SUBSCRIBE {
            return Err("Expected resource.subscribe envelope.".to_string());
        }
        let device_id = clean_device_id(envelope.device_id.as_deref())?;
        let target = RemoteResourceSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        )?;
        Ok(RuntimeSubscriptionChange {
            device_id,
            resource: target.resource,
            project_id: target.project_id,
            session_id: target.session_id,
            baseline: target.baseline,
        })
    }

    pub fn subscribe_change(&self, change: &RuntimeSubscriptionChange) {
        self.subscriptions.subscribe(
            &change.resource,
            change.project_id.as_deref(),
            change.session_id.as_deref(),
            &change.device_id,
        );
    }

    pub fn unsubscribe_envelope(
        &self,
        envelope: &RemoteEnvelope,
    ) -> Result<RuntimeSubscriptionChange, String> {
        if envelope.kind != REMOTE_RESOURCE_UNSUBSCRIBE {
            return Err("Expected resource.unsubscribe envelope.".to_string());
        }
        let device_id = clean_device_id(envelope.device_id.as_deref())?;
        let target = RemoteResourceSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        )?;
        self.subscriptions.unsubscribe(
            &target.resource,
            target.project_id.as_deref(),
            target.session_id.as_deref(),
            &device_id,
        );
        Ok(RuntimeSubscriptionChange {
            device_id,
            resource: target.resource,
            project_id: target.project_id,
            session_id: target.session_id,
            baseline: target.baseline,
        })
    }

    pub fn remove_device(&self, device_id: &str) {
        self.subscriptions.remove_device(device_id);
    }

    pub fn subscribe(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        device_id: &str,
    ) {
        self.subscriptions
            .subscribe(resource, project_id, session_id, device_id);
    }

    pub fn unsubscribe(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        device_id: &str,
    ) {
        self.subscriptions
            .unsubscribe(resource, project_id, session_id, device_id);
    }

    pub fn remove_project(&self, project_id: &str) {
        self.subscriptions.remove_project(project_id);
        self.versions.remove_project(project_id);
    }

    pub fn remove_session(&self, session_id: &str) {
        self.subscriptions.remove_session(session_id);
        self.versions.remove_session(session_id);
    }

    pub fn clear(&self) {
        self.subscriptions.clear();
        self.versions.clear();
    }

    pub fn devices_for(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> HashSet<String> {
        self.subscriptions
            .devices_for(resource, project_id, session_id)
    }

    pub fn devices_for_resource(&self, resource: &str) -> HashSet<String> {
        self.subscriptions.devices_for_resource(resource)
    }

    pub fn devices_for_exact(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> HashSet<String> {
        self.subscriptions
            .devices_for_exact(resource, project_id, session_id)
    }

    /// Bump and return the next version for a resource key. Stamp this onto the
    /// payload right before fan-out so every subscriber (and the local UI) sees
    /// the same monotonic version for this state change.
    pub fn next_version(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> u64 {
        self.versions.next(resource, project_id, session_id)
    }

    /// The current version for a resource key, for version-stamping snapshot
    /// replies on (re)subscribe.
    pub fn current_version(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> u64 {
        self.versions.current(resource, project_id, session_id)
    }
}

fn clean_device_id(device_id: Option<&str>) -> Result<String, String> {
    device_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Device id is required.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_protocol::{
        REMOTE_PROJECT_LIST, REMOTE_RESOURCE_GIT_STATUS, REMOTE_RESOURCE_SUBSCRIBE,
        REMOTE_RESOURCE_TERMINALS, REMOTE_RESOURCE_UNSUBSCRIBE,
    };
    use serde_json::json;

    #[test]
    fn subscribe_and_unsubscribe_envelopes_drive_resource_targets() {
        let router = RuntimeSubscriptionRouter::new();
        let subscribe = RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            request_id: None,
            seq: None,
            payload: json!({
                "resource": REMOTE_RESOURCE_GIT_STATUS,
                "projectId": "project-1",
                "baseline": true,
            }),
        };

        let change = router.subscribe_envelope(&subscribe).unwrap();
        assert_eq!(change.device_id, "device-1");
        assert_eq!(change.resource, REMOTE_RESOURCE_GIT_STATUS);
        assert_eq!(change.project_id.as_deref(), Some("project-1"));
        assert!(change.baseline);
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None)
                .contains("device-1")
        );

        let unsubscribe = RemoteEnvelope {
            kind: REMOTE_RESOURCE_UNSUBSCRIBE.to_string(),
            payload: subscribe.payload.clone(),
            ..subscribe
        };
        router.unsubscribe_envelope(&unsubscribe).unwrap();
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None)
                .is_empty()
        );
    }

    #[test]
    fn remove_device_and_scope_clear_subscriptions() {
        let router = RuntimeSubscriptionRouter::new();
        router
            .subscriptions
            .subscribe(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None, "a");
        router
            .subscriptions
            .subscribe(REMOTE_RESOURCE_GIT_STATUS, Some("project-2"), None, "b");

        router.remove_project("project-1");
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None)
                .is_empty()
        );
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-2"), None)
                .contains("b")
        );

        router.remove_device("b");
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-2"), None)
                .is_empty()
        );
    }

    #[test]
    fn rejects_wrong_envelope_kind_and_missing_device() {
        let router = RuntimeSubscriptionRouter::new();
        let wrong_kind = RemoteEnvelope {
            kind: REMOTE_PROJECT_LIST.to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            request_id: None,
            seq: None,
            payload: json!({
                "resource": REMOTE_RESOURCE_GIT_STATUS,
                "projectId": "project-1",
            }),
        };

        assert_eq!(
            router.subscribe_envelope(&wrong_kind).unwrap_err(),
            "Expected resource.subscribe envelope."
        );

        let missing_device = RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("  ".to_string()),
            ..wrong_kind
        };
        assert_eq!(
            router.subscribe_envelope(&missing_device).unwrap_err(),
            "Device id is required."
        );
    }

    #[test]
    fn resource_versions_bump_per_key_and_report_current() {
        let versions = ResourceVersions::default();
        assert_eq!(
            versions.current(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            0
        );
        assert_eq!(
            versions.next(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            1
        );
        assert_eq!(
            versions.next(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            2
        );
        assert_eq!(
            versions.current(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            2
        );
        // A different key versions independently.
        assert_eq!(
            versions.next(REMOTE_RESOURCE_GIT_STATUS, Some("p2"), None),
            1
        );
        assert_eq!(versions.next(REMOTE_PROJECT_LIST, None, None), 1);
        assert_eq!(
            versions.current(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            2
        );
    }

    #[test]
    fn router_delegates_resource_versioning() {
        let router = RuntimeSubscriptionRouter::new();
        assert_eq!(
            router.next_version(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            1
        );
        assert_eq!(
            router.next_version(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            2
        );
        assert_eq!(
            router.current_version(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            2
        );
        assert_eq!(
            router.current_version(REMOTE_RESOURCE_GIT_STATUS, Some("p2"), None),
            0
        );
    }

    #[test]
    fn scope_cleanup_removes_matching_resource_versions() {
        let router = RuntimeSubscriptionRouter::new();
        router.next_version(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None);
        router.next_version(REMOTE_RESOURCE_GIT_STATUS, Some("p2"), None);
        router.next_version(REMOTE_RESOURCE_TERMINALS, None, Some("s1"));

        router.remove_project("p1");
        router.remove_session("s1");

        assert_eq!(
            router.current_version(REMOTE_RESOURCE_GIT_STATUS, Some("p1"), None),
            0
        );
        assert_eq!(
            router.current_version(REMOTE_RESOURCE_GIT_STATUS, Some("p2"), None),
            1
        );
        assert_eq!(
            router.current_version(REMOTE_RESOURCE_TERMINALS, None, Some("s1")),
            0
        );

        router.clear();
        assert_eq!(
            router.current_version(REMOTE_RESOURCE_GIT_STATUS, Some("p2"), None),
            0
        );
    }

    #[test]
    fn routes_session_scoped_terminal_subscriptions() {
        let router = RuntimeSubscriptionRouter::new();
        let subscribe = RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some("session-1".to_string()),
            request_id: None,
            seq: None,
            payload: json!({
                "resource": REMOTE_RESOURCE_TERMINALS,
                "baseline": true,
            }),
        };

        let change = router.subscribe_envelope(&subscribe).unwrap();
        assert_eq!(change.session_id.as_deref(), Some("session-1"));
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_TERMINALS, None, Some("session-1"))
                .contains("device-1")
        );

        router.remove_session("session-1");
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_TERMINALS, None, Some("session-1"))
                .is_empty()
        );

        router.subscribe_envelope(&subscribe).unwrap();
        router.clear();
        assert!(
            router
                .devices_for(REMOTE_RESOURCE_TERMINALS, None, Some("session-1"))
                .is_empty()
        );
    }
}
