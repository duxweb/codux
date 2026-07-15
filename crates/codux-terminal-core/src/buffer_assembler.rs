use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalBufferAssemblyResult {
    pub ready: bool,
    pub progress: Option<f64>,
    pub payload: Option<serde_json::Value>,
}

pub struct TerminalBufferAssembler {
    max_chars: usize,
    assemblies: HashMap<String, TerminalBufferAssembly>,
}

impl TerminalBufferAssembler {
    pub fn new(max_chars: usize) -> Self {
        Self {
            max_chars,
            assemblies: HashMap::new(),
        }
    }

    pub fn accept(
        &mut self,
        session_id: &str,
        payload: serde_json::Value,
    ) -> TerminalBufferAssemblyResult {
        if payload.get("buffer").and_then(|value| value.as_bool()) != Some(true)
            || payload.get("chunked").and_then(|value| value.as_bool()) != Some(true)
        {
            return TerminalBufferAssemblyResult {
                ready: true,
                progress: None,
                payload: Some(payload),
            };
        }

        let snapshot_id = payload
            .get("snapshotId")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let chunk_index = payload.get("chunkIndex").and_then(json_i64_value);
        let chunk_count = payload.get("chunkCount").and_then(json_i64_value);
        let Some(chunk_index) = chunk_index else {
            return TerminalBufferAssemblyResult::pending();
        };
        let Some(chunk_count) = chunk_count else {
            return TerminalBufferAssemblyResult::pending();
        };
        if snapshot_id.is_empty()
            || chunk_count <= 0
            || chunk_index < 0
            || chunk_index >= chunk_count
        {
            return TerminalBufferAssemblyResult::pending();
        }

        let request_id = payload
            .get("requestId")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let key = format!("{session_id}:{request_id}:{snapshot_id}");
        let prefix = format!("{session_id}:{request_id}:");
        self.assemblies
            .retain(|other_key, _| !other_key.starts_with(&prefix) || other_key == &key);

        let assembly =
            self.assemblies
                .entry(key.clone())
                .or_insert_with(|| TerminalBufferAssembly {
                    chunk_count: chunk_count as usize,
                    base_payload: payload.clone(),
                    max_chars: self.max_chars,
                    chunks: HashMap::new(),
                    chars: 0,
                });
        if assembly.chunk_count != chunk_count as usize {
            self.assemblies.remove(&key);
            return TerminalBufferAssemblyResult::pending();
        }
        assembly.merge_metadata(&payload);

        let data = payload
            .get("data")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assembly.add(chunk_index as usize, data);
        if !assembly.complete() {
            return TerminalBufferAssemblyResult {
                ready: false,
                progress: Some(assembly.progress()),
                payload: None,
            };
        }

        let payload = assembly.payload();
        self.assemblies.remove(&key);
        TerminalBufferAssemblyResult {
            ready: true,
            progress: Some(1.0),
            payload: Some(payload),
        }
    }

    pub fn remove(&mut self, session_id: &str) {
        let prefix = format!("{session_id}:");
        self.assemblies.retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn reset(&mut self) {
        self.assemblies.clear();
    }
}

impl TerminalBufferAssemblyResult {
    fn pending() -> Self {
        Self {
            ready: false,
            progress: None,
            payload: None,
        }
    }
}

#[derive(Debug, Clone)]
struct TerminalBufferAssembly {
    chunk_count: usize,
    base_payload: serde_json::Value,
    max_chars: usize,
    chunks: HashMap<usize, String>,
    chars: usize,
}

impl TerminalBufferAssembly {
    fn merge_metadata(&mut self, payload: &serde_json::Value) {
        let Some(base) = self.base_payload.as_object_mut() else {
            return;
        };
        let Some(current) = payload.as_object() else {
            return;
        };
        for key in ["screenData", "screenWrappedRows"] {
            if !base.contains_key(key)
                && let Some(value) = current.get(key)
            {
                base.insert(key.to_string(), value.clone());
            }
        }
    }

    fn add(&mut self, index: usize, data: &str) {
        if self.chunks.contains_key(&index) {
            return;
        }
        let next_chars = self.chars.saturating_add(data.chars().count());
        if next_chars > self.max_chars {
            return;
        }
        self.chunks.insert(index, data.to_string());
        self.chars = next_chars;
    }

    fn complete(&self) -> bool {
        self.chunks.len() == self.chunk_count
    }

    fn progress(&self) -> f64 {
        if self.chunk_count == 0 {
            0.0
        } else {
            self.chunks.len() as f64 / self.chunk_count as f64
        }
    }

    fn payload(&self) -> serde_json::Value {
        let data = (0..self.chunk_count)
            .map(|index| self.chunks.get(&index).cloned().unwrap_or_default())
            .collect::<String>();
        let mut payload = self.base_payload.clone();
        if let Some(object) = payload.as_object_mut() {
            object.insert("data".to_string(), serde_json::Value::String(data));
            let offset = object
                .get("startOffset")
                .cloned()
                .or_else(|| object.get("offset").cloned())
                .unwrap_or(serde_json::Value::Null);
            object.insert("offset".to_string(), offset);
            object.insert("chunked".to_string(), serde_json::Value::Bool(false));
            object.insert("assembled".to_string(), serde_json::Value::Bool(true));
            object.remove("chunkIndex");
            object.remove("chunkCount");
        }
        payload
    }
}

fn json_i64_value(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
}
