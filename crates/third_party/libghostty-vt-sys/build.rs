use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned ghostty commit. Update this to pull a newer version.
const GHOSTTY_REPO: &str = "https://github.com/ghostty-org/ghostty.git";
const GHOSTTY_COMMIT: &str = "bebca84668947bfc92b9a30ed58712e1c34eee1d";
const UUCODE_URL: &str = "https://deps.files.ghostty.org/uucode-0.2.0-ZZjBPqZVVABQepOqZHR7vV_NcaN-wats0IB6o-Exj6m9.tar.gz";

fn main() {
    // docs.rs has no Zig toolchain. The checked-in bindings in src/bindings.rs
    // are enough for generating documentation, so skip the entire native
    // build when running under docs.rs.
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_SYS_NO_VENDOR");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=ZIG");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=HOST");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));
    let target = env::var("TARGET").expect("TARGET must be set");
    let host = env::var("HOST").expect("HOST must be set");

    // Locate ghostty source: env override > fetch into OUT_DIR.
    let ghostty_dir = match env::var("GHOSTTY_SOURCE_DIR") {
        Ok(dir) => {
            let p = PathBuf::from(dir);
            assert!(
                p.join("build.zig").exists(),
                "GHOSTTY_SOURCE_DIR does not contain build.zig: {}",
                p.display()
            );
            p
        }
        Err(_) => fetch_ghostty(&out_dir),
    };

    patch_ghostty_for_target(&ghostty_dir, &target);

    // Build libghostty-vt via zig. Zig 0.15's Windows build runner asserts
    // when some RunStep paths are rooted at an absolute prefix, so keep the
    // install prefix relative to the Ghostty source tree on Windows.
    let platform = TargetPlatform::from_triple(&target);
    let install_prefix = zig_install_prefix(&ghostty_dir, &out_dir, platform);
    let install_prefix_arg = zig_install_prefix_arg(&install_prefix, platform);
    if install_prefix.exists() {
        std::fs::remove_dir_all(&install_prefix).unwrap_or_else(|error| {
            panic!(
                "failed to remove stale Ghostty install prefix {}: {error}",
                install_prefix.display()
            )
        });
    }

    let zig = zig_command();

    if platform == TargetPlatform::Windows {
        vendor_uucode_for_windows(&zig, &ghostty_dir);
        let mut fetch = Command::new(&zig);
        fetch.arg("build");
        add_ghostty_zig_build_args(&mut fetch, &install_prefix_arg);
        add_zig_target_args(&mut fetch, &target, &host);
        fetch.arg("--fetch=needed").current_dir(&ghostty_dir);
        run(fetch, "zig fetch ghostty dependencies");
    }

    let mut build = Command::new(&zig);
    build.arg("build");
    add_ghostty_zig_build_args(&mut build, &install_prefix_arg);
    add_zig_target_args(&mut build, &target, &host);
    build.current_dir(&ghostty_dir);

    run(build, "zig build");

    let lib_dir = install_prefix.join("lib");
    let include_dir = install_prefix.join("include");

    let static_lib_name = platform.static_lib_name();
    let static_link_name = platform.static_link_name();
    let shared_lib_path = if platform == TargetPlatform::Android {
        let shared_lib_name = platform
            .shared_lib_name()
            .expect("Android must have a shared library name");
        Some(
            find_nonempty_file(&ghostty_dir.join(".zig-cache").join("o"), shared_lib_name)
                .unwrap_or_else(|| {
                    panic!(
                        "expected non-empty shared library named {shared_lib_name} under {}",
                        ghostty_dir.join(".zig-cache").join("o").display()
                    )
                }),
        )
    } else {
        platform.shared_lib_name().map(|name| lib_dir.join(name))
    };

    if let Some(shared_lib_path) = &shared_lib_path {
        assert!(
            shared_lib_path.exists(),
            "expected shared library at {}",
            shared_lib_path.display()
        );
    }
    let static_lib_path = find_nonempty_file(&lib_dir, static_lib_name).unwrap_or_else(|| {
        panic!(
            "expected non-empty static library named {static_lib_name} under {}",
            lib_dir.display()
        )
    });
    assert!(
        include_dir.join("ghostty").join("vt.h").exists(),
        "expected header at {}",
        include_dir.join("ghostty").join("vt.h").display()
    );

    if platform == TargetPlatform::Android {
        let android_link_dir = out_dir.join("android-link-lib");
        std::fs::create_dir_all(&android_link_dir).unwrap_or_else(|error| {
            panic!(
                "failed to create Android link directory {}: {error}",
                android_link_dir.display()
            )
        });
        let unversioned = android_link_dir.join("libghostty-vt.so");
        std::fs::copy(shared_lib_path.as_ref().unwrap(), &unversioned).unwrap_or_else(|error| {
            panic!(
                "failed to copy {} to {}: {error}",
                shared_lib_path.as_ref().unwrap().display(),
                unversioned.display()
            )
        });
        println!(
            "cargo:rustc-link-search=native={}",
            android_link_dir.display()
        );
        println!("cargo:rustc-link-lib=dylib=ghostty-vt");
    } else {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        if let Some(parent) = static_lib_path.parent() {
            println!("cargo:rustc-link-search=native={}", parent.display());
        }
        println!("cargo:rustc-link-lib=static={static_link_name}");
        link_zig_static_dependency(&ghostty_dir, "simdutf", platform);
        link_zig_static_dependency(&ghostty_dir, "highway", platform);
        link_zig_static_dependency(&ghostty_dir, "utfcpp", platform);
    }
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    }
    if target.contains("apple-darwin") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }
    println!("cargo:include={}", include_dir.display());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetPlatform {
    Apple,
    Android,
    Windows,
    Unix,
}

