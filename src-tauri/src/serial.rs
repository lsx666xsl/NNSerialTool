use serde::Serialize;
use serialport::{DataBits, Parity, SerialPort, StopBits};
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use tauri::{AppHandle, Emitter, Manager};

/*
*   描述：串口默认读/写超时（毫秒）。
*         读线程依赖该超时做 5ms 轮询节奏检查停止标志；
*         写线程会按“长度/波特率”临时放大写超时，写完立即恢复成该值。
*/
const IO_TIMEOUT_MS: u64 = 100;

/*
*   描述：串口句柄
*/
pub(crate) struct PortHandle {
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    stop_flag: Arc<AtomicBool>,
    // 读线程句柄：关闭时 join，确保 Box<dyn SerialPort> 真正释放（端口占用解除）
    read_thread: Option<JoinHandle<()>>,
    // 当前波特率：写超时按“字节数 / 波特率”估算
    baud: u32,
}

/*
*   描述：用于管理多个串口状态的哈希表
*/
pub struct SerialState {
    // 已打开端口 -> 句柄
    pub(crate) ports: HashMap<String, PortHandle>,
    // 自动发送任务表：port -> 停止标志（Rust 线程直接循环写，绕过前端 IPC）
    pub(crate) auto_sends: HashMap<String, Arc<AtomicBool>>,
    // 正在打开中的端口（占位）：防止并发 serial_open 造成双开/句柄互相覆盖
    pub(crate) opening: HashSet<String>,
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
            opening: HashSet::new(),
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
*   返回：读线程 JoinHandle，关闭时 join 以释放串口句柄
*/
fn serial_read_thread(
    port_name: String,
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut rbuf = [0u8; 4096];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            // 先查有没有数据可用（非阻塞，不占锁太久）
            let rxbufsize = {
                let sp = match port.lock() {
                    Ok(sp) => sp,
                    Err(_) => break,
                };
                match sp.bytes_to_read() {
                    Ok(n) => n as usize,
                    Err(_) => {
                        let _ = app_handle.emit("serial-disconnect", &port_name);
                        break;
                    }
                }
            }; // 锁在此释放

            if rxbufsize == 0 {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }

            // 有数据才锁住读取
            let mut sp = match port.lock() {
                Ok(sp) => sp,
                Err(_) => break,
            };
            let n = rxbufsize.min(rbuf.len());
            match sp.read(&mut rbuf[..n]) {
                Ok(read) => {
                    drop(sp); // 先放锁再 emit，避免事件派发期间占着串口
                    let _ = app_handle.emit(
                        "serial-read_data",
                        SerialDataEvent {
                            port: port_name.clone(),
                            data: rbuf[..read].to_vec(),
                        },
                    );
                }
                // 读写共用同一 timeout：偶发超时只是本轮没读到数据，继续轮询而不是误判断线
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                    ) =>
                {
                    drop(sp);
                    continue;
                }
                Err(_) => {
                    drop(sp);
                    let _ = app_handle.emit("serial-disconnect", &port_name);
                    break;
                }
            }
        }
    })
}

/*
*   描述：高精度等待：Windows 的 thread::sleep 粒度约 15.6ms，
*         自动发送的短间隔（如 10ms）误差会高达 50%+。
*         策略：先粗睡 interval-2ms，剩余 ≤2ms 自旋到点（自旋上限固定 2ms，
*         避免短间隔时 20% 单核空转）。
*/
pub(crate) fn wait_precisely(interval: std::time::Duration) {
    const SPIN_LIMIT: std::time::Duration = std::time::Duration::from_millis(2);
    let start = std::time::Instant::now();
    let coarse = interval.saturating_sub(SPIN_LIMIT);
    if coarse > std::time::Duration::ZERO {
        thread::sleep(coarse);
    }
    while start.elapsed() < interval {
        std::hint::spin_loop();
    }
}

/*
*   描述：按波特率与字节数估算写超时（8N1 每字节 10 bit，留 50ms 余量）。
*         最小 100ms、上限 5s：低波特率发长包时不至于被 100ms 误判超时，
*         同时避免驱动异常时把写线程锁死过久。
*/
fn write_timeout_for(baud: u32, len: usize) -> std::time::Duration {
    let baud = baud.max(1) as u64;
    let tx_ms = (len as u64).saturating_mul(10_000) / baud;
    std::time::Duration::from_millis((tx_ms + 50).clamp(100, 5_000))
}

