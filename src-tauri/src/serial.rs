use serde::Serialize;
use serialport::{DataBits, Parity, SerialPort, StopBits};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use tauri::{AppHandle, Emitter};

/*
*   描述：串口句柄
*/
struct PortHandle {
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    stop_flag: Arc<AtomicBool>,
}

/*
*   描述：用于管理多个串口状态的哈希表
*/
pub struct SerialState {
    pub ports: HashMap<String, PortHandle>,
    // 自动发送任务表：port -> 停止标志（Rust 线程直接循环写，绕过前端 IPC）
    pub auto_sends: HashMap<String, Arc<AtomicBool>>,
}

/*
*   描述：全局共享哈希表（Arc 包装：async 命令需要把所有权移入 spawn_blocking 闭包）
*/
pub type SharedState = Arc<Mutex<SerialState>>;

/*
*   描述：默认生成的哈希表
*/
impl Default for SerialState {
    fn default() -> Self {
        Self {
            ports: HashMap::new(),
            auto_sends: HashMap::new(),
        }
    }
}

/*
*   描述：传递串口消息事件
*/
#[derive(Serialize, Clone)]
struct SerialDataEvent {
    port: String,
    data: Vec<u8>,
}

/*
*   描述：自动发送通知事件（前端据此记录 TX，保证自动发送的内容在界面上可见）
*/
#[derive(Serialize, Clone)]
struct AutoSentEvent {
    key: String,
    data: Vec<u8>,
}

/*
*   描述：串口信息（端口号 + 产品名），返回给前端展示
*/
#[derive(Serialize, Debug)]
pub struct SerialPortInfo {
    portnum: String,
    portproduct: String,
}

/*
*   描述：读取串口数据（独立 OS 线程，抢占式调度，不占主线程）
*/
fn serial_read_thread(
    port_name: String,
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) -> () {
    thread::spawn(move || {
        let mut rbuf = [0u8; 4096];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            // 先查有没有数据可用（非阻塞，不占锁太久）
            let rxbufsize = {
                let sp = port.lock().unwrap();
                match sp.bytes_to_read() {
                    Ok(n) => n as usize,
                    Err(_) => {
                        let _ = app_handle.emit("serial-disconnect", &port_name);
                        break;
                    }
                }
            }; // 锁在此释放

            if rxbufsize == 0 {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }

            // 有数据才锁住读取
            let mut sp = port.lock().unwrap();
            let n = rxbufsize.min(rbuf.len());
            match sp.read(&mut rbuf[..n]) {
                Ok(n) => {
                    let _ = app_handle.emit(
                        "serial-read_data",
                        SerialDataEvent {
                            port: port_name.clone(),
                            data: rbuf[..n].to_vec(),
                        },
                    );
                }
                Err(_) => {
                    let _ = app_handle.emit("serial-disconnect", &port_name);
                    break;
                }
            }
        }
    });
}

/*
*   描述：高精度等待：Windows 的 thread::sleep 粒度约 15.6ms，
*         自动发送的短间隔（如 10ms）误差会高达 50%+。
*         策略：先粗睡到目标的 80%，剩余部分自旋到点（自旋时长 ≤20% 间隔）。
*/
pub(crate) fn wait_precisely(interval: std::time::Duration) {
    let start = std::time::Instant::now();
    let coarse = std::time::Duration::from_nanos((interval.as_nanos() as u128 * 8 / 10) as u64);
    if coarse > std::time::Duration::ZERO {
        thread::sleep(coarse);
    }
    while start.elapsed() < interval {
        std::hint::spin_loop();
    }
}

/*
*   描述：串口写入的公共实现（发送命令与自动发送线程共用）
*/
fn serial_write_bytes(handle: &PortHandle, bytes: &[u8]) -> Result<(), String> {
    let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
    sp.write_all(bytes)
        .map_err(|_| "发送失败，串口已断开".to_string())
}

/*
*   描述：列出系统上所有可用的串口
*   说明：async 命令 —— 枚举串口的系统调用可能在 Windows 上耗时，
*         移入 spawn_blocking 线程池执行，避免每 2 秒一次的轮询卡顿 UI 主线程
*/
#[tauri::command]
pub async fn serial_list_ports() -> Vec<SerialPortInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let port = match serialport::available_ports() {
            Ok(p) => p,
            Err(e) => {
                println!("获取串口列表失败: {}", e);
                return Vec::new();
            }
        };

        let mut port_list: Vec<SerialPortInfo> = Vec::new();
        for p in port {
            port_list.push(SerialPortInfo {
                portnum: p.port_name,
                portproduct: match p.port_type {
                    serialport::SerialPortType::UsbPort(info) => info
                        .product
                        .unwrap_or_else(|| "未知设备".into())
                        .split(" (")
                        .next()
                        .unwrap_or_else(|| "未知设备".into())
                        .to_string(),
                    _ => "未知设备".to_string(),
                },
            });
        }
        port_list
    })
    .await
    .unwrap_or_default()
}

