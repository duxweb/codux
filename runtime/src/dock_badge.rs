pub fn set_dock_badge_count(count: Option<i64>) -> Result<(), String> {
    set_dock_badge_count_impl(count)
}

#[cfg(target_os = "macos")]
fn set_dock_badge_count_impl(count: Option<i64>) -> Result<(), String> {
    use dispatch2::DispatchQueue;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    fn run_on_main<F>(f: F)
    where
        F: FnOnce(MainThreadMarker) + Send + 'static,
    {
        if let Some(marker) = MainThreadMarker::new() {
            f(marker);
            return;
        }
        DispatchQueue::main().exec_sync(move || {
            let marker = unsafe { MainThreadMarker::new_unchecked() };
            f(marker);
        });
    }

    run_on_main(move |marker| {
        let label = count.map(|value| NSString::from_str(&value.to_string()));
        let app = NSApplication::sharedApplication(marker);
        let dock_tile = app.dockTile();
        dock_tile.setShowsApplicationBadge(label.is_some());
        dock_tile.setBadgeLabel(label.as_deref());
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn set_dock_badge_count_impl(_count: Option<i64>) -> Result<(), String> {
    Ok(())
}
