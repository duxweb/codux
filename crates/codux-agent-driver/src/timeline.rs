//! The normalized, CLI-agnostic conversation model the UI renders, plus the
//! merge logic that collapses streamed deltas and start/complete pairs into one
//! item per id — the "clean, merged" display (à la the Codex/Claude desktop apps).
//!
//! This is an in-memory projection of the protocol stream. Nothing is persisted;
//! `raw` keeps the untouched protocol item so the UI can show original values
//! without us re-transcribing them.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimelineKind {
    UserPrompt,
    AssistantMessage,
    Reasoning,
    Plan,
    Command,
    FileChange,
    ToolCall,
    Error,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub id: String,
    pub kind: TimelineKind,
    /// The CLI's own item type, e.g. `commandExecution`, `mcpToolCall`.
    pub item_type: String,
    /// Short header line for the card (command line, tool name, or type).
    pub title: String,
    /// Accumulated assistant/reasoning text.
    pub text: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub exit_code: Option<i64>,
    /// Accumulated command/tool output.
    pub output: String,
    pub status: ItemStatus,
    /// The original protocol item, untouched, for faithful rendering.
    pub raw: Value,
}

impl TimelineItem {
    fn placeholder(id: &str, kind: TimelineKind, item_type: &str) -> Self {
        Self {
            id: id.to_string(),
            kind,
            item_type: item_type.to_string(),
            title: String::new(),
            text: String::new(),
            command: None,
            cwd: None,
            exit_code: None,
            output: String::new(),
            status: ItemStatus::InProgress,
            raw: Value::Null,
        }
    }
}

/// In-memory, ordered conversation. `index` maps item id → position in `items`.
#[derive(Default)]
pub struct Timeline {
    items: Vec<TimelineItem>,
    index: HashMap<String, usize>,
}

impl Timeline {
    pub fn items(&self) -> &[TimelineItem] {
        &self.items
    }

    /// Insert or replace an item by id (start/complete both route here).
    pub fn upsert(&mut self, item: TimelineItem) {
        if let Some(&pos) = self.index.get(&item.id) {
            self.items[pos] = item;
        } else {
            self.index.insert(item.id.clone(), self.items.len());
            self.items.push(item);
        }
    }

    fn slot(&mut self, id: &str, kind: TimelineKind, item_type: &str) -> &mut TimelineItem {
        if let Some(&pos) = self.index.get(id) {
            return &mut self.items[pos];
        }
        self.index.insert(id.to_string(), self.items.len());
        self.items
            .push(TimelineItem::placeholder(id, kind, item_type));
        self.items.last_mut().unwrap()
    }

    pub fn append_text(&mut self, id: &str, delta: &str, kind: TimelineKind, item_type: &str) {
        self.slot(id, kind, item_type).text.push_str(delta);
    }

    pub fn append_output(&mut self, id: &str, delta: &str) {
        self.slot(id, TimelineKind::Command, "commandExecution")
            .output
            .push_str(delta);
    }

    /// Replace a fileChange item's raw `changes` (patchUpdated keeps diffs live).
    pub fn set_changes(&mut self, id: &str, changes: Value) {
        let item = self.slot(id, TimelineKind::FileChange, "fileChange");
        if item.raw.is_null() {
            item.raw = serde_json::json!({});
        }
        item.raw["changes"] = changes;
    }

    /// Count items of a given kind (e.g. user turns).
    pub fn count_kind(&self, kind: TimelineKind) -> usize {
        self.items.iter().filter(|it| it.kind == kind).count()
    }

    /// Drop the item with `id` and everything after it, rebuilding the index.
    /// Used to mirror a `thread/rollback` when editing a past message.
    pub fn truncate_before(&mut self, id: &str) {
        if let Some(&pos) = self.index.get(id) {
            self.items.truncate(pos);
            self.index.clear();
            for (i, item) in self.items.iter().enumerate() {
                self.index.insert(item.id.clone(), i);
            }
        }
    }
}
