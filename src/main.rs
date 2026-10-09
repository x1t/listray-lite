#![windows_subsystem = "windows"]
//! listray-lite：在"打开 / 保存 / 选择文件夹"对话框中，一键跳转到 Directory Opus 或资源管理器的当前文件夹。

mod dialog;
mod source;
mod switcher;
mod tray;
mod win;

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EVENT_SYSTEM_FOREGROUND,
    GetForegroundWindow, GetMessageW, KillTimer, MSG, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetTimer, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
};
use windows::core::w;

use source::FileManager;
use switcher::{Switcher, Window};
use tray::Command;

const HOTKEY_ID: i32 = 1;
const RECHECK_TIMER: usize = 1;
/// 对话框刚弹出时内部控件可能还没建好，每 200ms 重新识别一次，最多 5 次
const RECHECKS: u8 = 5;

thread_local! {
    static APP: RefCell<App> = RefCell::new(App {
        hwnd: HWND::default(),
        switcher: Switcher::new(),
        hotkey: false,
        recheck: None,
    });
}

/// Explorer 重启后会广播此消息，需要重新添加托盘图标
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct App {
    hwnd: HWND,
    switcher: Switcher,
    hotkey: bool,
    /// 待重新识别的窗口及剩余次数
    recheck: Option<(HWND, u8)>,
}

impl App {
    /// Ctrl+G 只在文件对话框处于前台时注册，不影响其他程序
    fn set_hotkey(&mut self, on: bool) {
        if on == self.hotkey {
            return;
        }
        self.hotkey = on;
        unsafe {
            let _ = if on {
                RegisterHotKey(
                    Some(self.hwnd),
                    HOTKEY_ID,
                    MOD_CONTROL | MOD_NOREPEAT,
                    u32::from(b'G'),
                )
            } else {
                UnregisterHotKey(Some(self.hwnd), HOTKEY_ID)
            };
        }
    }
}

fn main() {
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\listray-lite-single-instance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        let Some(hwnd) = create_window() else { return };
        APP.with_borrow_mut(|app| app.hwnd = hwnd);
        tray::add(hwnd, false);
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_win_event),
            0,
            0,
            flags,
        );

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
        tray::remove(hwnd);
    }
}

fn create_window() -> Option<HWND> {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(None).ok()?.into(),
            lpszClassName: w!("listray-lite"),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("listray-lite"),
            w!("listray-lite"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
        .ok()
    }
}

unsafe extern "system" fn on_win_event(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    on_foreground(hwnd, RECHECKS);
}

fn on_foreground(hwnd: HWND, rechecks_left: u8) {
    let window = classify(hwnd);
    // 跳转涉及跨进程 COM / 消息调用，期间可能重入本回调，所以先释放状态再跳转
    let target = APP.with_borrow_mut(|app| {
        let retry =
            window == Window::Other && rechecks_left > 0 && win::class_name(hwnd) == "#32770";
        app.recheck = retry.then(|| (hwnd, rechecks_left - 1));
        unsafe {
            if retry {
                SetTimer(Some(app.hwnd), RECHECK_TIMER, 200, None);
            } else {
                let _ = KillTimer(Some(app.hwnd), RECHECK_TIMER);
            }
        }
        let is_dialog = matches!(window, Window::Dialog(_));
        app.set_hotkey(is_dialog && !app.switcher.paused);
        app.switcher.on_foreground(window)
    });
    if let Some(manager) = target {
        jump(hwnd, manager);
    }
}

fn classify(hwnd: HWND) -> Window {
    if let Some(manager) = FileManager::from_window(hwnd) {
        Window::Manager(manager)
    } else if dialog::is_file_dialog(hwnd) {
        Window::Dialog(hwnd)
    } else {
        Window::Other
    }
}

fn jump(dialog: HWND, manager: FileManager) {
    if let Some(folder) = manager.current_folder() {
        dialog::navigate(dialog, &folder);
    }
}

fn on_recheck_timer() {
    let recheck = APP.with_borrow_mut(|app| app.recheck.take());
    match recheck {
        Some((hwnd, left)) if unsafe { GetForegroundWindow() } == hwnd => on_foreground(hwnd, left),
        _ => APP.with_borrow(|app| unsafe {
            let _ = KillTimer(Some(app.hwnd), RECHECK_TIMER);
        }),
    }
}

fn on_hotkey() {
    let dialog = unsafe { GetForegroundWindow() };
    let target = APP.with_borrow(|app| app.switcher.manual_target());
    if let Some(manager) = target.filter(|_| dialog::is_file_dialog(dialog)) {
        jump(dialog, manager);
    }
}

fn on_tray_menu(hwnd: HWND) {
    let (auto_jump, paused) = APP.with_borrow(|app| (app.switcher.auto_jump, app.switcher.paused));
    let Some(command) = tray::show_menu(hwnd, auto_jump, paused) else {
        return;
    };
    APP.with_borrow_mut(|app| match command {
        Command::ToggleAutoJump => app.switcher.auto_jump = !auto_jump,
        Command::TogglePause => {
            app.switcher.paused = !paused;
            app.set_hotkey(false);
            tray::update_tip(hwnd, !paused);
        }
        Command::ToggleAutostart => tray::set_autostart(!tray::autostart_enabled()),
        Command::Exit => unsafe { PostQuitMessage(0) },
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => on_hotkey(),
        WM_TIMER if wparam.0 == RECHECK_TIMER => on_recheck_timer(),
        tray::WM_TRAY if matches!(lparam.0 as u32, WM_RBUTTONUP | WM_LBUTTONUP) => {
            on_tray_menu(hwnd)
        }
        _ if msg == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            tray::add(hwnd, APP.with_borrow(|app| app.switcher.paused));
        }
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}
