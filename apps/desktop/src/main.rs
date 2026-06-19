#![allow(unexpected_cfgs)]
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod assets;
mod heroicons;
mod terminal;
mod theme;

use anyhow::Result;
use app::{
    CoduxApp, MAIN_WINDOW_DEFAULT_HEIGHT, MAIN_WINDOW_DEFAULT_WIDTH, MAIN_WINDOW_MIN_HEIGHT,
    MAIN_WINDOW_MIN_WIDTH,
};
use assets::CoduxAssets;
use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, KeyBinding, Unbind, WindowBounds, WindowOptions, px,
    size,
};
use gpui_component::Root;
use parking_lot::Mutex;
use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

fn main() -> Result<()> {
    if handle_cli_args() {
        return Ok(());
    }

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    disable_macos_autofill_heuristics();

    let app = gpui_platform::application().with_assets(CoduxAssets);
    let main_window_handle: Rc<Cell<Option<AnyWindowHandle>>> = Rc::new(Cell::new(None));
    let reopen_main_window = main_window_handle.clone();
    app.on_reopen(move |cx| {
        if let Some(handle) = reopen_main_window.get() {
            if handle
                .update(cx, |_view, window, _cx| window.activate_window())
                .is_ok()
            {
                cx.activate(true);
                return;
            }
            reopen_main_window.set(None);
        }

        let settings = app::active_settings_snapshot().unwrap_or_default();
        if open_main_window(cx, &reopen_main_window, &settings) {
            cx.activate(true);
        }
    });

    app.run(move |cx: &mut App| {
        app::macos_window::install_dock_reopen_handler();
        gpui_component::init(cx);
        load_embedded_fonts(cx);
        disable_root_tab_focus_bindings(cx);
        let initial_state = codux_runtime::runtime_state::RuntimeState::load();
        let _ = codux_runtime::app_icon::apply_app_icon(&initial_state.settings.icon_style);
        app::set_active_settings_snapshot(initial_state.settings.clone());
        theme::apply_component_theme(
            &initial_state.settings.theme,
            &initial_state.settings.theme_color,
            None,
            cx,
        );
        cx.set_menus(crate::app::native_menu::codux_menus(
            &initial_state.settings.language,
        ));
        if !open_main_window(cx, &main_window_handle, &initial_state.settings) {
            cx.quit();
            return;
        }

        cx.activate(true);
    });

    Ok(())
}

fn handle_cli_args() -> bool {
    match std::env::args().nth(1).as_deref() {
        Some("--version") | Some("-V") => {
            println!("codux {}", env!("CARGO_PKG_VERSION"));
            true
        }
        Some("--help") | Some("-h") => {
            println!("Codux {}", env!("CARGO_PKG_VERSION"));
            println!("Usage: codux [--version]");
            true
        }
        _ => false,
    }
}

