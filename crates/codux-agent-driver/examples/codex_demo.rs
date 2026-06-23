//! Phase 1 acceptance harness: drive a real `codex app-server` through the
//! `CodexSession` driver and print the merged timeline live — the Rust
//! equivalent of the Phase 0 Python spike, proving the driver layer end to end.
//!
//!   cargo run -p codux-agent-driver --example codex_demo
//!   CODEX_BIN=/path/to/codex cargo run -p codux-agent-driver --example codex_demo -- "your prompt"

use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use codux_agent_driver::{
    AgentEvent, ApprovalDecision, CodexAgentDriver, CodexSession, ItemStatus, SessionConfig,
};

const WRAPPER_CODEX: &str =
    "/var/folders/6h/2xk4m_gj7l74pkhhxv0sdp280000gn/T/codux/runtime-root/scripts/wrappers/bin/codex";

fn main() -> Result<(), String> {
    let program = env::var("CODEX_BIN").unwrap_or_else(|_| WRAPPER_CODEX.to_string());
    let cwd = env::current_dir().unwrap().to_string_lossy().to_string();
    let prompt = env::args().nth(1).unwrap_or_else(|| {
        "List the top-level files and directories by running `ls`, then say in one \
         sentence what kind of project this is."
            .to_string()
    });

    let driver = CodexAgentDriver {
        program,
        env: Vec::new(),
    };
    let cfg = SessionConfig::read_only(&cwd);

    let done = Arc::new(AtomicBool::new(false));

    // The session applies merges before calling this sink, so we can either react
    // to events (as here) or just read `session.timeline_snapshot()` at the end.
    let sink_done = done.clone();
    let session_for_sink: Arc<std::sync::OnceLock<CodexSession>> = Arc::new(std::sync::OnceLock::new());
    let session_slot = session_for_sink.clone();
    let sink = Box::new(move |ev: &AgentEvent| match ev {
        AgentEvent::ThreadStarted { thread_id } => println!("🧵 thread {thread_id}"),
        AgentEvent::TurnStarted => println!("▶  turn started"),
        AgentEvent::ItemStarted(it) => println!("·  start  [{:?}] {}", it.kind, it.title),
        AgentEvent::MessageDelta { text, .. } => {
            print!("{text}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        AgentEvent::ReasoningDelta { .. } => {}
        AgentEvent::CommandOutputDelta { .. } => {}
        AgentEvent::ItemCompleted(it) => {
            println!("\n■  done   [{:?}] {} (exit={:?})", it.kind, it.title, it.exit_code)
        }
        AgentEvent::TokenUsage(u) => println!("📊 tokens total={} (in={} out={})", u.total_tokens, u.input_tokens, u.output_tokens),
        AgentEvent::ApprovalRequest(req) => {
            println!("🔐 approval «{}»: {} -> auto-accept", req.method, req.summary);
            if let Some(s) = session_slot.get() {
                let _ = s.respond_approval(&req.token, ApprovalDecision::Accept);
            }
        }
        AgentEvent::Status(s) => println!("→  {s}"),
        AgentEvent::TurnCompleted => {
            println!("\n✅ turn completed");
            sink_done.store(true, Ordering::SeqCst);
        }
        AgentEvent::Error(e) => println!("❌ {e}"),
    });

    println!("=== starting codex session (cwd={cwd}) ===");
    let session = CodexSession::start(&driver, &cfg, sink)?;
    let _ = session_for_sink.set(session.clone());
    println!("thread_id = {}\nprompt: {prompt}\n", session.thread_id());

    session.send_user_message(&prompt)?;

    let deadline = Instant::now() + Duration::from_secs(120);
    while !done.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("\n=== merged timeline ({} items) ===", session.timeline_snapshot().len());
    for it in session.timeline_snapshot() {
        let mark = match it.status {
            ItemStatus::Completed => "✓",
            ItemStatus::Failed => "✗",
            ItemStatus::InProgress => "…",
        };
        println!("{mark} [{:?}] {}", it.kind, it.title);
        if !it.text.is_empty() {
            println!("    {}", it.text.replace('\n', "\n    "));
        }
        if !it.output.is_empty() {
            let out = it.output.trim_end();
            println!("    out: {}", out.replace('\n', "\n    out: "));
        }
    }

    session.shutdown();
    Ok(())
}
