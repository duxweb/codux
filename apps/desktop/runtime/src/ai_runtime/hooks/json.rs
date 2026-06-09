use serde_json::{Map, Value, json};
use std::{fs, path::Path};

pub(super) fn load_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let data = fs::read(path).map_err(|error| error.to_string())?;
    if data.is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_slice(&data).unwrap_or_else(|_| json!({}));
    Ok(value.as_object().cloned().unwrap_or_default())
}

pub(super) fn write_json_object(path: &Path, root: Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let data =
        serde_json::to_vec_pretty(&Value::Object(root)).map_err(|error| error.to_string())?;
    if fs::read(path).ok().as_deref() == Some(data.as_slice()) {
        return Ok(());
    }
    fs::write(path, data).map_err(|error| error.to_string())
}