fn load_embedded_fonts(cx: &App) {
    let asset_source = cx.asset_source();
    let Ok(font_paths) = asset_source.list("fonts") else {
        return;
    };
    let embedded_fonts = Mutex::new(Vec::<Cow<'static, [u8]>>::new());
    let executor = cx.background_executor();

    cx.foreground_executor().block_on(executor.scoped(|scope| {
        for font_path in &font_paths {
            if !font_path.ends_with(".ttf") {
                continue;
            }
            scope.spawn(async {
                if let Ok(Some(font_bytes)) = asset_source.load(font_path) {
                    embedded_fonts.lock().push(font_bytes);
                }
            });
        }
    }));

    let fonts = embedded_fonts.into_inner();
    if !fonts.is_empty() {
        let _ = cx.text_system().add_fonts(fonts);
    }
}

fn open_main_window(
    cx: &mut App,
    main_window_handle: &Rc<Cell<Option<AnyWindowHandle>>>,
    settings: &codux_runtime::settings::SettingsSummary,
) -> bool {
    let bounds = Bounds::centered(
        None,
        size(
            px(MAIN_WINDOW_DEFAULT_WIDTH),
            px(MAIN_WINDOW_DEFAULT_HEIGHT),
        ),
        cx,
    );
    let result = cx.open_window(
        WindowOptions {
            titlebar: Some(theme::codux_main_titlebar(
                codux_runtime::runtime_paths::app_display_name(),
            )),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(MAIN_WINDOW_MIN_WIDTH), px(MAIN_WINDOW_MIN_HEIGHT))),
            icon: Some(std::sync::Arc::new(window_icon_image(settings))),
            ..Default::default()
        },
        |window, cx| {
            app::macos_window::configure_main_window_controls(window);
            let app = CoduxApp::new(window, cx).expect("failed to create Codux app");
            let view = cx.new(|_| app);
            view.update(cx, |app, _| {
                app.observe_main_window_appearance(view.clone(), window);
            });
            view.update(cx, |app, cx| {
                app.observe_main_window_activation(window, cx);
            });
            view.update(cx, |app, cx| app.observe_main_window_bounds(window, cx));
            view.update(cx, |app, cx| {
                app.initialize_main_window_focus(window, cx);
            });
            view.update(cx, |app, cx| app.start_runtime_event_loop(cx));
            // Attach any remote-hosted terminals restored at boot now that the
            // entity exists (the async attach chokepoint needs a Context<Self>).
            view.update(cx, |app, cx| app.attach_boot_pending_terminals(cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    );

    match result {
        Ok(handle) => {
            main_window_handle.set(Some(handle.into()));
            true
        }
        Err(error) => {
            eprintln!("failed to open Codux window: {error}");
            false
        }
    }
}

fn window_icon_image(settings: &codux_runtime::settings::SettingsSummary) -> image::RgbaImage {
    #[cfg(target_os = "windows")]
    let icon = codux_runtime::app_icon::render_windows_app_icon(
        &settings.icon_style,
        codux_runtime::app_icon::ICON_SIZE,
    );
    #[cfg(not(target_os = "windows"))]
    let icon = codux_runtime::app_icon::render_app_icon(
        &settings.icon_style,
        codux_runtime::app_icon::ICON_SIZE,
    );
    image::RgbaImage::from_raw(icon.width, icon.height, icon.pixels)
        .unwrap_or_else(|| image::RgbaImage::new(icon.width, icon.height))
}

#[cfg(target_os = "macos")]
fn disable_macos_autofill_heuristics() {
    use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
    use core_foundation_sys::number::kCFBooleanFalse;
    use core_foundation_sys::preferences::{
        CFPreferencesAppSynchronize, CFPreferencesSetAppValue, kCFPreferencesCurrentApplication,
    };
    use core_foundation_sys::string::{CFStringCreateWithCString, kCFStringEncodingUTF8};
    use std::ffi::CString;

    let key = CString::new("NSAutoFillHeuristicControllerEnabled")
        .expect("static string contains no nul");
    let key_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, key.as_ptr(), kCFStringEncodingUTF8)
    };
    if key_ref.is_null() {
        return;
    }

    unsafe {
        CFPreferencesSetAppValue(
            key_ref,
            kCFBooleanFalse.cast(),
            kCFPreferencesCurrentApplication,
        );
        let _ = CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
        CFRelease(key_ref.cast());
    }
}

#[cfg(not(target_os = "macos"))]
fn disable_macos_autofill_heuristics() {}

