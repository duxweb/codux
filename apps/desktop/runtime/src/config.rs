use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    thread,
    time::Duration,
};

static CONFIG_STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<ConfigStore>>>> = OnceLock::new();
static CONFIG_DOCUMENT_STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<ConfigDocumentStore>>>> =
    OnceLock::new();

pub struct ConfigStore {
    path: PathBuf,
    snapshot: Arc<RwLock<Map<String, Value>>>,
    write_tx: flume::Sender<()>,
}

impl ConfigStore {
    pub fn for_support_dir(support_dir: impl Into<PathBuf>) -> Arc<Self> {
        Self::for_file(state_file_path(support_dir))
    }

    pub fn for_settings_dir(support_dir: impl Into<PathBuf>) -> Arc<Self> {
        Self::for_file(settings_file_path(support_dir))
    }

    pub fn for_file(path: impl Into<PathBuf>) -> Arc<Self> {
        let path = path.into();
        let stores = CONFIG_STORES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut stores = stores.lock();
        if let Some(store) = stores.get(&path) {
            return store.clone();
        }

        let store = Self::load(path.clone());
        stores.insert(path, store.clone());
        store
    }

    pub fn snapshot(&self) -> Map<String, Value> {
        self.snapshot.read().clone()
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.snapshot.read().get(key).cloned()
    }

    pub fn get_as<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key)
            .and_then(|value| serde_json::from_value::<T>(value).ok())
    }

    pub fn get_path(&self, path: &[&str]) -> Option<Value> {
        let snapshot = self.snapshot.read();
        get_path_value(&snapshot, path).cloned()
    }

    pub fn get_path_as<T: DeserializeOwned>(&self, path: &[&str]) -> Option<T> {
        self.get_path(path)
            .and_then(|value| serde_json::from_value::<T>(value).ok())
    }

    pub fn set(&self, key: impl Into<String>, value: Value) -> Result<(), String> {
        self.update(|snapshot| {
            snapshot.insert(key.into(), value);
            Ok(())
        })
    }

    pub fn set_as<T: Serialize>(&self, key: impl Into<String>, value: &T) -> Result<(), String> {
        let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
        self.set(key, value)
    }

    pub fn set_path(&self, path: &[&str], value: Value) -> Result<(), String> {
        if path.is_empty() {
            return Err("config path is empty.".to_string());
        }
        self.update(|snapshot| {
            set_path_value(snapshot, path, value)?;
            Ok(())
        })
    }

    pub fn del(&self, key: &str) -> Result<Option<Value>, String> {
        self.update(|snapshot| Ok(snapshot.remove(key)))
    }

    pub fn del_path(&self, path: &[&str]) -> Result<Option<Value>, String> {
        if path.is_empty() {
            return Err("config path is empty.".to_string());
        }
        self.update(|snapshot| Ok(remove_path_value(snapshot, path)))
    }

    pub fn save_snapshot(&self, snapshot: &Map<String, Value>) -> Result<(), String> {
        *self.snapshot.write() = snapshot.clone();
        self.schedule_write()
    }

    pub fn update<R>(
        &self,
        update: impl FnOnce(&mut Map<String, Value>) -> Result<R, String>,
    ) -> Result<R, String> {
        let result = {
            let mut snapshot = self.snapshot.write();
            update(&mut snapshot)?
        };
        self.schedule_write()?;
        Ok(result)
    }

    fn load(path: PathBuf) -> Arc<Self> {
        let initial = read_snapshot(&path);
        let snapshot = Arc::new(RwLock::new(initial));
        let (write_tx, write_rx) = flume::bounded::<()>(1);
        let writer_snapshot = snapshot.clone();
        let writer_path = path.clone();
        thread::Builder::new()
            .name("codux-state-json-writer".to_string())
            .spawn(move || {
                while write_rx.recv().is_ok() {
                    while write_rx.recv_timeout(Duration::from_millis(80)).is_ok() {}
                    let snapshot = writer_snapshot.read().clone();
                    if let Err(error) = write_snapshot(&writer_path, &snapshot) {
                        crate::runtime_trace::runtime_trace(
                            "config",
                            &format!("failed to write {}: {error}", writer_path.display()),
                        );
                    }
                }
            })
            .expect("spawn state json writer");

        Arc::new(Self {
            path,
            snapshot,
            write_tx,
        })
    }

    fn schedule_write(&self) -> Result<(), String> {
        match self.write_tx.try_send(()) {
            Ok(()) | Err(flume::TrySendError::Full(_)) => Ok(()),
            Err(flume::TrySendError::Disconnected(_)) => {
                let snapshot = self.snapshot.read().clone();
                write_snapshot(&self.path, &snapshot)
            }
        }
    }
}

