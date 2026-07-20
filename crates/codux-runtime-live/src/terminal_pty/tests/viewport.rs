use super::*;

#[cfg(unix)]
#[test]
fn runtime_screen_answers_colors_before_ui_attach_and_tracks_viewport_owner() {
    let manager = TerminalManager::new();
    manager.set_query_colors(TerminalQueryColors {
        foreground: (0x2a, 0x31, 0x40),
        background: (0xfa, 0xfb, 0xfc),
    });
    let temp = std::env::temp_dir().join(format!("codux-terminal-query-colors-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp).unwrap();
    let command = concat!(
        "stty -echo -icanon min 1 time 20; ",
        "printf '\\033]10;?\\a'; dd bs=1 count=24 2>/dev/null; ",
        "dd bs=1 count=1 >/dev/null 2>&1; ",
        "printf '\\033]10;?\\a'; dd bs=1 count=24 2>/dev/null; ",
        "dd bs=1 count=1 >/dev/null 2>&1; ",
        "printf '\\033]10;?\\a'; dd bs=1 count=24 2>/dev/null"
    );
    let (session, output) = manager
        .attach_or_create_with_context(
            TerminalPtyConfig {
                shell: Some("/bin/sh".to_string()),
                command: Some(command.to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            None,
            Arc::new(|_| true),
        )
        .expect("create terminal");
    let handle = session.clone_handle();
    let local_report = "\x1b]10;rgb:2a2a/3131/4040\x07";
    let updated_local_report = "\x1b]10;rgb:1111/2222/3333\x07";
    let remote_report = "\x1b]10;rgb:e6e6/eded/f3f3\x07";

    assert!(
        recv_until_contains(&output, local_report, Duration::from_secs(2)).contains(local_report)
    );
    handle
        .claim_viewport("remote:phone")
        .expect("claim remote viewport");
    manager.set_query_colors(TerminalQueryColors {
        foreground: (0x11, 0x22, 0x33),
        background: (0xf1, 0xf2, 0xf3),
    });
    session.write(b"x").expect("continue remote query");
    assert!(
        recv_until_contains(&output, remote_report, Duration::from_secs(2)).contains(remote_report)
    );
    handle
        .release_viewport("remote:phone")
        .expect("release remote viewport")
        .expect("released viewport state");
    session.write(b"x").expect("continue local query");
    assert!(
        recv_until_contains(&output, updated_local_report, Duration::from_secs(2))
            .contains(updated_local_report)
    );

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}

#[cfg(unix)]
#[test]
fn automatic_viewport_claim_respects_explicit_handoffs() {
    let manager = TerminalManager::new();
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-viewport-auto-claim-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&temp).unwrap();
    let session_id = manager
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    let session = manager.session(&session_id).expect("session");
    let handle = session.clone_handle();

    assert!(!session.viewport.lock().explicit_owner);
    let state = handle
        .claim_viewport_auto("remote:phone")
        .expect("automatic phone claim");
    assert_eq!(state.owner, "remote:phone");
    assert!(!session.viewport.lock().explicit_owner);

    handle
        .claim_viewport(terminal_viewport_local_owner())
        .expect("explicit desktop claim");
    assert!(session.viewport.lock().explicit_owner);
    let state = handle
        .claim_viewport_auto("remote:phone")
        .expect("blocked automatic phone claim");
    assert_eq!(state.owner, terminal_viewport_local_owner());
    assert!(session.viewport.lock().explicit_owner);

    let state = handle
        .claim_viewport("remote:phone")
        .expect("forced phone claim");
    assert_eq!(state.owner, "remote:phone");
    assert!(session.viewport.lock().explicit_owner);

    handle
        .release_viewport("remote:phone")
        .expect("release phone viewport")
        .expect("released viewport state");
    assert!(!session.viewport.lock().explicit_owner);
    let state = handle
        .claim_viewport_auto("remote:phone")
        .expect("automatic claim after release");
    assert_eq!(state.owner, "remote:phone");

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}

#[cfg(unix)]
#[test]
fn remote_visible_viewport_expires_back_to_desktop() {
    let manager = TerminalManager::new();
    let temp =
        std::env::temp_dir().join(format!("codux-terminal-viewport-lock-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp).unwrap();
    let session_id = manager
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    let session = manager.session(&session_id).expect("session");
    let handle = session.clone_handle();

    handle
        .claim_viewport("remote:phone")
        .expect("remote visible claim");
    handle
        .resize_viewport("remote:phone", 72, 18)
        .expect("remote resize")
        .expect("remote resize accepted");
    {
        let mut viewport = session.viewport.lock();
        viewport.expires_at = Instant::now() - Duration::from_secs(1);
    }

    let expired = handle
        .release_expired_viewport_lease()
        .expect("expired viewport state");
    assert_eq!(expired.owner, terminal_viewport_local_owner());
    // Ownership is a handoff token: the remote drove the FULL grid (72x18)
    // while it held the lease, so on expiry the grid keeps that size until
    // the desktop reclaims it by resizing (next assertion).
    assert_eq!((expired.cols, expired.rows), (72, 18));

    let accepted = handle
        .resize_viewport(terminal_viewport_local_owner(), 100, 32)
        .expect("desktop resize after lease expiry")
        .expect("desktop resize accepted");
    let state = handle.viewport_state();
    assert_eq!(state.owner, terminal_viewport_local_owner());
    assert_eq!((accepted.cols, accepted.rows), (100, 32));

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}

#[test]
fn expired_remote_viewport_hands_off_to_another_active_viewer() {
    let manager = TerminalManager::new();
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-viewport-handoff-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&temp).unwrap();
    let session_id = manager
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    let session = manager.session(&session_id).expect("session");
    let handle = session.clone_handle();

    handle
        .claim_viewport("remote:phone-a")
        .expect("phone-a claim");
    {
        let mut viewport = session.viewport.lock();
        viewport.expires_at = Instant::now() - Duration::from_secs(1);
    }

    // A resolver names phone-b as another active viewer: the expired lease is
    // handed to it instead of snapping back to the host desktop.
    let reclaimed = handle
        .reclaim_expired_viewport_lease(|expired| {
            assert_eq!(expired, "remote:phone-a");
            Some("remote:phone-b".to_string())
        })
        .expect("handoff state");
    assert_eq!(reclaimed.owner, "remote:phone-b");
    assert_eq!(handle.viewport_state().owner, "remote:phone-b");

    // With no replacement viewer, expiry reverts to the host desktop.
    {
        let mut viewport = session.viewport.lock();
        viewport.expires_at = Instant::now() - Duration::from_secs(1);
    }
    let reverted = handle
        .reclaim_expired_viewport_lease(|_| None)
        .expect("revert state");
    assert_eq!(reverted.owner, terminal_viewport_local_owner());

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}
#[cfg(unix)]
#[test]
fn desktop_resize_waits_for_remote_viewport_release() {
    let manager = TerminalManager::new();
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-viewport-release-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&temp).unwrap();
    let session_id = manager
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    let session = manager.session(&session_id).expect("session");
    let handle = session.clone_handle();

    handle.claim_viewport("remote:phone").expect("remote claim");
    handle
        .resize_viewport("remote:phone", 72, 18)
        .expect("remote resize")
        .expect("remote resize accepted");

    let ignored = handle
        .resize_viewport(terminal_viewport_local_owner(), 120, 40)
        .expect("desktop resize while remote owns");
    assert!(ignored.is_none());
    assert_eq!(handle.viewport_state().owner, "remote:phone");
    // The owning remote drives the FULL grid (cols AND rows), so it reflows
    // to its 72x18 -- a handoff, not a host-floored mirror.
    assert_eq!(
        (handle.viewport_state().cols, handle.viewport_state().rows),
        (72, 18)
    );

    handle
        .release_viewport("remote:phone")
        .expect("remote release")
        .expect("release state");
    let accepted = handle
        .resize_viewport(terminal_viewport_local_owner(), 120, 40)
        .expect("desktop resize after release")
        .expect("desktop resize accepted");
    assert_eq!(accepted.owner, terminal_viewport_local_owner());
    assert_eq!((accepted.cols, accepted.rows), (120, 40));

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}
#[cfg(unix)]
#[test]
fn viewport_keepalive_prevents_remote_lease_expiry() {
    let manager = TerminalManager::new();
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-viewport-keepalive-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&temp).unwrap();
    let session_id = manager
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(temp.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    let session = manager.session(&session_id).expect("session");
    let handle = session.clone_handle();

    handle.claim_viewport("remote:phone").expect("remote claim");
    {
        let mut viewport = session.viewport.lock();
        viewport.expires_at = Instant::now() - Duration::from_secs(1);
    }
    handle.touch_viewport_lease("remote:phone");
    assert!(handle.release_expired_viewport_lease().is_none());
    assert_eq!(handle.viewport_state().owner, "remote:phone");

    let _ = session.kill();
    fs::remove_dir_all(temp).ok();
}
