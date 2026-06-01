use crate::{
    ai_runtime::AIRuntimeBridge,
    runtime_paths::{
        runtime_event_dir_in, runtime_socket_path_in, runtime_temp_dir as default_runtime_temp_dir,
    },
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, net::UnixListener};

#[derive(Clone, Debug)]
pub struct RuntimeIngressService {
    runtime_temp_dir: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeIngressStatus {
    pub socket_path: PathBuf,
    pub event_dir: PathBuf,
    pub started: bool,
    pub message: String,
}

impl RuntimeIngressService {
    pub fn new() -> Self {
        Self {
            runtime_temp_dir: default_runtime_temp_dir(),
        }
    }

    pub fn start_background(&self) -> RuntimeIngressStatus {
        start_background_at(self.runtime_temp_dir.clone(), None)
    }

    pub fn start_background_with_ai_runtime(&self, ai_runtime: Arc<AIRuntimeBridge>) -> RuntimeIngressStatus {
        start_background_at(self.runtime_temp_dir.clone(), Some(ai_runtime))
    }
}

fn start_background_at(
    runtime_temp_dir: PathBuf,
    ai_runtime: Option<Arc<AIRuntimeBridge>>,
) -> RuntimeIngressStatus {
    let socket_path = runtime_socket_path_in(&runtime_temp_dir);
    let event_dir = runtime_event_dir_in(&runtime_temp_dir);

    #[cfg(not(unix))]
    {
        return RuntimeIngressStatus {
            socket_path,
            event_dir,
            started: false,
            message: "runtime socket ingress is only available on Unix".to_string(),
        };
    }

    #[cfg(unix)]
    {
        if let Err(error) = fs::create_dir_all(&event_dir) {
            return RuntimeIngressStatus {
                socket_path,
                event_dir,
                started: false,
                message: format!("failed to create runtime event dir: {error}"),
            };
        }
        if let Some(parent) = socket_path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return RuntimeIngressStatus {
                    socket_path,
                    event_dir,
                    started: false,
                    message: format!("failed to create runtime temp dir: {error}"),
                };
            }
        }

        if socket_path.exists() && unix_socket_is_live(&socket_path) {
            return RuntimeIngressStatus {
                socket_path,
                event_dir,
                started: false,
                message: "runtime socket already owned by another live process".to_string(),
            };
        }
        let _ = fs::remove_file(&socket_path);

        let listener = match UnixListener::bind(&socket_path) {
            Ok(listener) => listener,
            Err(error) => {
                return RuntimeIngressStatus {
                    socket_path,
                    event_dir,
                    started: false,
                    message: format!("failed to bind runtime socket: {error}"),
                };
            }
        };
        let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o700));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_socket = socket_path.clone();
        let thread_event_dir = event_dir.clone();
        let thread_shutdown = shutdown.clone();
        let thread_ai_runtime = ai_runtime.clone();
        let spawn_result = thread::Builder::new()
            .name("codux-gpui-runtime-ingress".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    if thread_shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut stream) = stream else {
                        break;
                    };
                    let mut data = Vec::new();
                    if stream.read_to_end(&mut data).is_ok() && !data.is_empty() {
                        if let Some(ai_runtime) = &thread_ai_runtime {
                            if ai_runtime.submit_runtime_frame(data.clone()).is_ok() {
                                continue;
                            }
                        }
                        let _ = persist_runtime_frame(&thread_event_dir, &data);
                    }
                }
                let _ = fs::remove_file(thread_socket);
            });

        match spawn_result {
            Ok(_) => RuntimeIngressStatus {
                socket_path,
                event_dir,
                started: true,
                message: "runtime socket ingress started".to_string(),
            },
            Err(error) => RuntimeIngressStatus {
                socket_path,
                event_dir,
                started: false,
                message: format!("failed to spawn runtime ingress thread: {error}"),
            },
        }
    }
}

fn persist_runtime_frame(event_dir: &Path, data: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(event_dir).map_err(|error| error.to_string())?;
    let now = now_millis();
    let path = event_dir.join(format!("gpui-{now}.json"));
    let temp_path = event_dir.join(format!("gpui-{now}.json.tmp"));
    fs::write(&temp_path, data).map_err(|error| error.to_string())?;
    fs::rename(&temp_path, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(unix)]
fn unix_socket_is_live(path: &Path) -> bool {
    use std::os::unix::net::UnixStream;
    UnixStream::connect(path)
        .and_then(|mut stream| stream.write_all(b""))
        .is_ok()
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn persists_runtime_frame_as_json_event_file() {
        let dir = std::env::temp_dir().join(format!("codux-gpui-ingress-test-{}", Uuid::new_v4()));
        let payload = br#"{"kind":"ai-hook","payload":{"kind":"promptSubmitted"}}"#;

        let path = persist_runtime_frame(&dir, payload).unwrap();
        let saved = fs::read(&path).unwrap();

        assert_eq!(saved, payload);
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn start_background_creates_socket_and_event_dir() {
        let runtime_dir = std::env::temp_dir().join(format!(
            "cg{}",
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        ));
        let status = start_background_at(runtime_dir.clone(), None);

        #[cfg(unix)]
        {
            assert!(status.started, "{}", status.message);
            assert!(status.socket_path.exists());
            assert!(status.event_dir.is_dir());
            let _ = std::os::unix::net::UnixStream::connect(&status.socket_path)
                .and_then(|mut stream| stream.write_all(br#"{"kind":"ai-hook","payload":{}}"#));
            std::thread::sleep(std::time::Duration::from_millis(50));
            let event_count = fs::read_dir(&status.event_dir)
                .unwrap()
                .filter_map(Result::ok)
                .count();
            assert!(event_count >= 1);
        }

        let _ = fs::remove_file(runtime_socket_path_in(&runtime_dir));
        let _ = fs::remove_dir_all(runtime_dir);
    }
}
