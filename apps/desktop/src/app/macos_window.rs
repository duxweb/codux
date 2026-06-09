#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{
        NSApp, NSColor, NSEvent, NSMenu, NSMenuItem, NSScreen, NSView, NSWindow, NSWindowButton,
        NSWindowStyleMask,
    },
    base::{NO, YES, id, nil},
    foundation::{NSAutoreleasePool, NSPoint, NSRect, NSString},
};
use codux_runtime::desktop_pet::{
    DesktopPetHitLayout, DesktopPetPhysicalPosition, DesktopPetPhysicalSize, DesktopPetSide,
    DesktopPetWorkArea, desktop_pet_local_point_is_hotspot, desktop_pet_side_for_position,
};
use gpui::Window;
#[cfg(target_os = "macos")]
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{
        Class, Imp, Object, Sel, class_getInstanceMethod, method_getImplementation,
        method_setImplementation,
    },
    sel, sel_impl,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::{collections::HashMap, sync::Mutex};
#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
    },
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        ScreenToClient,
    },
    UI::WindowsAndMessaging::{
        AppendMenuW, CallWindowProcW, CreatePopupMenu, DefWindowProcW, DestroyMenu, GA_ROOT,
        GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC, GetAncestor, GetClientRect, GetCursorPos,
        GetWindowLongPtrW, GetWindowRect, HTCAPTION, HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA,
        MF_SEPARATOR, MF_STRING, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW,
        SetWindowPos, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenu,
        WINDOW_LONG_PTR_INDEX, WM_NCHITTEST, WNDPROC, WS_BORDER, WS_CAPTION, WS_DLGFRAME,
        WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_EX_WINDOWEDGE, WS_SYSMENU, WS_THICKFRAME,
    },
};

#[derive(Clone)]
pub(in crate::app) enum NativeMenuEntry {
    Item {
        label: String,
        action_id: &'static str,
    },
    Separator,
}

