use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WslInstallOperation {
    Distribution,
    Runtime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslInstallProgress {
    pub distribution: String,
    pub operation: WslInstallOperation,
    pub percent: Option<u8>,
    pub message: String,
}

pub(super) type ProgressCallback = Arc<dyn Fn(WslInstallProgress) + Send + Sync>;

#[cfg(target_os = "windows")]
pub(super) fn install_distribution(
    distribution: &str,
    progress: ProgressCallback,
) -> Result<(), String> {
    let mut command = super::command();
    command.args(["--install", distribution, "--no-launch"]);
    run_wsl_command(
        command,
        distribution,
        WslInstallOperation::Distribution,
        progress,
    )
}

#[cfg(not(target_os = "windows"))]
pub(super) fn install_distribution(
    _distribution: &str,
    _progress: ProgressCallback,
) -> Result<(), String> {
    Err("WSL distributions can be installed on Windows only".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn run_runtime_installer(
    distribution: &str,
    script: &str,
    progress: ProgressCallback,
) -> Result<(), String> {
    let mut command = super::command();
    command.args([
        "--distribution",
        distribution,
        "--user",
        "root",
        "--exec",
        "sh",
        "-lc",
        script,
    ]);
    run_wsl_command(
        command,
        distribution,
        WslInstallOperation::Runtime,
        progress,
    )
}

#[cfg(target_os = "windows")]
fn run_wsl_command(
    mut command: std::process::Command,
    distribution: &str,
    operation: WslInstallOperation,
    progress: ProgressCallback,
) -> Result<(), String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;

    enum StreamChunk {
        Stdout(Vec<u8>),
        Stderr(Vec<u8>),
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to start WSL installer: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "WSL installer stdout is unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "WSL installer stderr is unavailable".to_string())?;
    let (sender, receiver) = mpsc::channel();
    let stdout_sender = sender.clone();
    std::thread::spawn(move || read_stream(stdout, stdout_sender, StreamChunk::Stdout));
    std::thread::spawn(move || read_stream(stderr, sender, StreamChunk::Stderr));

    let mut stdout_decoder = WslProgressDecoder::default();
    let mut stderr_decoder = WslProgressDecoder::default();
    let mut messages = Vec::new();
    while let Ok(chunk) = receiver.recv() {
        let decoded = match chunk {
            StreamChunk::Stdout(bytes) => stdout_decoder.push(&bytes),
            StreamChunk::Stderr(bytes) => stderr_decoder.push(&bytes),
        };
        for message in decoded {
            emit_progress(distribution, operation, &message, &progress);
            messages.push(message);
        }
    }
    for message in stdout_decoder.finish() {
        emit_progress(distribution, operation, &message, &progress);
        messages.push(message);
    }
    for message in stderr_decoder.finish() {
        emit_progress(distribution, operation, &message, &progress);
        messages.push(message);
    }
    let status = child
        .wait()
        .map_err(|error| format!("Unable to wait for WSL installer: {error}"))?;
    if status.success() {
        return Ok(());
    } else {
        return Err(messages
            .into_iter()
            .rev()
            .find(|message| !message.trim().is_empty())
            .unwrap_or_else(|| format!("WSL installer exited with {status}")));
    }

    fn read_stream<R: Read + Send + 'static>(
        mut stream: R,
        sender: mpsc::Sender<StreamChunk>,
        wrap: fn(Vec<u8>) -> StreamChunk,
    ) {
        let mut buffer = [0_u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(length) => {
                    if sender.send(wrap(buffer[..length].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn emit_progress(
    distribution: &str,
    operation: WslInstallOperation,
    message: &str,
    progress: &ProgressCallback,
) {
    let message = message.trim();
    if message.is_empty() {
        return;
    }
    progress(WslInstallProgress {
        distribution: distribution.to_string(),
        operation,
        percent: parse_percent(message),
        message: message.to_string(),
    });
}

#[cfg(any(target_os = "windows", test))]
fn parse_percent(message: &str) -> Option<u8> {
    let percent_at = message.rfind('%')?;
    let prefix = &message[..percent_at];
    let start = prefix
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!character.is_ascii_digit() && character != '.')
                .then_some(index + character.len_utf8())
        })
        .unwrap_or(0);
    prefix[start..]
        .trim()
        .parse::<f32>()
        .ok()
        .map(|percent| percent.clamp(0.0, 100.0).round() as u8)
}

#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct WslProgressDecoder {
    bytes: Vec<u8>,
    utf16: Option<bool>,
}

#[cfg(any(target_os = "windows", test))]
impl WslProgressDecoder {
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.bytes.extend_from_slice(bytes);
        self.decode(false)
    }

    fn finish(&mut self) -> Vec<String> {
        self.decode(true)
    }

    fn decode(&mut self, finish: bool) -> Vec<String> {
        if self.utf16.is_none()
            && (finish
                || super::discovery::is_utf16_le_output(&self.bytes)
                || self.bytes.contains(&b'\n')
                || self.bytes.contains(&b'\r')
                || self.bytes.len() >= 256)
        {
            self.utf16 = Some(super::discovery::is_utf16_le_output(&self.bytes));
        }
        let Some(utf16) = self.utf16 else {
            return Vec::new();
        };
        if utf16 {
            self.decode_utf16(finish)
        } else {
            self.decode_utf8(finish)
        }
    }

    fn decode_utf16(&mut self, finish: bool) -> Vec<String> {
        let mut messages = Vec::new();
        if self.bytes.starts_with(&[0xff, 0xfe]) {
            self.bytes.drain(..2);
        }
        let mut start = 0;
        let mut index = 0;
        while index + 1 < self.bytes.len() {
            let unit = u16::from_le_bytes([self.bytes[index], self.bytes[index + 1]]);
            if unit == b'\r' as u16 || unit == b'\n' as u16 {
                if index > start {
                    messages.push(decode_utf16_segment(&self.bytes[start..index]));
                }
                start = index + 2;
            }
            index += 2;
        }
        if finish {
            if start < self.bytes.len() {
                messages.push(decode_utf16_segment(&self.bytes[start..]));
            }
            self.bytes.clear();
        } else if start > 0 {
            self.bytes.drain(..start);
        }
        messages.retain(|message| !message.trim().is_empty());
        messages
    }

    fn decode_utf8(&mut self, finish: bool) -> Vec<String> {
        let mut messages = Vec::new();
        let mut start = 0;
        for (index, byte) in self.bytes.iter().enumerate() {
            if *byte == b'\r' || *byte == b'\n' {
                if index > start {
                    messages.push(String::from_utf8_lossy(&self.bytes[start..index]).into_owned());
                }
                start = index + 1;
            }
        }
        if finish {
            if start < self.bytes.len() {
                messages.push(String::from_utf8_lossy(&self.bytes[start..]).into_owned());
            }
            self.bytes.clear();
        } else if start > 0 {
            self.bytes.drain(..start);
        }
        messages
    }
}

#[cfg(any(target_os = "windows", test))]
fn decode_utf16_segment(bytes: &[u8]) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .filter(|unit| *unit != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer_and_decimal_percentages() {
        assert_eq!(parse_percent("Downloading: 42%"), Some(42));
        assert_eq!(parse_percent("Installing 71.6%"), Some(72));
        assert_eq!(parse_percent("Installing"), None);
    }

    #[test]
    fn decodes_chunked_utf16_progress_updates() {
        let units = "Downloading 12%\rInstalling 80%\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut decoder = WslProgressDecoder::default();
        let mut messages = decoder.push(&units[..7]);
        messages.extend(decoder.push(&units[7..]));
        messages.extend(decoder.finish());
        assert_eq!(messages, ["Downloading 12%", "Installing 80%"]);
    }
}
