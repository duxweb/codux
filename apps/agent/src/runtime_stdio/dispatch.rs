use super::{RuntimeStdioWriter, terminal::RuntimeStdioTerminals};
use codux_runtime_core::file::{
    file_copy, file_delete, file_list_payload, file_make_directory, file_move,
    file_read_blob_bytes, file_read_payload, file_rename, file_write, file_write_bytes,
};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) struct RuntimeStdioService {
    terminals: RuntimeStdioTerminals,
}

impl RuntimeStdioService {
    pub(super) fn new(data_dir: PathBuf, writer: RuntimeStdioWriter) -> Self {
        Self {
            terminals: RuntimeStdioTerminals::new(data_dir, writer),
        }
    }

    pub(super) fn dispatch(&self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "file.list" => Ok(file_list_payload(
                params.get("path").and_then(Value::as_str),
                params.get("purpose").and_then(Value::as_str),
            )),
            "file.read" => file_read_payload(required_str(params, "path")?),
            "file.readBytes" => {
                use base64::Engine;
                Ok(json!({
                    "bytes": base64::engine::general_purpose::STANDARD.encode(
                        file_read_blob_bytes(required_str(params, "path")?)?
                    )
                }))
            }
            "file.write" => {
                let path = required_str(params, "path")?;
                file_write(path, required_str(params, "content")?)?;
                Ok(json!({ "path": path }))
            }
            "file.rename" => {
                let path = required_str(params, "path")?;
                let new_path = required_str(params, "newPath")?;
                file_rename(path, new_path)?;
                Ok(json!({ "path": path, "newPath": new_path }))
            }
            "file.delete" => {
                let path = required_str(params, "path")?;
                file_delete(path)?;
                Ok(json!({ "path": path }))
            }
            "file.mkdir" => {
                let path = required_str(params, "path")?;
                file_make_directory(path)?;
                Ok(json!({ "path": path }))
            }
            "file.writeBytes" => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(required_str(params, "bytes")?)
                    .map_err(|error| error.to_string())?;
                Ok(json!({
                    "path": file_write_bytes(
                        required_str(params, "directory")?,
                        required_str(params, "name")?,
                        &bytes,
                    )?
                }))
            }
            "file.copy" => Ok(json!({
                "path": file_copy(
                    required_str(params, "path")?,
                    required_str(params, "targetDir")?,
                )?
            })),
            "file.move" => Ok(json!({
                "path": file_move(
                    required_str(params, "path")?,
                    required_str(params, "targetDir")?,
                    params.get("overwrite").and_then(Value::as_bool).unwrap_or(false),
                )?
            })),
            "git.status" => {
                let path = required_str(params, "projectPath")?;
                serde_json::to_value(codux_git::wire::status(path))
                    .map_err(|error| error.to_string())
            }
            "git.invoke" => {
                let path = required_str(params, "projectPath")?;
                let op = required_str(params, "op")?;
                let args = params.get("args").cloned().unwrap_or(Value::Null);
                codux_git::wire::invoke(path, op, &args)?;
                serde_json::to_value(codux_git::wire::status(path))
                    .map_err(|error| error.to_string())
            }
            "git.read" => codux_git::wire::read(
                required_str(params, "projectPath")?,
                required_str(params, "op")?,
                params.get("args").unwrap_or(&Value::Null),
            ),
            "worktree.list" => Ok(crate::worktree::worktree_list_payload(
                required_str(params, "projectId")?,
                required_str(params, "projectPath")?,
            )),
            "worktree.create" | "worktree.remove" | "worktree.merge" => {
                self.worktree_mutation(method, params)
            }
            "terminal.list" => self.terminals.list(),
            "terminal.create" => self.terminals.create(params),
            "terminal.input" => self.terminals.input(params),
            "terminal.resize" => self.terminals.resize(params),
            "terminal.close" => self.terminals.close(params),
            _ => Err(format!("runtime stdio method is not supported: {method}")),
        }
    }

    fn worktree_mutation(&self, method: &str, params: &Value) -> Result<Value, String> {
        let project_id = required_str(params, "projectId")?;
        let project_path = required_str(params, "projectPath")?;
        match method {
            "worktree.create" => {
                return crate::worktree::worktree_create_payload(
                    project_id,
                    project_path,
                    required_str(params, "branchName")?,
                    params.get("baseBranch").and_then(Value::as_str),
                );
            }
            "worktree.merge" => crate::worktree::worktree_merge(
                project_path,
                required_str(params, "worktreePath")?,
                params.get("baseBranch").and_then(Value::as_str),
                params
                    .get("removeBranch")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )?,
            _ => crate::worktree::worktree_remove(
                project_path,
                required_str(params, "worktreePath")?,
                params
                    .get("removeBranch")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )?,
        }
        Ok(crate::worktree::worktree_list_payload(
            project_id,
            project_path,
        ))
    }
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}
