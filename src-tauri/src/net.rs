use serde::Serialize;
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/*
*   描述：网络读线程的轮询超时时间，兼顾断开响应速度与 CPU 占用
*/
const POLL_INTERVAL_MS: u64 = 100;

/*
*   描述：网络连接类型
*/
#[derive(Clone, Copy, PartialEq)]
pub enum NetKind {
    TcpClient,
    TcpServer,
    Udp,
}

impl NetKind {
    // 从前端传入的字符串解析连接类型
    fn parse(s: &str) -> Option<NetKind> {
        match s {
            "tcp_client" => Some(NetKind::TcpClient),
            "tcp_server" => Some(NetKind::TcpServer),
            "udp" => Some(NetKind::Udp),
            _ => None,
        }
    }
}

/*
*   描述：TCP 服务端已接入的一条客户端连接
*/
struct TcpClientConn {
    addr: SocketAddr,
    stream: Arc<Mutex<TcpStream>>,
}

/*
*   描述：网络连接句柄，三种网络形态统一保存
*/
pub struct NetHandle {
    kind: NetKind,
    label: String,
    stop_flag: Arc<AtomicBool>,
    stream: Option<Arc<Mutex<TcpStream>>>,           // TCP 客户端使用
    clients: Option<Arc<Mutex<Vec<TcpClientConn>>>>, // TCP 服务端使用
    socket: Option<Arc<Mutex<UdpSocket>>>,           // UDP 使用
}

/*
*   描述：全局网络连接哈希表（key = 前端会话 id）
*/
pub struct NetState {
    pub conns: HashMap<String, NetHandle>,
    // 自动发送任务表：key -> 停止标志（Rust 线程直接循环写，绕过前端 IPC）
    pub auto_sends: HashMap<String, Arc<AtomicBool>>,
}

/*
*   描述：全局共享网络状态
*/
pub type SharedNetState = Arc<Mutex<NetState>>;

/*
*   描述：默认生成的网络哈希表
*/
impl Default for NetState {
    fn default() -> Self {
        Self {
            conns: HashMap::new(),
            auto_sends: HashMap::new(),
        }
    }
}

/*
*   描述：网络收发数据事件，from 为来源地址（无来源信息时为空字符串）
*/
#[derive(Serialize, Clone)]
struct NetDataEvent {
    key: String,
    data: Vec<u8>,
    from: String,
}

/*
*   描述：网络写入的公共实现（发送命令与自动发送线程共用）
*/
fn net_write_bytes(handle: &NetHandle, bytes: &[u8]) -> Result<(), String> {
    match handle.kind {
        NetKind::TcpClient => {
            let stream = handle.stream.as_ref().ok_or("句柄异常: 缺少 TCP 流")?;
            let mut sp = stream.lock().map_err(|e| e.to_string())?;
            sp.write_all(bytes)
                .map_err(|_| format!("发送失败，连接 {} 已断开", handle.label))?;
            Ok(())
        }
        NetKind::TcpServer => {
            let clients = handle.clients.as_ref().ok_or("句柄异常: 缺少客户端列表")?;
            let mut list = clients.lock().map_err(|e| e.to_string())?;
            let mut sent = 0usize;
            list.retain(|conn| {
                let ok = match conn.stream.lock() {
                    Ok(mut sp) => sp.write_all(bytes).is_ok(),
                    Err(_) => false,
                };
                if ok {
                    sent += 1;
                }
                ok
            });
            Ok(())
        }
        NetKind::Udp => {
            let socket = handle.socket.as_ref().ok_or("句柄异常: 缺少 UDP 套接字")?;
            let sp = socket.lock().map_err(|e| e.to_string())?;
            // Windows：目标无监听时 send 报 ConnectionReset（ICMP 回报），忽略继续尝试
            match sp.send(bytes) {
                Ok(_n) => Ok(()),
                Err(e) if e.kind() == ErrorKind::ConnectionReset => Ok(()),
                Err(_) => Err(format!("发送失败，请检查 {} 的目标地址设置", handle.label)),
            }
        }
    }
}

/*
*   描述：自动发送通知事件（前端据此记录 TX）
*/
#[derive(Serialize, Clone)]
struct AutoSentEvent {
    key: String,
    data: Vec<u8>,
}

