//! 跳转时机判断：只有"对话框 → 文件管理器 → 回到同一个对话框"时才自动跳转。

use windows::Win32::Foundation::HWND;

use crate::source::FileManager;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Window {
    Manager(FileManager),
    Dialog(HWND),
    Other,
}

pub struct Switcher {
    pub auto_jump: bool,
    pub paused: bool,
    manager: Option<FileManager>,
    dialog: Option<HWND>,
    /// 上次激活对话框之后，是否去过文件管理器
    visited_manager: bool,
}

impl Switcher {
    pub fn new() -> Self {
        Self {
            auto_jump: true,
            paused: false,
            manager: None,
            dialog: None,
            visited_manager: false,
        }
    }

    /// 前台窗口变化时调用，返回需要自动跳转到的文件管理器。
    pub fn on_foreground(&mut self, window: Window) -> Option<FileManager> {
        match window {
            Window::Manager(manager) => {
                self.manager = Some(manager);
                self.visited_manager = true;
                None
            }
            Window::Dialog(dialog) => {
                // 新弹出的对话框不自动跳，避免很久以前看过的文件夹打乱程序的默认位置
                let returning = self.dialog == Some(dialog);
                let jump = returning && self.visited_manager && self.auto_jump && !self.paused;
                self.dialog = Some(dialog);
                self.visited_manager = false;
                if jump { self.manager } else { None }
            }
            Window::Other => None,
        }
    }

    /// Ctrl+G：不论是否刚去过文件管理器，都跳到最近一个文件管理器的当前文件夹。
    pub fn manual_target(&self) -> Option<FileManager> {
        if self.paused { None } else { self.manager }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hwnd(n: usize) -> HWND {
        HWND(std::ptr::without_provenance_mut(n))
    }

    const DIALOG: Window = Window::Dialog(HWND(std::ptr::without_provenance_mut(1)));
    const DOPUS: FileManager = FileManager::DOpus(HWND(std::ptr::without_provenance_mut(2)));
    const EXPLORER: FileManager = FileManager::Explorer(HWND(std::ptr::without_provenance_mut(3)));

    #[test]
    fn jumps_when_returning_from_manager() {
        let mut s = Switcher::new();
        assert_eq!(s.on_foreground(DIALOG), None);
        assert_eq!(s.on_foreground(Window::Manager(DOPUS)), None);
        assert_eq!(s.on_foreground(DIALOG), Some(DOPUS));
    }

    #[test]
    fn uses_last_visited_manager() {
        let mut s = Switcher::new();
        s.on_foreground(DIALOG);
        s.on_foreground(Window::Manager(DOPUS));
        s.on_foreground(Window::Manager(EXPLORER));
        s.on_foreground(Window::Other);
        assert_eq!(s.on_foreground(DIALOG), Some(EXPLORER));
    }

    #[test]
    fn jumps_only_once_per_visit() {
        let mut s = Switcher::new();
        s.on_foreground(DIALOG);
        s.on_foreground(Window::Manager(DOPUS));
        s.on_foreground(DIALOG);
        s.on_foreground(Window::Other);
        assert_eq!(s.on_foreground(DIALOG), None);
    }

    #[test]
    fn new_dialog_does_not_jump() {
        let mut s = Switcher::new();
        s.on_foreground(Window::Manager(DOPUS));
        assert_eq!(s.on_foreground(DIALOG), None);
        assert_eq!(s.on_foreground(Window::Dialog(hwnd(9))), None);
        assert_eq!(s.manual_target(), Some(DOPUS));
    }

    #[test]
    fn respects_auto_jump_and_pause() {
        let mut s = Switcher::new();
        s.auto_jump = false;
        s.on_foreground(DIALOG);
        s.on_foreground(Window::Manager(DOPUS));
        assert_eq!(s.on_foreground(DIALOG), None);
        assert_eq!(s.manual_target(), Some(DOPUS));

        s.auto_jump = true;
        s.paused = true;
        s.on_foreground(Window::Manager(DOPUS));
        assert_eq!(s.on_foreground(DIALOG), None);
        assert_eq!(s.manual_target(), None);
    }
}
