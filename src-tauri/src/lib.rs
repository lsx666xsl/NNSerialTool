// ============ 模块声明 ============
mod net;
mod serial;

use net::*;
use serial::*;

// ============ Tauri 应用入口 ============
// #[cfg_attr(mobile, ...)] 是条件编译：
//   - 桌面端（Windows/macOS/Linux）：这行不起作用，run 就是普通函数
//   - 移动端（iOS/Android）：给 run 函数加上 mobile_entry_point 属性
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default() // 创建 Tauri 应用构建器
        .plugin(tauri_plugin_opener::init()) // 注册 opener 插件（打开 URL/文件）
        .manage(SharedState::default()) // 将串口状态注入为全局共享状态
        .manage(SharedNetState::default()) // 将网络连接状态注入为全局共享状态
        .invoke_handler(tauri::generate_handler![
            // 注册所有命令（前端才能 invoke 调用）
            serial_list_ports, // 列出可用串口
            serial_baudrate_list,
            serial_databit_list,
            serial_paritybit_list,
            serial_stopbit_list,
            serial_open,
            serial_close,
            serial_write,
            serial_log_write,
            serial_auto_send_start,
            serial_auto_send_stop,
            serial_set_baudrate,
            serial_set_databits,
            serial_set_parity,
            serial_set_stopbits,
            net_open, // 打开网络连接（TCP 客户端/服务端/UDP）
            net_close,
            net_write,
            net_auto_send_start,
            net_auto_send_stop,
            net_local_ips,
        ])
        .run(tauri::generate_context!()) // 启动应用
        .expect("error while running tauri application");
}
