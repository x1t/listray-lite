<div align="center">

# 📂 listray-lite

**在任何"打开 / 保存 / 选择文件夹"对话框里，一键跳到你刚才在文件管理器里看的那个文件夹。**

单文件绿色 exe · 312 KB · 常驻内存约 1.6 MB · 免安装

[![CI](https://github.com/x1t/listray-lite/actions/workflows/ci.yml/badge.svg)](https://github.com/x1t/listray-lite/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/x1t/listray-lite?label=release)](https://github.com/x1t/listray-lite/releases/latest)
![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)
![Platform](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Size](https://img.shields.io/badge/exe-312%20KB-success)

### [⬇️ 下载最新版 listray-lite.exe](https://github.com/x1t/listray-lite/releases/latest/download/listray-lite.exe)

</div>

---

## ✨ 它解决什么问题

保存一个文件时，对话框总是停在"上次的位置"，而你想存的文件夹明明已经在 Directory Opus 或资源管理器里打开了。
以前只能在对话框里一级一级点，或者复制路径再粘贴。

现在：

1. 弹出"另存为"对话框
2. 切到文件管理器，进入目标文件夹
3. **切回对话框 → 自动跳到那个文件夹** ⚡

灵感来自 Listary 的 Quick Switch，只保留这一个功能，体积和内存小两个数量级。

## 🎯 功能

| 功能 | 说明 |
|---|---|
| 🔄 **自动跳转** | 对话框 → 文件管理器 → 回到同一个对话框时，自动跳到文件管理器当前文件夹 |
| ⌨️ **Ctrl+G** | 新弹出的对话框不会自动跳（避免打乱软件自己的默认位置），按 `Ctrl+G` 手动跳到最近用过的文件夹 |
| 🗂️ **Directory Opus** | 精确识别双栏模式：跳到**当前激活那一栏**的**激活标签** |
| 🪟 **资源管理器** | 支持 Win11 多标签页，跳到当前激活的标签 |
| 📝 **保留文件名** | "另存为"里已经填好的文件名，跳转后不会丢 |
| 🧰 **托盘菜单** | 自动跳转开关 / 暂停 / 开机自启 / 退出 |
| 🔒 **单实例** | 重复双击不会开出多个进程 |

支持的对话框：**打开**、**另存为**、**选择文件夹**（Vista 风格的新式对话框）。

## 🚀 使用

1. 从 [Releases](https://github.com/x1t/listray-lite/releases/latest) 下载 `listray-lite.exe`（也可以按下方"从源码编译"自行编译）。
   校验文件 `listray-lite.exe.sha256` 同在 Release 里，PowerShell 中用 `Get-FileHash listray-lite.exe` 对比即可
2. 双击运行，右下角托盘出现一个文件夹图标
3. 照常使用，需要时按 `Ctrl+G`

> 💡 想开机自启：右键托盘图标 → 勾选 **开机自启**。关闭时会自动清除注册表项，不留痕迹。

### 托盘菜单

```
✔ 切回对话框时自动跳转
  暂停
✔ 开机自启
──────────────
  退出
```

左键或右键点击托盘图标都会弹出菜单。

## ⚙️ 工作原理

```
前台窗口切换 ──► 识别窗口类型 ──► 记录"最近的文件管理器"
(SetWinEventHook)   │
                    ├─ 文件管理器 ─► 记下来
                    └─ 文件对话框 ─► 满足条件 ─► 读取文件夹路径 ─► 写入文件名框并确定
```

- **监听前台窗口**：`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`，事件驱动，没有轮询，空闲时 CPU 为 0。
- **识别对话框**：类名 `#32770`，并且包含 `SHELLDLL_DefView` 与文件名输入框。
- **读取 Directory Opus 路径**：调用 Opus 自带的 `dopusrt.exe /info <文件>,paths`，解析输出里 `active_lister="1"` 且 `tab_state="1"` 的那一行，也就是最近激活窗口的源栏激活标签。
- **读取资源管理器路径**：通过 `IShellWindows` 找到当前激活标签对应的浏览器，取 `IFolderView` 的当前文件夹。
- **跳转**：把路径写进对话框的文件名框并点"确定"，系统对话框会自己进入该文件夹，原来的文件名保持不变。

### 跳转时机

只有 **"对话框 → 文件管理器 → 回到同一个对话框"** 才会自动跳转，并且每次只跳一次。
这样不会在你只是切换窗口时乱跳，也不会在新对话框弹出时覆盖软件的默认位置。

## ⚠️ 已知限制

- 只支持 Windows 标准文件对话框。Qt、Java、Electron 等自绘对话框不支持（Listary 也一样）。
- 老式的"浏览文件夹"树形小窗口（没有路径输入框）暂不支持。
- "此电脑"、"控制面板"这类没有真实文件夹路径的位置会被跳过。
- 以管理员身份运行的程序弹出的对话框，需要本工具也以管理员身份运行才能操作。
- 切到 Directory Opus 时会调用一次 `dopusrt.exe`，大约几十毫秒，只在跳转时发生。

## 🛠️ 从源码编译

需要 Rust 工具链（MSVC）。

```powershell
cargo build --release
# 产物：target\release\listray-lite.exe
```

编译配置已针对体积优化（`opt-level = "s"`、LTO、`panic = "abort"`、strip），并静态链接 CRT，
产物只依赖 Windows 自带的系统 DLL，**不需要安装 VC++ 运行库**。

## 🧪 测试

```powershell
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

- **单元测试**：跳转时机判断、Directory Opus 输出解析、注册表自启读写，以及读取本机正在运行的 Opus 窗口。
- **端到端测试**（`tests/e2e.rs`）：启动真实的 `listray-lite.exe`、系统对话框、资源管理器和 Directory Opus，验证对话框地址栏确实跳到目标文件夹、文件名保留、新对话框不自动跳转而 `Ctrl+G` 可以跳转。

> ⚠️ 端到端测试会抢占前台窗口并模拟按键，约 18 秒，运行期间请勿操作鼠标键盘。运行它需要本机装有 Directory Opus。

## 📁 项目结构

```
src/
├── main.rs      入口、消息循环、前台窗口监听、Ctrl+G 热键
├── switcher.rs  跳转时机的状态机（纯逻辑，可单元测试）
├── dialog.rs    识别文件对话框、执行跳转
├── source.rs    读取 Directory Opus / 资源管理器的当前文件夹
├── tray.rs      托盘图标、右键菜单、开机自启
└── win.rs       Win32 窗口辅助函数
tests/
└── e2e.rs       真实窗口端到端测试
```

## 🙏 致谢

- [Listary](https://www.listary.com) 的 Quick Switch —— 这个小工具的灵感来源
- [QuickSwitch (AutoHotkey)](https://github.com/gepruts/QuickSwitch) —— 证明了读取 Directory Opus 路径这条路线可行
- [windows-rs](https://github.com/microsoft/windows-rs) —— 微软官方的 Rust Win32 绑定