impl TargetPlatform {
    fn from_triple(target: &str) -> Self {
        if target.contains("apple") {
            Self::Apple
        } else if target.contains("android") {
            Self::Android
        } else if target.contains("windows") {
            Self::Windows
        } else {
            Self::Unix
        }
    }

    fn shared_lib_name(self) -> Option<&'static str> {
        match self {
            Self::Apple => Some("libghostty-vt.0.1.0.dylib"),
            Self::Android => Some("libghostty-vt.so"),
            Self::Windows => None,
            Self::Unix => Some("libghostty-vt.so.0.1.0"),
        }
    }

    fn static_lib_name(self) -> &'static str {
        match self {
            Self::Windows => "ghostty-vt-static.lib",
            _ => "libghostty-vt.a",
        }
    }

    fn static_link_name(self) -> &'static str {
        match self {
            Self::Windows => "ghostty-vt-static",
            _ => "ghostty-vt",
        }
    }

    fn static_dependency_file_names(self, name: &str) -> Vec<String> {
        match self {
            Self::Windows => vec![format!("{name}.lib"), format!("lib{name}.lib")],
            _ => vec![format!("lib{name}.a")],
        }
    }
}

fn zig_install_prefix(ghostty_dir: &Path, out_dir: &Path, platform: TargetPlatform) -> PathBuf {
    match platform {
        TargetPlatform::Windows => ghostty_dir.join("zig-out-codux"),
        _ => out_dir.join("ghostty-install"),
    }
}

fn zig_install_prefix_arg(install_prefix: &Path, platform: TargetPlatform) -> PathBuf {
    match platform {
        TargetPlatform::Windows => PathBuf::from(
            install_prefix
                .file_name()
                .expect("Windows Ghostty install prefix must have a final path component"),
        ),
        _ => install_prefix.to_path_buf(),
    }
}