/*
*   描述：串口写入的公共实现（发送命令与自动发送线程共用）
*   说明：调用方必须在锁外持有 port Arc（不能持串口状态表锁）
*/
fn serial_write_bytes(
    port: &Arc<Mutex<Box<dyn SerialPort>>>,
    baud: u32,
    bytes: &[u8],
) -> Result<(), String> {
    let mut sp = port
        .lock()
        .map_err(|_| "发送失败，串口已断开".to_string())?;
    sp.set_timeout(write_timeout_for(baud, bytes.len()))
        .map_err(|_| "发送失败，串口已断开".to_string())?;
    let result = sp.write_all(bytes);
    // 读线程依赖 100ms 超时轮询，写完必须恢复，避免长超时拖慢读线程对停止标志的响应
    let _ = sp.set_timeout(std::time::Duration::from_millis(IO_TIMEOUT_MS));
    match result {
        Ok(()) => Ok(()),
        Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
            Err("发送超时，请检查波特率/线缆".to_string())
        }
        Err(_) => Err("发送失败，串口已断开".to_string()),
    }
}

/*
*   描述：按参数打开串口（不持任何锁；驱动调用可能阻塞）
*/
fn open_serial_port(
    port: &str,
    baud_rate: u32,
    data_bits: u8,
    parity_bits: &str,
    stop_bits: u8,
) -> Result<Box<dyn SerialPort>, String> {
    serialport::new(port, baud_rate)
        .timeout(std::time::Duration::from_millis(IO_TIMEOUT_MS))
        .data_bits(match data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        })
        .parity(match parity_bits {
            "Odd" => Parity::Odd,
            "Even" => Parity::Even,
            _ => Parity::None,
        })
        .stop_bits(match stop_bits {
            2 => StopBits::Two,
            _ => StopBits::One,
        })
        .open()
        .map_err(|e| format!("打开串口失败: {}", e))
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
*   说明：async 命令 —— serialport::open 与驱动交互可能阻塞，移入线程池执行。
*         打开流程四步：加锁查重+占位 opening → 放锁打开 → 加锁登记句柄 → 启动读线程，
*         并发的两次打开只会有一次成功，且读线程启动时句柄一定已在状态表中。
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
        // ① 加锁查重并占位：锁只用来登记，查完立刻放
        {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            if guard.ports.contains_key(&port) || guard.opening.contains(&port) {
                return Err(format!("串口 {} 已打开", port));
            }
            guard.opening.insert(port.clone());
        }

        // ② 打开串口（不持锁，卡多久都不影响其他命令）
        let sp = match open_serial_port(&port, baud_rate, data_bits, &parity_bits, stop_bits) {
            Ok(sp) => sp,
            Err(e) => {
                // 失败必须清理占位，否则该端口会永久卡在“正在打开”
                if let Ok(mut guard) = state.lock() {
                    guard.opening.remove(&port);
                }
                return Err(e);
            }
        };
        let port_arc = Arc::new(Mutex::new(sp));
        let stop_arc = Arc::new(AtomicBool::new(false));

        // ③ 先登记句柄（读线程未启动），再启动读线程
        {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            guard.opening.remove(&port);
            guard.ports.insert(
                port.clone(),
                PortHandle {
                    port: port_arc.clone(),
                    stop_flag: stop_arc.clone(),
                    read_thread: None,
                    baud: baud_rate,
                },
            );
        }

        // ④ 启动后台读线程
        let read_thread = serial_read_thread(port.clone(), port_arc, stop_arc.clone(), app_handle);
        // 若端口在插入后、线程登记前被关闭：句柄已移出状态表，置停止标志让线程自然退出
        match state.lock() {
            Ok(mut guard) => match guard.ports.get_mut(&port) {
                Some(handle) => handle.read_thread = Some(read_thread),
                None => stop_arc.store(true, Ordering::SeqCst),
            },
            Err(_) => stop_arc.store(true, Ordering::SeqCst),
        }

        Ok(format!("串口 {} 已打开", port))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：关闭指定串口（async：close 涉及驱动交互）
