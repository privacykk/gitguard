//! 桌面可执行文件入口。
//!
//! 发布版在 Windows 上隐藏控制台窗口，否则启动时会多弹一个黑框。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 应用逻辑放在 lib 里，便于测试与后续复用。
    gitguard_desktop_lib::run()
}