pub fn flush_all_config_writes() {
    if let Some(stores) = CONFIG_STORES.get() {
        for store in stores.lock().values() {
            let snapshot = store.snapshot.read().clone();
            if let Err(error) = write_snapshot(&store.path, &snapshot) {
                crate::runtime_trace::runtime_trace(
                    "config",
                    &format!("failed to flush {}: {error}", store.path.display()),
                );
            }
        }
    }
    if let Some(stores) = CONFIG_DOCUMENT_STORES.get() {
        for store in stores.lock().values() {
            let snapshot = store.snapshot.read().clone();
            if let Err(error) = write_value_snapshot(&store.path, &snapshot) {
                crate::runtime_trace::runtime_trace(
                    "config",
                    &format!("failed to flush {}: {error}", store.path.display()),
                );
            }
        }
    }
}

pub struct ConfigDocumentStore {
    path: PathBuf,
    snapshot: Arc<RwLock<Value>>,
    write_tx: flume::Sender<()>,
}

impl ConfigDocumentStore {
    pub fn for_file(path: impl Into<PathBuf>) -> Arc<Self> {
        let path = path.into();
        let stores = CONFIG_DOCUMENT_STORES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut stores = stores.lock();
        if let Some(store) = stores.get(&path) {
            return store.clone();
        }

        let store = Self::load(path.clone());
        stores.insert(path, store.clone());
        store
    }

    pub fn snapshot(&self) -> Value {
        self.snapshot.read().clone()
    }

    pub fn snapshot_as<T: DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_value::<T>(self.snapshot()).ok()
    }

    pub fn save_snapshot<T: Serialize>(&self, snapshot: &T) -> Result<(), String> {
        let value = serde_json::to_value(snapshot).map_err(|error| error.to_string())?;
        *self.snapshot.write() = value;
        self.schedule_write()
    }

    pub fn update<R>(
        &self,
        update: impl FnOnce(&mut Value) -> Result<R, String>,
    ) -> Result<R, String> {
        let result = {
            let mut snapshot = self.snapshot.write();
            update(&mut snapshot)?
        };
        self.schedule_write()?;
        Ok(result)
    }

    fn load(path: PathBuf) -> Arc<Self> {
        let initial = read_value_snapshot(&path).unwrap_or(Value::Null);
        let snapshot = Arc::new(RwLock::new(initial));
        let (write_tx, write_rx) = flume::bounded::<()>(1);
        let writer_snapshot = snapshot.clone();
        let writer_path = path.clone();
        thread::Builder::new()
            .name("codux-config-json-writer".to_string())
            .spawn(move || {
                while write_rx.recv().is_ok() {
                    while write_rx.recv_timeout(Duration::from_millis(80)).is_ok() {}
                    let snapshot = writer_snapshot.read().clone();
                    if let Err(error) = write_value_snapshot(&writer_path, &snapshot) {
                        crate::runtime_trace::runtime_trace(
                            "config",
                            &format!("failed to write {}: {error}", writer_path.display()),
                        );
                    }
                }
            })
            .expect("spawn config json writer");

        Arc::new(Self {
            path,
            snapshot,
            write_tx,
        })
    }

    fn schedule_write(&self) -> Result<(), String> {
        match self.write_tx.try_send(()) {
            Ok(()) | Err(flume::TrySendError::Full(_)) => Ok(()),
            Err(flume::TrySendError::Disconnected(_)) => {
                let snapshot = self.snapshot.read().clone();
                write_value_snapshot(&self.path, &snapshot)
            }
        }
    }
}

pub fn state_file_path(support_dir: impl Into<PathBuf>) -> PathBuf {
    support_dir.into().join("state.json")
}

pub fn settings_file_path(support_dir: impl Into<PathBuf>) -> PathBuf {
    support_dir.into().join("settings.json")
}

pub fn raw_state_snapshot(path: &Path) -> Map<String, Value> {
    ConfigStore::for_file(path.to_path_buf()).snapshot()
}

pub fn save_raw_state_snapshot(path: &Path, snapshot: &Map<String, Value>) -> Result<(), String> {
    let mut snapshot = snapshot.clone();
    sanitize_state_snapshot(&mut snapshot);
    ConfigStore::for_file(path.to_path_buf()).save_snapshot(&snapshot)
}

