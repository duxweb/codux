use super::*;

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