fn add_ghostty_zig_build_args(command: &mut Command, install_prefix_arg: &Path) {
    command
        .arg("-Demit-lib-vt")
        .arg("-Demit-exe=false")
        .arg("-Demit-docs=false")
        .arg("-Demit-bench=false")
        .arg("-Demit-helpgen=false")
        .arg("-Demit-test-exe=false")
        .arg("-Demit-unicode-table-gen=false")
        .arg("-Demit-terminfo=false")
        .arg("-Demit-termcap=false")
        .arg("-Demit-themes=false")
        .arg("-Demit-webdata=false")
        // Zig defaults to Debug, which enables ghostty's "slow runtime
        // safety" integrity checks: scrollback reflow gets an order of
        // magnitude slower and Debug-only assertions abort the process
        // (PageList integrity panic on resize). Build the VT library the
        // way upstream ships it, regardless of the cargo profile.
        .arg("-Doptimize=ReleaseFast")
        .arg("--prefix")
        .arg(install_prefix_arg);
}

fn add_zig_target_args(command: &mut Command, target: &str, host: &str) {
    // Only pass -Dtarget when cross-compiling. For native builds, let zig
    // auto-detect the host (matches how ghostty's own CMakeLists.txt works).
    if target == host {
        return;
    }

    let zig_target = zig_target(target);
    command.arg(format!("-Dtarget={zig_target}"));
    if let Some(zig_cpu) = zig_cpu(target) {
        command.arg(format!("-Dcpu={zig_cpu}"));
    }
}

fn patch_ghostty_for_target(ghostty_dir: &Path, target: &str) {
    if !target.contains("android") {
        return;
    }

    let path = ghostty_dir
        .join("src")
        .join("build")
        .join("GhosttyLibVt.zig");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let needle = r#"    const lib = b.addLibrary(.{
        .name = if (kind == .static) "ghostty-vt-static" else "ghostty-vt",
        .linkage = linkage,
        .root_module = zig.vt_c,
        .version = std.SemanticVersion{ .major = 0, .minor = 1, .patch = 0 },
    });"#;
    let replacement = r#"    const lib = b.addLibrary(.{
        .name = if (kind == .static) "ghostty-vt-static" else "ghostty-vt",
        .linkage = linkage,
        .root_module = zig.vt_c,
        .version = if (linkage == .dynamic and target.result.abi.isAndroid())
            null
        else
            std.SemanticVersion{ .major = 0, .minor = 1, .patch = 0 },
    });"#;

    if source.contains(replacement) {
        return;
    }
    let patched = source.replace(needle, replacement);
    assert!(
        patched != source,
        "failed to patch Android Ghostty SONAME in {}",
        path.display()
    );
    std::fs::write(&path, patched)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn vendor_uucode_for_windows(zig: &Path, ghostty_dir: &Path) {
    let fetch_cache = ghostty_dir.join(".zig-fetch-codux");
    let mut fetch = Command::new(zig);
    fetch
        .arg("fetch")
        .arg("--global-cache-dir")
        .arg(&fetch_cache)
        .arg(UUCODE_URL)
        .current_dir(ghostty_dir);
    let package_hash = run_output(fetch, "zig fetch uucode").trim().to_owned();
    assert!(
        !package_hash.is_empty(),
        "zig fetch uucode did not print a package hash"
    );

    let uucode_dir = ghostty_dir.join("vendor").join("uucode-codux");
    if uucode_dir.exists() {
        std::fs::remove_dir_all(&uucode_dir).unwrap_or_else(|error| {
            panic!(
                "failed to remove stale uucode vendor directory {}: {error}",
                uucode_dir.display()
            )
        });
    }
    let fetched_dir = resolve_zig_fetched_package(&fetch_cache, &package_hash);
    copy_dir_all(&fetched_dir, &uucode_dir);
    patch_uucode_build_for_windows(&uucode_dir);
    patch_ghostty_uucode_dependency(ghostty_dir);
}

