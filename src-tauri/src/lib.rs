// ============ 模块声明 ============
use tauri::Manager;

mod fonts;
mod net;
mod serial;

use fonts::*;
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
        .plugin(tauri_plugin_dialog::init()) // 注册 dialog 插件（原生目录选择器）
        .plugin(tauri_plugin_updater::Builder::new().build()) // 原地更新（签名校验 + 静默安装）
        .plugin(tauri_plugin_process::init()) // 更新安装后自动重启
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
            serial_close_all, // 关闭全部串口（前端初始化时清理残留句柄）
            serial_write,
            serial_log_write,
            serial_log_dir, // 默认日志导出目录（exe 所在目录下的 log）
            system_fonts_list, // 枚举 Windows 已安装字体（设置字体下拉）
            serial_auto_send_start,
            serial_auto_send_stop,
            serial_set_baudrate,
            serial_set_databits,
            serial_set_parity,
            serial_set_stopbits,
            net_open, // 打开网络连接（TCP 客户端/服务端/UDP）
            net_close,
            net_close_all, // 关闭全部网络连接（前端初始化时清理残留监听）
            net_write,
            net_auto_send_start,
            net_auto_send_stop,
            net_local_ips,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // 应用退出（窗口关闭/进程结束）：强制释放全部串口句柄与网络监听，
            // 避免退出后端口仍被占用（下次启动报 os error 10048 / 串口打不开）
            if let tauri::RunEvent::Exit = event {
                serial::close_all_sync(app_handle.state::<SharedState>().inner());
                net::close_all_sync(app_handle.state::<SharedNetState>().inner());
            }
        });
}
