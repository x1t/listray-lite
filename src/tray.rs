//! 托盘图标、右键菜单，以及开机自启（HKCU\...\Run）。

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::Shell::{
    ExtractIconExW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, MF_CHECKED, MF_SEPARATOR,
    MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_NULL,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::win::to_wide;

pub const WM_TRAY: u32 = WM_APP + 1;
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("listray-lite");

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    ToggleAutoJump = 1,
    TogglePause,
    ToggleAutostart,
    Exit,
}

pub fn add(hwnd: HWND, paused: bool) {
    let mut data = icon_data(hwnd, paused);
    // shell32.dll 第 4 号图标：打开的文件夹
    unsafe { ExtractIconExW(w!("shell32.dll"), 4, None, Some(&mut data.hIcon), 1) };
    data.uFlags |= NIF_ICON;
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &data);
    }
}

pub fn update_tip(hwnd: HWND, paused: bool) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &icon_data(hwnd, paused));
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd, false));
    }
}

fn icon_data(hwnd: HWND, paused: bool) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: HICON::default(),
        ..Default::default()
    };
    let tip = if paused {
        "listray-lite（已暂停）"
    } else {
        "listray-lite：文件对话框快速跳转"
    };
    for (dst, src) in data.szTip.iter_mut().zip(tip.encode_utf16()) {
        *dst = src;
    }
    data
}

pub fn show_menu(hwnd: HWND, auto_jump: bool, paused: bool) -> Option<Command> {
    let items = [
        (Command::ToggleAutoJump, "切回对话框时自动跳转", auto_jump),
        (Command::TogglePause, "暂停", paused),
        (Command::ToggleAutostart, "开机自启", autostart_enabled()),
    ];
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for (cmd, text, checked) in items {
            let flags = if checked {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            };
            AppendMenuW(menu, flags, cmd as usize, &HSTRING::from(text)).ok()?;
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, None).ok()?;
        AppendMenuW(menu, MF_STRING, Command::Exit as usize, w!("退出")).ok()?;

        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        // 菜单要求宿主窗口在前台，否则点击别处时菜单不会消失
        let _ = SetForegroundWindow(hwnd);
        let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY;
        let chosen = TrackPopupMenu(menu, flags, pt.x, pt.y, None, hwnd, None).0;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        [
            Command::ToggleAutoJump,
            Command::TogglePause,
            Command::ToggleAutostart,
            Command::Exit,
        ]
        .into_iter()
        .find(|&cmd| cmd as i32 == chosen)
    }
}

pub fn autostart_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    }
    .is_ok()
}

pub fn set_autostart(enable: bool) {
    unsafe {
        if !enable {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let data = to_wide(&format!("\"{}\"", exe.display()));
        let bytes = (data.len() * 2) as u32;
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(data.as_ptr().cast()),
            bytes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_round_trip_in_real_registry() {
        // 老板已开启自启时不动它，避免把测试程序路径写进去
        if autostart_enabled() {
            return;
        }
        set_autostart(true);
        let enabled = autostart_enabled();
        set_autostart(false);
        assert!(enabled, "写入 Run 注册表项失败");
        assert!(!autostart_enabled(), "删除 Run 注册表项失败");
    }
}