fn resolve_zig_fetched_package(fetch_cache: &Path, package_hash: &str) -> PathBuf {
    let package_path = fetch_cache.join("p").join(package_hash);
    if package_path.join("build.zig").exists() {
        return package_path;
    }

    if package_path.is_file() {
        let unpack_dir = fetch_cache.join("unpacked").join(package_hash);
        if unpack_dir.exists() {
            std::fs::remove_dir_all(&unpack_dir).unwrap_or_else(|error| {
                panic!(
                    "failed to remove stale uucode unpack directory {}: {error}",
                    unpack_dir.display()
                )
            });
        }
        std::fs::create_dir_all(&unpack_dir).unwrap_or_else(|error| {
            panic!(
                "failed to create uucode unpack directory {}: {error}",
                unpack_dir.display()
            )
        });

        let mut tar = Command::new("tar");
        tar.arg("-xf").arg(&package_path).arg("-C").arg(&unpack_dir);
        run(tar, "unpack uucode package");

        let unpacked_package = find_dir_with_file(&unpack_dir, "build.zig").unwrap_or_else(|| {
            panic!(
                "expected unpacked uucode package with build.zig under {}",
                unpack_dir.display()
            )
        });
        return unpacked_package;
    }

    panic!(
        "expected fetched uucode package at {}",
        package_path.display()
    );
}

fn patch_uucode_build_for_windows(uucode_dir: &Path) {
    let path = uucode_dir.join("build.zig");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let needle = r#"    run_build_tables_exe.setCwd(b.path(""));"#;
    let replacement = r#"    // Codux: Zig 0.15.2 on Windows asserts while converting the
    // b.path("") cwd for generated output arguments. Keep uucode's cwd
    // semantics so the table generator can read ucd/*, but use a path relative
    // to Ghostty's build root.
    run_build_tables_exe.cwd = .{ .cwd_relative = "vendor/uucode-codux" };"#;

    if source.contains("Codux: Zig 0.15.2 on Windows asserts") {
        return;
    }
    if !source.contains(needle) {
        assert!(
            !source.contains("run_build_tables_exe.setCwd"),
            "unsupported uucode setCwd form in {}",
            path.display()
        );
        return;
    }
    let patched = source.replace(needle, replacement);
    std::fs::write(&path, patched)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn patch_ghostty_uucode_dependency(ghostty_dir: &Path) {
    let path = ghostty_dir.join("build.zig.zon");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let replacement = r#"        .uucode = .{ .path = "./vendor/uucode-codux" },"#;

    if source.contains(replacement) {
        return;
    }

    let start = source
        .find("        .uucode = .{")
        .unwrap_or_else(|| panic!("failed to find uucode dependency in {}", path.display()));
    let after_start = start + "        .uucode = .{".len();
    let relative_end = source[after_start..].find("        },").unwrap_or_else(|| {
        panic!(
            "failed to find end of uucode dependency in {}",
            path.display()
        )
    });
    let end = after_start + relative_end + "        },".len();
    let block = &source[start..end];
    assert!(
        block.contains(UUCODE_URL),
        "refusing to replace unexpected uucode dependency block in {}",
        path.display()
    );

    let mut patched = String::with_capacity(source.len());
    patched.push_str(&source[..start]);
    patched.push_str(replacement);
    patched.push_str(&source[end..]);
    std::fs::write(&path, patched)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn copy_dir_all(from: &Path, to: &Path) {
    std::fs::create_dir_all(to)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", to.display()));
    for entry in std::fs::read_dir(from)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", from.display()))
    {
        let entry = entry.unwrap_or_else(|error| {
            panic!("failed to read entry under {}: {error}", from.display())
        });
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        if from_path.is_dir() {
            copy_dir_all(&from_path, &to_path);
        } else {
            std::fs::copy(&from_path, &to_path).unwrap_or_else(|error| {
                panic!(
                    "failed to copy {} to {}: {error}",
                    from_path.display(),
                    to_path.display()
                )
            });
        }
    }
}

fn find_nonempty_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    find_file_with(root, file_name, |path| {
        path.metadata().map(|meta| meta.len() > 0).unwrap_or(false)
    })
}

fn link_zig_static_dependency(ghostty_dir: &Path, name: &str, platform: TargetPlatform) {
    let Some(path) = platform
        .static_dependency_file_names(name)
        .iter()
        .find_map(|file_name| find_file(&ghostty_dir.join(".zig-cache").join("o"), file_name))
    else {
        return;
    };
    if let Some(parent) = path.parent() {
        println!("cargo:rustc-link-search=native={}", parent.display());
        println!("cargo:rustc-link-lib=static={name}");
    }
}

fn find_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    find_file_with(root, file_name, |_| true)
}

