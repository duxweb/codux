use crate::ai_runtime::state::{canonical_tool_name, normalized_string};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeBinding {
    pub runtime_binding_id: String,
    pub terminal_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    pub tool: String,
    pub project_id: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default)]
    pub session_title: String,
    pub launch_started_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_origin: Option<String>,
    #[serde(default)]
    pub updated_at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AIRuntimeBindingEvent {
    pub path: PathBuf,
    pub binding: AIRuntimeBinding,
    pub modified_millis: u128,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIRuntimeBindingFileEvent {
    pub path: PathBuf,
    pub signature: AIRuntimeFileSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AIRuntimeFileSignature {
    pub modified_millis: u128,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AIRuntimeBindingScanState {
    signatures: HashMap<PathBuf, AIRuntimeFileSignature>,
}

impl AIRuntimeBinding {
    pub fn normalized(mut self) -> Option<Self> {
        self.runtime_binding_id = normalized_string(Some(&self.runtime_binding_id))?;
        self.terminal_id = normalized_string(Some(&self.terminal_id))?;
        self.terminal_instance_id = normalized_string(self.terminal_instance_id.as_deref());
        self.tool = canonical_tool_name(&self.tool)?;
        self.project_id = normalized_string(Some(&self.project_id))?;
        self.project_path = normalized_string(self.project_path.as_deref());
        self.project_name = normalized_runtime_project_name(
            Some(&self.project_name),
            self.project_path.as_deref(),
            Some(&self.project_id),
        );
        self.session_title =
            normalized_string(Some(&self.session_title)).unwrap_or_else(|| "Terminal".to_string());
        self.external_session_id = normalized_string(self.external_session_id.as_deref());
        self.transcript_path = normalized_string(self.transcript_path.as_deref());
        self.model = normalized_string(self.model.as_deref());
        self.session_origin =
            normalized_string(self.session_origin.as_deref()).filter(|value| value == "restored");
        if self.updated_at <= 0.0 {
            self.updated_at = self.launch_started_at;
        }
        (self.launch_started_at > 0.0).then_some(self)
    }
}

pub(crate) fn normalized_runtime_project_name(
    project_name: Option<&str>,
    project_path: Option<&str>,
    project_id: Option<&str>,
) -> String {
    normalized_string(project_name)
        .filter(|name| !name.eq_ignore_ascii_case("Workspace"))
        .or_else(|| project_path.and_then(project_name_from_path))
        .or_else(|| normalized_string(project_id))
        .unwrap_or_else(|| "Workspace".to_string())
}

fn project_name_from_path(path: &str) -> Option<String> {
    let path = normalized_string(Some(path))?;
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .find_map(|segment| normalized_string(Some(segment)))
}

pub fn scan_runtime_bindings(
    dir: &Path,
    state: &mut AIRuntimeBindingScanState,
) -> Vec<AIRuntimeBindingEvent> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    let mut seen = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        seen.push(path.clone());
        let Some(signature) = runtime_file_signature(&path) else {
            continue;
        };
        if let Some(event) = read_changed_runtime_binding(path, signature, state) {
            events.push(event);
        }
    }
    state
        .signatures
        .retain(|path, _| seen.iter().any(|seen| seen == path));
    events
}

pub fn clear_runtime_bindings(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for path in entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
    {
        if fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn read_changed_runtime_binding(
    path: PathBuf,
    signature: AIRuntimeFileSignature,
    state: &mut AIRuntimeBindingScanState,
) -> Option<AIRuntimeBindingEvent> {
    if path.extension().and_then(|value| value.to_str()) != Some("json") {
        return None;
    }
    if state.signatures.get(&path) == Some(&signature) {
        return None;
    }
    state.signatures.insert(path.clone(), signature);
    let binding = read_runtime_binding(&path)?;
    Some(AIRuntimeBindingEvent {
        path,
        binding,
        modified_millis: signature.modified_millis,
        size: signature.size,
    })
}

pub fn read_runtime_binding(path: &Path) -> Option<AIRuntimeBinding> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice::<AIRuntimeBinding>(&data)
        .ok()?
        .normalized()
}

pub fn runtime_file_signature(path: &Path) -> Option<AIRuntimeFileSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified_millis = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some(AIRuntimeFileSignature {
        modified_millis,
        size: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn scan_runtime_bindings_emits_changed_files_only() {
        let dir = std::env::temp_dir().join(format!("codux-runtime-bindings-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binding.json");
        fs::write(
            &path,
            r#"{"runtimeBindingId":"bind-1","terminalId":"term-1","terminalInstanceId":"pty-1","tool":"codex","projectId":"project-1","projectPath":"/tmp/project","sessionTitle":"Codex","launchStartedAt":10.0,"updatedAt":10.0}"#,
        )
        .unwrap();

        let mut state = AIRuntimeBindingScanState::default();
        let first = scan_runtime_bindings(&dir, &mut state);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].binding.tool, "codex");
        assert!(scan_runtime_bindings(&dir, &mut state).is_empty());

        fs::write(
            &path,
            r#"{"runtimeBindingId":"bind-1","terminalId":"term-1","terminalInstanceId":"pty-1","tool":"codex","projectId":"project-1","projectPath":"/tmp/project","sessionTitle":"Codex","launchStartedAt":10.0,"externalSessionId":"session-1","updatedAt":11.0}"#,
        )
        .unwrap();
        let second = scan_runtime_bindings(&dir, &mut state);
        assert_eq!(second.len(), 1);
        assert_eq!(
            second[0].binding.external_session_id.as_deref(),
            Some("session-1")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn runtime_binding_project_name_falls_back_to_path_basename() {
        let binding = AIRuntimeBinding {
            runtime_binding_id: "bind-1".to_string(),
            terminal_id: "term-1".to_string(),
            terminal_instance_id: None,
            tool: "codex".to_string(),
            project_id: "project-1".to_string(),
            project_name: String::new(),
            project_path: Some("/tmp/codux-gpui/".to_string()),
            session_title: "Codex".to_string(),
            launch_started_at: 10.0,
            external_session_id: None,
            transcript_path: None,
            model: None,
            session_origin: None,
            updated_at: 10.0,
        }
        .normalized()
        .expect("binding");

        assert_eq!(binding.project_name, "codux-gpui");
    }

    #[test]
    fn read_changed_runtime_binding_skips_unchanged_signature() {
        let dir = std::env::temp_dir().join(format!("codux-runtime-bindings-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binding.json");
        fs::write(
            &path,
            r#"{"runtimeBindingId":"bind-1","terminalId":"term-1","terminalInstanceId":"pty-1","tool":"codex","projectId":"project-1","projectPath":"/tmp/project","sessionTitle":"Codex","launchStartedAt":10.0,"updatedAt":10.0}"#,
        )
        .unwrap();
        let signature = runtime_file_signature(&path).unwrap();
        let mut state = AIRuntimeBindingScanState::default();

        let first = read_changed_runtime_binding(path.clone(), signature, &mut state);
        assert!(first.is_some());
        let second = read_changed_runtime_binding(path.clone(), signature, &mut state);
        assert!(second.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clear_runtime_bindings_removes_only_json_files() {
        let dir = std::env::temp_dir().join(format!("codux-runtime-bindings-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one.json"), b"{}").unwrap();
        fs::write(dir.join("two.json"), b"{}").unwrap();
        fs::write(dir.join("keep.tmp"), b"ignored").unwrap();

        let removed = clear_runtime_bindings(&dir);

        assert_eq!(removed, 2);
        assert!(!dir.join("one.json").exists());
        assert!(!dir.join("two.json").exists());
        assert!(dir.join("keep.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
