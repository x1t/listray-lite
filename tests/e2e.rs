//! 端到端测试：启动真实的 listray-lite.exe、系统文件对话框、资源管理器和 Directory Opus，
//! 验证对话框地址栏确实跳到了目标文件夹。会抢占前台窗口，运行期间请勿操作鼠标键盘。

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowTextW,
    GetWindowThreadProcessId, IDCANCEL, IsWindowVisible, PostMessageW, SetForegroundWindow,
    WM_CLOSE, WM_COMMAND, WM_GETTEXT,
};
use windows::core::{BOOL, HSTRING, w};

const DOPUSRT: &str = r"C:\Program Files\GPSoftware\Directory Opus\dopusrt.exe";
static SERIAL: Mutex<()> = Mutex::new(());

/// 测试结束时自动结束进程 / 关闭窗口
struct Tool(Child);
impl Drop for Tool {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Dialog {
    hwnd: HWND,
    host: Child,
}
impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd),
                WM_COMMAND,
                WPARAM(IDCANCEL.0 as usize),
                LPARAM(0),
            );
        }
        let _ = self.host.wait();
    }
}

struct TopWindow(HWND);
impl Drop for TopWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.0), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn start_tool() -> Tool {
    let tool = Tool(
        Command::new(env!("CARGO_BIN_EXE_listray-lite"))
            .spawn()
            .expect("启动 listray-lite"),
    );
    sleep(Duration::from_millis(500));
    tool
}

fn open_dialog(kind: &str, title: &str) -> Dialog {
    let create = match kind {
        "save" => {
            "$d = New-Object System.Windows.Forms.SaveFileDialog; $d.FileName = 'probe.txt'; $d.Title = $t"
        }
        _ => {
            "$d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = $t; $d.UseDescriptionForTitle = $true"
        }
    };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $t = '{title}'; {create}; $d.InitialDirectory = 'C:\\Windows'; [void]$d.ShowDialog()"
    );
    let host = Command::new("pwsh")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .spawn()
        .expect("启动 pwsh");
    let hwnd = wait_for(|| unsafe { FindWindowW(w!("#32770"), &HSTRING::from(title)) }.ok())
        .expect("对话框未出现");
    Dialog { hwnd, host }
}

fn open_explorer(dir: &Path) -> TopWindow {
    Command::new("explorer.exe")
        .arg(dir)
        .status()
        .expect("启动资源管理器");
    // 标题形如"lr-e2e-explorer - 文件资源管理器"
    let prefix = format!("{} - ", dir.file_name().unwrap().to_string_lossy());
    let found = wait_for(|| {
        top_windows("CabinetWClass")
            .into_iter()
            .find(|&h| text(h).starts_with(&prefix))
    });
    let window = TopWindow(found.expect("资源管理器窗口未出现"));
    settle();
    window
}

fn open_dopus(dir: &Path) -> TopWindow {
    let before = top_windows("dopus.lister");
    Command::new(DOPUSRT)
        .raw_arg(format!("/cmd Go \"{}\" NEW", dir.display()))
        .status()
        .expect("调用 dopusrt");
    let window = TopWindow(
        wait_for(|| {
            top_windows("dopus.lister")
                .into_iter()
                .find(|h| !before.contains(h))
        })
        .expect("DOpus 窗口未出现"),
    );
    settle();
    window
}

/// 新开的文件管理器窗口会在稍后自己再抢一次前台，等它稳定
fn settle() {
    sleep(Duration::from_millis(2000));
}

