// Windows 平台下，Release 模式不弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 唯一的作用：调用 lib.rs 中的 run() 函数，启动 Tauri 应用
    serialtool_lib::run()
}