/// 列出可选波特率（纯数据，无需 async）
#[tauri::command]
pub fn serial_baudrate_list() -> Vec<u32> {
    vec![
        1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200, 128000, 230400, 256000, 460800,
        512000, 750000, 921600, 1000000, 1500000, 2000000,
    ]
}

/// 列出可选数据位（纯数据，无需 async）
#[tauri::command]
pub fn serial_databit_list() -> Vec<u8> {
    vec![6, 7, 8]
}

/// 列出可选校验位（纯数据，无需 async）
#[tauri::command]
pub fn serial_paritybit_list() -> Vec<String> {
    vec!["None".to_string(), "Even".to_string(), "Odd".to_string()]
}

/// 列出可选停止位（纯数据，无需 async）
#[tauri::command]
pub fn serial_stopbit_list() -> Vec<u8> {
    vec![1, 2]
}

/*
*   描述：打开指定的串口
*   说明：async 命令 —— serialport::open 与驱动交互可能阻塞，移入线程池执行
*/
#[tauri::command]
pub async fn serial_open(
    port: String,
    baud_rate: u32,
    data_bits: u8,
    parity_bits: String,
    stop_bits: u8,
    state: tauri::State<'_, SharedState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // ① 快查：锁只用来检查是否已打开，查完立刻放
        {
            let guard = state.lock().map_err(|e| e.to_string())?;
            if guard.ports.contains_key(&port) {
                return Err(format!("串口 {} 已打开", port));
            }
        }

        // ② 打开串口（不持锁，卡多久都不影响其他命令）
        let sp = serialport::new(&port, baud_rate)
            .timeout(std::time::Duration::from_millis(100))
            .data_bits(match data_bits {
                5 => DataBits::Five,
                6 => DataBits::Six,
                7 => DataBits::Seven,
                _ => DataBits::Eight,
            })
            .parity(match parity_bits.as_str() {
                "Odd" => Parity::Odd,
                "Even" => Parity::Even,
                _ => Parity::None,
            })
            .stop_bits(match stop_bits {
                2 => StopBits::Two,
                _ => StopBits::One,
            })
            .open()
            .map_err(|e| format!("打开串口失败: {}", e))?;

        let port_arc = Arc::new(Mutex::new(sp));
        let stop_arc = Arc::new(AtomicBool::new(false));

        // ③ 启动后台读线程
        serial_read_thread(port.clone(), port_arc.clone(), stop_arc.clone(), app_handle);

        // ④ 再次锁住存入（这一瞬间的事）
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.ports.insert(
            port.clone(),
            PortHandle {
                port: port_arc,
                stop_flag: stop_arc,
            },
        );

        Ok(format!("串口 {} 已打开", port))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：关闭指定串口（async：close 涉及驱动交互）
*/
#[tauri::command]
pub async fn serial_close(port: String, state: tauri::State<'_, SharedState>) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        match state.ports.remove(&port) {
            Some(handle) => {
                handle.stop_flag.store(true, Ordering::SeqCst);
                // 连接关闭时同步停止该端口的自动发送任务
                if let Some(task) = state.auto_sends.remove(&port) {
                    task.store(true, Ordering::SeqCst);
                }
                Ok(format!("串口 {} 已关闭", port))
            }
            None => Err(format!("串口 {} 未打开", port)),
        }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：发送数据到串口（async：WriteFile 在驱动层可能阻塞数十毫秒，这是“发送卡一下”的元凶）
*/
#[tauri::command]
pub async fn serial_write(
    port: String,
    data: String,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 获取哈希键值对的锁
        let guard = state.lock().map_err(|e| e.to_string())?;
        // 查看键值对里的指定端口是否存在
        let handle = guard
            .ports
            .get(&port)
            .ok_or(format!("串口 {} 未打开", port))?;
        // 将数据写入串口（公共实现）
        let handle = handle;
        serial_write_bytes(handle, data.as_bytes())?;
        Ok(format!("已发送 {} 字节", data.len()))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：动态修改已打开串口的波特率（async：底层为驱动调用）
*/
#[tauri::command]
pub async fn serial_set_baudrate(
    port: String,
    baud_rate: u32,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = state.lock().map_err(|e| e.to_string())?;
        let handle = guard
            .ports
            .get(&port)
            .ok_or(format!("串口 {} 未打开", port))?;
        let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
        sp.set_baud_rate(baud_rate)
            .map_err(|e| format!("修改波特率失败: {}", e))?;
        Ok(format!("串口 {} 波特率已改为 {}", port, baud_rate))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 动态修改数据位（async：底层为驱动调用）
#[tauri::command]
pub async fn serial_set_databits(
    port: String,
    data_bits: u8,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = state.lock().map_err(|e| e.to_string())?;
        let handle = guard.ports.get(&port).ok_or(format!("串口 {} 未打开", port))?;
        let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
        let bits = match data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        };
        sp.set_data_bits(bits)
            .map_err(|e| format!("修改数据位失败: {}", e))?;
        Ok(format!("串口 {} 数据位已改为 {}", port, data_bits))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 动态修改校验位（async：底层为驱动调用）
#[tauri::command]
pub async fn serial_set_parity(
    port: String,
    parity: String,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = state.lock().map_err(|e| e.to_string())?;
        let handle = guard.ports.get(&port).ok_or(format!("串口 {} 未打开", port))?;
        let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
        let p = match parity.as_str() {
            "Odd" => Parity::Odd,
            "Even" => Parity::Even,
            _ => Parity::None,
        };
        sp.set_parity(p).map_err(|e| format!("修改校验位失败: {}", e))?;
        Ok(format!("串口 {} 校验位已改为 {}", port, parity))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 动态修改停止位（async：底层为驱动调用）
#[tauri::command]
pub async fn serial_set_stopbits(
    port: String,
    stop_bits: u8,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = state.lock().map_err(|e| e.to_string())?;
        let handle = guard.ports.get(&port).ok_or(format!("串口 {} 未打开", port))?;
        let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
        let bits = match stop_bits {
            2 => StopBits::Two,
            _ => StopBits::One,
        };
        sp.set_stop_bits(bits)
            .map_err(|e| format!("修改停止位失败: {}", e))?;
        Ok(format!("串口 {} 停止位已改为 {}", port, stop_bits))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：追加写入串口日志到项目 log 目录（async：文件 IO 移入线程池）
*/
#[tauri::command]
pub async fn serial_log_write(date: String, text: String) -> Result<(), String> {
    use std::fs::OpenOptions;

    tauri::async_runtime::spawn_blocking(move || {
        // 定位项目根目录：tauri dev 的工作目录是 src-tauri，需要上跳一级才是项目根
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = if cwd.file_name().map(|n| n == "src-tauri").unwrap_or(false) {
            cwd.parent().map(|p| p.to_path_buf()).unwrap_or(cwd)
        } else {
            cwd
        };

        // log 目录不存在则新建
        let log_dir = root.join("log");
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;

        // 按天分文件：log/serial_YYYY-MM-DD.log，追加模式写入
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("serial_{}.log", date)))
            .map_err(|e| format!("打开日志文件失败: {}", e))?;

        file.write_all(text.as_bytes())
            .map_err(|e| format!("写入日志失败: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}


/*
*   描述：启动自动发送（Rust 线程按间隔直接写串口，绕过前端 IPC，根治高频发送卡顿）
*   传参：port：端口号, data：待发送内容, interval_ms：间隔（最小 10ms）
*/
#[tauri::command]
pub async fn serial_auto_send_start(
    port: String,
    data: String,
    interval_ms: u64,
    state: tauri::State<'_, SharedState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        // 已有任务先停
        if let Some(old) = guard.auto_sends.remove(&port) {
            old.store(true, Ordering::SeqCst);
        }
        if !guard.ports.contains_key(&port) {
            return Err(format!("串口 {} 未打开", port));
        }
        let stop = Arc::new(AtomicBool::new(false));
        guard.auto_sends.insert(port.clone(), stop.clone());
        drop(guard);

        // 独立线程循环发送，直到停止标志置位或串口失效（高精度等待）
        let interval = std::time::Duration::from_millis(interval_ms.max(1));
        thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            wait_precisely(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let guard = match state.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            match guard.ports.get(&port) {
                Some(handle) => {
                    if let Err(e) = serial_write_bytes(handle, data.as_bytes()) {
                        // 写失败：通知前端弹回自动发送开关（否则静默失效，用户以为还在发送）
                        let _ = app_handle.emit(
                            "auto-send-stopped",
                            AutoSentEvent {
                                key: port.clone(),
                                data: e.into_bytes(),
                            },
                        );
                        break;
                    }
                    // 通知前端记录 TX（自动发送绕过了前端 sendData，必须显式上报）
                    let _ = app_handle.emit(
                        "auto-sent",
                        AutoSentEvent {
                            key: port.clone(),
                            data: data.as_bytes().to_vec(),
                        },
                    );
                }
                None => break,
            }
        });
        Ok(format!("自动发送已启动，间隔 {}ms", interval_ms))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：停止指定端口的自动发送任务
*/
#[tauri::command]
pub async fn serial_auto_send_stop(port: String, state: tauri::State<'_, SharedState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if let Some(task) = guard.auto_sends.remove(&port) {
            task.store(true, Ordering::SeqCst);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