fn test_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("e2e")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 绕过前台锁：临时挂到当前前台线程的输入队列上再切换
fn activate(hwnd: HWND) {
    unsafe {
        let current = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let _ = AttachThreadInput(me, current, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = AttachThreadInput(me, current, false);
    }
    let activated = wait_for(|| (unsafe { GetForegroundWindow() } == hwnd).then_some(()));
    let fg = unsafe { GetForegroundWindow() };
    assert!(
        activated.is_some(),
        "无法激活 {} [{}]，前台是 {} [{}]",
        class_of(hwnd),
        text(hwnd),
        class_of(fg),
        text(fg)
    );
    sleep(Duration::from_millis(800));
}

fn press_ctrl_g() {
    let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let g = VIRTUAL_KEY(u16::from(b'G'));
    let inputs = [
        key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key(g, KEYBD_EVENT_FLAGS(0)),
        key(g, KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

/// 对话框地址栏文字，形如"地址: F:\xxx"
fn address(dialog: HWND) -> String {
    let crumb = find_descendant(dialog, "Breadcrumb Parent").expect("找不到地址栏");
    text(
        unsafe { FindWindowExW(Some(crumb), None, w!("ToolbarWindow32"), None) }
            .expect("找不到地址栏工具条"),
    )
}

fn assert_address(dialog: HWND, dir: &Path) {
    let expected = dir.display().to_string();
    let reached = wait_for(|| address(dialog).ends_with(&expected).then_some(()));
    assert!(
        reached.is_some(),
        "地址栏应为 {expected}，实际为 {}",
        address(dialog)
    );
}

fn file_name_text(dialog: HWND) -> String {
    let sink = find_descendant(dialog, "FloatNotifySink").expect("找不到文件名框");
    let combo = unsafe { FindWindowExW(Some(sink), None, w!("ComboBox"), None) }.unwrap();
    let edit = unsafe { FindWindowExW(Some(combo), None, w!("Edit"), None) }.unwrap();
    let mut buf = [0u16; 512];
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            edit,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        )
    };
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap()])
}

fn find_descendant(parent: HWND, class: &str) -> Option<HWND> {
    let direct = unsafe { FindWindowExW(Some(parent), None, &HSTRING::from(class), None) }.ok();
    direct.or_else(|| {
        children(parent)
            .into_iter()
            .find_map(|child| find_descendant(child, class))
    })
}

fn children(parent: HWND) -> Vec<HWND> {
    let mut list = Vec::new();
    let mut after = None;
    while let Ok(child) = unsafe { FindWindowExW(Some(parent), after, None, None) } {
        list.push(child);
        after = Some(child);
    }
    list
}

fn top_windows(class: &str) -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe { (*(lparam.0 as *mut Vec<HWND>)).push(hwnd) };
        true.into()
    }
    let mut all = Vec::new();
    unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)).unwrap() };
    all.into_iter()
        .filter(|&h| unsafe { IsWindowVisible(h) }.as_bool() && class_of(h) == class)
        .collect()
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..len])
}

fn text(hwnd: HWND) -> String {
    let mut buf = [0u16; 1024];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..len])
}

fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(value) = probe() {
            return Some(value);
        }
        sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn save_dialog_jumps_to_explorer_folder_and_keeps_file_name() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = test_dir("lr-e2e-explorer");
    let _tool = start_tool();
    let dialog = open_dialog("save", "LR-E2E-SAVE");
    activate(dialog.hwnd);
    let explorer = open_explorer(&dir);
    activate(explorer.0);
    activate(dialog.hwnd);
    assert_address(dialog.hwnd, &dir);
    assert_eq!(file_name_text(dialog.hwnd), "probe.txt");
}

#[test]
fn folder_dialog_jumps_to_dopus_source_tab() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    assert!(Path::new(DOPUSRT).exists(), "本机未安装 Directory Opus");
    let dir = test_dir("lr-e2e-dopus");
    let _tool = start_tool();
    let dialog = open_dialog("folder", "LR-E2E-FOLDER");
    activate(dialog.hwnd);
    let lister = open_dopus(&dir);
    activate(lister.0);
    activate(dialog.hwnd);
    assert_address(dialog.hwnd, &dir);
}

#[test]
fn new_dialog_waits_for_ctrl_g() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = test_dir("lr-e2e-hotkey");
    let _tool = start_tool();
    let explorer = open_explorer(&dir);
    activate(explorer.0);
    let dialog = open_dialog("save", "LR-E2E-HOTKEY");
    activate(dialog.hwnd);
    assert!(
        address(dialog.hwnd).ends_with(r"C:\Windows"),
        "新对话框不应自动跳转：{}",
        address(dialog.hwnd)
    );
    press_ctrl_g();
    assert_address(dialog.hwnd, &dir);
}
