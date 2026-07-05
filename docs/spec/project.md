# Project Overview

## Tech Stack
- Language: Rust
- Runtime: GPUI (Zed's GPU-accelerated UI framework)
- Build: cargo
- Platform: macOS (desktop), Flutter (mobile)

## Conventions
- Code style: rustfmt + clippy
- Testing: `cargo test`, `cargo check`
- Architecture: crates/ (runtime, protocol, terminal) + apps/ (desktop, agent, mobile)
