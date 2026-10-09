//! 识别标准文件对话框（打开 / 保存 / 选择文件夹），并让它跳转到指定文件夹。

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, GetParent, IDOK, PostMessageW, WM_COMMAND, WM_SETTEXT,
};

use crate::win::{class_name, find_child, parent_class, send, to_wide};

/// "选择文件夹"对话框里"文件夹:"输入框的控件 ID
const FOLDER_EDIT_ID: i32 = 0x480;

pub fn is_file_dialog(hwnd: HWND) -> bool {
    class_name(hwnd) == "#32770"
        && find_child(hwnd, |h| class_name(h) == "SHELLDLL_DefView").is_some()
        && file_edit(hwnd).is_some()
}

/// 往文件名框写入文件夹路径再按"确定"：对话框会进入该文件夹，并自动保留原来的文件名。
pub fn navigate(dialog: HWND, folder: &str) -> bool {
    let Some(edit) = file_edit(dialog) else {
        return false;
    };
    let text = to_wide(folder);
    if !send(edit, WM_SETTEXT, WPARAM(0), LPARAM(text.as_ptr() as isize)) {
        return false;
    }
    let ok_button = unsafe { GetDlgItem(Some(dialog), IDOK.0) }.map_or(0, |h| h.0 as isize);
    unsafe {
        PostMessageW(
            Some(dialog),
            WM_COMMAND,
            WPARAM(IDOK.0 as usize),
            LPARAM(ok_button),
        )
    }
    .is_ok()
}

fn file_edit(dialog: HWND) -> Option<HWND> {
    // 打开 / 另存为：FloatNotifySink > ComboBox > Edit
    find_child(dialog, |h| {
        class_name(h) == "Edit"
            && parent_class(h) == "ComboBox"
            && unsafe { GetParent(h) }.is_ok_and(|combo| parent_class(combo) == "FloatNotifySink")
    })
    .or_else(|| {
        unsafe { GetDlgItem(Some(dialog), FOLDER_EDIT_ID) }
            .ok()
            .filter(|&h| class_name(h) == "Edit")
    })
}