/*
*   描述：判断 IO 错误是否为轮询超时（非致命，继续循环即可）
*/
fn is_poll_timeout(e: &std::io::Error) -> bool {
    e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut
}

/*
*   描述：网络数据统一上报给前端
*/
fn emit_net_data(app_handle: &AppHandle, key: &str, data: &[u8], from: &str) {
    let _ = app_handle.emit(
        "net-read_data",
        NetDataEvent {
            key: key.to_string(),
            data: data.to_vec(),
            from: from.to_string(),
        },
    );
}

/*
*   描述：连接死亡时的处理。独立客户端会上报 net-disconnect 让前端标记断开；
*         服务端子连接只从客户端列表剔除，不影响整个会话。
*/
fn on_conn_dead(
    key: &str,
    addr: SocketAddr,
    clients: &Option<Arc<Mutex<Vec<TcpClientConn>>>>,
    app_handle: &AppHandle,
) {
    match clients {
        Some(list) => list.lock().unwrap().retain(|c| c.addr != addr),
        None => {
            let _ = app_handle.emit("net-disconnect", key);
        }
    }
}

/*
*   描述：TCP 读线程，独立客户端与服务端子连接共用。
*         100ms 读超时轮询， stop_flag 置位后退出。
*/
fn tcp_read_thread(
    key: String,
    addr: SocketAddr,
    mut rd: TcpStream,
    clients: Option<Arc<Mutex<Vec<TcpClientConn>>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    // 阻塞式读：数据到达内核即返回（接收延迟 ~0），不再 100ms 轮询。
    // 读线程独占 clone 句柄，与写路径的互斥锁完全分离；停止/关闭由 shutdown(Both) 唤醒。
    thread::spawn(move || {
        let mut rbuf = [0u8; 4096];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match rd.read(&mut rbuf) {
                Ok(0) => {
                    // 对端正常关闭
                    on_conn_dead(&key, addr, &clients, &app_handle);
                    break;
                }
                Ok(n) => {
                    emit_net_data(&app_handle, &key, &rbuf[..n], &addr.to_string());
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    // 被信号中断，重试
                    continue;
                }
                Err(_) => {
                    on_conn_dead(&key, addr, &clients, &app_handle);
                    break;
                }
            }
        }
    });
}

/*
*   描述：TCP 服务端接受连接线程，每接入一个客户端就派生一个独立读线程
*/
fn tcp_accept_thread(
    key: String,
    listener: TcpListener,
    clients: Arc<Mutex<Vec<TcpClientConn>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    thread::spawn(move || {
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match listener.accept() {
                Ok((stream, addr)) => {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));
                    // 读改阻塞模式（无 read timeout），shutdown 唤醒；Nagle 禁用小包立即发
                    let _ = stream.set_nodelay(true);
                    let shared = Arc::new(Mutex::new(stream));
                    clients.lock().unwrap().push(TcpClientConn {
                        addr,
                        stream: shared.clone(),
                    });

                    // 每个客户端一个独立读线程（独占 clone 句柄，与写锁分离）
                    let rd_result = shared.lock().unwrap().try_clone();
                    if let Ok(rd) = rd_result {
                        tcp_read_thread(
                            key.clone(),
                            addr,
                            rd,
                            Some(clients.clone()),
                            stop_flag.clone(),
                            app_handle.clone(),
                        );
                    }
                }
                Err(_) => {
                    // 非阻塞模式没有新连接，稍作等待避免空转
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
    });
}

/*
*   描述：UDP 读线程，recv_from 轮询并把来源地址一并上报
*/
fn udp_read_thread(
    key: String,
    socket: Arc<Mutex<UdpSocket>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    thread::spawn(move || {
        let mut rbuf = [0u8; 4096];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let sp = socket.lock().unwrap();
            match sp.recv_from(&mut rbuf) {                Ok((n, addr)) => {
                    let from = addr.to_string();
                    drop(sp);
                    emit_net_data(&app_handle, &key, &rbuf[..n], &from);
                }
                Err(e) => {
                    // Windows UDP：向无监听目标发送后内核回报 ICMP 不可达，recv_from 持续报 ConnectionReset——正常现象而非断线
                    let is_reset = e.kind() == ErrorKind::ConnectionReset;
                    let dead = !is_poll_timeout(&e) && !is_reset;
                    drop(sp);
                    if dead {
                        let _ = app_handle.emit("net-disconnect", &key);
                        break;
                    }
                }
            }
        }
    });
}