#[cfg(target_os = "macos")]
static SELECTED_MENU_TAG: AtomicIsize = AtomicIsize::new(-1);
#[cfg(target_os = "macos")]
static REOPEN_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static ORIGINAL_REOPEN_HANDLER: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "windows")]
static DESKTOP_PET_WNDPROCS: std::sync::OnceLock<Mutex<HashMap<isize, isize>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(crate) fn install_dock_reopen_handler() {
    if REOPEN_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    unsafe {
        let app = NSApp();
        let delegate: id = msg_send![app, delegate];
        if delegate.is_null() {
            REOPEN_HANDLER_INSTALLED.store(false, Ordering::SeqCst);
            return;
        }

        let delegate_class: *const Class = msg_send![delegate, class];
        let method = class_getInstanceMethod(
            delegate_class,
            Sel::register("applicationShouldHandleReopen:hasVisibleWindows:"),
        );
        if method.is_null() {
            REOPEN_HANDLER_INSTALLED.store(false, Ordering::SeqCst);
            return;
        }

        let original = method_getImplementation(method);
        ORIGINAL_REOPEN_HANDLER.store(original as usize, Ordering::SeqCst);
        let replacement: Imp = std::mem::transmute(
            dock_should_handle_reopen as unsafe extern "C" fn(&mut Object, Sel, id, bool),
        );
        let _ = method_setImplementation(method.cast_mut(), replacement);
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn install_dock_reopen_handler() {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(crate) fn configure_main_window_controls(window: &mut Window) {
    configure_native_window_buttons(window, false);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn configure_main_window_controls(_window: &mut Window) {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn configure_child_window_controls(window: &mut Window) {
    configure_native_window_buttons(window, true);
}

#[cfg(not(target_os = "macos"))]
pub(in crate::app) fn configure_child_window_controls(_window: &mut Window) {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn configure_document_child_window_controls(window: &mut Window) {
    configure_native_window_buttons(window, false);
}

#[cfg(not(target_os = "macos"))]
pub(in crate::app) fn configure_document_child_window_controls(_window: &mut Window) {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn configure_native_window_buttons(window: &mut Window, close_only: bool) {
    let Some(ns_window) = appkit_window(window) else {
        return;
    };

    unsafe {
        let close_button = ns_window.standardWindowButton_(NSWindowButton::NSWindowCloseButton);
        let min_button = ns_window.standardWindowButton_(NSWindowButton::NSWindowMiniaturizeButton);
        let zoom_button = ns_window.standardWindowButton_(NSWindowButton::NSWindowZoomButton);

        set_button_hidden(min_button, close_only);
        set_button_hidden(zoom_button, close_only);
        if !close_only {
            offset_window_button(close_button);
            offset_window_button(min_button);
            offset_window_button(zoom_button);
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
unsafe fn set_button_hidden(button: id, hidden: bool) {
    if button.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![button, setHidden: if hidden { YES } else { NO }];
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
unsafe fn offset_window_button(button: id) {
    if button.is_null() {
        return;
    }
    unsafe {
        let mut frame: NSRect = msg_send![button, frame];
        frame.origin.y += 5.0;
        let _: () = msg_send![button, setFrame: frame];
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn dock_should_handle_reopen(
    this: &mut Object,
    cmd: Sel,
    application: id,
    _has_visible_windows: bool,
) {
    let original = ORIGINAL_REOPEN_HANDLER.load(Ordering::SeqCst);
    if original == 0 {
        return;
    }

    let original: unsafe extern "C" fn(&mut Object, Sel, id, bool) =
        unsafe { std::mem::transmute(original) };
    unsafe {
        original(this, cmd, application, false);
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn make_desktop_pet_window_transparent(window: &mut Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };

    unsafe {
        let ns_view = handle.ns_view.as_ptr() as id;
        if ns_view.is_null() {
            return;
        }
        let ns_window: id = msg_send![ns_view, window];
        if ns_window.is_null() {
            return;
        }

        let style_mask = ns_window.styleMask();
        ns_window.setStyleMask_(
            style_mask
                - NSWindowStyleMask::NSTitledWindowMask
                - NSWindowStyleMask::NSClosableWindowMask
                - NSWindowStyleMask::NSMiniaturizableWindowMask,
        );
        ns_window.setOpaque_(NO);
        ns_window.setHasShadow_(NO);
        ns_window.setBackgroundColor_(NSColor::clearColor(nil));

        let content_view = ns_window.contentView();
        clear_layer_background(content_view);
        clear_layer_background(ns_view);

        let _: () = msg_send![ns_window, setIgnoresMouseEvents: NO];
        let _: () = msg_send![ns_window, setMovableByWindowBackground: NO];
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn sync_desktop_pet_mouse_passthrough(window: &mut Window) {
    let Some(ns_window) = appkit_window(window) else {
        return;
    };

    unsafe {
        let mouse = NSEvent::mouseLocation(nil);
        let frame = NSWindow::frame(ns_window);
        let scale_factor = NSWindow::backingScaleFactor(ns_window);
        let local_x = mouse.x - frame.origin.x;
        let local_y = frame.size.height - (mouse.y - frame.origin.y);
        let side = desktop_pet_side_for_native_window(ns_window, frame);
        let layout = DesktopPetHitLayout {
            position: DesktopPetPhysicalPosition { x: 0.0, y: 0.0 },
            size: DesktopPetPhysicalSize {
                width: frame.size.width,
                height: frame.size.height,
            },
            scale_factor,
            side,
        };
        let should_ignore = !desktop_pet_local_point_is_hotspot(layout, local_x, local_y, false);
        let is_ignored = ns_window.ignoresMouseEvents() == YES;
        if is_ignored != should_ignore {
            let _: () = msg_send![
                ns_window,
                setIgnoresMouseEvents: if should_ignore { YES } else { NO }
            ];
        }
    }
}

#[cfg(target_os = "windows")]
pub(in crate::app) fn sync_desktop_pet_mouse_passthrough(window: &mut Window) {
    let Some(hwnd) = win32_hwnd(window) else {
        return;
    };

    let mut cursor = POINT::default();
    let mut window_rect = RECT::default();
    unsafe {
        if GetCursorPos(&mut cursor) == 0 || GetWindowRect(hwnd, &mut window_rect) == 0 {
            return;
        }
    }

    let width = (window_rect.right - window_rect.left).max(1) as f64;
    let height = (window_rect.bottom - window_rect.top).max(1) as f64;
    let scale_x = width / codux_runtime::desktop_pet::DESKTOP_PET_BASE_WIDTH;
    let scale_y = height / codux_runtime::desktop_pet::DESKTOP_PET_BASE_HEIGHT;
    let local_x = (cursor.x - window_rect.left) as f64 / scale_x;
    let local_y = (cursor.y - window_rect.top) as f64 / scale_y;
    let layout = DesktopPetHitLayout {
        position: DesktopPetPhysicalPosition { x: 0.0, y: 0.0 },
        size: DesktopPetPhysicalSize {
            width: codux_runtime::desktop_pet::DESKTOP_PET_BASE_WIDTH,
            height: codux_runtime::desktop_pet::DESKTOP_PET_BASE_HEIGHT,
        },
        scale_factor: 1.0,
        side: desktop_pet_side_for_hwnd(hwnd),
    };
    let should_passthrough = !desktop_pet_local_point_is_hotspot(layout, local_x, local_y, false);
    set_desktop_pet_window_passthrough(hwnd, should_passthrough);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::app) fn sync_desktop_pet_mouse_passthrough(_window: &mut Window) {}

#[cfg(target_os = "macos")]
unsafe fn desktop_pet_side_for_native_window(ns_window: id, frame: NSRect) -> DesktopPetSide {
    let screen: id = unsafe { ns_window.screen() };
    let work_area = if screen.is_null() {
        DesktopPetWorkArea {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame
                .size
                .width
                .max(codux_runtime::desktop_pet::DESKTOP_PET_BASE_WIDTH),
            height: frame
                .size
                .height
                .max(codux_runtime::desktop_pet::DESKTOP_PET_BASE_HEIGHT),
            scale_factor: 1.0,
        }
    } else {
        let screen_frame = unsafe { screen.visibleFrame() };
        DesktopPetWorkArea {
            x: screen_frame.origin.x,
            y: screen_frame.origin.y,
            width: screen_frame.size.width,
            height: screen_frame.size.height,
            scale_factor: 1.0,
        }
    };
    desktop_pet_side_for_position(
        DesktopPetPhysicalPosition {
            x: frame.origin.x,
            y: frame.origin.y,
        },
        DesktopPetPhysicalSize {
            width: frame.size.width,
            height: frame.size.height,
        },
        work_area,
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::app) fn make_desktop_pet_window_transparent(_window: &mut gpui::Window) {}

#[cfg(target_os = "windows")]
pub(in crate::app) fn make_desktop_pet_window_transparent(window: &mut gpui::Window) {
    let Some(hwnd) = win32_hwnd(window) else {
        return;
    };

    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = style & !(WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME | WS_SYSMENU);
        let _ = SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let ex_style = (ex_style | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED)
            & !(WS_EX_APPWINDOW | WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
        let _ = SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);

        let no_corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &no_corner as *const _ as *const _,
            std::mem::size_of_val(&no_corner) as u32,
        );

        let no_border = windows_sys::Win32::Graphics::Dwm::DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            &no_border as *const _ as *const _,
            std::mem::size_of_val(&no_border) as u32,
        );

        let no_nc_rendering = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            &no_nc_rendering as *const _ as *const _,
            std::mem::size_of_val(&no_nc_rendering) as u32,
        );

        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        install_desktop_pet_hit_test(hwnd);
    }
}

#[cfg(target_os = "windows")]
fn install_desktop_pet_hit_test(hwnd: HWND) {
    let procs = DESKTOP_PET_WNDPROCS.get_or_init(|| Mutex::new(HashMap::new()));
    if procs
        .lock()
        .map_or(false, |procs| procs.contains_key(&(hwnd as isize)))
    {
        return;
    }
    unsafe {
        let previous = SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC as WINDOW_LONG_PTR_INDEX,
            desktop_pet_window_proc as *const () as isize,
        );
        if previous != 0 {
            let _ = procs
                .lock()
                .map(|mut procs| procs.insert(hwnd as isize, previous));
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn desktop_pet_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        return desktop_pet_hit_test(hwnd, lparam);
    }
    call_desktop_pet_original_window_proc(hwnd, msg, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn desktop_pet_hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let mut point = POINT {
        x: lparam_loword(lparam),
        y: lparam_hiword(lparam),
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = ScreenToClient(hwnd, &mut point);
        let _ = GetClientRect(hwnd, &mut rect);
    }

    let width = (rect.right - rect.left).max(1) as f64;
    let height = (rect.bottom - rect.top).max(1) as f64;
    let scale_x = width / codux_runtime::desktop_pet::DESKTOP_PET_BASE_WIDTH;
    let scale_y = height / codux_runtime::desktop_pet::DESKTOP_PET_BASE_HEIGHT;
    let x = point.x as f64 / scale_x;
    let y = point.y as f64 / scale_y;
    let layout = DesktopPetHitLayout {
        position: DesktopPetPhysicalPosition { x: 0.0, y: 0.0 },
        size: DesktopPetPhysicalSize {
            width: codux_runtime::desktop_pet::DESKTOP_PET_BASE_WIDTH,
            height: codux_runtime::desktop_pet::DESKTOP_PET_BASE_HEIGHT,
        },
        scale_factor: 1.0,
        side: desktop_pet_side_for_hwnd(hwnd),
    };
    if desktop_pet_local_point_is_hotspot(layout, x, y, false) {
        HTCAPTION as LRESULT
    } else {
        HTTRANSPARENT as LRESULT
    }
}

#[cfg(target_os = "windows")]
fn desktop_pet_side_for_hwnd(hwnd: HWND) -> DesktopPetSide {
    let mut window_rect = RECT::default();
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetWindowRect(hwnd, &mut window_rect) == 0 {
            return DesktopPetSide::Right;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut monitor_info) == 0 {
            return DesktopPetSide::Right;
        }
    }

    desktop_pet_side_for_position(
        DesktopPetPhysicalPosition {
            x: window_rect.left as f64,
            y: window_rect.top as f64,
        },
        DesktopPetPhysicalSize {
            width: (window_rect.right - window_rect.left).max(1) as f64,
            height: (window_rect.bottom - window_rect.top).max(1) as f64,
        },
        DesktopPetWorkArea {
            x: monitor_info.rcWork.left as f64,
            y: monitor_info.rcWork.top as f64,
            width: (monitor_info.rcWork.right - monitor_info.rcWork.left).max(1) as f64,
            height: (monitor_info.rcWork.bottom - monitor_info.rcWork.top).max(1) as f64,
            scale_factor: 1.0,
        },
    )
}

#[cfg(target_os = "windows")]
fn set_desktop_pet_window_passthrough(hwnd: HWND, passthrough: bool) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let next_style = if passthrough {
            style | WS_EX_TRANSPARENT
        } else {
            style & !WS_EX_TRANSPARENT
        };
        if next_style == style {
            return;
        }

        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_style as isize);
        let _ = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(target_os = "windows")]
fn lparam_loword(value: LPARAM) -> i32 {
    (value & 0xffff) as u16 as i16 as i32
}

#[cfg(target_os = "windows")]
fn lparam_hiword(value: LPARAM) -> i32 {
    ((value >> 16) & 0xffff) as u16 as i16 as i32
}

#[cfg(target_os = "windows")]
fn call_desktop_pet_original_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let previous = DESKTOP_PET_WNDPROCS
        .get()
        .and_then(|procs| procs.lock().ok()?.get(&(hwnd as isize)).copied());
    if let Some(previous) = previous {
        let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
        unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) }
    } else {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn spawn_native_popup_menu(
    window: &mut Window,
    position: gpui::Point<gpui::Pixels>,
    entries: Vec<NativeMenuEntry>,
    on_select: fn(
        &mut crate::app::CoduxApp,
        &'static str,
        &mut Window,
        &mut gpui::Context<crate::app::CoduxApp>,
    ),
    cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
    let Some(ns_view) = appkit_view(window) else {
        return;
    };
    let Some(window_handle) = Window::window_handle(window).downcast::<crate::app::CoduxApp>()
    else {
        return;
    };
    cx.spawn(
        async move |_this: gpui::WeakEntity<crate::app::CoduxApp>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            if let Some(action_id) =
                show_desktop_pet_native_menu_for_view(ns_view, position, &entries)
            {
                let _ = window_handle.update(cx, |app, window, cx| {
                    on_select(app, action_id, window, cx);
                });
            }
        },
    )
    .detach();
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub(in crate::app) fn spawn_desktop_pet_native_menu(
    window: &mut Window,
    position: gpui::Point<gpui::Pixels>,
    entries: Vec<NativeMenuEntry>,
    cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
    spawn_native_popup_menu(
        window,
        position,
        entries,
        crate::app::CoduxApp::apply_desktop_pet_action,
        cx,
    );
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn show_desktop_pet_native_menu_for_view(
    ns_view: id,
    position: gpui::Point<gpui::Pixels>,
    entries: &[NativeMenuEntry],
) -> Option<&'static str> {
    let menu_point = NSPoint::new(position.x.to_f64(), position.y.to_f64());

    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        SELECTED_MENU_TAG.store(-1, Ordering::SeqCst);
        let menu = NSMenu::new(nil);
        menu.setAutoenablesItems(NO);
        let target: id = msg_send![desktop_pet_menu_target_class(), new];

        for (index, entry) in entries.iter().enumerate() {
            match entry {
                NativeMenuEntry::Separator => menu.addItem_(NSMenuItem::separatorItem(nil)),
                NativeMenuEntry::Item { label, .. } => {
                    let title = NSString::alloc(nil).init_str(label);
                    let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
                        title,
                        sel!(desktopPetMenuItemSelected:),
                        NSString::alloc(nil).init_str(""),
                    );
                    let _: () = msg_send![item, setTag: index as isize];
                    let _: () = msg_send![item, setTarget: target];
                    menu.addItem_(item);
                }
            }
        }

        let _: bool = msg_send![
            menu,
            popUpMenuPositioningItem: nil
            atLocation: menu_point
            inView: ns_view
        ];
        let _: () = msg_send![target, release];
        let _: () = msg_send![menu, release];

        let tag = SELECTED_MENU_TAG.load(Ordering::SeqCst);
        if tag < 0 {
            return None;
        }
        match entries.get(tag as usize) {
            Some(NativeMenuEntry::Item { action_id, .. }) => Some(*action_id),
            _ => None,
        }
    }
}

#[cfg(target_os = "macos")]
fn desktop_pet_menu_target_class() -> &'static Class {
    static CLASS: std::sync::OnceLock<&'static Class> = std::sync::OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("CoduxDesktopPetMenuTarget", superclass)
            .expect("CoduxDesktopPetMenuTarget class should register once");
        decl.add_method(
            sel!(desktopPetMenuItemSelected:),
            desktop_pet_menu_item_selected as extern "C" fn(&Object, Sel, id),
        );
        decl.register()
    })
}

#[cfg(target_os = "macos")]
extern "C" fn desktop_pet_menu_item_selected(_this: &Object, _cmd: Sel, sender: id) {
    unsafe {
        let tag: isize = msg_send![sender, tag];
        SELECTED_MENU_TAG.store(tag, Ordering::SeqCst);
    }
}

#[cfg(target_os = "windows")]
pub(in crate::app) fn spawn_desktop_pet_native_menu(
    window: &mut Window,
    position: gpui::Point<gpui::Pixels>,
    entries: Vec<NativeMenuEntry>,
    cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
    spawn_native_popup_menu(
        window,
        position,
        entries,
        crate::app::CoduxApp::apply_desktop_pet_action,
        cx,
    );
}

#[cfg(target_os = "windows")]
pub(in crate::app) fn spawn_native_popup_menu(
    window: &mut Window,
    position: gpui::Point<gpui::Pixels>,
    entries: Vec<NativeMenuEntry>,
    on_select: fn(
        &mut crate::app::CoduxApp,
        &'static str,
        &mut Window,
        &mut gpui::Context<crate::app::CoduxApp>,
    ),
    cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
    let Some(hwnd) = win32_hwnd(window) else {
        return;
    };
    let Some(window_handle) = Window::window_handle(window).downcast::<crate::app::CoduxApp>()
    else {
        return;
    };
    cx.spawn(
        async move |_this: gpui::WeakEntity<crate::app::CoduxApp>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            if let Some(action_id) = show_desktop_pet_native_menu_for_hwnd(hwnd, position, &entries)
            {
                let _ = window_handle.update(cx, |app, window, cx| {
                    on_select(app, action_id, window, cx);
                });
            }
        },
    )
    .detach();
}

#[cfg(target_os = "windows")]
pub(in crate::app) fn show_desktop_pet_native_menu(
    window: &mut Window,
    position: gpui::Point<gpui::Pixels>,
    entries: &[NativeMenuEntry],
) -> Option<&'static str> {
    let hwnd = win32_hwnd(window)?;
    show_desktop_pet_native_menu_for_hwnd(hwnd, position, entries)
}

#[cfg(target_os = "windows")]
fn show_desktop_pet_native_menu_for_hwnd(
    hwnd: HWND,
    position: gpui::Point<gpui::Pixels>,
    entries: &[NativeMenuEntry],
) -> Option<&'static str> {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }

        let mut menu_id = 1usize;
        let mut action_ids = Vec::new();
        for entry in entries {
            match entry {
                NativeMenuEntry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                }
                NativeMenuEntry::Item { label, action_id } => {
                    let label = wide_string(label);
                    let _ = AppendMenuW(menu, MF_STRING, menu_id, label.as_ptr());
                    action_ids.push(*action_id);
                    menu_id += 1;
                }
            }
        }

        let mut point = POINT {
            x: position.x.as_f32().round() as i32,
            y: position.y.as_f32().round() as i32,
        };
        let _ = ClientToScreen(hwnd, &mut point);
        let _ = SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        let _ = DestroyMenu(menu);

        if command <= 0 {
            return None;
        }
        let index = command as usize - 1;
        action_ids.get(index).copied()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::app) fn spawn_desktop_pet_native_menu(
    _window: &mut Window,
    _position: gpui::Point<gpui::Pixels>,
    _entries: Vec<NativeMenuEntry>,
    _cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::app) fn spawn_native_popup_menu(
    _window: &mut Window,
    _position: gpui::Point<gpui::Pixels>,
    _entries: Vec<NativeMenuEntry>,
    _on_select: fn(
        &mut crate::app::CoduxApp,
        &'static str,
        &mut Window,
        &mut gpui::Context<crate::app::CoduxApp>,
    ),
    _cx: &mut gpui::Context<crate::app::CoduxApp>,
) {
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::app) fn show_desktop_pet_native_menu(
    _window: &mut Window,
    _position: gpui::Point<gpui::Pixels>,
    _entries: &[NativeMenuEntry],
) -> Option<&'static str> {
    None
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
unsafe fn clear_layer_background(view: id) {
    if view.is_null() {
        return;
    }
    unsafe {
        view.setWantsLayer(YES);
        let layer: id = view.layer();
        if layer.is_null() {
            return;
        }
        let _: () = msg_send![layer, setOpaque: NO];
        let _: () = msg_send![layer, setBackgroundColor: nil];
        let _: () = msg_send![layer, setCornerRadius: 0f64];
        let _: () = msg_send![layer, setMasksToBounds: NO];
    }
}

#[cfg(target_os = "macos")]
fn appkit_view(window: &mut Window) -> Option<id> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    let ns_view = handle.ns_view.as_ptr() as id;
    (!ns_view.is_null()).then_some(ns_view)
}

#[cfg(target_os = "macos")]
fn appkit_window(window: &mut Window) -> Option<id> {
    let ns_view = appkit_view(window)?;
    unsafe {
        let ns_window: id = msg_send![ns_view, window];
        (!ns_window.is_null()).then_some(ns_window)
    }
}

#[cfg(target_os = "windows")]
fn win32_hwnd(window: &mut Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    let hwnd = handle.hwnd.get() as HWND;
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    Some(if root.is_null() { hwnd } else { root })
}

#[cfg(target_os = "windows")]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
