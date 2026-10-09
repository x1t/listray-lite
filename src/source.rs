//! 读取文件管理器（资源管理器 / Directory Opus）当前显示的文件夹。

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{
    GPFIDL_DEFAULT, IFolderView, IPersistFolder2, IShellBrowser, IShellWindows,
    SHGetPathFromIDListEx, SID_STopLevelBrowser, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId};
use windows::core::{Interface, PWSTR, w};

use crate::win::class_name;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileManager {
    Explorer(HWND),
    DOpus(HWND),
}

impl FileManager {
    pub fn from_window(hwnd: HWND) -> Option<Self> {
        match class_name(hwnd).as_str() {
            "CabinetWClass" => Some(Self::Explorer(hwnd)),
            "dopus.lister" => Some(Self::DOpus(hwnd)),
            _ => None,
        }
    }

    pub fn current_folder(self) -> Option<String> {
        match self {
            Self::Explorer(hwnd) => explorer_folder(hwnd),
            Self::DOpus(hwnd) => dopus_folder(hwnd),
        }
    }
}

/// 在所有 Shell 窗口里找到该资源管理器窗口当前激活的标签页（Win11 多标签共用一个顶层窗口）。
fn explorer_folder(hwnd: HWND) -> Option<String> {
    // 激活的标签页排在 Z 序最前，FindWindowEx 返回的第一个就是它
    let active_tab =
        unsafe { FindWindowExW(Some(hwnd), None, w!("ShellTabWindowClass"), None) }.ok();
    let windows: IShellWindows =
        unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }.ok()?;
    for i in 0..unsafe { windows.Count() }.ok()? {
        let Ok(browser) = shell_browser(&windows, i) else {
            continue;
        };
        let Ok(owner) = (unsafe { browser.GetWindow() }) else {
            continue;
        };
        if Some(owner) == active_tab || owner == hwnd {
            return browser_folder(&browser);
        }
    }
    None
}

fn shell_browser(windows: &IShellWindows, index: i32) -> windows::core::Result<IShellBrowser> {
    let provider: IServiceProvider = unsafe { windows.Item(&VARIANT::from(index)) }?.cast()?;
    unsafe { provider.QueryService(&SID_STopLevelBrowser) }
}

fn browser_folder(browser: &IShellBrowser) -> Option<String> {
    unsafe {
        let view: IFolderView = browser.QueryActiveShellView().ok()?.cast().ok()?;
        let folder: IPersistFolder2 = view.GetFolder().ok()?;
        let pidl = folder.GetCurFolder().ok()?;
        let mut buf = vec![0u16; 32768];
        // "此电脑"等虚拟位置没有文件系统路径，这里会失败
        let ok = SHGetPathFromIDListEx(pidl, &mut buf, GPFIDL_DEFAULT).as_bool();
        CoTaskMemFree(Some(pidl as *const _));
        ok.then(|| crate::win::from_wide(&buf))
    }
}

/// 调用 DOpus 自带的 dopusrt.exe 导出所有标签页路径，取激活窗口中源栏的激活标签。
fn dopus_folder(lister: HWND) -> Option<String> {
    let dopusrt = process_path(lister)?.with_file_name("dopusrt.exe");
    let xml_path = std::env::temp_dir().join(format!("listray-lite-{}.xml", std::process::id()));
    Command::new(dopusrt)
        .raw_arg(format!("/info \"{}\",paths", xml_path.display()))
        .creation_flags(CREATE_NO_WINDOW.0)
        .status()
        .ok()?;
    let xml = std::fs::read_to_string(&xml_path).ok();
    let _ = std::fs::remove_file(&xml_path);
    active_dopus_path(&xml?)
}

fn process_path(hwnd: HWND) -> Option<PathBuf> {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    unsafe { CloseHandle(process) }.ok()?;
    result.ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(
        &buf[..len as usize],
    )))
}

/// dopusrt 每个标签输出一行 `<path ...>路径</path>`：
/// `active_lister="1"` 表示最近激活的窗口，`tab_state="1"` 表示源栏的激活标签。
fn active_dopus_path(xml: &str) -> Option<String> {
    xml.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("<path "))
        .filter(|line| line.contains(r#"active_lister="1""#) && line.contains(r#"tab_state="1""#))
        .find_map(|line| {
            let start = line.find('>')? + 1;
            let end = line.rfind("</path>")?;
            Some(unescape_xml(&line[start..end]))
        })
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 本机 dopusrt.exe /info 的真实输出格式（双栏、多标签）
    const DUAL_PANE: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<results command=\"paths\" result=\"1\">
	<path active_lister=\"1\" display_path=\"G:\\workspace\" lister=\"0xd08ba\" side=\"1\" tab=\"0x140ace\">G:\\workspace</path>
	<path active_lister=\"1\" active_tab=\"1\" display_path=\"F:\\workspace\\listray-lite\" lister=\"0xd08ba\" side=\"1\" tab=\"0x60f48\" tab_state=\"2\">F:\\workspace\\listray-lite</path>
	<path active_lister=\"1\" active_tab=\"2\" display_path=\"C:\\用户\\xctcc\\.claude\" lister=\"0xd08ba\" side=\"2\" tab=\"0x140696\" tab_state=\"1\">C:\\Users\\xctcc\\.claude</path>
	<path lister=\"0x2\" active_tab=\"1\" side=\"1\" tab=\"0x9\" tab_state=\"1\">D:\\other lister</path>
</results>";

    #[test]
    fn picks_source_tab_of_active_lister() {
        assert_eq!(
            active_dopus_path(DUAL_PANE).as_deref(),
            Some(r"C:\Users\xctcc\.claude")
        );
    }

    #[test]
    fn returns_none_without_active_tab() {
        assert_eq!(
            active_dopus_path("<results command=\"paths\" result=\"1\">\n</results>"),
            None
        );
    }

    #[test]
    fn reads_real_dopus_lister() {
        let lister = unsafe {
            windows::Win32::UI::WindowsAndMessaging::FindWindowW(w!("dopus.lister"), None)
        }
        .expect("需要先打开 Directory Opus");
        let folder = FileManager::from_window(lister).and_then(FileManager::current_folder);
        let folder = folder.expect("应读到 DOpus 源栏路径");
        assert!(
            std::path::Path::new(&folder).is_dir(),
            "{folder} 不是文件夹"
        );
    }

    #[test]
    fn unescapes_xml_entities() {
        let xml =
            r#"<path active_lister="1" tab_state="1">D:\A &amp; B\&lt;x&gt; &apos;q&apos;</path>"#;
        assert_eq!(active_dopus_path(xml).as_deref(), Some(r"D:\A & B\<x> 'q'"));
    }
}