/*
*   描述：枚举本机所有网卡的 IPv4 地址（不含回环），供前端地址下拉快速选择
*   调用方法：前端调用: invoke("net_local_ips")
*   传参：无
*   返回: 本机 IPv4 地址列表（如 192.168.1.100）
*/
#[tauri::command]
pub async fn net_local_ips() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
    let mut ips: Vec<String> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) => Some(v4.ip.to_string()),
            _ => None,
        })
        .filter(|ip| !ip.starts_with("127."))
        .collect();
    ips.sort();
    ips.dedup();
    ips
    })
    .await
    .unwrap_or_default()
}

/*
*   描述：打开网络连接（TCP 客户端 / TCP 服务端 / UDP）
*   调用方法：前端调用: invoke("net_open")
*   传参：key：会话唯一标识, kind：tcp_client/tcp_server/udp,
*         host：目标地址(服务端为监听地址, 可空默认 0.0.0.0),
*         port：目标端口(服务端为监听端口),
*         local_port：UDP 本地绑定端口(0 表示自动分配),
*         local_host：UDP 本地绑定地址(可空默认 0.0.0.0 监听所有网卡)
*   返回: 错误状态
*/
#[tauri::command]
pub async fn net_open(
    key: String,
    kind: String,
    host: String,
    port: u16,
    local_port: u16,
    local_host: Option<String>,
    state: tauri::State<'_, SharedNetState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let kind = NetKind::parse(&kind).ok_or_else(|| format!("未知网络类型: {}", kind))?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {

    // ① 快查：锁只用来检查是否已打开，查完立刻放
    {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.conns.contains_key(&key) {
            return Err(format!("连接已打开: {}", key));
        }
    }

    // ② 建立 socket（不持锁，卡多久都不影响其他命令）
    let label: String;
    let handle = match kind {
        NetKind::TcpClient => {
            let target = host.trim().to_string();
            if target.is_empty() {
                return Err("目标地址不能为空".into());
            }
            let addr = (target.as_str(), port)
                .to_socket_addrs()
                .map_err(|e| format!("地址解析失败: {}", e))?
                .next()
                .ok_or("地址解析失败")?;
            label = format!("TCP {}", addr);

            let stream = TcpStream::connect(addr)
                .map_err(|e| format!("连接 {} 失败: {}", addr, e))?;
            // 读改阻塞模式：数据到达内核即返回（接收延迟 ~0），停止/关闭由 shutdown 唤醒
            // 写超时防止慢消费者拖死写路径
            stream
                .set_write_timeout(Some(Duration::from_millis(1000)))
                .map_err(|e| format!("设置写入超时失败: {}", e))?;
            // 禁用 Nagle：调试工具的小包必须立即发出，避免 200ms+ 的合并延迟
            stream
                .set_nodelay(true)
                .map_err(|e| format!("设置 TCP_NODELAY 失败: {}", e))?;
            let stream = Arc::new(Mutex::new(stream));
            let stop_flag = Arc::new(AtomicBool::new(false));

            // ③ 启动后台读线程（独占 try_clone 句柄，与写路径的锁完全分离）
            let rd = stream
                .lock()
                .map_err(|e| e.to_string())?
                .try_clone()
                .map_err(|e| format!("克隆读句柄失败: {}", e))?;
            tcp_read_thread(key.clone(), addr, rd, None, stop_flag.clone(), app_handle);

            NetHandle {
                kind,
                label: label.clone(),
                stop_flag,
                stream: Some(stream),
                clients: None,
                socket: None,
            }
        }
        NetKind::TcpServer => {
            let bind_host = if host.trim().is_empty() {
                "0.0.0.0".to_string()
            } else {
                host.trim().to_string()
            };
            let bind_addr = format!("{}:{}", bind_host, port);
            label = format!("TCP服务 {}", bind_addr);

            let listener = TcpListener::bind(&bind_addr)
                .map_err(|e| format!("监听 {} 失败: {}", bind_addr, e))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| format!("设置非阻塞失败: {}", e))?;
            let clients = Arc::new(Mutex::new(Vec::new()));
            let stop_flag = Arc::new(AtomicBool::new(false));

            tcp_accept_thread(key.clone(), listener, clients.clone(), stop_flag.clone(), app_handle);

            NetHandle {
                kind,
                label: label.clone(),
                stop_flag,
                stream: None,
                clients: Some(clients),
                socket: None,
            }
        }
        NetKind::Udp => {
            let bind_port = if local_port > 0 { local_port } else { port };
            let bind_host = local_host
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("0.0.0.0")
                .to_string();
            let bind_addr = format!("{}:{}", bind_host, bind_port);
            label = format!("UDP {}:{}", bind_host, bind_port);

            let socket = UdpSocket::bind(&bind_addr)
                .map_err(|e| format!("绑定 {} 失败: {}", bind_addr, e))?;
            socket
                .set_read_timeout(Some(Duration::from_millis(POLL_INTERVAL_MS)))
                .map_err(|e| format!("设置读取超时失败: {}", e))?;

            // 目标地址非空则固定默认发送目标（connect 后只收发该对端，贴近调试工具用法）
            let target = host.trim().to_string();
            if !target.is_empty() && port > 0 {
                let peer = format!("{}:{}", target, port);
                socket
                    .connect(&peer)
                    .map_err(|e| format!("设置目标 {} 失败: {}", peer, e))?;
            }
            let socket = Arc::new(Mutex::new(socket));
            let stop_flag = Arc::new(AtomicBool::new(false));

            // 读线程使用 try_clone 的独立套接字：recv_from 阻塞等待时不持有共享锁，
            // 避免自动发送/手动发送的写路径被读轮询阻塞（间隔抖动的根因）
            let read_sock = socket.lock().unwrap().try_clone()
                .map_err(|e| format!("克隆 UDP 套接字失败: {}", e))?;
            udp_read_thread(
                key.clone(),
                Arc::new(Mutex::new(read_sock)),
                stop_flag.clone(),
                app_handle,
            );

            NetHandle {
                kind,
                label: label.clone(),
                stop_flag,
                stream: None,
                clients: None,
                socket: Some(socket),
            }
        }
    };

        // ④ 再次锁住存入（这一瞬间的事）
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.conns.insert(key, handle);

        Ok(format!("连接 {} 已打开", label))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：关闭指定网络连接（读线程随 stop_flag 退出，socket 随引用归零自动关闭）