fn find_dir_with_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join(file_name).exists() {
                return Some(path);
            }
            if let Some(found) = find_dir_with_file(&path, file_name) {
                return Some(found);
            }
        }
    }
    None
}

fn find_file_with(
    root: &Path,
    file_name: &str,
    predicate: impl Copy + Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|value| value.to_str()) == Some(file_name) && predicate(&path)
        {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_file_with(&path, file_name, predicate)
        {
            return Some(found);
        }
    }
    None
}

fn zig_command() -> PathBuf {
    let mut probes = Vec::new();

    if let Ok(value) = env::var("ZIG") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            probes.push(probe_zig_candidate(&path, true));
            return select_zig_command_from_probes(&probes)
                .unwrap_or_else(|error| panic!("failed to resolve ZIG from environment: {error}"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        probes.push(probe_zig_candidate(
            Path::new("/opt/homebrew/opt/zig@0.15/bin/zig"),
            false,
        ));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
            probes.push(probe_zig_candidate(
                &PathBuf::from(home)
                    .join("tools")
                    .join("zig-0.15.2")
                    .join("zig.exe"),
                false,
            ));
        }
        probes.push(probe_zig_candidate(
            Path::new("C:\\tools\\zig-0.15.2\\zig.exe"),
            false,
        ));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        probes.push(probe_zig_candidate(
            Path::new("/usr/local/bin/zig-0.15.2"),
            false,
        ));
        probes.push(probe_zig_candidate(Path::new("/opt/zig-0.15.2/zig"), false));
    }

    probes.push(probe_zig_candidate(Path::new("zig"), false));
    select_zig_command_from_probes(&probes).unwrap_or_else(|error| panic!("{error}"))
}

fn probe_zig_candidate(path: &Path, explicit: bool) -> ZigProbe {
    ZigProbe {
        path: path.to_path_buf(),
        version: zig_version(path).ok(),
        explicit,
    }
}

