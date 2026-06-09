#[cfg(windows)]
fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("runtime-assets")
        .join("icons")
        .join("icon.ico");
    if icon.is_file() {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(&icon.display().to_string());
        let _ = resource.compile();
    }
}

#[cfg(not(windows))]
fn main() {}