*   说明：锁内只摘除句柄与停止标志，锁外 join 读线程，保证串口句柄真正释放
*/
#[tauri::command]
pub async fn serial_close(
    port: String,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (handle, auto_send) = {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            let handle = guard
                .ports
                .remove(&port)
                .ok_or(format!("串口 {} 未打开", port))?;
            // 连接关闭时同步停止该端口的自动发送任务
            (handle, guard.auto_sends.remove(&port))
        };

        if let Some(task) = auto_send {
            task.store(true, Ordering::SeqCst);
        }
        handle.stop_flag.store(true, Ordering::SeqCst);
        // join 读线程：其 5ms 轮询/100ms 读超时会很快退出；join 后 Arc 归零，端口真正释放
        if let Some(read_thread) = handle.read_thread {
            let _ = read_thread.join();
        }
        Ok(format!("串口 {} 已关闭", port))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：同步版全量关闭（供应用退出事件调用——退出路径不能走异步任务）
*/
pub fn close_all_sync(state: &SharedState) {
    let mut handles: Vec<PortHandle> = Vec::new();
    let mut auto_sends: Vec<Arc<AtomicBool>> = Vec::new();
    if let Ok(mut s) = state.lock() {
        auto_sends.extend(s.auto_sends.drain().map(|(_, task)| task));
        handles.extend(s.ports.drain().map(|(_, handle)| handle));
    }
    // 锁已释放：先停自动发送，再置停止标志并等读线程退出
    for task in auto_sends {
        task.store(true, Ordering::SeqCst);
    }
    for handle in handles {
        handle.stop_flag.store(true, Ordering::SeqCst);
        if let Some(read_thread) = handle.read_thread {
            let _ = read_thread.join();
        }
    }
}

/*
*   描述：关闭全部串口连接并停止全部自动发送任务
*         （前端页面重载/HMR 后会话列表清空，后端残留的串口句柄若不释放会一直占用端口）
*   调用方法：前端初始化（initApp）时调用一次，保证后端状态与界面一致
*/
#[tauri::command]
pub async fn serial_close_all(state: tauri::State<'_, SharedState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = state.lock().map_err(|e| e.to_string())?.ports.len();
        close_all_sync(&state);
        Ok(count)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：发送数据到串口（async：WriteFile 在驱动层可能阻塞数十毫秒，这是“发送卡一下”的元凶）
*   说明：锁内只 clone 串口 Arc，放锁后再做驱动写，避免写期间阻塞其他命令
*/
#[tauri::command]
pub async fn serial_write(
    port: String,
    data: String,
    bytes: Option<Vec<u8>>,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 锁内只取写目标（Arc 克隆与波特率），驱动态写操作在锁外完成
        let (port_arc, baud) = {
            let guard = state.lock().map_err(|e| e.to_string())?;
            let handle = guard
                .ports
                .get(&port)
                .ok_or(format!("串口 {} 未打开", port))?;
            (handle.port.clone(), handle.baud)
        };
        // 二进制模式优先使用原始字节
        let payload = bytes.unwrap_or_else(|| data.as_bytes().to_vec());
        serial_write_bytes(&port_arc, baud, &payload)?;
        Ok(format!("已发送 {} 字节", payload.len()))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：动态修改已打开串口的波特率（async：底层为驱动调用）
*   说明：改完同步更新句柄里的 baud，后续写超时按新波特率估算
*/
#[tauri::command]
pub async fn serial_set_baudrate(
    port: String,
    baud_rate: u32,
    state: tauri::State<'_, SharedState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 锁内只取串口 Arc（驱动调用可能阻塞，不能持全局状态锁做 IO）
        let port_arc = {
            let guard = state.lock().map_err(|e| e.to_string())?;
            guard
                .ports
                .get(&port)
                .ok_or(format!("串口 {} 未打开", port))?
                .port
                .clone()
        };
        {
            let mut sp = port_arc.lock().map_err(|e| e.to_string())?;
            sp.set_baud_rate(baud_rate)
                .map_err(|e| format!("修改波特率失败: {}", e))?;
        }
        // 驱动修改成功后再更新写超时估算用的 baud；期间若端口被关闭则忽略
        if let Ok(mut guard) = state.lock() {
            if let Some(handle) = guard.ports.get_mut(&port) {
                handle.baud = baud_rate;
            }
        }
        Ok(format!("串口 {} 波特率已改为 {}", port, baud_rate))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：默认日志目录：优先应用日志目录（app_log_dir），失败回退 exe 所在目录下的 log。
*         与启动时的工作目录无关，避免 cwd 漂移。
*/
fn default_log_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    if let Ok(dir) = app.path().app_log_dir() {
        return Ok(dir);
    }
    std::env::current_exe()
        .map_err(|e| format!("定位程序目录失败: {}", e))?
        .parent()
        .map(|p| p.join("log"))
        .ok_or_else(|| "定位程序目录失败".to_string())
}

/*
*   描述：追加写入日志文件（async：文件 IO 移入线程池）
*   传参：app：应用句柄（取应用日志目录，Tauri 自动注入）
*         dir：导出目录（用户在设置里配置；空/缺省 = 应用日志目录，取不到回退 exe/log）
*         filename：文件名由前端按"端口号/IP_时间"生成，后端再过滤一次非法字符防路径逃逸
*   返回：实际写入的完整路径（前端状态栏展示用）
*/
#[tauri::command]
pub async fn serial_log_write(
    app: AppHandle,
    dir: Option<String>,
    filename: String,
    text: String,
) -> Result<String, String> {
    use std::fs::OpenOptions;

    tauri::async_runtime::spawn_blocking(move || {
        let log_dir = match dir {
            Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d.trim()),
            _ => default_log_dir(&app)?,
        };
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;

        // Windows 文件名非法字符兜底替换（正常情况下前端已保证合法）
        let safe_name: String = filename
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect();

        let path = log_dir.join(safe_name);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("打开日志文件失败: {}", e))?;

        file.write_all(text.as_bytes())
            .map_err(|e| format!("写入日志失败: {}", e))?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：查询默认日志导出目录（应用日志目录，取不到回退 exe 下的 log），设置界面展示占位提示用
*/
#[tauri::command]
pub async fn serial_log_dir(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(default_log_dir(&app)?.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：启动自动发送（Rust 线程按间隔直接写串口，绕过前端 IPC，根治高频发送卡顿）
*   传参：port：端口号, data：待发送文本, bytes：二进制原始字节（优先于 data）,
*         interval_ms：间隔（1ms ~ 1 小时）
*/
#[tauri::command]
pub async fn serial_auto_send_start(
    port: String,
    data: String,
    bytes: Option<Vec<u8>>,
    interval_ms: u64,
    state: tauri::State<'_, SharedState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 二进制模式优先使用原始字节，与 serial_write 保持一致
        let payload = bytes.unwrap_or_else(|| data.into_bytes());
        let interval_ms = interval_ms.clamp(1, 3_600_000);
        let stop = Arc::new(AtomicBool::new(false));

        // 锁内只登记停止标志并确认端口已打开，写操作在每轮循环里取到 Arc 后放锁执行
        {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            // 已有任务先停
            if let Some(old) = guard.auto_sends.remove(&port) {
                old.store(true, Ordering::SeqCst);
            }
            if !guard.ports.contains_key(&port) {
                return Err(format!("串口 {} 未打开", port));
            }
            guard.auto_sends.insert(port.clone(), stop.clone());
        }

        // 独立线程循环发送，直到停止标志置位或串口失效（高精度等待）
        let interval = std::time::Duration::from_millis(interval_ms);
        thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            wait_precisely(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }

            // 锁内只 clone 写目标，驱动写放到锁外
            let target = match state.lock() {
                Ok(guard) => guard
                    .ports
                    .get(&port)
                    .map(|handle| (handle.port.clone(), handle.baud)),
                Err(_) => None,
            };
            let Some((port_arc, baud)) = target else {
                break;
            };

            if let Err(e) = serial_write_bytes(&port_arc, baud, &payload) {
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
                    data: payload.clone(),
                },
            );
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
pub async fn serial_auto_send_stop(
    port: String,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
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
