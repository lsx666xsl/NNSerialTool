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
}

/*
*   描述：全局共享哈希表
*/
pub type SharedState = Mutex<SerialState>;

/*
*   描述：默认生成的哈希表
*/
impl Default for SerialState {
    fn default() -> Self {
        Self {
            ports: HashMap::new(),
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
*   描述：用结构体保存端口和端口信息
*/
#[derive(Serialize, Debug)]
pub struct SerialPortInfo {
    portnum: String,
    portproduct: String,
}

/// 读取串口数据
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
*   描述：列出系统上所有可用的串口
*   用法：前端调用: invoke("serial_list_ports")
*   传参：无
*   返回: 一个结构体包含端口号和端口产品信息
*/
#[tauri::command]
pub fn serial_list_ports() -> Vec<SerialPortInfo> {
    //调用系统的api获取可用串口
    let port = match serialport::available_ports() {
        Ok(p) => p,
        Err(e) => {
            println!("获取串口列表失败: {}", e);
            return vec![];
        }
    };

    //创建一个结构体列表
    let mut port_list: Vec<SerialPortInfo> = Vec::new();

    //打印出所有串口
    for p in port {
        port_list.push(SerialPortInfo {
            portnum: p.port_name,
            portproduct: match p.port_type {
                serialport::SerialPortType::UsbPort(info) => info
                    .product //获取系统
                    .unwrap_or_else(|| "未知设备".into())
                    .split(" (")
                    .next()
                    .unwrap_or_else(|| "未知设备".into())
                    .to_string(),
                _ => "未知设备".to_string(),
            },
        });
    }

    // println!("{:#?}", port_list);

    port_list
}

/*
*   描述：设置串口波特率
*   调用方法：前端调用: invoke("serial_set_baudrate")
*   传参：无
*   返回: 一个结构体包含端口号和端口产品信息
*/
#[tauri::command]
pub fn serial_baudrate_list() -> Vec<u32> {
    vec![
        1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200, 128000, 230400, 256000, 460800,
        512000, 750000, 921600, 1000000, 1500000, 2000000,
    ]
}

/*
*   描述：设置串口数据位
*   调用方法：前端调用: invoke("serial_databit_list")
*   传参：无
*   返回: u8类型的数据
*/
#[tauri::command]
pub fn serial_databit_list() -> Vec<u8> {
    vec![6, 7, 8]
}

/*
*   描述：设置串口校验位
*   调用方法：前端调用: invoke("serial_paritybit_list")
*   传参：无
*   返回: String类型数据
*/
#[tauri::command]
pub fn serial_paritybit_list() -> Vec<String> {
    vec!["None".to_string(), "Even".to_string(), "Odd".to_string()]
}

/*
*   描述：设置串口停止位
*   调用方法：前端调用: invoke("serial_stopbit_list")
*   传参：无
*   返回: 一个结构体StopBitOption包含停止位字符和数值
*/
#[tauri::command]
pub fn serial_stopbit_list() -> Vec<u8> {
    vec![1, 2]
}

/*
*   描述：打开指定的串口
*   调用方法：前端调用: invoke("serial_open")
*   传参：port：端口号, baud_rate：波特率, data_bits: 数据位, parity_bits: 校验位, stop_bits: 停止位, state: 互斥锁
*   返回: 错误状态
*/
#[tauri::command]
pub fn serial_open(
    port: String,
    baud_rate: u32,
    data_bits: u8,
    parity_bits: String,
    stop_bits: u8,
    state: tauri::State<SharedState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    // ① 快查：锁只用来检查是否已打开，查完立刻放
    {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.ports.contains_key(&port) {
            return Err(format!("串口 {} 已打开", port));
        }
    } // 锁在此释放

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
}

/// 关闭指定串口
#[tauri::command]
pub fn serial_close(port: String, state: tauri::State<SharedState>) -> Result<String, String> {
    let mut state = state.lock().map_err(|e| e.to_string())?;
    match state.ports.remove(&port) {
        Some(handle) => {
            handle.stop_flag.store(true, Ordering::SeqCst);
            Ok(format!("串口 {} 已关闭", port))
        }
        None => Err(format!("串口 {} 未打开", port)),
    }
}

/// 查询串口是否打开（纯查询，清理由 serial_list_ports 负责）
#[tauri::command]
pub fn serial_is_open(port: String, state: tauri::State<SharedState>) -> bool {
    let state = state.lock().unwrap();
    state.ports.contains_key(&port)
}

/// 发送数据到串口
#[tauri::command]
pub fn serial_write(
    port: String,
    data: String,
    state: tauri::State<SharedState>,
) -> Result<String, String> {
    //获取哈希键值对的锁
    let guard = state.lock().map_err(|e| e.to_string())?;

    //查看键值对里的指定端口是否存在
    let handle = guard
        .ports
        .get(&port)
        .ok_or(format!("串口 {} 未打开", port))?;

    //如果存在则打开串口读写锁，获取这个操作句柄
    let mut sp = handle.port.lock().map_err(|e| e.to_string())?;

    //将数据写入串口,失败输出log,成功则跳过
    sp.write_all(data.as_bytes())
        .map_err(|_| format!("发送失败，串口 {} 已断开", port))?;

    //
    Ok(format!("已发送 {} 字节", data.len()))
}

/// 动态修改已打开串口的波特率（即时生效）
#[tauri::command]
pub fn serial_set_baudrate(
    port: String,
    baud_rate: u32,
    state: tauri::State<SharedState>,
) -> Result<String, String> {
    // ① 锁 HashMap，查找端口句柄
    let guard = state.lock().map_err(|e| e.to_string())?;
    let handle = guard
        .ports
        .get(&port)
        .ok_or(format!("串口 {} 未打开", port))?;

    // ② 锁这个端口，调用硬件 API 修改波特率
    let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
    sp.set_baud_rate(baud_rate)
        .map_err(|e| format!("修改波特率失败: {}", e))?;

    Ok(format!("串口 {} 波特率已改为 {}", port, baud_rate))
}

/// 动态修改数据位
#[tauri::command]
pub fn serial_set_databits(
    port: String,
    data_bits: u8,
    state: tauri::State<SharedState>,
) -> Result<String, String> {
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
}

/// 动态修改校验位
#[tauri::command]
pub fn serial_set_parity(
    port: String,
    parity: String,
    state: tauri::State<SharedState>,
) -> Result<String, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    let handle = guard.ports.get(&port).ok_or(format!("串口 {} 未打开", port))?;
    let mut sp = handle.port.lock().map_err(|e| e.to_string())?;
    let p = match parity.as_str() {
        "Odd" => Parity::Odd,
        "Even" => Parity::Even,
        _ => Parity::None,
    };
    sp.set_parity(p)
        .map_err(|e| format!("修改校验位失败: {}", e))?;
    Ok(format!("串口 {} 校验位已改为 {}", port, parity))
}

/// 动态修改停止位
#[tauri::command]
pub fn serial_set_stopbits(
    port: String,
    stop_bits: u8,
    state: tauri::State<SharedState>,
) -> Result<String, String> {
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
}
