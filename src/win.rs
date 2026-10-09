//! 少量 Win32 窗口辅助函数。

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetParent, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
};
use windows::core::BOOL;

pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..len])
}

pub fn parent_class(hwnd: HWND) -> String {
    unsafe { GetParent(hwnd) }
        .map(class_name)
        .unwrap_or_default()
}

/// 深度遍历子窗口，返回第一个满足条件的。
pub fn find_child(parent: HWND, mut pred: impl FnMut(HWND) -> bool) -> Option<HWND> {
    struct Search<'a> {
        pred: &'a mut dyn FnMut(HWND) -> bool,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        if (search.pred)(hwnd) {
            search.found = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut search = Search {
        pred: &mut pred,
        found: None,
    };
    unsafe {
        let _ = EnumChildWindows(
            Some(parent),
            Some(visit),
            LPARAM(&mut search as *mut Search as isize),
        );
    }
    search.found
}

/// 跨进程发消息，目标程序卡死时 1 秒内放弃，避免把本程序也拖住。
pub fn send(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    unsafe { SendMessageTimeoutW(hwnd, msg, wparam, lparam, SMTO_ABORTIFHUNG, 1000, None) }.0 != 0
}