fn sanitize_state_snapshot(snapshot: &mut Map<String, Value>) {
    snapshot.remove("terminalLayouts");
    snapshot.remove("fileEditorLayouts");
    if let Some(worktrees) = snapshot.get_mut("worktrees").and_then(Value::as_array_mut) {
        for worktree in worktrees {
            if let Some(worktree) = worktree.as_object_mut() {
                worktree.remove("gitSummary");
                worktree.remove("git_summary");
            }
        }
    }
}

fn get_path_value<'a>(snapshot: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut value = snapshot.get(*first)?;
    for key in rest {
        value = value.as_object()?.get(*key)?;
    }
    Some(value)
}

fn set_path_value(
    snapshot: &mut Map<String, Value>,
    path: &[&str],
    value: Value,
) -> Result<(), String> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| "config path is empty.".to_string())?;
    let mut current = snapshot;
    for key in parents {
        if !matches!(current.get(*key), Some(Value::Object(_))) {
            current.insert((*key).to_string(), Value::Object(Map::new()));
        }
        current = current
            .get_mut(*key)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("{key} is not an object."))?;
    }
    current.insert((*last).to_string(), value);
    Ok(())
}

fn remove_path_value(snapshot: &mut Map<String, Value>, path: &[&str]) -> Option<Value> {
    let (last, parents) = path.split_last()?;
    let mut current = snapshot;
    for key in parents {
        current = current.get_mut(*key)?.as_object_mut()?;
    }
    current.remove(*last)
}

fn read_snapshot(path: &Path) -> Map<String, Value> {
    read_value_snapshot(path)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_snapshot(path: &Path, snapshot: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(path, format!("{content}\n")).map_err(|error| error.to_string())
}

fn read_value_snapshot(path: &Path) -> Result<Value, String> {
    fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|content| {
            serde_json::from_str::<Value>(&content).map_err(|error| error.to_string())
        })
}

fn write_value_snapshot(path: &Path, snapshot: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(path, format!("{content}\n")).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn document_store_keeps_root_arrays_in_memory_snapshot() {
        let path = temp_config_path("document-array");
        let store = ConfigDocumentStore::for_file(path.clone());

        store
            .save_snapshot(&vec![json!({"id": "one"}), json!({"id": "two"})])
            .expect("save document");

        let values = store.snapshot_as::<Vec<Value>>().expect("array snapshot");
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].get("id").and_then(Value::as_str), Some("one"));

        let same_store = ConfigDocumentStore::for_file(path.clone());
        assert_eq!(same_store.snapshot_as::<Vec<Value>>().unwrap().len(), 2);
        fs::remove_file(path).ok();
    }

    #[test]
    fn raw_state_save_strips_redb_owned_fields() {
        let path = temp_config_path("state-sanitize");
        let mut snapshot = Map::new();
        snapshot.insert("terminalLayouts".to_string(), json!({"p1": {}}));
        snapshot.insert("fileEditorLayouts".to_string(), json!({"w1": {}}));
        snapshot.insert(
            "worktrees".to_string(),
            json!([
                {"id": "w1", "gitSummary": {"changes": 9}, "git_summary": {"changes": 8}}
            ]),
        );

        save_raw_state_snapshot(&path, &snapshot).expect("save state");
        std::thread::sleep(Duration::from_millis(140));
        let saved = raw_state_snapshot(&path);

        assert!(saved.get("terminalLayouts").is_none());
        assert!(saved.get("fileEditorLayouts").is_none());
        let worktree = saved["worktrees"].as_array().unwrap()[0]
            .as_object()
            .unwrap();
        assert!(worktree.get("gitSummary").is_none());
        assert!(worktree.get("git_summary").is_none());

        fs::remove_file(path).ok();
    }

    #[test]
    fn flush_all_config_writes_persists_pending_state_snapshot() {
        let path = temp_config_path("flush-state");
        let mut snapshot = Map::new();
        snapshot.insert(
            "projects".to_string(),
            json!([{ "id": "p1", "name": "Project", "path": "/tmp/project" }]),
        );

        save_raw_state_snapshot(&path, &snapshot).expect("save state");
        flush_all_config_writes();
        let content = fs::read_to_string(&path).expect("flushed state file");

        assert!(content.contains("\"projects\""));
        assert!(content.contains("\"p1\""));
        fs::remove_file(path).ok();
    }

    fn temp_config_path(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("codux-{label}-{stamp}.json"))
    }
}