fn disable_root_tab_focus_bindings(cx: &mut App) {
    let mut bindings = vec![
        KeyBinding::new("tab", Unbind("root::Tab".into()), Some("Root")),
        KeyBinding::new("shift-tab", Unbind("root::TabPrev".into()), Some("Root")),
        KeyBinding::new(
            "cmd-w",
            crate::app::native_menu::CloseActive,
            Some("CoduxMainWindow"),
        ),
        KeyBinding::new("cmd-n", crate::app::native_menu::NewProject, None),
        KeyBinding::new("cmd-o", crate::app::native_menu::OpenProjectFolder, None),
        KeyBinding::new("cmd-,", crate::app::native_menu::OpenSettings, None),
        KeyBinding::new("cmd-alt-1", crate::app::native_menu::ViewTerminal, None),
        KeyBinding::new("cmd-alt-2", crate::app::native_menu::ViewFiles, None),
        KeyBinding::new("cmd-alt-3", crate::app::native_menu::ViewReview, None),
        KeyBinding::new("cmd-alt-p", crate::app::native_menu::ToggleProjects, None),
        KeyBinding::new("cmd-alt-t", crate::app::native_menu::ToggleTasks, None),
        KeyBinding::new("cmd-shift-g", crate::app::native_menu::OpenGitPanel, None),
        KeyBinding::new("cmd-shift-f", crate::app::native_menu::OpenFilesPanel, None),
        KeyBinding::new("cmd-shift-a", crate::app::native_menu::OpenAiPanel, None),
        KeyBinding::new("cmd-shift-s", crate::app::native_menu::OpenSshPanel, None),
        KeyBinding::new("cmd-t", crate::app::native_menu::CreateSplit, None),
        KeyBinding::new("cmd-shift-t", crate::app::native_menu::CreateTab, None),
        KeyBinding::new("cmd-shift-n", crate::app::native_menu::CreateTask, None),
        KeyBinding::new("cmd-s", crate::app::native_menu::EditorSave, None),
        KeyBinding::new("cmd-f", crate::app::native_menu::EditorSearch, None),
        KeyBinding::new("cmd-q", crate::app::native_menu::QuitCodux, None),
        KeyBinding::new("cmd-m", crate::app::native_menu::MinimizeWindow, None),
        KeyBinding::new("cmd-alt-m", crate::app::native_menu::MinimizeWindow, None),
        KeyBinding::new("cmd-h", crate::app::native_menu::HideCodux, None),
        KeyBinding::new("cmd-alt-h", crate::app::native_menu::HideOthers, None),
        KeyBinding::new(
            "cmd-ctrl-f",
            crate::app::native_menu::ToggleFullscreen,
            None,
        ),
    ];

    add_non_macos_control_shortcuts(&mut bindings);
    cx.bind_keys(bindings);
}

#[cfg(not(target_os = "macos"))]
fn add_non_macos_control_shortcuts(bindings: &mut Vec<KeyBinding>) {
    bindings.extend([
        KeyBinding::new(
            "ctrl-w",
            crate::app::native_menu::CloseActive,
            Some("CoduxMainWindow"),
        ),
        KeyBinding::new("ctrl-n", crate::app::native_menu::NewProject, None),
        KeyBinding::new("ctrl-o", crate::app::native_menu::OpenProjectFolder, None),
        KeyBinding::new("ctrl-,", crate::app::native_menu::OpenSettings, None),
        KeyBinding::new("ctrl-alt-1", crate::app::native_menu::ViewTerminal, None),
        KeyBinding::new("ctrl-alt-2", crate::app::native_menu::ViewFiles, None),
        KeyBinding::new("ctrl-alt-3", crate::app::native_menu::ViewReview, None),
        KeyBinding::new("ctrl-alt-p", crate::app::native_menu::ToggleProjects, None),
        KeyBinding::new("ctrl-alt-t", crate::app::native_menu::ToggleTasks, None),
        KeyBinding::new("ctrl-shift-g", crate::app::native_menu::OpenGitPanel, None),
        KeyBinding::new(
            "ctrl-shift-f",
            crate::app::native_menu::OpenFilesPanel,
            None,
        ),
        KeyBinding::new("ctrl-shift-a", crate::app::native_menu::OpenAiPanel, None),
        KeyBinding::new("ctrl-shift-s", crate::app::native_menu::OpenSshPanel, None),
        KeyBinding::new("ctrl-t", crate::app::native_menu::CreateSplit, None),
        KeyBinding::new("ctrl-shift-t", crate::app::native_menu::CreateTab, None),
        KeyBinding::new("ctrl-shift-n", crate::app::native_menu::CreateTask, None),
        KeyBinding::new("ctrl-s", crate::app::native_menu::EditorSave, None),
        KeyBinding::new("ctrl-f", crate::app::native_menu::EditorSearch, None),
        KeyBinding::new(
            "ctrl-shift-f11",
            crate::app::native_menu::ToggleFullscreen,
            None,
        ),
    ]);
}

#[cfg(target_os = "macos")]
fn add_non_macos_control_shortcuts(_bindings: &mut Vec<KeyBinding>) {}