*   调用方法：前端调用: invoke("net_close")
*   传参：key：会话唯一标识
*   返回: 错误状态
*/
#[tauri::command]
pub async fn net_close(key: String, state: tauri::State<'_, SharedNetState>) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        match state.conns.remove(&key) {
            Some(handle) => {
                handle.stop_flag.store(true, Ordering::SeqCst);
                Ok(format!("连接 {} 已关闭", handle.label))
            }
            None => Err(format!("连接未打开: {}", key)),
        }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：同步版全量关闭（供应用退出事件调用——退出路径不能走异步任务）
*/
pub fn close_all_sync(state: &SharedNetState) {
    if let Ok(mut s) = state.lock() {
        for (_, handle) in s.conns.drain() {
            handle.stop_flag.store(true, Ordering::SeqCst);
        }
    }
}

/*
*   描述：关闭全部网络连接（前端页面重载/HMR 后会话列表清空，后端残留的监听/连接
*         若不释放会一直占用端口，导致"明明没使用却绑定失败 os error 10048"）
*   调用方法：前端初始化（initApp）时调用一次，保证后端状态与界面一致
*/
#[tauri::command]
pub async fn net_close_all(state: tauri::State<'_, SharedNetState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = state.lock().map_err(|e| e.to_string())?.conns.len();
        close_all_sync(&state);
        Ok(count)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：通过网络连接发送数据。TCP 服务端会向所有已接入客户端广播。
*   调用方法：前端调用: invoke("net_write")
*   传参：key：会话唯一标识, data：待发送文本
*   返回: 错误状态
*/
#[tauri::command]
pub async fn net_write(
    key: String,
    data: String,
    bytes: Option<Vec<u8>>,
    state: tauri::State<'_, SharedNetState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
    let guard = state.lock().map_err(|e| e.to_string())?;
    let handle = guard.conns.get(&key).ok_or(format!("连接未打开: {}", key))?;
    // 二进制模式优先使用原始字节（十六进制发送），否则按 UTF-8 字符串
    let payload = bytes.unwrap_or_else(|| data.as_bytes().to_vec());
    let bytes = payload.as_slice();

    match handle.kind {
        NetKind::TcpClient => {
            let stream = handle.stream.as_ref().ok_or("句柄异常: 缺少 TCP 流")?;
            let mut sp = stream.lock().map_err(|e| e.to_string())?;
            sp.write_all(bytes)
                .map_err(|_| format!("发送失败，连接 {} 已断开", handle.label))?;
            Ok(format!("已发送 {} 字节", bytes.len()))
        }
        NetKind::TcpServer => {
            let clients = handle.clients.as_ref().ok_or("句柄异常: 缺少客户端列表")?;
            let mut list = clients.lock().map_err(|e| e.to_string())?;
            if list.is_empty() {
                return Err(format!("{} 暂无客户端接入", handle.label));
            }
            // 逐个客户端写入，写失败的客户端从列表剔除
            let mut sent = 0usize;
            list.retain(|conn| {
                let ok = match conn.stream.lock() {
                    Ok(mut sp) => sp.write_all(bytes).is_ok(),
                    Err(_) => false,
                };
                if ok {
                    sent += 1;
                }
                ok
            });
            Ok(format!("已向 {} 个客户端发送 {} 字节", sent, bytes.len()))
        }
        NetKind::Udp => {
            let socket = handle.socket.as_ref().ok_or("句柄异常: 缺少 UDP 套接字")?;
            let sp = socket.lock().map_err(|e| e.to_string())?;
            // 未设置目标地址时 send 会报 NotConnected，转成友好提示
            sp.send(bytes)
                .map_err(|_| format!("发送失败，请检查 {} 的目标地址设置", handle.label))?;
            Ok(format!("已发送 {} 字节", bytes.len()))
        }
    }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}


/*
*   描述：启动自动发送（Rust 线程按间隔直接写网络连接，绕过前端 IPC）
*   传参：key：会话唯一标识, data：待发送内容, interval_ms：间隔（最小 10ms）
*/
#[tauri::command]
pub async fn net_auto_send_start(
    key: String,
    data: String,
    interval_ms: u64,
    state: tauri::State<'_, SharedNetState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if let Some(old) = guard.auto_sends.remove(&key) {
            old.store(true, Ordering::SeqCst);
        }
        let handle = guard.conns.get(&key).ok_or(format!("连接未打开: {}", key))?;
        let label = handle.label.clone();
        let stop = Arc::new(AtomicBool::new(false));
        guard.auto_sends.insert(key.clone(), stop.clone());
        drop(guard);

        let interval = std::time::Duration::from_millis(interval_ms.max(1));
        thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            crate::serial::wait_precisely(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let guard = match state.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            match guard.conns.get(&key) {
                Some(handle) => {
                    if let Err(e) = net_write_bytes(handle, data.as_bytes()) {
                        // 写失败：通知前端弹回自动发送开关
                        let _ = app_handle.emit(
                            "auto-send-stopped",
                            AutoSentEvent {
                                key: key.clone(),
                                data: e.into_bytes(),
                            },
                        );
                        break;
                    }
                    // 通知前端记录 TX
                    let _ = app_handle.emit(
                        "auto-sent",
                        AutoSentEvent {
                            key: key.clone(),
                            data: data.as_bytes().to_vec(),
                        },
                    );
                }
                None => break,
            }
        });
        Ok(format!("{} 自动发送已启动，间隔 {}ms", label, interval_ms))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：停止指定会话的自动发送任务
*/
#[tauri::command]
pub async fn net_auto_send_stop(key: String, state: tauri::State<'_, SharedNetState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if let Some(task) = guard.auto_sends.remove(&key) {
            task.store(true, Ordering::SeqCst);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
