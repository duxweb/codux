use parking_lot::Mutex;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, Weak},
    thread,
    time::Duration,
};

const CACHE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cache");
const CACHE_FILE_NAME: &str = "state-cache.redb";

static CACHE_STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<PersistentCacheStore>>>> = OnceLock::new();

pub struct PersistentCacheStore {
    path: PathBuf,
    database: Database,
    write_tx: flume::Sender<QueuedWrite>,
}

enum QueuedWrite {
    Put {
        namespace: String,
        key: String,
        bytes: Vec<u8>,
    },
    Delete {
        namespace: String,
        key: String,
    },
}

enum PendingWrite {
    Put(Vec<u8>),
    Delete,
}

impl PersistentCacheStore {
    pub fn for_support_dir(support_dir: impl Into<PathBuf>) -> Result<Arc<Self>, String> {
        Self::for_file(support_dir.into().join(CACHE_FILE_NAME))
    }

    pub fn for_file(path: impl Into<PathBuf>) -> Result<Arc<Self>, String> {
        let path = path.into();
        let stores = CACHE_STORES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut stores = stores.lock();
        if let Some(store) = stores.get(&path) {
            return Ok(store.clone());
        }

        let store = Self::open(path.clone())?;
        stores.insert(path, store.clone());
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get_json<T: DeserializeOwned>(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<T>, String> {
        let cache_key = cache_key(namespace, key)?;
        let transaction = self
            .database
            .begin_read()
            .map_err(|error| error.to_string())?;
        let table = match transaction.open_table(CACHE_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let Some(value) = table
            .get(cache_key.as_str())
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        serde_json::from_slice(value.value())
            .map(Some)
            .map_err(|error| error.to_string())
    }

    pub fn put_json<T: Serialize>(
        &self,
        namespace: &str,
        key: &str,
        value: &T,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        self.write_bytes(namespace, key, &bytes)
    }

    pub fn put_json_debounced<T: Serialize>(
        &self,
        namespace: &str,
        key: &str,
        value: &T,
    ) -> Result<(), String> {
        let namespace = normalized_part("cache namespace", namespace)?;
        let key = normalized_part("cache key", key)?;
        let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        self.queue_write(QueuedWrite::Put {
            namespace,
            key,
            bytes,
        })
    }

    pub fn scan_json<T: DeserializeOwned>(
        &self,
        namespace: &str,
    ) -> Result<HashMap<String, T>, String> {
        let namespace = namespace.trim();
        if namespace.is_empty() {
            return Err("cache namespace is required.".to_string());
        }
        let prefix = format!("{namespace}\0");
        let transaction = self
            .database
            .begin_read()
            .map_err(|error| error.to_string())?;
        let table = match transaction.open_table(CACHE_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(HashMap::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut values = HashMap::new();
        for entry in table.iter().map_err(|error| error.to_string())? {
            let (key, value) = entry.map_err(|error| error.to_string())?;
            let key = key.value();
            let Some(item_key) = key.strip_prefix(&prefix) else {
                continue;
            };
            let value = serde_json::from_slice(value.value()).map_err(|error| error.to_string())?;
            values.insert(item_key.to_string(), value);
        }
        Ok(values)
    }

    pub fn delete_json(&self, namespace: &str, key: &str) -> Result<bool, String> {
        let cache_key = cache_key(namespace, key)?;
        let write = self
            .database
            .begin_write()
            .map_err(|error| error.to_string())?;
        let removed = {
            let mut table = write
                .open_table(CACHE_TABLE)
                .map_err(|error| error.to_string())?;
            table
                .remove(cache_key.as_str())
                .map_err(|error| error.to_string())?
                .is_some()
        };
        write.commit().map_err(|error| error.to_string())?;
        Ok(removed)
    }

    pub fn delete_json_debounced(&self, namespace: &str, key: &str) -> Result<(), String> {
        let namespace = normalized_part("cache namespace", namespace)?;
        let key = normalized_part("cache key", key)?;
        self.queue_write(QueuedWrite::Delete { namespace, key })
    }

    fn open(path: PathBuf) -> Result<Arc<Self>, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let database = Database::create(&path).map_err(|error| error.to_string())?;
        {
            let write = database.begin_write().map_err(|error| error.to_string())?;
            {
                write
                    .open_table(CACHE_TABLE)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
        }
        let (write_tx, write_rx) = flume::bounded::<QueuedWrite>(1024);
        let store = Arc::new(Self {
            path,
            database,
            write_tx,
        });
        spawn_debounced_writer(Arc::downgrade(&store), write_rx);
        Ok(store)
    }

    fn queue_write(&self, write: QueuedWrite) -> Result<(), String> {
        self.write_tx.try_send(write).map_err(|error| match error {
            flume::TrySendError::Full(_) => "persistent cache write queue is full.".to_string(),
            flume::TrySendError::Disconnected(_) => {
                "persistent cache write queue is closed.".to_string()
            }
        })
    }

    fn apply_queued_write(&self, write: QueuedWrite) -> Result<(), String> {
        match write {
            QueuedWrite::Put {
                namespace,
                key,
                bytes,
            } => self.write_bytes(&namespace, &key, &bytes),
            QueuedWrite::Delete { namespace, key } => {
                self.delete_json(&namespace, &key).map(|_| ())
            }
        }
    }

    fn write_bytes(&self, namespace: &str, key: &str, bytes: &[u8]) -> Result<(), String> {
        let cache_key = cache_key(namespace, key)?;
        let write = self
            .database
            .begin_write()
            .map_err(|error| error.to_string())?;
        {
            let mut table = write
                .open_table(CACHE_TABLE)
                .map_err(|error| error.to_string())?;
            table
                .insert(cache_key.as_str(), bytes)
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())
    }
}

fn spawn_debounced_writer(
    store: Weak<PersistentCacheStore>,
    write_rx: flume::Receiver<QueuedWrite>,
) {
    thread::Builder::new()
        .name("codux-redb-cache-writer".to_string())
        .spawn(move || {
            while let Ok(write) = write_rx.recv() {
                let mut pending = HashMap::new();
                merge_pending_write(&mut pending, write);
                while let Ok(write) = write_rx.recv_timeout(Duration::from_millis(80)) {
                    merge_pending_write(&mut pending, write);
                }
                let Some(store) = store.upgrade() else {
                    return;
                };
                for ((namespace, key), pending_write) in pending {
                    let write = match pending_write {
                        PendingWrite::Put(bytes) => QueuedWrite::Put {
                            namespace,
                            key,
                            bytes,
                        },
                        PendingWrite::Delete => QueuedWrite::Delete { namespace, key },
                    };
                    if let Err(error) = store.apply_queued_write(write) {
                        crate::runtime_trace::runtime_trace(
                            "config",
                            &format!(
                                "failed to write persistent cache {}: {error}",
                                store.path.display()
                            ),
                        );
                    }
                }
            }
        })
        .expect("spawn redb cache writer");
}

fn merge_pending_write(pending: &mut HashMap<(String, String), PendingWrite>, write: QueuedWrite) {
    match write {
        QueuedWrite::Put {
            namespace,
            key,
            bytes,
        } => {
            pending.insert((namespace, key), PendingWrite::Put(bytes));
        }
        QueuedWrite::Delete { namespace, key } => {
            pending.insert((namespace, key), PendingWrite::Delete);
        }
    }
}

fn cache_key(namespace: &str, key: &str) -> Result<String, String> {
    let namespace = normalized_part("cache namespace", namespace)?;
    let key = normalized_part("cache key", key)?;
    Ok(format!("{namespace}\0{key}"))
}

fn normalized_part(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label} is required."));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn stores_json_by_namespace_and_key() {
        let path = temp_cache_path("json");
        let store = PersistentCacheStore::for_file(path.clone()).expect("cache store");

        store
            .put_json("git", "project-a", &json!({"files": ["a.rs"]}))
            .expect("put json");

        let value = store
            .get_json::<serde_json::Value>("git", "project-a")
            .expect("get json")
            .expect("stored value");
        assert_eq!(value["files"][0], "a.rs");

        let missing = store
            .get_json::<serde_json::Value>("git", "project-b")
            .expect("get missing");
        assert!(missing.is_none());
    }

    #[test]
    fn deletes_json_entries() {
        let path = temp_cache_path("delete");
        let store = PersistentCacheStore::for_file(path.clone()).expect("cache store");
        store.put_json("files", "w1", &json!({"tabs": 2})).unwrap();

        assert!(store.delete_json("files", "w1").unwrap());
        assert!(!store.delete_json("files", "w1").unwrap());
        assert!(
            store
                .get_json::<serde_json::Value>("files", "w1")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn scans_json_entries_by_namespace() {
        let path = temp_cache_path("scan");
        let store = PersistentCacheStore::for_file(path.clone()).expect("cache store");
        store
            .put_json("layout", "one", &json!({"active": 1}))
            .unwrap();
        store
            .put_json("layout", "two", &json!({"active": 2}))
            .unwrap();
        store.put_json("git", "one", &json!({"files": 1})).unwrap();

        let values = store
            .scan_json::<serde_json::Value>("layout")
            .expect("scan namespace");
        assert_eq!(values.len(), 2);
        assert_eq!(values["one"]["active"], 1);
        assert_eq!(values["two"]["active"], 2);
    }

    fn temp_cache_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("codux-persistent-cache-{label}-{nanos}.redb"))
    }
}