fn zig_version(path: &Path) -> Result<String, String> {
    let mut command = Command::new(path);
    command.arg("version");
    let output = command
        .output()
        .map_err(|error| format!("failed to execute {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} version failed with status {}\nstdout:\n{}\nstderr:\n{}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| format!("{} printed non-UTF-8 stdout: {error}", path.display()))
        .map(|version| version.trim().to_owned())
}

fn select_zig_command_from_probes(probes: &[ZigProbe]) -> Result<PathBuf, String> {
    const REQUIRED_ZIG_VERSION: &str = "0.15.2";

    let mut last_explicit_error = None;
    for probe in probes {
        match probe.version.as_deref() {
            Some(version) if version == REQUIRED_ZIG_VERSION => {
                return Ok(probe.path.clone());
            }
            Some(version) if probe.explicit => {
                last_explicit_error = Some(format!(
                    "Zig {} at {} does not meet the required build version of {}",
                    version,
                    probe.path.display(),
                    REQUIRED_ZIG_VERSION
                ));
            }
            None if probe.explicit => {
                last_explicit_error = Some(format!(
                    "failed to determine Zig version for explicit path {}",
                    probe.path.display()
                ));
            }
            _ => {}
        }
    }

    if let Some(error) = last_explicit_error {
        return Err(error);
    }

    Err(format!(
        "Zig {} was not found. Set ZIG to a Zig {} executable or install it at one of the expected locations.",
        REQUIRED_ZIG_VERSION, REQUIRED_ZIG_VERSION
    ))
}

#[derive(Debug, Clone)]
struct ZigProbe {
    path: PathBuf,
    version: Option<String>,
    explicit: bool,
}

/// Clone ghostty at the pinned commit into OUT_DIR/ghostty-src.
/// Reuses an existing clone if the commit matches.
fn fetch_ghostty(out_dir: &Path) -> PathBuf {
    let src_dir = out_dir.join("ghostty-src");
    let stamp = src_dir.join(".ghostty-commit");

    // Skip fetch if we already have the right commit.
    if stamp.exists()
        && let Ok(existing) = std::fs::read_to_string(&stamp)
        && existing.trim() == GHOSTTY_COMMIT
    {
        return src_dir;
    }

    // Clean and clone fresh.
    if src_dir.exists() {
        std::fs::remove_dir_all(&src_dir)
            .unwrap_or_else(|e| panic!("failed to remove {}: {e}", src_dir.display()));
    }

    eprintln!("Fetching ghostty {GHOSTTY_COMMIT} ...");

    let mut clone = Command::new("git");
    clone
        .arg("clone")
        .arg("--filter=blob:none")
        .arg("--no-checkout")
        .arg(GHOSTTY_REPO)
        .arg(&src_dir);
    run(clone, "git clone ghostty");

    let mut checkout = Command::new("git");
    checkout
        .arg("checkout")
        .arg(GHOSTTY_COMMIT)
        .current_dir(&src_dir);
    run(checkout, "git checkout ghostty commit");

    std::fs::write(&stamp, GHOSTTY_COMMIT).unwrap_or_else(|e| panic!("failed to write stamp: {e}"));

    src_dir
}

fn run(mut command: Command, context: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {context}: {error}"));
    assert!(status.success(), "{context} failed with status {status}");
}

fn run_output(mut command: Command, context: &str) -> String {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to execute {context}: {error}"));
    assert!(
        output.status.success(),
        "{context} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("{context} printed non-UTF-8 stdout: {error}"))
}

fn zig_target(target: &str) -> String {
    let value = match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        "aarch64-apple-darwin" => "aarch64-macos-none",
        "x86_64-apple-darwin" => "x86_64-macos-none",
        "aarch64-apple-ios" => "aarch64-ios-none",
        "aarch64-apple-ios-sim" => "aarch64-ios-simulator",
        "x86_64-apple-ios" => "x86_64-ios-simulator",
        "aarch64-linux-android" => "aarch64-linux-android",
        "armv7-linux-androideabi" => "arm-linux-androideabi",
        "i686-linux-android" => "x86-linux-android",
        "x86_64-linux-android" => "x86_64-linux-android",
        other => panic!("unsupported Rust target for vendored build: {other}"),
    };
    value.to_owned()
}

fn zig_cpu(target: &str) -> Option<&'static str> {
    match target {
        // Ghostty's own XCFramework build uses an Apple CPU model for arm64
        // simulator builds because Zig's generic baseline currently misses
        // the altnzcv feature required by simdutf's ARM intrinsic paths.
        "aarch64-apple-ios-sim" => Some("apple_a17"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(path: &str, version: Option<&str>, explicit: bool) -> ZigProbe {
        ZigProbe {
            path: PathBuf::from(path),
            version: version.map(str::to_owned),
            explicit,
        }
    }

    #[test]
    fn chooses_first_required_zig_version_from_candidates() {
        let selected = select_zig_command_from_probes(&[
            probe("zig-0.16.0", Some("0.16.0"), false),
            probe("zig-0.15.2", Some("0.15.2"), false),
        ])
        .expect("expected compatible Zig to be selected");

        assert_eq!(selected, PathBuf::from("zig-0.15.2"));
    }

    #[test]
    fn rejects_explicit_zig_with_wrong_version() {
        let error = select_zig_command_from_probes(&[probe("zig", Some("0.16.0"), true)])
            .expect_err("explicit incompatible Zig must fail");

        assert!(error.contains("0.15.2"));
        assert!(error.contains("0.16.0"));
    }
}
